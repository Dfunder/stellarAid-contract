use std::collections::HashMap;
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::Mutex;
use tracing::{debug, info, warn};

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

#[derive(Debug, Error)]
pub enum BatchError {
    #[error("batch submission failed: {0}")]
    SubmissionFailed(String),
    #[error("call {index} in batch failed: {reason}")]
    PartialFailure { index: usize, reason: String },
    #[error("batch is empty — nothing to submit")]
    EmptyBatch,
    #[error("batch size exceeds maximum ({0})")]
    TooLarge(usize),
}

// ---------------------------------------------------------------------------
// Contract call descriptor
// ---------------------------------------------------------------------------

/// A single contract call to be batched.
#[derive(Debug, Clone)]
pub struct ContractCall {
    /// Contract address (C... strkey).
    pub contract_id: String,
    /// Soroban function name.
    pub function_name: String,
    /// XDR-encoded parameters.
    pub params_xdr: Vec<u8>,
    /// Optional human-readable label for logging / metrics.
    pub label: Option<String>,
}

impl ContractCall {
    pub fn new(contract_id: impl Into<String>, function_name: impl Into<String>) -> Self {
        Self {
            contract_id: contract_id.into(),
            function_name: function_name.into(),
            params_xdr: Vec::new(),
            label: None,
        }
    }

    pub fn with_params(mut self, params: Vec<u8>) -> Self {
        self.params_xdr = params;
        self
    }

    pub fn with_label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }
}

// ---------------------------------------------------------------------------
// Batching efficiency metrics
// ---------------------------------------------------------------------------

/// Tracks efficiency metrics for batched contract calls.
#[derive(Debug, Clone, Default)]
pub struct BatchMetrics {
    /// Total number of calls submitted.
    pub total_calls: u64,
    /// Number of those calls that were batched.
    pub batched_calls: u64,
    /// Number of calls that fell back to single submission.
    pub single_calls: u64,
    /// Number of batch submissions attempted.
    pub batch_submissions: u64,
    /// Number of batch submissions that fell back.
    pub fallback_submissions: u64,
}

impl BatchMetrics {
    /// Percentage of calls that were successfully batched.
    pub fn batch_rate(&self) -> f64 {
        if self.total_calls == 0 {
            return 0.0;
        }
        self.batched_calls as f64 / self.total_calls as f64 * 100.0
    }

    /// Average calls per batch submission (0 if no batches).
    pub fn avg_batch_size(&self) -> f64 {
        if self.batch_submissions == 0 {
            return 0.0;
        }
        self.batched_calls as f64 / self.batch_submissions as f64
    }

    /// Percentage of batch submissions that had to fall back.
    pub fn fallback_rate(&self) -> f64 {
        let total = self.batch_submissions + self.fallback_submissions;
        if total == 0 {
            return 0.0;
        }
        self.fallback_submissions as f64 / total as f64 * 100.0
    }

    /// Reset all counters.
    pub fn reset(&mut self) {
        *self = Self::default();
    }
}

// ---------------------------------------------------------------------------
// Batch result
// ---------------------------------------------------------------------------

/// Result of submitting a batch of contract calls.
#[derive(Debug)]
pub enum BatchResult {
    /// All calls succeeded in a single batched transaction.
    Batched {
        /// Transaction hash from the network.
        tx_hash: String,
        /// Number of calls in the batch.
        call_count: usize,
    },
    /// Some or all calls succeeded individually after batch failure.
    Fallback {
        /// Per-call results: Ok(hash) or Err(reason).
        results: Vec<Result<String, String>>,
        /// Which calls were successfully submitted individually.
        success_count: usize,
    },
}

// ---------------------------------------------------------------------------
// Batch builder — collects calls before submission
// ---------------------------------------------------------------------------

/// Builds up a batch of contract calls and submits them as a group.
///
/// # Fallback strategy
///
/// If the batch submission fails, each call is retried individually.
/// This ensures that partial progress is still possible.
pub struct BatchBuilder {
    calls: Vec<ContractCall>,
    max_batch_size: usize,
    metrics: Arc<Mutex<BatchMetrics>>,
}

impl BatchBuilder {
    /// Create a new batch builder with a default max size of 20 calls.
    pub fn new() -> Self {
        Self::with_max_size(20)
    }

    /// Create a new batch builder with a custom max size.
    pub fn with_max_size(max_batch_size: usize) -> Self {
        Self {
            calls: Vec::new(),
            max_batch_size,
            metrics: Arc::new(Mutex::new(BatchMetrics::default())),
        }
    }

    /// Add a single contract call to the batch.
    pub fn add_call(&mut self, call: ContractCall) -> Result<(), BatchError> {
        if self.calls.len() >= self.max_batch_size {
            return Err(BatchError::TooLarge(self.max_batch_size));
        }
        self.calls.push(call);
        Ok(())
    }

    /// Add multiple calls at once.
    pub fn add_calls(&mut self, calls: impl IntoIterator<Item = ContractCall>) -> Result<(), BatchError> {
        for call in calls {
            self.add_call(call)?;
        }
        Ok(())
    }

    /// Number of calls currently in the batch.
    pub fn len(&self) -> usize {
        self.calls.len()
    }

    /// Whether the batch is empty.
    pub fn is_empty(&self) -> bool {
        self.calls.is_empty()
    }

    /// Get a snapshot of current metrics.
    pub async fn metrics(&self) -> BatchMetrics {
        self.metrics.lock().await.clone()
    }

    /// Take the accumulated calls out of the builder (resets the builder).
    pub fn take_calls(&mut self) -> Vec<ContractCall> {
        std::mem::take(&mut self.calls)
    }

    /// Group calls by contract address for transaction optimization.
    ///
    /// Calls to the same contract can be merged into a single transaction
    /// with multiple operations, reducing ledger entry contention.
    pub fn group_by_contract(&self) -> HashMap<&str, Vec<&ContractCall>> {
        let mut groups: HashMap<&str, Vec<&ContractCall>> = HashMap::new();
        for call in &self.calls {
            groups.entry(&call.contract_id).or_default().push(call);
        }
        groups
    }

    /// Submit the batch, falling back to individual calls on failure.
    ///
    /// The `submit_fn` is called with a list of calls and should return
    /// either a transaction hash on success or an error message.
    /// When `submit_fn` returns an error, each call is retried individually.
    pub async fn submit<F, Fut>(
        &mut self,
        submit_fn: F,
    ) -> Result<BatchResult, BatchError>
    where
        F: Fn(Vec<ContractCall>) -> Fut + Send + Sync + 'static,
        Fut: std::future::Future<Output = Result<String, String>> + Send,
    {
        if self.is_empty() {
            return Err(BatchError::EmptyBatch);
        }

        let calls = self.take_calls();
        let call_count = calls.len();

        // Try batch submission first
        match submit_fn(calls.clone()).await {
            Ok(tx_hash) => {
                let mut metrics = self.metrics.lock().await;
                metrics.total_calls += call_count as u64;
                metrics.batched_calls += call_count as u64;
                metrics.batch_submissions += 1;

                info!(
                    call_count = call_count,
                    tx_hash = %tx_hash,
                    batch_rate = metrics.batch_rate(),
                    "batch submitted successfully"
                );

                Ok(BatchResult::Batched { tx_hash, call_count })
            }
            Err(batch_err) => {
                warn!(
                    error = %batch_err,
                    call_count = call_count,
                    "batch submission failed, falling back to individual calls"
                );

                let mut results = Vec::with_capacity(call_count);
                let mut success_count = 0usize;

                for (i, call) in calls.into_iter().enumerate() {
                    match submit_fn(vec![call]).await {
                        Ok(hash) => {
                            results.push(Ok(hash));
                            success_count += 1;
                        }
                        Err(e) => {
                            warn!(index = i, error = %e, "individual call failed");
                            results.push(Err(e));
                        }
                    }
                }

                let mut metrics = self.metrics.lock().await;
                metrics.total_calls += call_count as u64;
                metrics.single_calls += call_count as u64;
                metrics.fallback_submissions += 1;

                debug!(
                    success_count = success_count,
                    total = call_count,
                    fallback_rate = metrics.fallback_rate(),
                    "fallback submission completed"
                );

                Ok(BatchResult::Fallback { results, success_count })
            }
        }
    }
}

impl Default for BatchBuilder {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Transaction grouping utilities
// ---------------------------------------------------------------------------

/// Groups contract calls into transaction-sized chunks.
///
/// Stellar transactions have a maximum operation count (currently 100).
/// This function respects that limit and produces groups ready for submission.
pub fn group_into_transactions(
    calls: &[ContractCall],
    max_ops_per_tx: usize,
) -> Vec<Vec<&ContractCall>> {
    calls
        .chunks(max_ops_per_tx)
        .map(|chunk| chunk.iter().collect())
        .collect()
}

/// Creates a single-call batch as a convenience for the fallback path.
pub fn single_call_batch(call: ContractCall) -> BatchBuilder {
    let mut builder = BatchBuilder::with_max_size(1);
    let _ = builder.add_call(call);
    builder
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -- Builder basics --

    #[test]
    fn test_builder_starts_empty() {
        let builder = BatchBuilder::new();
        assert!(builder.is_empty());
        assert_eq!(builder.len(), 0);
    }

    #[test]
    fn test_builder_add_single_call() {
        let mut builder = BatchBuilder::new();
        let call = ContractCall::new("CABC...", "transfer");
        builder.add_call(call).unwrap();
        assert_eq!(builder.len(), 1);
    }

    #[test]
    fn test_builder_rejects_over_max_size() {
        let mut builder = BatchBuilder::with_max_size(2);
        builder.add_call(ContractCall::new("C1", "fn1")).unwrap();
        builder.add_call(ContractCall::new("C2", "fn2")).unwrap();
        let result = builder.add_call(ContractCall::new("C3", "fn3"));
        assert!(matches!(result, Err(BatchError::TooLarge(2))));
    }

    #[test]
    fn test_builder_add_calls_bulk() {
        let mut builder = BatchBuilder::new();
        let calls = vec![
            ContractCall::new("C1", "fn1"),
            ContractCall::new("C2", "fn2"),
            ContractCall::new("C3", "fn3"),
        ];
        builder.add_calls(calls).unwrap();
        assert_eq!(builder.len(), 3);
    }

    #[test]
    fn test_builder_take_calls_resets() {
        let mut builder = BatchBuilder::new();
        builder.add_call(ContractCall::new("C1", "fn1")).unwrap();
        builder.add_call(ContractCall::new("C2", "fn2")).unwrap();
        let taken = builder.take_calls();
        assert_eq!(taken.len(), 2);
        assert!(builder.is_empty());
    }

    // -- Group by contract --

    #[test]
    fn test_group_by_contract() {
        let mut builder = BatchBuilder::new();
        builder.add_call(ContractCall::new("CA", "fn1")).unwrap();
        builder.add_call(ContractCall::new("CB", "fn2")).unwrap();
        builder.add_call(ContractCall::new("CA", "fn3")).unwrap();

        let groups = builder.group_by_contract();
        assert_eq!(groups.len(), 2);
        assert_eq!(groups["CA"].len(), 2);
        assert_eq!(groups["CB"].len(), 1);
    }

    // -- Metrics --

    #[test]
    fn test_metrics_batch_rate() {
        let mut m = BatchMetrics::default();
        m.total_calls = 10;
        m.batched_calls = 8;
        assert_eq!(m.batch_rate(), 80.0);
    }

    #[test]
    fn test_metrics_avg_batch_size() {
        let mut m = BatchMetrics::default();
        m.batched_calls = 20;
        m.batch_submissions = 4;
        assert_eq!(m.avg_batch_size(), 5.0);
    }

    #[test]
    fn test_metrics_fallback_rate() {
        let mut m = BatchMetrics::default();
        m.batch_submissions = 8;
        m.fallback_submissions = 2;
        assert_eq!(m.fallback_rate(), 20.0);
    }

    #[test]
    fn test_metrics_zero_calls() {
        let m = BatchMetrics::default();
        assert_eq!(m.batch_rate(), 0.0);
        assert_eq!(m.avg_batch_size(), 0.0);
        assert_eq!(m.fallback_rate(), 0.0);
    }

    // -- Transaction grouping --

    #[test]
    fn test_group_into_transactions() {
        let calls: Vec<ContractCall> = (0..25)
            .map(|i| ContractCall::new(format!("C{}", i), "fn"))
            .collect();
        let groups = group_into_transactions(&calls, 10);
        assert_eq!(groups.len(), 3);
        assert_eq!(groups[0].len(), 10);
        assert_eq!(groups[1].len(), 10);
        assert_eq!(groups[2].len(), 5);
    }

    #[test]
    fn test_group_into_single_tx() {
        let calls: Vec<ContractCall> = (0..3)
            .map(|i| ContractCall::new(format!("C{}", i), "fn"))
            .collect();
        let groups = group_into_transactions(&calls, 100);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].len(), 3);
    }

    // -- Single call batch --

    #[test]
    fn test_single_call_batch() {
        let call = ContractCall::new("CABC...", "transfer");
        let builder = single_call_batch(call);
        assert_eq!(builder.len(), 1);
    }

    // -- Async submit tests --

    #[tokio::test]
    async fn test_submit_empty_batch_returns_error() {
        let mut builder = BatchBuilder::new();
        let result = builder.submit(|_| async { Ok("hash".into()) }).await;
        assert!(matches!(result, Err(BatchError::EmptyBatch)));
    }

    #[tokio::test]
    async fn test_submit_success_returns_batched() {
        let mut builder = BatchBuilder::new();
        builder.add_call(ContractCall::new("C1", "fn1")).unwrap();
        builder.add_call(ContractCall::new("C2", "fn2")).unwrap();

        let result = builder
            .submit(|calls| async move {
                Ok(format!("batch-{}-calls", calls.len()))
            })
            .await
            .unwrap();

        match result {
            BatchResult::Batched { tx_hash, call_count } => {
                assert_eq!(call_count, 2);
                assert!(tx_hash.contains("batch"));
            }
            _ => panic!("expected Batched result"),
        }

        let metrics = builder.metrics().await;
        assert_eq!(metrics.total_calls, 2);
        assert_eq!(metrics.batched_calls, 2);
        assert_eq!(metrics.batch_submissions, 1);
    }

    #[tokio::test]
    async fn test_submit_falls_back_on_batch_failure() {
        let mut builder = BatchBuilder::new();
        builder.add_call(ContractCall::new("C1", "fn1")).unwrap();
        builder.add_call(ContractCall::new("C2", "fn2")).unwrap();

        let result = builder
            .submit(|calls| async move {
                if calls.len() > 1 {
                    Err("batch too large".into())
                } else {
                    Ok(format!("single-{}", calls[0].contract_id))
                }
            })
            .await
            .unwrap();

        match result {
            BatchResult::Fallback { results, success_count } => {
                assert_eq!(results.len(), 2);
                assert_eq!(success_count, 2);
                for r in &results {
                    assert!(r.is_ok());
                }
            }
            _ => panic!("expected Fallback result"),
        }

        let metrics = builder.metrics().await;
        assert_eq!(metrics.fallback_submissions, 1);
        assert_eq!(metrics.single_calls, 2);
    }

    #[tokio::test]
    async fn test_submit_partial_failure_in_fallback() {
        let mut builder = BatchBuilder::new();
        builder.add_call(ContractCall::new("C1", "fn1")).unwrap();
        builder.add_call(ContractCall::new("C2", "fn2")).unwrap();

        let result = builder
            .submit(|calls| async move {
                if calls.len() > 1 {
                    Err("batch failed".into())
                } else if calls[0].contract_id == "C1" {
                    Ok("hash-c1".into())
                } else {
                    Err("C2 contract not found".into())
                }
            })
            .await
            .unwrap();

        match result {
            BatchResult::Fallback { results, success_count } => {
                assert_eq!(success_count, 1);
                assert!(results[0].is_ok());
                assert!(results[1].is_err());
            }
            _ => panic!("expected Fallback result"),
        }
    }

    #[tokio::test]
    async fn test_metrics_reset() {
        let mut builder = BatchBuilder::new();
        builder.add_call(ContractCall::new("C1", "fn1")).unwrap();
        let _ = builder.submit(|_| async { Ok("hash".into()) }).await;

        let mut metrics = builder.metrics().await;
        assert!(metrics.total_calls > 0);
        metrics.reset();
        assert_eq!(metrics.total_calls, 0);
    }

    // -- ContractCall builder --

    #[test]
    fn test_contract_call_builder_chain() {
        let call = ContractCall::new("CABC...", "transfer")
            .with_params(vec![1, 2, 3])
            .with_label("token transfer");

        assert_eq!(call.contract_id, "CABC...");
        assert_eq!(call.function_name, "transfer");
        assert_eq!(call.params_xdr, vec![1, 2, 3]);
        assert_eq!(call.label.as_deref(), Some("token transfer"));
    }
}

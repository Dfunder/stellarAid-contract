//! Resumable batch submission: a durable checkpoint for [`BatchBuilder`] (#979).
//!
//! [`crate::batch`] is a capable batch builder, but `take_calls()` is
//! one-shot: if the process dies between two submissions, the batch is gone.
//! This module adds the missing *durability* dimension. It is deliberately
//! additive — no existing `BatchBuilder` method changes — and is complementary
//! to [`crate::retry`]:
//!
//! | Concern | Mechanism |
//! |---|---|
//! | A transient error *within* a run | [`crate::retry::retry_async`] with backoff |
//! | A run that *ended* (crash, restart, deploy) | [`BatchCheckpoint`] + [`ResumableBatch`] |
//!
//! # At-least-once, not exactly-once
//!
//! Resuming re-submits every call that is not recorded as succeeded. A crash
//! between *submitting* a call and *persisting* its outcome therefore re-submits
//! that call on the next run. The guarantee is therefore **at-least-once**:
//! every call is submitted one or more times, never zero.
//!
//! Callers who cannot tolerate a duplicate must make their calls idempotent. The
//! recommended mechanism is a deterministic per-call marker — the same value on
//! every attempt, e.g. a hash of `(contract_id, function_name, params_xdr)`
//! embedded in the call's params or in the transaction memo — so the callee can
//! recognise and ignore a replay. The SDK does not do this for you because it
//! cannot know which callee is idempotent.
//!
//! # Maximum-loss window
//!
//! The checkpoint is persisted after **every** call outcome, and a
//! [`BatchCheckpointStore`] must implement `save` atomically (write-temp then
//! rename; see [`FileCheckpointStore`]). The maximum state lost to a crash is
//! therefore the outcome of **one** call — the one in flight — which is exactly
//! the call that will be retried. Batches are never silently truncated.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

use crate::batch::{BatchError, ContractCall};

/// Outcome of a single call in a checkpointed batch.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CallState {
    /// Not attempted yet in any run.
    Pending,
    /// Submitted and confirmed; holds the transaction hash.
    Succeeded(String),
    /// Attempted and failed; holds the reason. Retried on the next run.
    Failed(String),
}

impl Default for CallState {
    fn default() -> Self {
        CallState::Pending
    }
}

/// A durable snapshot of a batch: the calls, their per-call outcomes, and the
/// cursor within the current run.
///
/// Serialisable so it can live in a file, a database row, or object storage —
/// the SDK never picks the medium.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BatchCheckpoint {
    /// Schema version of the checkpoint envelope itself.
    pub version: u32,
    /// The full ordered call list.
    pub calls: Vec<ContractCall>,
    /// Per-call outcome, index-aligned with `calls`.
    pub states: Vec<CallState>,
    /// Progress within the current run (not persisted meaning; reset on resume).
    #[serde(default)]
    pub cursor: usize,
}

impl BatchCheckpoint {
    /// Current checkpoint envelope version.
    pub const VERSION: u32 = 1;

    /// Create a fresh checkpoint for `calls`, all pending.
    pub fn new(calls: Vec<ContractCall>) -> Self {
        let states = vec![CallState::Pending; calls.len()];
        Self {
            version: Self::VERSION,
            calls,
            states,
            cursor: 0,
        }
    }

    pub fn len(&self) -> usize {
        self.calls.len()
    }

    pub fn is_empty(&self) -> bool {
        self.calls.is_empty()
    }

    /// Fail if this checkpoint was written by an incompatible SDK version.
    pub fn validate(&self) -> Result<(), BatchError> {
        if self.version != Self::VERSION {
            return Err(BatchError::Checkpoint(format!(
                "unsupported checkpoint version {} (expected {})",
                self.version,
                Self::VERSION
            )));
        }
        if self.states.len() != self.calls.len() {
            return Err(BatchError::Checkpoint(
                "checkpoint is corrupt: states/calls length mismatch".into(),
            ));
        }
        Ok(())
    }

    /// Calls never attempted.
    pub fn pending_indices(&self) -> Vec<usize> {
        self.indices_where(|s| matches!(s, CallState::Pending))
    }

    /// Calls that were attempted and failed.
    pub fn failed_indices(&self) -> Vec<usize> {
        self.indices_where(|s| matches!(s, CallState::Failed(_)))
    }

    /// Calls that still need work: pending or previously failed. This is what a
    /// resume pass submits, and what makes "retry only the failures" work.
    pub fn retryable_indices(&self) -> Vec<usize> {
        self.indices_where(|s| !matches!(s, CallState::Succeeded(_)))
    }

    /// Calls already confirmed.
    pub fn succeeded_indices(&self) -> Vec<usize> {
        self.indices_where(|s| matches!(s, CallState::Succeeded(_)))
    }

    pub fn succeeded_count(&self) -> usize {
        self.succeeded_indices().len()
    }

    /// True once every call has succeeded.
    pub fn is_complete(&self) -> bool {
        !self.calls.is_empty() && self.states.iter().all(|s| matches!(s, CallState::Succeeded(_)))
    }

    pub fn state(&self, index: usize) -> Option<&CallState> {
        self.states.get(index)
    }

    /// Record a successful call. `index` must be in range.
    pub fn mark_succeeded(
        &mut self,
        index: usize,
        tx_hash: impl Into<String>,
    ) -> Result<(), BatchError> {
        match self.states.get_mut(index) {
            Some(slot) => {
                *slot = CallState::Succeeded(tx_hash.into());
                Ok(())
            }
            None => Err(BatchError::Checkpoint(format!(
                "call index {} out of range ({})",
                index,
                self.calls.len()
            ))),
        }
    }

    /// Record a failed call. `index` must be in range.
    pub fn mark_failed(&mut self, index: usize, reason: impl Into<String>) -> Result<(), BatchError> {
        match self.states.get_mut(index) {
            Some(slot) => {
                *slot = CallState::Failed(reason.into());
                Ok(())
            }
            None => Err(BatchError::Checkpoint(format!(
                "call index {} out of range ({})",
                index,
                self.calls.len()
            ))),
        }
    }

    /// Serialise the checkpoint for a store to persist.
    pub fn to_json(&self) -> Result<String, BatchError> {
        serde_json::to_string(self)
            .map_err(|e| BatchError::Checkpoint(format!("serialise failed: {e}")))
    }

    /// Deserialise and validate a checkpoint.
    pub fn from_json(raw: &str) -> Result<Self, BatchError> {
        let checkpoint: Self = serde_json::from_str(raw)
            .map_err(|e| BatchError::Checkpoint(format!("deserialise failed: {e}")))?;
        checkpoint.validate()?;
        Ok(checkpoint)
    }

    fn indices_where(&self, f: impl Fn(&CallState) -> bool) -> Vec<usize> {
        self.states
            .iter()
            .enumerate()
            .filter(|(_, s)| f(s))
            .map(|(i, _)| i)
            .collect()
    }

    fn begin_run(&mut self) {
        self.cursor = 0;
    }

    fn advance(&mut self, index: usize) {
        self.cursor = index + 1;
    }

    fn next_unsucceeded(&self) -> Option<usize> {
        (self.cursor..self.calls.len())
            .find(|&i| !matches!(self.states[i], CallState::Succeeded(_)))
    }
}

/// The caller-supplied medium for a checkpoint. Implement it for a file, a
/// database row, Redis, object storage — whatever the host already uses.
///
/// # Contract
///
/// [`BatchCheckpointStore::save`] **must be atomic**: a reader must never see a
/// partial or torn checkpoint. Implementations satisfy this by writing to a
/// temporary location and renaming over the destination. The SDK relies on this
/// to promise that a crash loses at most one call's outcome.
pub trait BatchCheckpointStore: Send + Sync {
    fn save(&self, checkpoint: &BatchCheckpoint) -> Result<(), BatchError>;
    fn load(&self) -> Result<Option<BatchCheckpoint>, BatchError>;
    fn clear(&self) -> Result<(), BatchError>;
}

/// In-memory store. Useful for tests and for a caller that already has a
/// durable layer elsewhere.
#[derive(Default)]
pub struct MemoryCheckpointStore {
    inner: Mutex<Option<BatchCheckpoint>>,
}

impl MemoryCheckpointStore {
    pub fn new() -> Self {
        Self::default()
    }

    /// Peek at the current checkpoint without consuming it.
    pub fn snapshot(&self) -> Option<BatchCheckpoint> {
        self.inner.lock().unwrap().clone()
    }
}

impl BatchCheckpointStore for MemoryCheckpointStore {
    fn save(&self, checkpoint: &BatchCheckpoint) -> Result<(), BatchError> {
        *self.inner.lock().unwrap() = Some(checkpoint.clone());
        Ok(())
    }

    fn load(&self) -> Result<Option<BatchCheckpoint>, BatchError> {
        Ok(self.inner.lock().unwrap().clone())
    }

    fn clear(&self) -> Result<(), BatchError> {
        *self.inner.lock().unwrap() = None;
        Ok(())
    }
}

/// File-backed store for scripts. `save` writes `path.tmp` then renames it over
/// `path`, so a concurrent reader sees either the old or the new checkpoint,
/// never a half-written one.
pub struct FileCheckpointStore {
    path: PathBuf,
}

impl FileCheckpointStore {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    fn temp_path(&self) -> PathBuf {
        let mut tmp = self.path.clone();
        let ext = match self.path.extension().and_then(|e| e.to_str()) {
            Some(e) => format!("{e}.tmp"),
            None => "tmp".to_string(),
        };
        tmp.set_extension(ext);
        tmp
    }
}

impl BatchCheckpointStore for FileCheckpointStore {
    fn save(&self, checkpoint: &BatchCheckpoint) -> Result<(), BatchError> {
        let json = checkpoint.to_json()?;
        let tmp = self.temp_path();
        std::fs::write(&tmp, json.as_bytes())
            .map_err(|e| BatchError::Checkpoint(format!("write {}: {e}", tmp.display())))?;
        // `rename` replaces the destination atomically on the same filesystem.
        std::fs::rename(&tmp, &self.path)
            .map_err(|e| BatchError::Checkpoint(format!("rename into {}: {e}", self.path.display())))
    }

    fn load(&self) -> Result<Option<BatchCheckpoint>, BatchError> {
        match std::fs::read_to_string(&self.path) {
            Ok(raw) => Ok(Some(BatchCheckpoint::from_json(&raw)?)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(BatchError::Checkpoint(format!(
                "read {}: {e}",
                self.path.display()
            ))),
        }
    }

    fn clear(&self) -> Result<(), BatchError> {
        match std::fs::remove_file(&self.path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(BatchError::Checkpoint(format!(
                "remove {}: {e}",
                self.path.display()
            ))),
        }
    }
}

/// What one resume run did.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BatchSummary {
    pub attempted: usize,
    pub succeeded: usize,
    pub failed: usize,
}

/// A checkpoint plus the loop that resumes it.
#[derive(Debug, Clone)]
pub struct ResumableBatch {
    pub checkpoint: BatchCheckpoint,
}

impl ResumableBatch {
    pub fn new(checkpoint: BatchCheckpoint) -> Self {
        Self { checkpoint }
    }

    /// Start a fresh resumable batch from an ordered call list.
    pub fn from_calls(calls: Vec<ContractCall>) -> Self {
        Self::new(BatchCheckpoint::new(calls))
    }

    /// Load the checkpoint from `store` and continue it.
    pub fn resume(store: &dyn BatchCheckpointStore) -> Result<Self, BatchError> {
        match store.load()? {
            Some(checkpoint) => {
                checkpoint.validate()?;
                Ok(Self::new(checkpoint))
            }
            None => Err(BatchError::Checkpoint("no checkpoint to resume".into())),
        }
    }

    /// Submit every as-yet-unsucceeded call, persisting the checkpoint after
    /// each outcome.
    ///
    /// Already-succeeded calls are skipped, so this retries only the calls that
    /// are pending or previously failed. See the module docs for the
    /// at-least-once guarantee and the one-call maximum-loss window.
    pub async fn submit_remaining<F, Fut>(
        &mut self,
        store: &dyn BatchCheckpointStore,
        submit_fn: F,
    ) -> Result<BatchSummary, BatchError>
    where
        F: Fn(ContractCall) -> Fut,
        Fut: std::future::Future<Output = Result<String, String>>,
    {
        self.checkpoint.begin_run();
        let mut summary = BatchSummary::default();

        while let Some(index) = self.checkpoint.next_unsucceeded() {
            let call = self.checkpoint.calls[index].clone();
            summary.attempted += 1;
            match submit_fn(call).await {
                Ok(hash) => {
                    self.checkpoint.mark_succeeded(index, hash)?;
                    summary.succeeded += 1;
                }
                Err(reason) => {
                    self.checkpoint.mark_failed(index, reason)?;
                    summary.failed += 1;
                }
            }
            self.checkpoint.advance(index);
            // Persist after every outcome: the maximum window a crash can lose
            // is this one call, which is exactly the one that gets retried.
            store.save(&self.checkpoint)?;
        }

        Ok(summary)
    }
}

/// Warn when a batch is being submitted at the fallback rate.
///
/// [`crate::batch::BatchMetrics::fallback_rate`] is an estimator that exists to
/// be acted on; this surfaces it as a loggable string when it crosses
/// `threshold_pct`.
pub fn fallback_rate_warning(
    metrics: &crate::batch::BatchMetrics,
    threshold_pct: f64,
) -> Option<String> {
    let rate = metrics.fallback_rate();
    if rate > threshold_pct {
        Some(format!(
            "batch fallback rate {rate:.1}% exceeds {threshold_pct:.1}% \
             ({} of {} submissions fell back) — inspect batch sizes and contract errors",
            metrics.fallback_submissions,
            metrics.batch_submissions + metrics.fallback_submissions
        ))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    fn call(id: &str) -> ContractCall {
        ContractCall::new(id, "fn").with_params(vec![1, 2, 3])
    }

    fn three_calls() -> Vec<ContractCall> {
        vec![call("C1"), call("C2"), call("C3")]
    }

    #[test]
    fn checkpoint_starts_all_pending() {
        let cp = BatchCheckpoint::new(three_calls());
        assert_eq!(cp.len(), 3);
        assert_eq!(cp.pending_indices(), vec![0, 1, 2]);
        assert!(cp.failed_indices().is_empty());
        assert!(!cp.is_complete());
    }

    #[test]
    fn json_roundtrip_preserves_everything() {
        let mut cp = BatchCheckpoint::new(three_calls());
        cp.mark_succeeded(0, "hash-0").unwrap();
        cp.mark_failed(1, "boom").unwrap();

        let raw = cp.to_json().unwrap();
        let back = BatchCheckpoint::from_json(&raw).unwrap();
        assert_eq!(back, cp);
    }

    #[test]
    fn version_mismatch_is_rejected() {
        let mut cp = BatchCheckpoint::new(three_calls());
        cp.version = 999;
        assert!(matches!(cp.validate(), Err(BatchError::Checkpoint(_))));
        assert!(BatchCheckpoint::from_json(&cp.to_json().unwrap()).is_err());
    }

    #[test]
    fn retryable_excludes_succeeded_only() {
        let mut cp = BatchCheckpoint::new(three_calls());
        cp.mark_succeeded(0, "hash-0").unwrap();
        cp.mark_failed(1, "boom").unwrap();
        // 2 is still Pending.
        assert_eq!(cp.retryable_indices(), vec![1, 2]);
        assert_eq!(cp.failed_indices(), vec![1]);
        assert_eq!(cp.pending_indices(), vec![2]);
        assert_eq!(cp.succeeded_count(), 1);
    }

    #[test]
    fn memory_store_roundtrips() {
        let store = MemoryCheckpointStore::new();
        assert!(store.load().unwrap().is_none());
        let cp = BatchCheckpoint::new(three_calls());
        store.save(&cp).unwrap();
        assert_eq!(store.load().unwrap().unwrap(), cp);
        store.clear().unwrap();
        assert!(store.load().unwrap().is_none());
    }

    #[test]
    fn file_store_roundtrips_and_replaces_atomically() {
        let dir = std::env::temp_dir();
        let path = dir.join(format!("stellar-aid-checkpoint-{}.json", std::process::id()));
        let store = FileCheckpointStore::new(&path);

        assert!(store.load().unwrap().is_none());
        let mut cp = BatchCheckpoint::new(three_calls());
        store.save(&cp).unwrap();
        cp.mark_succeeded(0, "hash-0").unwrap();
        store.save(&cp).unwrap();

        let loaded = store.load().unwrap().unwrap();
        assert_eq!(loaded.succeeded_count(), 1);
        // The temp file must not survive a save.
        assert!(!store.temp_path().exists());

        store.clear().unwrap();
        assert!(store.load().unwrap().is_none());
    }

    #[test]
    fn resume_errors_when_no_checkpoint() {
        let store = MemoryCheckpointStore::new();
        assert!(ResumableBatch::resume(&store).is_err());
    }

    #[tokio::test]
    async fn resume_after_interruption_skips_succeeded_and_retries_the_rest() {
        // Simulate a first run that succeeded on call 0, then "crashed"
        // before call 1 was recorded.
        let store = MemoryCheckpointStore::new();
        let mut cp = BatchCheckpoint::new(three_calls());
        cp.mark_succeeded(0, "hash-0").unwrap();
        store.save(&cp).unwrap();

        let mut batch = ResumableBatch::resume(&store).unwrap();
        let submitted = Arc::new(Mutex::new(Vec::<String>::new()));
        let seen = submitted.clone();

        let summary = batch
            .submit_remaining(&store, move |c| {
                let seen = seen.clone();
                async move {
                    seen.lock().unwrap().push(c.contract_id.clone());
                    Ok(format!("hash-{}", c.contract_id))
                }
            })
            .await
            .unwrap();

        // Only calls 1 and 2 are re-submitted; call 0 is skipped.
        assert_eq!(summary.attempted, 2);
        assert_eq!(summary.succeeded, 2);
        assert_eq!(summary.failed, 0);
        assert_eq!(*submitted.lock().unwrap(), vec!["C2", "C3"]);
        assert!(batch.checkpoint.is_complete());
        // The store reflects the completed run.
        assert!(store.snapshot().unwrap().is_complete());
    }

    #[tokio::test]
    async fn retry_only_failed_on_next_run() {
        let store = MemoryCheckpointStore::new();
        store.save(&BatchCheckpoint::new(three_calls())).unwrap();

        let attempts = Arc::new(AtomicUsize::new(0));
        let seen = attempts.clone();
        let mut batch = ResumableBatch::resume(&store).unwrap();

        // First run: C1 succeeds, C2 fails, C3 succeeds.
        let first = batch
            .submit_remaining(&store, move |c| {
                let seen = seen.clone();
                async move {
                    seen.fetch_add(1, Ordering::SeqCst);
                    if c.contract_id == "C2" {
                        Err("C2 rejected".into())
                    } else {
                        Ok(format!("hash-{}", c.contract_id))
                    }
                }
            })
            .await
            .unwrap();
        assert_eq!(first, BatchSummary { attempted: 3, succeeded: 2, failed: 1 });

        // Second run over a fresh resume: only the failed call is retried.
        let mut batch = ResumableBatch::resume(&store).unwrap();
        let second_calls = Arc::new(Mutex::new(Vec::<String>::new()));
        let seen = second_calls.clone();
        let second = batch
            .submit_remaining(&store, move |c| {
                let seen = seen.clone();
                async move {
                    seen.lock().unwrap().push(c.contract_id.clone());
                    Ok(format!("hash-{}", c.contract_id))
                }
            })
            .await
            .unwrap();

        assert_eq!(second, BatchSummary { attempted: 1, succeeded: 1, failed: 0 });
        assert_eq!(*second_calls.lock().unwrap(), vec!["C2"]);
        assert!(batch.checkpoint.is_complete());
    }

    #[tokio::test]
    async fn checkpoint_is_persisted_after_each_call() {
        // The submit_fn reads the store before each call, proving that the
        // previous call's outcome was already durable by the time the next call
        // starts — the one-call maximum-loss window, observed directly.
        let store = Arc::new(MemoryCheckpointStore::new());
        store.save(&BatchCheckpoint::new(three_calls())).unwrap();
        let mut batch = ResumableBatch::resume(&*store).unwrap();

        let observed = Arc::new(Mutex::new(Vec::<usize>::new()));
        let seen = observed.clone();
        let store_for_fn = store.clone();
        let _ = batch
            .submit_remaining(&*store, move |c| {
                let seen = seen.clone();
                let store_for_fn = store_for_fn.clone();
                async move {
                    // Before submitting call N, everything before it is durable.
                    let done = store_for_fn.snapshot().map(|c| c.succeeded_count()).unwrap_or(0);
                    seen.lock().unwrap().push(done);
                    Ok(format!("hash-{}", c.contract_id))
                }
            })
            .await
            .unwrap();

        // First call sees 0 persisted outcomes, then 1, then 2.
        assert_eq!(*observed.lock().unwrap(), vec![0, 1, 2]);
    }

    #[test]
    fn builder_checkpoint_requeues_only_retryable() {
        let mut builder = crate::batch::BatchBuilder::new();
        builder.add_call(call("C1")).unwrap();
        builder.add_call(call("C2")).unwrap();

        let mut cp = builder.checkpoint();
        cp.mark_succeeded(0, "hash-0").unwrap();

        let rebuilt = crate::batch::BatchBuilder::from_checkpoint(&cp);
        assert_eq!(rebuilt.len(), 1);
    }

    #[test]
    fn fallback_rate_warning_fires_above_threshold() {
        let mut metrics = crate::batch::BatchMetrics::default();
        metrics.batch_submissions = 1;
        metrics.fallback_submissions = 9; // 90%
        assert!(fallback_rate_warning(&metrics, 50.0).is_some());
        assert!(fallback_rate_warning(&metrics, 95.0).is_none());
    }
}

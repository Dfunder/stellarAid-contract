/// Represents a payment transaction that could not be matched to a campaign.
#[derive(Debug, Clone)]
pub struct UnmatchedPayment {
    pub transaction_hash: String,
    pub memo: Option<String>,
    pub amount: String,
    pub retry_count: u32,
}

/// Maximum number of retry attempts before a payment is marked as permanently failed.
const MAX_RETRIES: u32 = 3;

impl UnmatchedPayment {
    pub fn new(tx_hash: impl Into<String>, memo: Option<String>, amount: impl Into<String>) -> Self {
        Self {
            transaction_hash: tx_hash.into(),
            memo,
            amount: amount.into(),
            retry_count: 0,
        }
    }

    /// Returns true if this payment can still be retried.
    pub fn can_retry(&self) -> bool {
        self.retry_count < MAX_RETRIES
    }

    /// Increments the retry counter and returns the updated count.
    pub fn record_retry(&mut self) -> u32 {
        self.retry_count += 1;
        self.retry_count
    }
}

/// Logs an unmatched payment to stderr so it is visible in worker output.
pub fn log_unmatched(payment: &UnmatchedPayment) {
    eprintln!(
        "[UNMATCHED] tx={} memo={:?} amount={} retries={}",
        payment.transaction_hash, payment.memo, payment.amount, payment.retry_count
    );
}

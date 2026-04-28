use chrono::Utc;
use std::collections::HashMap;

/// Issue #140 – Audit Withdrawal Logs
/// Records every withdrawal action with a complete audit trail.

#[derive(Debug, Clone, PartialEq)]
pub enum WithdrawalAction {
    Requested,
    Approved,
    Rejected,
    Submitted,
}

impl std::fmt::Display for WithdrawalAction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            WithdrawalAction::Requested => write!(f, "REQUESTED"),
            WithdrawalAction::Approved => write!(f, "APPROVED"),
            WithdrawalAction::Rejected => write!(f, "REJECTED"),
            WithdrawalAction::Submitted => write!(f, "SUBMITTED"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct WithdrawalLogEntry {
    pub campaign_id: u64,
    pub action: WithdrawalAction,
    pub actor: String,
    pub amount: i128,
    pub timestamp: i64,
    pub note: Option<String>,
}

/// In-memory audit log for withdrawal actions.
#[derive(Default)]
pub struct WithdrawalAuditLog {
    entries: Vec<WithdrawalLogEntry>,
}

impl WithdrawalAuditLog {
    pub fn new() -> Self {
        Self::default()
    }

    /// Log a withdrawal action.
    pub fn log(&mut self, campaign_id: u64, action: WithdrawalAction, actor: &str, amount: i128, note: Option<String>) {
        self.entries.push(WithdrawalLogEntry {
            campaign_id,
            action,
            actor: actor.to_string(),
            amount,
            timestamp: Utc::now().timestamp(),
            note,
        });
    }

    /// Returns all log entries for a campaign.
    pub fn get_by_campaign(&self, campaign_id: u64) -> Vec<&WithdrawalLogEntry> {
        self.entries.iter().filter(|e| e.campaign_id == campaign_id).collect()
    }

    /// Returns all entries in the log.
    pub fn all(&self) -> &[WithdrawalLogEntry] {
        &self.entries
    }

    /// Returns a summary: count of each action type per campaign.
    pub fn summary(&self) -> HashMap<u64, HashMap<String, usize>> {
        let mut result: HashMap<u64, HashMap<String, usize>> = HashMap::new();
        for entry in &self.entries {
            result
                .entry(entry.campaign_id)
                .or_default()
                .entry(entry.action.to_string())
                .and_modify(|c| *c += 1)
                .or_insert(1);
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn logs_and_retrieves_entries() {
        let mut log = WithdrawalAuditLog::new();
        log.log(1, WithdrawalAction::Requested, "creator_A", 500, None);
        log.log(1, WithdrawalAction::Approved, "admin", 500, Some("looks good".to_string()));

        let entries = log.get_by_campaign(1);
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].action, WithdrawalAction::Requested);
        assert_eq!(entries[1].action, WithdrawalAction::Approved);
        assert_eq!(entries[1].note.as_deref(), Some("looks good"));
    }

    #[test]
    fn different_campaigns_are_isolated() {
        let mut log = WithdrawalAuditLog::new();
        log.log(1, WithdrawalAction::Requested, "creator_A", 100, None);
        log.log(2, WithdrawalAction::Requested, "creator_B", 200, None);

        assert_eq!(log.get_by_campaign(1).len(), 1);
        assert_eq!(log.get_by_campaign(2).len(), 1);
    }

    #[test]
    fn summary_counts_actions() {
        let mut log = WithdrawalAuditLog::new();
        log.log(1, WithdrawalAction::Requested, "creator_A", 100, None);
        log.log(1, WithdrawalAction::Approved, "admin", 100, None);
        log.log(1, WithdrawalAction::Submitted, "admin", 100, None);

        let summary = log.summary();
        let campaign_summary = &summary[&1];
        assert_eq!(campaign_summary["REQUESTED"], 1);
        assert_eq!(campaign_summary["APPROVED"], 1);
        assert_eq!(campaign_summary["SUBMITTED"], 1);
    }
}

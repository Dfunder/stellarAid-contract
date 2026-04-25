/// Extracts the campaign ID from a Stellar payment memo.
///
/// Convention: memo text is `"campaign:<id>"`, e.g. `"campaign:42"`.
pub fn campaign_id_from_memo(memo: &str) -> Option<u64> {
    let stripped = memo.strip_prefix("campaign:")?;
    stripped.trim().parse::<u64>().ok()
}

/// Returns true when the payment memo matches the expected campaign.
pub fn matches_campaign(memo: &str, campaign_id: u64) -> bool {
    campaign_id_from_memo(memo) == Some(campaign_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_campaign_id_from_valid_memo() {
        assert_eq!(campaign_id_from_memo("campaign:42"), Some(42));
    }

    #[test]
    fn returns_none_for_unrelated_memo() {
        assert_eq!(campaign_id_from_memo("hello world"), None);
    }

    #[test]
    fn matches_campaign_true_for_correct_id() {
        assert!(matches_campaign("campaign:7", 7));
    }

    #[test]
    fn matches_campaign_false_for_wrong_id() {
        assert!(!matches_campaign("campaign:7", 99));
    }
}

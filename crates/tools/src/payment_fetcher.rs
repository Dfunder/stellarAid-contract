use std::collections::HashMap;

/// A parsed incoming payment from the Stellar Horizon API.
#[derive(Debug, Clone)]
pub struct IncomingPayment {
    pub transaction_hash: String,
    pub from: String,
    pub amount: String,
    pub asset_code: String,
    pub memo: Option<String>,
}

/// Parses a single Horizon payment record (as a flat key→value map) into an `IncomingPayment`.
///
/// In production, deserialise directly from the Horizon JSON response instead.
pub fn parse_payment(record: &HashMap<&str, &str>) -> Option<IncomingPayment> {
    Some(IncomingPayment {
        transaction_hash: record.get("transaction_hash")?.to_string(),
        from: record.get("from")?.to_string(),
        amount: record.get("amount")?.to_string(),
        asset_code: record.get("asset_code").unwrap_or(&"XLM").to_string(),
        memo: record.get("memo").map(|s| s.to_string()),
    })
}

/// Filters a slice of payments, keeping only those directed to `account`.
pub fn filter_by_recipient<'a>(
    payments: &'a [IncomingPayment],
    account: &str,
) -> Vec<&'a IncomingPayment> {
    payments.iter().filter(|p| p.from != account).collect()
}

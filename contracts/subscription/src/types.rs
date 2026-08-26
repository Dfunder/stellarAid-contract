use soroban_sdk::{contracttype, Address, String, Symbol, Vec};

#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SubscriptionStatus {
    /// Paid up and renewing.
    Active = 0,
    /// Auto-renewal turned off; benefits run to the end of the paid period.
    Cancelled = 1,
    /// Lapsed after the grace period elapsed without a renewal.
    Expired = 2,
}

#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PaymentKind {
    Initial = 0,
    Renewal = 1,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Tier {
    pub tier_id: u32,
    pub name: String,
    pub price: i128,
    pub period_ledgers: u32,
    /// Entitlements granted by this tier, checked with `has_benefit`.
    pub benefits: Vec<Symbol>,
    pub active: bool,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Subscription {
    pub subscriber: Address,
    pub tier_id: u32,
    pub status: SubscriptionStatus,
    pub started_ledger: u32,
    /// Last ledger covered by the current paid period.
    pub period_end_ledger: u32,
    pub renewals: u32,
    pub total_paid: i128,
    pub auto_renew: bool,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PaymentRecord {
    pub sequence: u32,
    pub tier_id: u32,
    pub amount: i128,
    pub kind: PaymentKind,
    pub ledger: u32,
    pub period_end_ledger: u32,
}

#[contracttype]
pub enum DataKey {
    Admin,
    Token,
    GraceLedgers,
    HistoryLimit,
    Tier(u32),
    Subscription(Address),
    Credit(Address),
    Payments(Address),
}

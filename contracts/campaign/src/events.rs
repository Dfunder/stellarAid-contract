//! Canonical event schemas for the campaign contract.
//!
//! # Naming convention (issue #755)
//!
//! Every event follows `<entity>_<past_tense_verb>` in `snake_case`, e.g.
//! `campaign_initialized`, `donation_received`, `withdrawal_finalized`,
//! `dispute_raised`. The verb is always past tense and is the last segment, so
//! an indexer can group a single entity's lifecycle by a stable prefix
//! (`campaign_`, `donation_`, `withdrawal_`, `dispute_`, `refund_`).
//!
//! # Topic layout
//!
//! ```text
//! topics: ( Symbol("<event_name>"), Address /* emitting contract */ )
//! data:   <EventStruct>   // #[contracttype], field order is the wire order
//! ```
//!
//! Topic 0 is always the event name so that a Horizon/RPC `getEvents` filter
//! can select a single event by symbol; topic 1 is the emitting contract address
//! so that a shared emitter (for example the donation contract relaying into the
//! campaign contract) is still attributable.
//!
//! Adding a field to one of these structs is a breaking change for indexers;
//! see `docs/UPGRADE_AND_ROLLBACK.md` ("error code values are permanent").

use soroban_sdk::{contracttype, Address, BytesN, String, Symbol};

/// `campaign_initialized` — emitted once, at the end of `initialize`.
///
/// Topics: `["campaign_initialized", contract_address]`
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CampaignInitializedEvent {
    pub admin: Address,
    pub version: String,
    pub timestamp: u64,
}

/// `donation_received` — emitted after a successful token transfer *and* after
/// the campaign's `raised` total has been updated in storage, so an indexer can
/// read `raised_total` from the event without a follow-up query.
///
/// Topics: `["donation_received", contract_address]`
///
/// `campaign_id` is carried in the data (not as a third topic) to keep the
/// topic tuple exactly two segments wide and filterable by symbol + contract.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DonationReceivedEvent {
    pub campaign_id: u64,
    pub donor: Address,
    pub amount: i128,
    pub asset_code: String,
    pub raised_total: i128,
    pub timestamp: u64,
}

/// `withdrawal_requested` — emitted by `request_withdrawal` after the pending
/// request is written. Distinct from `withdrawal_finalized` (issue #757): the
/// request is a creator action that only schedules funds, the finalization is
/// the accounting + payout.
///
/// Topics: `["withdrawal_requested", contract_address]`
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WithdrawalRequestedEvent {
    pub campaign_id: u64,
    pub amount: i128,
    pub requested_at: u64,
    pub available_at: u64,
}

/// `withdrawal_finalized` — emitted by `finalize_withdrawal` after `raised` has
/// been debited and the fee has been computed. `recipient` is the campaign
/// owner (the creator is the only party allowed to finalize); `fee` is the
/// platform fee in stroops that the platform wallet is entitled to.
///
/// Topics: `["withdrawal_finalized", contract_address]`
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WithdrawalFinalizedEvent {
    pub campaign_id: u64,
    pub amount: i128,
    pub fee: i128,
    pub recipient: Address,
    pub timestamp: u64,
}

/// `withdrawal_cancelled` — emitted by `cancel_withdrawal_request` after the
/// pending request has been removed from storage.
///
/// Topics: `["withdrawal_cancelled", contract_address]`
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WithdrawalCancelledEvent {
    pub campaign_id: u64,
    pub amount: i128,
    pub cancelled_at: u64,
}

/// `dispute_raised` — emitted by `raise_dispute` after the dispute marker is
/// written. `reason_hash` is a `BytesN<32>` so the reason text never lands in
/// persistent storage (bounded input, see #765).
///
/// Topics: `["dispute_raised", contract_address]`
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DisputeRaisedEvent {
    pub campaign_id: u64,
    pub raised_by: Address,
    pub reason_hash: BytesN<32>,
    pub raised_at: u64,
}

/// `dispute_resolved` — emitted by `resolve_dispute`. `outcome` is
/// `true` when the withdrawal is allowed and `false` when it is blocked, so a
/// subscriber never has to interpret a second field.
///
/// Topics: `["dispute_resolved", contract_address]`
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DisputeResolvedEvent {
    pub campaign_id: u64,
    pub outcome: bool,
    pub resolved_at: u64,
}

/// `refund_issued` — emitted by `request_refund` after the donor's share has
/// been recorded, so a donor wallet can be credited off-chain without polling.
///
/// Topics: `["refund_issued", contract_address]`
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct RefundIssuedEvent {
    pub campaign_id: u64,
    pub donor: Address,
    pub amount: i128,
    pub timestamp: u64,
}

/// `campaign_ended` — emitted by `end_campaign` once the status is `Completed`.
///
/// Topics: `["campaign_ended", contract_address]`
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CampaignEndedEvent {
    pub campaign_id: u64,
    pub raised: i128,
    pub goal: i128,
    pub ended_at: u64,
}

/// `campaign_cancelled` — emitted by `cancel_campaign` once the status is
/// `Cancelled`. Only reachable when `raised == 0` (see #763 authorization
/// matrix); a campaign holding funds must be resolved through a dispute first.
///
/// Topics: `["campaign_cancelled", contract_address]`
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CampaignCancelledEvent {
    pub campaign_id: u64,
    pub cancelled_by: Address,
    pub reason_hash: BytesN<32>,
    pub cancelled_at: u64,
}

/// `deadline_extended` — emitted by `extend_deadline` after the new deadline
/// has been persisted, so a reminder job never has to diff two reads.
///
/// Topics: `["deadline_extended", contract_address]`
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeadlineExtendedEvent {
    pub campaign_id: u64,
    pub old_deadline: u64,
    pub new_deadline: u64,
    pub extended_at: u64,
}

/// Publishes `(event_name, contract_address)` topics for `data`.
///
/// Centralised so that every campaign event carries the same topic shape
/// (issue #755: "consistent naming convention"). `contract_address` is
/// evaluated lazily as `env.current_contract_address()`.
pub fn publish<T>(env: &soroban_sdk::Env, event_name: &str, data: T)
where
    T: soroban_sdk::IntoVal<soroban_sdk::Env, soroban_sdk::Val>,
{
    env.events().publish(
        (
            Symbol::new(env, event_name),
            env.current_contract_address(),
        ),
        data,
    );
}

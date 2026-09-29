//! Unified error catalogue shared across all Lumora contracts (closes #593).
//!
//! Each contract defines its own domain-specific `contracterror` enum for
//! Soroban's ABI, but all error *codes* are mapped here so that SDK developers
//! can decode any contract error through a single, consistent reference.
//!
//! ## Mapping scheme
//! Each domain occupies a 100-wide band:
//! | Range    | Domain                  |
//! |----------|-------------------------|
//! | 1 – 99   | General / shared        |
//! | 100–199  | Escrow                  |
//! | 200–299  | Commission agreement    |
//! | 300–399  | Campaign                |
//! | 400–499  | Donation                |
//! | 500–599  | Platform config         |
//! | 600–699  | Dispute arbiter         |

#![allow(unused)]

use soroban_sdk::{contracterror, symbol_short, Symbol};

// ── General errors (1–99) ────────────────────────────────────────────────────

/// Unified error codes that are meaningful across multiple contracts.
/// SDK consumers can use this table to interpret any `u32` error code
/// returned by a Lumora contract without knowing which domain it came from.
///
/// Closes #593.
#[repr(u32)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SharedErrorCode {
    // ── General ──────────────────────────────────────────────────────────────
    Unauthorized = 1,
    NotFound = 2,
    AlreadyExists = 3,
    InvalidAmount = 4,
    InvalidStatus = 5,
    ContractPaused = 6,
    ArithmeticOverflow = 7,
    DeadlineInPast = 8,
    DeadlineTooFar = 9,
    InputTooLong = 10,
    HealthUnhealthy = 11,
    RolloutRolledBack = 12,
    InvalidAlertConfig = 13,
    InvalidCanaryBps = 14,
    /// A scheduled pause recovery has not matured yet (#711).
    RecoveryPending = 15,
    /// No pause recovery is scheduled, so there is nothing to cancel (#711).
    RecoveryNotScheduled = 16,
    /// A pause recovery delay is zero or beyond the allowed maximum (#711).
    InvalidRecoveryDelay = 17,
    /// The contract is not paused (#711).
    ContractNotPaused = 18,

    // ── Escrow (100–199) ─────────────────────────────────────────────────────
    EscrowAlreadyExists = 100,
    EscrowNotFound = 101,
    EscrowInvalidStatus = 102,
    EscrowUnauthorized = 103,
    EscrowInvalidAmount = 104,
    EscrowInvalidFeeBps = 105,
    EscrowDisputeAlreadyOpen = 106,
    EscrowNotExpired = 107,
    EscrowReentrant = 108,
    EscrowInvalidAddress = 109,
    EscrowInsufficientBalance = 110,
    EscrowArithmeticOverflow = 111,
    EscrowContractPaused = 112,

    // ── Commission agreement (200–299) ───────────────────────────────────────
    AgreementAlreadyExists = 200,
    AgreementNotFound = 201,
    AgreementInvalidStatus = 202,
    AgreementUnauthorized = 203,
    AgreementInvalidAmount = 204,
    AgreementDeadlineInPast = 205,
    AgreementMilestoneBudgetExceeded = 206,
    AgreementNotAllMilestonesApproved = 207,
    AgreementArithmeticOverflow = 208,
    AgreementMilestoneLocked = 209,
    AgreementDeadlineTooFar = 210,
    AgreementTitleTooLong = 211,

    // ── Campaign (300–399) ───────────────────────────────────────────────────
    CampaignNotFound = 300,
    CampaignInvalidStatus = 301,
    CampaignUnauthorized = 302,
    CampaignDeadlineTooFar = 303,
    CampaignTitleTooLong = 304,
    CampaignContractPaused = 305,
    CampaignConfigInvalid = 306,

    // ── Donation (400–499) ───────────────────────────────────────────────────
    DonationInvalidAmount = 400,
    DonationCampaignNotActive = 401,
    DonationRefundNotAllowed = 402,
    DonationNonceAlreadyUsed = 403,
    DonationMemoTooLong = 404,
    DonationContractPaused = 405,
    DonationConfigInvalid = 406,

    // ── Platform config (500–599) ────────────────────────────────────────────
    ConfigUnauthorized = 500,
    ConfigAlreadyInitialized = 501,
    ConfigFeeBpsOutOfRange = 502,
    ConfigContractPaused = 503,

    // ── Dispute arbiter (600–699) ────────────────────────────────────────────
    DisputeNotFound = 600,
    DisputeAlreadyOpen = 601,
    DisputeInvalidStatus = 602,
    DisputeUnauthorized = 603,

    // ── Circuit breaker (700–709) ────────────────────────────────────────────
    /// Circuit breaker is tripped; operations are temporarily halted.
    CircuitBreakerTripped = 700,
    /// Automatic halt threshold was exceeded and the contract was halted.
    CircuitBreakerAutoHalted = 701,
    /// Manual halt requested but the contract is already halted.
    CircuitBreakerAlreadyHalted = 702,
    /// Manual reset requested but the contract is not halted.
    CircuitBreakerNotHalted = 703,
    /// Recovery is not yet available (time-lock or window not elapsed).
    CircuitBreakerRecoveryPending = 704,
}

impl SharedErrorCode {
    fn circuit_description(self) -> Option<&'static str> {
        match self {
            Self::CircuitBreakerTripped => Some("circuit breaker is tripped"),
            Self::CircuitBreakerAutoHalted => Some("auto-halt threshold exceeded"),
            Self::CircuitBreakerAlreadyHalted => Some("circuit breaker is already halted"),
            Self::CircuitBreakerNotHalted => Some("circuit breaker is not halted"),
            _ => None,
        }
    }
}

impl SharedErrorCode {
    /// Return a short human-readable description of this error code.
    pub fn description(self) -> &'static str {
        match self {
            Self::Unauthorized => "unauthorized",
            Self::NotFound => "not found",
            Self::AlreadyExists => "already exists",
            Self::InvalidAmount => "invalid amount",
            Self::InvalidStatus => "invalid status",
            Self::ContractPaused => "contract is paused",
            Self::ArithmeticOverflow => "arithmetic overflow",
            Self::DeadlineInPast => "deadline is in the past",
            Self::DeadlineTooFar => "deadline exceeds maximum allowed",
            Self::InputTooLong => "input string exceeds maximum length",
            Self::HealthUnhealthy => "contract health is unhealthy",
            Self::RolloutRolledBack => "rollout has been rolled back",
            Self::InvalidAlertConfig => "invalid health alert configuration",
            Self::InvalidCanaryBps => "canary traffic share exceeds 10000 bps",
            Self::RecoveryPending => "pause recovery is time-locked and has not matured",
            Self::RecoveryNotScheduled => "no pause recovery is scheduled",
            Self::InvalidRecoveryDelay => "pause recovery delay is out of range",
            Self::ContractNotPaused => "contract is not paused",

            Self::EscrowAlreadyExists => "escrow: already exists",
            Self::EscrowNotFound => "escrow: not found",
            Self::EscrowInvalidStatus => "escrow: invalid status",
            Self::EscrowUnauthorized => "escrow: unauthorized",
            Self::EscrowInvalidAmount => "escrow: invalid amount",
            Self::EscrowInvalidFeeBps => "escrow: invalid fee bps",
            Self::EscrowDisputeAlreadyOpen => "escrow: dispute already open",
            Self::EscrowNotExpired => "escrow: not expired",
            Self::EscrowReentrant => "escrow: reentrant call",
            Self::EscrowInvalidAddress => "escrow: invalid address",
            Self::EscrowInsufficientBalance => "escrow: insufficient balance",
            Self::EscrowArithmeticOverflow => "escrow: arithmetic overflow",
            Self::EscrowContractPaused => "escrow: contract paused",

            Self::AgreementAlreadyExists => "agreement: already exists",
            Self::AgreementNotFound => "agreement: not found",
            Self::AgreementInvalidStatus => "agreement: invalid status",
            Self::AgreementUnauthorized => "agreement: unauthorized",
            Self::AgreementInvalidAmount => "agreement: invalid amount",
            Self::AgreementDeadlineInPast => "agreement: deadline in past",
            Self::AgreementMilestoneBudgetExceeded => "agreement: milestone budget exceeded",
            Self::AgreementNotAllMilestonesApproved => "agreement: not all milestones approved",
            Self::AgreementArithmeticOverflow => "agreement: arithmetic overflow",
            Self::AgreementMilestoneLocked => "agreement: milestone transition locked",
            Self::AgreementDeadlineTooFar => "agreement: deadline exceeds maximum",
            Self::AgreementTitleTooLong => "agreement: title exceeds maximum length",

            Self::CampaignNotFound => "campaign: not found",
            Self::CampaignInvalidStatus => "campaign: invalid status",
            Self::CampaignUnauthorized => "campaign: unauthorized",
            Self::CampaignDeadlineTooFar => "campaign: deadline exceeds maximum",
            Self::CampaignTitleTooLong => "campaign: title exceeds maximum length",
            Self::CampaignContractPaused => "campaign: contract paused",
            Self::CampaignConfigInvalid => "campaign: config contract invocation failed",

            Self::DonationInvalidAmount => "donation: invalid amount",
            Self::DonationCampaignNotActive => "donation: campaign not active",
            Self::DonationRefundNotAllowed => "donation: refund not allowed",
            Self::DonationNonceAlreadyUsed => "donation: nonce already used",
            Self::DonationMemoTooLong => "donation: memo exceeds maximum length",
            Self::DonationContractPaused => "donation: contract paused",
            Self::DonationConfigInvalid => "donation: config contract invocation failed",

            Self::ConfigUnauthorized => "platform config: unauthorized",
            Self::ConfigAlreadyInitialized => "platform config: already initialized",
            Self::ConfigFeeBpsOutOfRange => "platform config: fee bps out of range",
            Self::ConfigContractPaused => "platform config: contract paused",

            Self::DisputeNotFound => "dispute: not found",
            Self::DisputeAlreadyOpen => "dispute: already open",
            Self::DisputeInvalidStatus => "dispute: invalid status",
            Self::DisputeUnauthorized => "dispute: unauthorized",

            Self::CircuitBreakerTripped => "circuit breaker is tripped; operations halted",
            Self::CircuitBreakerAutoHalted => "circuit breaker auto-halted: error threshold exceeded",
            Self::CircuitBreakerAlreadyHalted => "circuit breaker is already halted",
            Self::CircuitBreakerNotHalted => "circuit breaker is not halted; cannot reset",
            Self::CircuitBreakerRecoveryPending => "circuit breaker recovery is time-locked",
        }
    }
}

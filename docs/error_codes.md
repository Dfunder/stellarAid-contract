# Error Codes Documentation

This document lists all error codes used across stellarAid contracts.

## Common Error Codes

| Code | Name | Description |
|------|------|-------------|
| E001 | NotInitialized | Contract has not been initialized |
| E002 | AlreadyInitialized | Contract is already initialized |
| E003 | Unauthorized | Caller is not authorized to perform this action |
| E004 | InvalidAmount | The provided amount is invalid (zero or negative) |
| E005 | InsufficientBalance | Insufficient balance for the operation |
| E006 | InvalidAddress | The provided address is invalid or empty |
| E007 | OperationFailed | Generic operation failure |
| E008 | Overflow | Arithmetic overflow detected |
| E009 | DeadlineExpired | Transaction deadline has expired |
| E010 | NotFound | Requested resource was not found |

## Numeric bands

`contracts/shared/src/errors.rs` partitions the numeric space by contract so a
caller can tell which contract produced a code without extra context:

| Band | Contract |
|---|---|
| `1–99` | general / shared |
| `100–199` | escrow |
| `200–299` | commission agreement |
| `300–399` | **campaign** |
| `400–499` | donation |
| `500–599` | platform config |
| `600–699` | dispute arbiter |

Error code values are **permanent**: a retired code is never reused and an
existing code is never renumbered. See
[UPGRADE_AND_ROLLBACK.md](UPGRADE_AND_ROLLBACK.md).

## Campaign errors (`#[contracterror] Error`, codes 301–321)

Defined in `contracts/campaign/src/errors.rs`. Every variant carries a
one-line description available programmatically via `Error::description()`, a
human-readable `Display` form, and a ≤9-character client-facing shortcode from
`get_suggestion()`.

| Code | Variant | Description | Shortcode |
|---|---|---|---|
| 301 | `AlreadyInitialized` | contract has already been initialized | `ALREADY` |
| 302 | `NotInitialized` | contract has not been initialized | `NO_INIT` |
| 303 | `Unauthorized` | caller is not authorized to perform this action | `AUTH` |
| 304 | `CampaignEnded` | campaign is closed and accepts no further writes | `ENDED` |
| 305 | `CampaignNotActive` | campaign is not in a state that permits this action | `BAD_STS` |
| 306 | `AssetNotAccepted` | donation asset is not accepted by this campaign | `BAD_AST` |
| 307 | `DonationTooSmall` | donation amount is below the minimum accepted | `TOO_SML` |
| 308 | `InsufficientContractBalance` | contract balance cannot cover the requested payout | `NO_FUND` |
| 309 | `InvalidGoalAmount` | campaign goal must be greater than zero | `BAD_GOAL` |
| 310 | `InvalidEndTime` | campaign end time is invalid or outside the allowed horizon | `BAD_TIME` |
| 311 | `InvalidAssets` | asset code or asset descriptor is malformed | `BAD_AST` |
| 312 | `CannotCancelWithFunds` | campaign still holds raised funds and cannot be cancelled | `HOLD_FND` |
| 313 | `RefundWindowClosed` | the refund window for this campaign has closed | `RFND_SHUT` |
| 314 | `DisputeCooldownActive` | dispute cooldown has not elapsed since the last dispute | `DS_COOLD` |
| 315 | `DisputeAlreadyOpen` | a dispute is already open for this campaign | `DUP_DS` |
| 316 | `NoActiveDispute` | no dispute is open for this campaign | `NO_DS` |
| 317 | `WithdrawalWindowNotElapsed` | withdrawal notice window has not elapsed | `EARLY` |
| 318 | `NoPendingWithdrawal` | no pending withdrawal exists for this campaign | `NO_PEND` |
| 319 | `Overflow` | arithmetic operation would overflow or underflow | `OVERFL` |
| 320 | `Reentrant` | re-entrant call rejected | `REENTRY` |
| 321 | `ContractFrozen` | contract is frozen; all state-changing operations are blocked | `FROZEN` |

### Re-entrancy: two codes, one meaning

`Error::Reentrant` (320) is the campaign-level taxonomy entry. The panic itself
is raised by the shared guard and carries
`shared::guard::ReentrancyError::Reentrant` = **9**, which is the same value as
`EscrowError::Reentrant`. One numeric code for re-entrancy across the whole
system means an indexer does not have to switch on the emitting contract to
detect a re-entrancy rejection.

## Arithmetic failures

Overflow is never silent. Two mechanisms cover it:

* **Typed** — `contracts/campaign/src/math.rs` wraps every `checked_*` in
  `panic_with_error!(&env, Error::Overflow)`. Used for all `i128` money
  arithmetic, basis-point fee splits, and proportional (pro-rata) shares.
* **Contract-local** — other contracts report their own band:
  `EscrowError::ArithmeticOverflow` (12), `DisputeError::ArithmeticOverflow` (9).

`saturating_*` remains correct for counters and elapsed-ledger arithmetic, where
a saturating value is the fail-closed answer; it must not be used for balances.

## Usage

Error codes are returned as part of contract error responses. Each contract
maps these codes to specific ContractError enum variants.

## Insufficient input and payouts

Not every failure is a contract error. Two conditions are reported as panic
strings whose text is the **name** of the corresponding typed variant, so an
indexer sees the same identifier whether the contract has a `#[contracterror]`
enum yet or not:

| Panic string | Typed equivalent | Raised by |
|---|---|---|
| `InsufficientContractBalance` | `Error::InsufficientContractBalance` (308) | `campaign::balance::require_contract_balance`, `withdrawal::approve_withdrawal`, `donation::refund` |
| `InvalidAssets` | `Error::InvalidAssets` (311) | `campaign::sanitize::require_asset_code`, `require_reason_hash` |

Both are raised **before** the state change they protect, so a rejected call
leaves nothing behind. See
[the balance module (`balance.rs`)](../contracts/campaign/src/balance.rs) and
[the sanitization module (`sanitize.rs`)](../contracts/campaign/src/sanitize.rs).

## New codes

| Contract | Code | Name | Description |
|---|---|---|---|
| dispute arbiter | 10 | `NoteTooLong` | The resolution note exceeds `MAX_MEMO_LEN` (256 bytes). Appended — existing codes 1–9 are unchanged, per the permanence rule above. |

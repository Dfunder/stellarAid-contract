# Lumora Smart Contracts — Comprehensive API Documentation

> **closes #713** — Add Comprehensive API Documentation and Code Comments
>
> This document supplements the existing `docs/API_REFERENCE.md` with full
> inline-style documentation for every public contract entry point, parameter
> descriptions, return values, error conditions, and worked code examples.

---

## Table of Contents

1. [Overview](#overview)
2. [Architecture](#architecture)
3. [Installation & Setup](#installation--setup)
4. [Contract Reference](#contract-reference)
   - [escrow](#escrow-contract)
   - [commission\_agreement](#commission_agreement-contract)
   - [dispute\_arbiter](#dispute_arbiter-contract)
   - [platform\_config](#platform_config-contract)
   - [reputation](#reputation-contract)
   - [licensing](#licensing-contract)
   - [mentorship](#mentorship-contract)
5. [Type Definitions](#type-definitions)
6. [Error Codes](#error-codes)
7. [SDK Usage Examples](#sdk-usage-examples)
8. [Troubleshooting Guide](#troubleshooting-guide)
9. [Security Notes](#security-notes)

---

## Overview

Lumora contracts are **Soroban smart contracts** deployed on the Stellar
network.  All contracts follow a common set of conventions:

| Convention | Detail |
|---|---|
| Auth | Every state-changing function calls `require_auth()` on the relevant signer. |
| CEI | Checks → Effects → Interactions ordering is enforced to prevent re-entrancy. |
| Events | All state transitions emit a `publish` event for off-chain indexers. |
| Storage | Persistent storage keys use typed `DataKey` enums to avoid collisions. |
| TTL | Persistent entries have their TTL extended on every write. |
| Pause | Admin may `pause`/`unpause` each contract; paused contracts reject mutations. |
| Errors | Each contract exposes typed error enums with numeric codes and suggestions. |

---

## Architecture

```
contracts/
├── escrow/               # Core payment escrow
├── commission_agreement/ # Milestone-based commission workflow
├── dispute_arbiter/      # Neutral arbitration for disputes
├── platform_config/      # Fee tiers, token metadata, feature flags
├── reputation/           # On-chain ratings and reviews
├── licensing/            # Creative licensing marketplace (NEW — #616)
└── mentorship/           # Mentorship program contract  (NEW — #613)
```

See [docs/architecture.md](./architecture.md) and the individual ADRs under
[docs/ADRs/](./ADRs/) for design rationale.

---

## Installation & Setup

### Prerequisites

```bash
rustup target add wasm32-unknown-unknown
cargo install --locked stellar-cli
```

### Build all contracts

```bash
cargo build --target wasm32-unknown-unknown --release
```

### Run tests

```bash
cargo test --workspace
```

### Deploy to Testnet

```bash
./scripts/deploy_testnet.sh
```

See [docs/DEPLOY.md](./DEPLOY.md) for full deployment instructions.

---

## Contract Reference

---

### `escrow` Contract

**File:** `contracts/escrow/src/lib.rs`

The escrow contract holds funds on behalf of a client until a creator
completes the agreed work.  Supports single-asset escrows with optional
dispute handling, idempotency, and call-timeout policies.

#### `initialize(admin: Address)`

Initialises the contract and records the admin address.

| Parameter | Type | Description |
|---|---|---|
| `admin` | `Address` | The privileged admin address.  Must sign. |

**Errors:** Panics if already initialised.

---

#### `create_escrow(admin, token, amount, recipient, deadline_ledger)`

Creates a new escrow record.

| Parameter | Type | Description |
|---|---|---|
| `admin` | `Address` | Must match the stored admin and sign. |
| `token` | `Address` | SAX / USDC / XLM token contract address. |
| `amount` | `i128` | Amount in base units (stroops for XLM, μUSDC for USDC). |
| `recipient` | `Address` | Creator address that will receive funds on release. |
| `deadline_ledger` | `u32` | Ledger at which the escrow auto-expires (0 = no deadline). |

**Returns:** Nothing.  Emits `escrow_created` event.

**Errors:**
- `InvalidEscrow` — amount ≤ 0 or recipient == admin.
- `AlreadyActive` — an escrow with the same id already exists.

---

#### `release_payment(admin, escrow_id)`

Releases the held funds to the recipient.

| Parameter | Type | Description |
|---|---|---|
| `admin` | `Address` | Must sign. |
| `escrow_id` | `Bytes` | Unique escrow identifier. |

**Errors:** `NotActive`, `DisputeOpen`, `InvalidEscrow`.

---

#### `refund_client(admin, escrow_id)`

Returns funds to the client (cancels the escrow in favour of the payer).

**Errors:** `NotActive`, `DisputeOpen`.

---

#### `open_dispute(admin, escrow_id)`

Flags the escrow as disputed, blocking release and refund until resolved.

**Errors:** `NotActive`, `Expired`.

---

#### `get_escrow(escrow_id) → Option<EscrowRecord>`

Read-only query — returns the full escrow record or `None`.

---

### `commission_agreement` Contract

**File:** `contracts/commission_agreement/src/lib.rs`

Manages the full lifecycle of a creative commission: creation → acceptance →
milestone progression → payment release.

#### `create_agreement(client, creator, token, total_amount, deadline)`

Creates a new commission agreement in `Draft` status.

| Parameter | Type | Description |
|---|---|---|
| `client` | `Address` | Paying party. Must sign. |
| `creator` | `Address` | Commissioned artist. |
| `token` | `Address` | Payment token contract. |
| `total_amount` | `i128` | Total price in base units. |
| `deadline` | `u32` | Ledger deadline (0 = open-ended). |

**Returns:** `commission_id: Bytes`.  Emits `agreement_created`.

**Errors:** `InvalidAmount`, `SameParty`.

---

#### `accept_agreement(creator, commission_id)`

Creator accepts the commission; status transitions to `Active`.

**Errors:** `NotFound`, `NotDraft`, `OnlyCreator`.

---

#### `propose_milestone(creator, commission_id, title, amount)`

Creator proposes a new milestone.  Total milestone amounts may not exceed the
agreement total.

| Parameter | Type | Description |
|---|---|---|
| `title` | `String` | Human-readable milestone description. |
| `amount` | `i128` | Payment due on completion of this milestone. |

**Errors:** `NotActive`, `BudgetExceeded`.

---

#### `approve_milestone(client, commission_id, milestone_id)`

Client approves a submitted milestone.  Triggers partial payment release via
the escrow contract.

**Errors:** `NotActive`, `MilestoneNotSubmitted`, `OnlyClient`.

---

#### `complete_agreement(client, commission_id)`

Marks the commission as fully complete and releases any remaining funds.

---

### `dispute_arbiter` Contract

**File:** `contracts/dispute_arbiter/src/lib.rs`

Neutral arbitration for Lumora disputes.  An arbiter (appointed by admin) may
rule in favour of either party.

#### `register_arbiter(admin, arbiter)`

Registers a trusted arbiter address.

#### `raise_dispute(complainant, escrow_id, description) → dispute_id`

| Parameter | Type | Description |
|---|---|---|
| `complainant` | `Address` | Must sign; must be a party to the escrow. |
| `description` | `String` | Summary of the alleged issue. |

**Returns:** `dispute_id: u64`.

#### `rule(arbiter, dispute_id, favour_creator: bool)`

Arbiter casts a ruling.  Triggers escrow release or refund accordingly.

**Errors:** `NotArbiter`, `DisputeAlreadyResolved`.

---

### `platform_config` Contract

**File:** `contracts/platform_config/src/lib.rs`

Stores global platform settings: fee tiers, token metadata, feature flags, and
canary deployment state.

#### `set_fee_bps(admin, bps)`

Sets the platform fee in basis points (100 bps = 1 %).

**Valid range:** 0–1000 (0–10 %).

#### `compute_fees(amount, volume) → FeeBreakdown`

Returns `{ platform_fee, creator_amount }` based on the current tier and any
active promotion.

| Field | Type | Description |
|---|---|---|
| `platform_fee` | `i128` | Amount sent to the platform wallet. |
| `creator_amount` | `i128` | Net amount the creator receives. |

#### `set_feature_flag(admin, name, enabled)`

Toggles a named feature flag.  Contracts can gate behaviour on
`is_feature_enabled(name)`.

---

### `reputation` Contract

**File:** `contracts/reputation/src/lib.rs`

Stores on-chain ratings and reviews for creators and clients.

#### `submit_review(reviewer, subject, rating, comment)`

| Parameter | Type | Description |
|---|---|---|
| `reviewer` | `Address` | Must sign; must have completed a commission with `subject`. |
| `subject` | `Address` | Address being reviewed. |
| `rating` | `u32` | Integer 1–5. |
| `comment` | `String` | Free-text feedback (max 500 chars). |

**Errors:** `NoCompletedCommission`, `AlreadyReviewed`, `InvalidRating`.

#### `get_average_rating(subject) → u32`

Returns the running average rating (scaled ×100 to preserve two decimal
places, i.e. 450 = 4.50 stars).

---

### `licensing` Contract

**File:** `contracts/licensing/src/lib.rs`

Creative licensing marketplace supporting primary licenses, sub-licensing,
usage tracking, and dispute resolution.  **New contract — closes #616.**

#### `initialize(admin)`

Sets the admin and initialises counters.

#### `create_license(owner, licensee, asset_id, license_type, price, sub_licensable, expires_at) → license_id`

Issues a new primary license for a digital asset.

| Parameter | Type | Description |
|---|---|---|
| `owner` | `Address` | Rights holder. Must sign. |
| `licensee` | `Address` | Party receiving usage rights. |
| `asset_id` | `String` | Unique identifier for the digital work. |
| `license_type` | `LicenseType` | `Personal` / `Commercial` / `Exclusive`. |
| `price` | `i128` | Agreed price (informational; payment handled externally). |
| `sub_licensable` | `bool` | Whether the licensee may issue sub-licenses. |
| `expires_at` | `u32` | Expiry ledger; 0 = perpetual. |

**Returns:** `license_id: u64`.  Emits `licensed` event.

#### `create_sub_license(parent_id, sub_licensee, price, expires_at) → license_id`

Issues a sub-license derived from `parent_id`.  The caller must be the parent
licensee.

**Errors:** Panics if parent is not active, not sub-licensable, or caller is
not the parent licensee.

#### `revoke_license(caller, license_id)`

Revokes a license.  Only the original owner may revoke.

#### `record_usage(user, license_id, context)`

Records a usage event for audit purposes.  Emits `usage` event.

#### `get_usage_entries(license_id, offset, limit) → Vec<UsageEntry>`

Paginated read of usage entries.

#### `raise_dispute(complainant, license_id, description) → dispute_id`

Flags the license as disputed and creates a `LicenseDispute` record.

#### `resolve_dispute(admin, dispute_id, outcome)`

Admin-only.  Sets the dispute outcome and restores or revokes the license.

| `outcome` | Effect |
|---|---|
| `UpheldForLicensor` | License revoked |
| `UpheldForLicensee` | License restored to Active |
| `Settled` | License restored to Active |

#### `get_license(license_id) → LicenseRecord`

Read-only query.

---

### `mentorship` Contract

**File:** `contracts/mentorship/src/lib.rs`

Mentorship program connecting experienced artists with emerging talent.
**New contract — closes #613.**

#### `initialize(admin)`

Sets the admin and initialises counters.

#### `propose_engagement(mentor, mentee, description, milestones, compensation, token) → engagement_id`

Mentor proposes a mentorship engagement.

| Parameter | Type | Description |
|---|---|---|
| `mentor` | `Address` | Experienced professional. Must sign. |
| `mentee` | `Address` | Emerging artist. |
| `description` | `String` | Program overview. |
| `milestones` | `Vec<(String, String)>` | List of `(title, description)` pairs. |
| `compensation` | `i128` | Total payment to mentor in base token units. |
| `token` | `Address` | Token contract for compensation. |

**Returns:** `engagement_id: u64`.  Emits `proposed` event.

#### `accept_engagement(mentee, engagement_id)`

Mentee accepts the proposal; engagement becomes `Active`.

#### `cancel_engagement(caller, engagement_id)`

Either party may cancel a `Proposed` or `Active` engagement.

#### `submit_milestone(mentee, engagement_id, milestone_index)`

Mentee marks a milestone as submitted for review.

#### `review_milestone(mentor, engagement_id, milestone_index, approve: bool)`

Mentor approves or rejects a submitted milestone.  When the final milestone is
approved the engagement automatically completes and compensation is transferred
to the mentor.

#### `submit_feedback(author, engagement_id, rating, comment)`

Either party submits a rating (1–5) after the engagement ends.

#### `issue_certificate(caller, engagement_id)`

Issues a completion certificate (emits `certified` event for off-chain NFT
minting or credential recording).  May only be called once per engagement.

#### `get_engagement(engagement_id) → MentoringEngagement`

Read-only query.

#### `get_milestones(engagement_id) → Vec<MentoringMilestone>`

Returns all milestones with their current status.

#### `get_feedback(engagement_id) → Vec<FeedbackEntry>`

Returns all feedback submitted for the engagement.

---

## Type Definitions

### `LicenseType`
```rust
pub enum LicenseType {
    Personal,    // Non-commercial personal use
    Commercial,  // Commercial use by licensee
    Exclusive,   // Exclusive commercial rights
    SubLicense,  // Derived sub-license
}
```

### `LicenseStatus`
```rust
pub enum LicenseStatus {
    Active,
    Revoked,
    Expired,
    Disputed,
}
```

### `EngagementStatus`
```rust
pub enum EngagementStatus {
    Proposed,   // Awaiting mentee acceptance
    Active,     // Both parties confirmed
    Completed,  // All milestones approved
    Cancelled,
}
```

### `MilestoneStatus`
```rust
pub enum MilestoneStatus {
    Pending,
    Submitted,
    Approved,
    Rejected,
}
```

### `FeeBreakdown`
```rust
pub struct FeeBreakdown {
    pub platform_fee: i128,
    pub creator_amount: i128,
}
```

---

## Error Codes

| Code | Name | Contract | Description | Suggested Action |
|---|---|---|---|---|
| 1 | `NotFound` | escrow, commission_agreement | Record does not exist | Verify the id is correct |
| 2 | `NotActive` | escrow | Escrow is not in Active state | Check escrow status |
| 3 | `OnlyRecipient` | escrow | Caller is not the recipient | Use the correct wallet |
| 4 | `NoAuth` | all | Missing signature | Ensure the correct address signs |
| 5 | `DisputeOpen` | escrow | A dispute is open | Resolve dispute first |
| 6 | `InvalidTier` | platform_config | Fee tier parameters invalid | Min volume > 0, bps ≤ 10000 |
| 7 | `InvalidPromotion` | platform_config | Promotion dates invalid | Start < end ledger |
| 8 | `InvalidReferralBps` | platform_config | Referral bps > 10000 | Use a value ≤ 10000 |
| 9 | `PromotionNotActive` | platform_config | No active promotion | Check `is_promotion_active()` |
| 10 | `AlreadyReviewed` | reputation | Reviewer already submitted | One review per commission |
| 11 | `InvalidRating` | reputation | Rating outside 1–5 | Pass a value between 1 and 5 |

Full error code documentation: [docs/error_codes.md](./error_codes.md).

---

## SDK Usage Examples

### Create and release an escrow (TypeScript SDK)

```typescript
import { EscrowClient } from '@lumora/sdk';
import { Keypair } from '@stellar/stellar-sdk';

const admin = Keypair.fromSecret(process.env.ADMIN_SECRET!);
const client = new EscrowClient({ rpcUrl: 'https://soroban-testnet.stellar.org' });

// Create escrow
await client.create_escrow({
  admin: admin.publicKey(),
  token: USDC_CONTRACT,
  amount: BigInt(1000_000), // 1 USDC
  recipient: CREATOR_ADDRESS,
  deadline_ledger: 0,
});

// Release after work is complete
await client.release_payment({
  admin: admin.publicKey(),
  escrow_id: escrowId,
});
```

### Issue a creative license (TypeScript SDK)

```typescript
import { LicensingClient } from '@lumora/sdk';

const licensing = new LicensingClient({ rpcUrl });

const licenseId = await licensing.create_license({
  owner: artistAddress,
  licensee: clientAddress,
  asset_id: 'artwork-uuid-12345',
  license_type: { tag: 'Commercial' },
  price: BigInt(500_000), // in base units
  sub_licensable: false,
  expires_at: 0, // perpetual
});
```

### Run a mentorship program (TypeScript SDK)

```typescript
import { MentorshipClient } from '@lumora/sdk';

const mentorship = new MentorshipClient({ rpcUrl });

// Mentor proposes engagement
const engagementId = await mentorship.propose_engagement({
  mentor: mentorAddress,
  mentee: menteeAddress,
  description: '3-month UI/UX design mentorship',
  milestones: [
    ['Week 4 — Portfolio review', 'Mentor reviews mentee portfolio and gives feedback'],
    ['Week 8 — First project', 'Mentee completes assigned design project'],
    ['Week 12 — Final presentation', 'Mentee presents a client-ready design system'],
  ],
  compensation: BigInt(150_000),
  token: USDC_CONTRACT,
});

// Mentee accepts
await mentorship.accept_engagement({ mentee: menteeAddress, engagement_id: engagementId });
```

---

## Troubleshooting Guide

### Transaction fails with `NoAuth`

The address that needs to sign the transaction is different from the one
submitting it.  Ensure the correct keypair is used to sign the transaction
envelope.

### `NotFound` when querying an escrow or license

The id passed does not correspond to any record.  Common causes:
- The creation transaction was not confirmed yet (wait for ledger close).
- A different network (testnet vs mainnet) is being queried.
- The id was logged incorrectly.

### `DisputeOpen` blocking release

A dispute must be resolved by the arbiter before the escrow can be released
or refunded.  Call `dispute_arbiter.rule()` with the arbiter keypair.

### Milestone stuck in `Submitted`

The mentor has not yet called `review_milestone`.  Check that the correct
mentor address is being used.  If the mentor is unavailable, the admin may
intervene via the platform config contract.

### `InvalidRating` when submitting feedback

The `rating` parameter must be an integer between 1 and 5 inclusive.

### Contract paused

If a contract is in the paused state, all mutating transactions will fail.
Only the admin may call `unpause`.  See [docs/PAUSE_AND_EMERGENCY.md](./PAUSE_AND_EMERGENCY.md).

---

## Security Notes

- All admin functions call `require_auth()` — never hardcode private keys.
- Fee configuration is validated on-chain; values outside allowed ranges panic.
- The CEI pattern is enforced across all escrow and compensation flows.
- Usage and certification events are immutable once emitted; build audit
  pipelines on top of event streams.
- For the full threat model and audit checklist see
  [docs/SECURITY_REVIEW_CHECKLIST.md](./SECURITY_REVIEW_CHECKLIST.md).

# Contract Specification

This document describes all Soroban smart contracts in the StellarAid workspace.

## Architecture

```
                 ┌──────────────────┐
                 │  CampaignContract │
                 │  - create_campaign│
                 │  - get_campaign   │
                 │  - update_raised  │
                 │  - status mgmt    │
                 └────────┬─────────┘
                          │ cross-contract call (update_raised)
                          │
                 ┌────────▼─────────┐
                 │  DonationContract │
                 │  - donate         │
                 │  - refund         │
                 │  - get_total_raised│
                 └────────┬─────────┘
                          │ cross-contract call (get_total_raised)
                          │
                 ┌────────▼──────────┐
                 │ WithdrawalContract │
                 │  - request_withdrawal│
                 │  - approve_withdrawal│
                 │  - reject_withdrawal │
                 └───────────────────┘
```

All three contracts share a common `pause` mechanism that can halt state-changing operations in emergencies.

---

## Campaign Contract

**Purpose:** Manages fundraising campaign lifecycle.

### Storage Layout

| Key              | Type         | Description                        |
|------------------|--------------|------------------------------------|
| `Admin`          | Address      | Contract admin                     |
| `Initialized`    | bool         | Initialization flag                |
| `Campaign(u64)`  | Campaign     | Campaign data by ID                |
| `CampaignCount`  | u64          | Next campaign ID counter           |

### Functions

#### `initialize(admin: Address)`
Initializes the contract. Sets the admin and campaign counter to 0.

#### `create_campaign(owner: Address, goal: i128, deadline: u64) -> u64`
Creates a new campaign with `Active` status. Returns the campaign ID. Emits `campaign_registered`.

#### `get_campaign(campaign_id: u64) -> Option<Campaign>`
Returns campaign details or `None` if not found.

#### `update_campaign_status(admin: Address, campaign_id: u64, new_status: CampaignStatus)`
Changes a campaign's status. Emits `campaign_status_changed` with old and new status.

#### `update_raised(campaign_id: u64, amount: i128)`
Increments the `raised` field. Called by the Donation contract via cross-contract call.

#### `approve_campaign(admin: Address, campaign_id: u64)`
Sets campaign status to `Active`.

#### `reject_campaign(admin: Address, campaign_id: u64, reason: String)`
Sets campaign status to `Rejected`.

#### `suspend_campaign(admin: Address, campaign_id: u64)`
Sets campaign status to `Suspended`.

#### `get_campaign_count() -> u64`
Returns total number of campaigns created.

#### `transfer_admin(current_admin: Address, new_admin: Address)`
Transfers admin role to a new address.

#### `upgrade(admin: Address, new_wasm_hash: BytesN<32>)`
Upgrades contract WASM.

---

## Donation Contract

**Purpose:** Handles donation recording and cross-contract updates to campaigns.

### Storage Layout

| Key                      | Type          | Description                     |
|--------------------------|---------------|---------------------------------|
| `Admin`                  | Address       | Contract admin                  |
| `Initialized`            | bool          | Initialization flag             |
| `CampaignContract`       | Address       | Campaign contract address       |
| `CampaignDonations(u64)` | Vec\<Donation> | Donations per campaign          |
| `DonationHistory(Address)` | Vec\<Donation> | Donations per donor             |
| `CampaignRaised(u64)`    | i128          | Total raised per campaign       |

### Functions

#### `initialize(admin: Address, campaign_contract: Address)`
Initializes the contract with an admin and the campaign contract address.

#### `donate(donor: Address, campaign_id: u64, amount: i128)`
Records a donation, updates the local raised total, and calls `campaign.update_raised()` via cross-contract call. Emits `donation_made`. Reverts if the campaign is not active.

#### `refund(caller: Address, campaign_id: u64, donor: Address, amount: i128)`
Reduces the raised total. Only callable by admin or campaign owner. Emits `refund_recorded`.

#### `get_donations_for_campaign(campaign_id: u64) -> Vec<Donation>`
Returns all donations for a campaign.

#### `get_total_raised(campaign_id: u64) -> i128`
Returns the locally-tracked raised amount for a campaign.

#### `get_donor_history(donor: Address) -> Vec<Donation>`
Returns all donations made by a specific donor.

#### `upgrade(admin: Address, new_wasm_hash: BytesN<32>)`
Upgrades contract WASM.

---

## Withdrawal Contract

**Purpose:** Manages withdrawal requests against campaign funds.

### Storage Layout

| Key                       | Type             | Description                      |
|---------------------------|------------------|----------------------------------|
| `Admin`                   | Address          | Contract admin                   |
| `Initialized`             | bool             | Initialization flag              |
| `DonationContract`        | Address          | Donation contract address        |
| `Withdrawal(u64)`         | Withdrawal       | Withdrawal request by ID         |
| `WithdrawalsByCampaign(u64)` | Vec\<Withdrawal> | Withdrawals per campaign         |
| `WithdrawnAmount(u64)`    | i128             | Total withdrawn per campaign     |

### Functions

#### `initialize(admin: Address, donation_contract: Address)`
Initializes the contract with an admin and the donation contract address.

#### `request_withdrawal(campaign_id: u64, owner: Address, amount: i128, recipient: Address) -> u64`
Creates a pending withdrawal request. Emits `withdrawal_requested`.

#### `approve_withdrawal(withdrawal_id: u64, admin: Address)`
Approves a withdrawal after checking available balance (total raised minus already withdrawn). Emits `withdrawal_approved`.

#### `reject_withdrawal(withdrawal_id: u64, admin: Address, reason: String)`
Rejects a withdrawal request. Emits `withdrawal_rejected`.

#### `get_withdrawal(withdrawal_id: u64) -> Option<Withdrawal>`
Returns a withdrawal request by ID.

#### `get_withdrawals_by_campaign(campaign_id: u64) -> Vec<Withdrawal>`
Returns all withdrawal requests for a campaign.

#### `get_withdrawn_amount(campaign_id: u64) -> i128`
Returns total amount already withdrawn from a campaign.

#### `upgrade(admin: Address, new_wasm_hash: BytesN<32>)`
Upgrades contract WASM.

---

## Shared Types

All defined in `contracts/shared/src/types.rs`.

### Campaign

| Field    | Type            | Description          |
|----------|-----------------|----------------------|
| id       | u64             | Unique ID            |
| owner    | Address         | Campaign creator     |
| goal     | i128            | Fundraising target   |
| raised   | i128            | Amount raised        |
| status   | CampaignStatus  | Current status       |
| deadline | u64             | Expiration timestamp |

### Donation

| Field       | Type    | Description      |
|-------------|---------|------------------|
| donor       | Address | Donor address    |
| campaign_id | u64     | Target campaign  |
| amount      | i128    | Donation amount  |
| timestamp   | u64     | Donation time    |

### Withdrawal

| Field       | Type    | Description        |
|-------------|---------|--------------------|
| campaign_id | u64     | Target campaign    |
| recipient   | Address | Funds recipient    |
| amount      | i128    | Withdrawal amount  |
| approved    | bool    | Approval status    |

### CampaignStatus

| Enum       | Value |
|------------|-------|
| Active     | 0     |
| Completed  | 1     |
| Suspended  | 2     |
| Rejected   | 3     |

## Pause Mechanism

All contracts share the pause mechanism from `contracts/shared/src/pause.rs`. When paused, state-changing functions (donate, create_campaign, request_withdrawal, etc.) will panic with "contract is paused". Pause/unpause events are emitted for off-chain indexing.

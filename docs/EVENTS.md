# Contract Events

All Soroban contracts emit structured events for off-chain indexing.

## Campaign Contract

### `campaign_registered`

Emitted when a new campaign is created.

| Field        | Type      | Description                     |
|-------------|-----------|---------------------------------|
| campaign_id | u64       | Unique campaign identifier      |
| owner       | Address   | Campaign creator address        |
| goal        | i128      | Fundraising target amount       |
| deadline    | u64       | Campaign expiration timestamp   |

### `campaign_status_changed`

Emitted when a campaign's status is updated.

| Field      | Type            | Description               |
|-----------|-----------------|---------------------------|
| campaign_id | u64           | Campaign identifier       |
| old_status | CampaignStatus  | Previous status           |
| new_status | CampaignStatus  | New status                |

### `contract_paused`

Emitted when the contract is paused.

| Field | Type    | Description          |
|-------|---------|----------------------|
| admin | Address | Pausing admin address |

### `contract_unpaused`

Emitted when the contract is unpaused.

| Field | Type    | Description            |
|-------|---------|------------------------|
| admin | Address | Unpausing admin address |

## Donation Contract

### `donation_made`

Emitted when a donation is made.

| Field       | Type    | Description            |
|-------------|---------|------------------------|
| donor       | Address | Donor address          |
| campaign_id | u64     | Target campaign        |
| amount      | i128    | Donation amount        |

### `refund_recorded`

Emitted when a refund is processed.

| Field       | Type    | Description              |
|-------------|---------|--------------------------|
| campaign_id | u64     | Campaign identifier      |
| donor       | Address | Original donor address   |
| amount      | i128    | Refund amount            |
| caller      | Address | Address authorizing refund |

## Withdrawal Contract

### `withdrawal_requested`

Emitted when a withdrawal is requested.

| Field         | Type    | Description              |
|---------------|---------|--------------------------|
| withdrawal_id | u64     | Unique withdrawal ID     |
| campaign_id   | u64     | Campaign identifier      |
| recipient     | Address | Funds recipient address  |
| amount        | i128    | Withdrawal amount        |

### `withdrawal_approved`

Emitted when a withdrawal is approved.

| Field         | Type      | Description                        |
|---------------|-----------|------------------------------------|
| withdrawal_id | u64       | Unique withdrawal ID               |
| tx_hash       | BytesN<32> | Transaction hash (0 if unavailable) |

### `withdrawal_rejected`

Emitted when a withdrawal is rejected.

| Field         | Type    | Description              |
|---------------|---------|--------------------------|
| withdrawal_id | u64     | Unique withdrawal ID     |
| reason        | String  | Rejection reason         |

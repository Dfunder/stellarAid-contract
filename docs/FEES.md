# Fee Calculation (closes #690)

How `platform_config` computes fees across tiers, promotions, volume discounts,
and referral sharing.

## Overview

Fees are resolved in two steps:

1. `resolve_effective_fee_bps` — pick the single **effective fee rate** (in
   basis points) that applies to a caller, based on volume tier + promotion
   override, clamped to the contract's configured min/max.
2. `compute_fees` — split the gross amount into fee, payout, referral fee, and
   platform fee according to the effective rate and the optional referral
   config.

Config knobs and how they combine are described below. All functions live in
`contracts/platform_config/src/fees.rs` and are pure — no storage access — so
any caller (escrow, subscriptions, etc.) can derive the exact same numbers.

## Fee rate: tiers vs promotions

| Source | Key | Role |
|---|---|---|
| Tier table | `FeeTiers` (instance) | Volume-based baseline rate: `FeeTier { min_volume, fee_bps }` |
| Promotional override | `Promotion` (instance) | Fixed rate for a ledger window: `Promotion { start_ledger, end_ledger, fee_bps }` |
| Global clamps | `MIN_FEE_BPS` / `MAX_FEE_BPS` (instance) | Bounds applied to the final effective rate |

### Resolution order

```
tier_rate   = tier_fee(volume)                      // highest tier with min_volume <= volume
promo_rate  = promotion active ? promotion.fee_bps  : None
candidate   = promo_rate.unwrap_or(tier_rate)
effective   = clamp(candidate, min_fee_bps, max_fee_bps)
```

- If no tier exists (`FeeTiers` empty), the app-level minimum is used.
- A promotion **only** raises/lowers the rate while
  `start_ledger <= current <= end_ledger`; outside the window the tier rate
  applies again. Promotions never make a rate escape the global clamps.
- Invalid configuration is rejected at write time: duplicate/unordered tiers
  (`InvalidTier`), malformed promotion windows (`InvalidPromotion`), referral
  bps out of range (`InvalidReferralBps`). The contract refuses to run fees
  under an unclamped effective rate.

## Fee split

`compute_fees(env, amount, volume)` returns a `FeeBreakdown`:

| Field | Meaning |
|---|---|
| `effective_fee_bps` | The rate actually applied |
| `amount` | The gross amount passed in (echo) |
| `fee` | `amount * effective_fee_bps / 10_000` |
| `payout` | `amount - fee` (what the beneficiary receives) |
| `referral_fee` | Share of the fee given to the referrer |
| `platform_fee` | Share of the fee kept by the platform |

`referral_fee` and `platform_fee` are derived from the current `ReferralConfig`
(`bps`, capped at 10_000). When no referral config exists, the whole fee is
platform fee:

```
referral_fee = referral_config ? fee * referral_config.bps / 10_000 : 0
platform_fee = fee - referral_fee
```

Arithmetic is checked: an overflow yields `Err(FeeComputationError::ArithmeticOverflow)`.

## Admin-gated entry points

All configuration writes are admin-only (`require_auth` against the stored
admin; `PauseKey::Admin`):

- `upsert_fee_tier / remove_fee_tier / get_fee_tiers`
- `set_promotion / clear_promotion`
- `set_referral_config / get_referral_config`
- `record_volume / get_volume` — volume drives the tier lookup and trails
  `VOLUME_TTL_LEDGERS`

## Invariants (covered by tests)

- Effective rate is never below `min_fee_bps` or above `max_fee_bps`, even with
  an aggressive promotion.
- Empty tier table falls back to the app minimum; promotion outside its window
  is ignored.
- `fee + payout == amount` and `referral_fee + platform_fee == fee` always hold.
- Writing invalid tiers/promotions/referral config is rejected.

## Example

```text
volume  = 5_000_000        -> tier rate 200 bps (2.00%)
promotion active at 50 bps -> effective 50 bps (clamped >= MIN_FEE_BPS)
amount  = 10_000
fee     = 10_000 * 50 / 10_000 = 50
payout  = 9_950
referral config 250 bps     -> referral_fee = 50 * 250 / 10_000 = 1 (rounds down)
platform_fee     = 49
```

See `docs/` sibling guides (`docs/CONTRACTS.md`) for how calling contracts wire
`resolve_effective_fee_bps` / `compute_fees` into their flows.
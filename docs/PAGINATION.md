# Pagination

Bounded reads for contract list entry points. Closes #876.

The one place a page size is decided in this workspace is
`contracts/shared/src/pagination.rs`. Contracts do not roll their own
`min(limit, N)` arithmetic; they call a shared helper so that every paginated
read in the platform has the same bound, the same edge-case behaviour, and the
same metadata.

## Why

Several contracts expose a list that grows for the life of the deployment — a
campaign registry, an agency roster, a per-entity history log. Returning one of
those whole is unbounded by construction, and a read entry point's cost has to
be predictable *before* it runs.

Worse, several existing readers took a caller-supplied `limit` and used it
verbatim. `get_history(offset, limit)` in `dao` computed `end = min(offset +
limit, count)`, so a caller passing `limit = u64::MAX` asked for the entire log
and got it. A caller who controls `limit` controls the cost of the read.

## The helper

```rust
pub const MAX_LIMIT: u32 = 100;    // never return more than this, whatever `limit` says
pub const DEFAULT_LIMIT: u32 = 50; // substituted when a caller passes `limit == 0`

pub struct PageInfo {
    pub total: u32,          // items behind this page, not just those returned
    pub start: u32,          // cursor this page began at, after clamping
    pub limit: u32,          // index positions this page spans, after clamping
    pub count: u32,          // items actually present; < limit if indices were pruned
    pub has_more: bool,      // anything past the end of this page?
    pub next_start: Option<u32>, // cursor for the next call, None on the last page
}
```

Two shapes, because the workspace has two shapes of list:

| List shape | Storage | Helper |
|------------|---------|--------|
| Materialised | one entry holding a whole `Vec<T>` | `paginate` / `paginated` |
| Indexed | a count key plus one key per index | `collect_window` |

A contract entry point returns `(Vec<SomeRecord>, PageInfo)`. `PageInfo` is
deliberately not generic: `soroban-sdk`'s contract-spec lowering rejects generic
type parameters, so a `Page<T>` could not appear in a `#[contractimpl]`
signature. Keeping the item type out of `PageInfo` is what makes the tuple
spec-compatible.

### Edge cases

| Input | Result |
|-------|--------|
| `start > total` | empty page, `has_more == false`, `next_start == None` |
| `limit == 0` | `DEFAULT_LIMIT` items — see below |
| `limit > MAX_LIMIT` | clamped to `MAX_LIMIT` |
| `start == total` | empty page, not a trap |
| `start = u32::MAX` | saturates and clamps to `total`; no overflow |
| empty list | empty page, `has_more == false` |

Nothing in the module can trap, and nothing can return more than `MAX_LIMIT`
items, so it is safe to call with caller-controlled arguments from a view entry
point.

### Why `limit == 0` is not an empty page

`analytics::get_earnings` and `messaging::get_messages` treat a zero limit as an
empty page. This module does not, and the difference is deliberate. There, the
`limit` is the whole contract of the entry point. Here, the limit is applied
*after* the list has already been read out of storage, so answering a zero with
an empty vector would pay the full read cost and then throw the result away,
while looking to the caller exactly like a legitimately empty page. Rejecting it
instead would force every contract to map a shared error into its own error
enum just to accommodate a helper. `DEFAULT_LIMIT` is the boring answer that
keeps the cost bounded and the call useful.

## Endpoints

### New `*_page` entry points

These are additive. Nothing that existed before was removed or renamed.

| Contract | Entry point | List |
|----------|-------------|------|
| `campaign-factory` | `get_all_campaigns_page` | campaign registry |
| `campaign-factory` | `get_campaigns_by_creator_page` | campaigns by admin |
| `commission_agreement` | `get_milestones_page` | agreement milestones |
| `commission_agreement` | `get_team_members_page` | commission team |
| `commission_agreement` | `get_roster_page` | agency roster |
| `dao` | `get_history_page` | governance history |
| `ecosystem_funding` | `get_milestones_page` | program milestones |
| `ecosystem_funding` | `get_outcomes_page` | outcome log |
| `licensing` | `get_usage_entries_page` | usage entries |
| `mentorship` | `get_milestones_page` | engagement milestones |
| `mentorship` | `get_feedback_page` | engagement feedback |
| `nft` | `get_transfer_history_page` | transfer history |
| `recruitment` | `get_applicants_page` | job applicants |

### Existing readers whose behaviour changed

These readers already took an offset and a limit. Their **signatures are
unchanged**, but the `limit` they honour is now capped, so a page never spans
more than `MAX_LIMIT` items:

- `dao::get_history`
- `nft::get_transfer_history`
- `licensing::get_usage_entries`
- `ecosystem_funding::get_outcomes`

`mentorship::get_feedback` also changed: it returned a whole `Vec` and now
returns at most `MAX_LIMIT` entries. It is a whole-vector read with no paging
parameters, so the only way to bound it is to truncate. Callers that need every
entry should use `get_feedback_page` and loop.

The whole-vector readers below were **left unbounded**, because they take no
paging parameters and truncating them would silently lose data for any existing
caller. They gained a `*_page` sibling instead: `campaign-factory`'s
`get_all_campaigns` and `get_campaigns_by_creator`, `commission_agreement`'s
`get_milestones`, `get_team_members` and `get_roster`, and
`ecosystem_funding`/`mentorship`'s `get_milestones`. Each is documented in
place as unbounded with a pointer to its bounded sibling.

## Client usage

```rust
let mut start = 0u32;
loop {
    let (page, info) = client.get_history_page(&start, &50);
    for entry in page.iter() {
        // ...
    }
    match info.next_start {
        Some(next) => start = next,
        None => break,
    }
}
```

Do not compute the next cursor yourself. `next_start` is already clamped, so
`start + limit` can disagree with it on the final page and can overflow for
hostile inputs. `has_more` and `next_start` always agree: `has_more == false`
implies `next_start == None`.

## Tests

`contracts/shared/src/pagination.rs` carries 30 unit tests covering the
arithmetic directly: the limit clamp in both directions, half-open windows, a
cursor past the end, `u32::MAX` on both ends, empty lists, missing indexed
entries producing a short page, the effective limit reported rather than the
requested one, non-primitive item types, a loader called at most once per
position, and a full walk that must cover every item exactly once with no gaps
or repeats.

`contracts/ecosystem_funding` and `contracts/campaign_factory` carry
contract-level tests that drive real entry points through a client: multi-page
walks, short final pages, `limit == 0`, an oversized limit, a cursor past the
end, a filtered total that counts matches rather than the underlying list, and a
creator with no matches reading as empty rather than erroring.

## Honest status

- **The `shared` pagination unit tests were executed and all 30 pass.** So were
  the 6 in `ecosystem_funding`, the 3 in `campaign_factory`, and the 15
  pre-existing `recruitment` tests. `nft`, `licensing` and `mentorship` compile
  and have no test module.
- **`campaign-factory` and `campaign_instance` were built to
  `wasm32-unknown-unknown --release`**, which is the strongest check available
  here: it proves the `*_page` return types survive contract-spec lowering,
  which is the thing most likely to be wrong about a new endpoint.
- **The `dao`, `commission_agreement` and `campaign-factory` test suites were
  never executed**, because those crates do not compile on `main` for reasons
  unrelated to this issue. Verified by building each at `HEAD` with the branch
  changes stashed and getting byte-identical errors:
  - `dao` — `no method named total_supply found for struct Client`.
  - `commission_agreement` — `types.rs` and the `include!`d `semver_types.rs`
    both define `AgreementRecord`/`AgreementStatus`/`DataKey`, and
    `symbol_short!("check_rate_limit")` exceeds the 9-character limit.
  - `campaign-factory` — depends on `contracts/campaign`, which has an unclosed
    `env.events().publish(` in `finalize_withdrawal` plus missing `require!`
    arms. Note this is a *different* crate from the `campaign_factory` package
    built above, which lives in `contracts/factory`.
  Consequently the `commission_agreement` `*_page` endpoints in this change are
  unverified by a compiler. They follow the same pattern as the
  `ecosystem_funding` and `campaign_factory` endpoints that did compile.
- **No performance numbers were measured.** The bound on read cost is structural
  (`collect_window` visits at most `MAX_LIMIT` indices, which is unit-tested via
  a counting loader), not benchmarked.
- For a materialised list the bound limits the *response*, not the underlying
  read: the whole `Vec` is pulled out of storage before it is sliced. Bounding
  the read itself would need a different storage layout. See
  [QUERY_OPTIMIZATION.md](./QUERY_OPTIMIZATION.md) §4.

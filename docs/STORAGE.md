# Storage and TTL Management in stellarAid Contracts

This document outlines the storage strategy and Time-to-Live (TTL) management for the stellarAid Soroban contracts.

> - **Retention and pruning policy:** [RETENTION_AND_PRUNING.md](./RETENTION_AND_PRUNING.md)
>   — what may be pruned, what must never be, the retention windows, and the
>   bounded prune entry point that exists today. Read this before adding any
>   `remove()` call.
> - **Query patterns and read costs:** [QUERY_OPTIMIZATION.md](./QUERY_OPTIMIZATION.md)
>   — the bounded-page convention every view entry point follows, and the
>   storage-layout observations behind it.
> - **Design rationale:** [ADR-0007](./ADRs/0007-storage-data-model-and-ttl-management.md).

## Storage Philosophy

Our goal is to ensure data persistence while managing costs and adhering to Soroban's storage model. We primarily use `Persistent` storage for data that must be long-lived, such as campaign details and donation records.

## TTL (Time-to-Live) Management

Soroban's ledger entries have a limited TTL. To prevent critical data from expiring, we must actively manage its lifecycle.

### Campaign TTL

Campaign data is the most critical piece of information that needs to be preserved. We use a "bump" strategy to extend the TTL of campaign data.

- **`MIN_TTL`**:  Set to `17280` ledgers (approximately 1 day). This is the minimum time a campaign's TTL is extended by.
- **`MAX_TTL`**: Set to `6312000` ledgers (approximately 1 year). This is the maximum time a campaign's TTL can be extended to.

The TTL for a campaign is extended in the following scenarios:

1.  **Campaign Creation**: When a new campaign is created, its TTL is immediately extended.
2.  **Donation**: Each time a donation is made to a campaign, the campaign's TTL is extended. This ensures that active and popular campaigns remain on the ledger.

This strategy ensures that campaign data persists as long as the campaign is active or receiving donations.
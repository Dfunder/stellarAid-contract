# StellarAid Event Schemas (superseded)

> **Do not implement against this file.** The authoritative event
> specification is [`EVENTS.md`](EVENTS.md).

Earlier versions of this file described events as named structs
(`AgreementCreated`, `DisputeRaised`, ...) with `u64` IDs and `String`
addresses. The contracts never emitted events in that shape. Every StellarAid
event is published as a tuple of `Symbol` topics, usually
`(domain, action)`, with a positional `ScVal` payload. IDs are `Bytes` and
parties are `Address`. Indexers built from the old tables would not decode a
single event.

The old names map to the real topics as follows:

| Old name (do not use) | Actual topics | Spec |
|---|---|---|
| `AgreementCreated` | `agr` / `created` | [EVENTS.md](EVENTS.md#agr--created) |
| `AgreementSigned` | `agr_ok` (accept) | [EVENTS.md](EVENTS.md#2-commission-agreement-contract) |
| `CommissionReleased` | `escrow` / `released` | [EVENTS.md](EVENTS.md#escrow--released) |
| `DisputeRaised` | `escrow` / `disputed` and `dispute` / `opened` | [EVENTS.md](EVENTS.md#dispute--opened) |
| `DisputeResolved` | `dispute` / `resolved` | [EVENTS.md](EVENTS.md#dispute--resolved) |
| `EscrowFunded` | `escrow` / `funded` | [EVENTS.md](EVENTS.md#escrow--funded) |
| `EscrowSettled` | `escrow` / `settled` | [EVENTS.md](EVENTS.md#escrow--settled) |

Do not add new event descriptions here. Document every new or changed event
in [`EVENTS.md`](EVENTS.md) only.

# [ADR-0001] Adoption of Stellar Soroban Rust Smart Contract Platform

* **Status:** Accepted
* **Date:** 2026-01-20
* **Authors:** StellarAid Architecture Team
* **Deciders:** Lead Architects & Foundation Stewards
* **Component:** Core / Platform

---

## Context and Problem Statement

StellarAid requires a high-performance, cost-effective, and secure smart contract platform for global humanitarian aid, escrow transactions, and commission settlements. The platform must handle high transaction volume with deterministic low fees and instant finality, while providing robust cryptographic security and asset interoperability.

## Decision Drivers

* Sub-second ledger confirmation and predictable fee structure.
* Native multi-asset and USDC support on Stellar network.
* Type safety, memory safety, and formal verification capabilities of Rust/WASM.
* State storage rent model preventing blockchain bloat.

## Considered Options

1. **Option 1: Stellar Soroban (Rust / WASM)** — Native Stellar smart contract platform.
2. **Option 2: EVM Layer 2 Rollup (e.g., Arbitrum/Optimism)** — Ethereum virtual machine contracts in Solidity.
3. **Option 3: Stellar Classic Operations (Muxed Accounts + Timelocks)** — Script-less transaction envelopes.

## Decision Outcome

**Chosen Option:** **Option 1: Stellar Soroban (Rust / WASM)**.

Soroban provides native execution alongside the Stellar Ledger, sub-5-second finality, negligible fees denominated in Stroops/XLM, and seamless interaction with Stellar Classic assets (USDC, EURC) via the Soroban Token Interface.

### Positive Consequences

* High throughput and predictable gas costs (~0.00001 XLM per invocation).
* Safe Rust development tooling (`soroban-sdk`, `soroban-env-host`).
* First-class support for authorization framework (`require_auth()`).
* Native TTL and storage tiering (Persistent, Instance, Temporary).

### Negative Consequences / Trade-offs

* Must actively manage state expiration and TTL extension to prevent contract archiving.
* Requires specialized Soroban SDK development expertise.

---

## Pros and Cons of Options

### Option 1: Stellar Soroban (Rust/WASM)
* **Good:** Native integration with Stellar payment rails and anchors.
* **Good:** Rust type safety and compiled WASM efficiency.
* **Good:** Granular auth verification with cryptographic signatures.
* **Bad:** State archiving requires TTL maintenance routines.

### Option 2: EVM Layer 2
* **Good:** Large existing ecosystem of Solidity tooling.
* **Bad:** High bridge friction to fiat anchors and Stellar USDC rails.
* **Bad:** Gas price volatility during network spikes.

### Option 3: Stellar Classic Operations
* **Good:** Zero smart contract gas footprint.
* **Bad:** Inability to execute complex multi-party milestone logic or automated arbitration.

---

## Implementation & Code References

* Workspace root: `Cargo.toml`
* Toolchain: `rust-toolchain.toml` targeting `wasm32-unknown-unknown`
* Core SDK dependency: `soroban-sdk = "21.7.7"`

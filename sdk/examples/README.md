# StellarAid TypeScript SDK Integration Examples

Comprehensive examples demonstrating end-to-end integration patterns using the **StellarAid SDK** and **@stellar/stellar-sdk** for Soroban smart contracts.

---

## Example Catalog

| File | Feature / Domain | Description |
|------|------------------|-------------|
| [`escrow-workflow.ts`](escrow-workflow.ts) | **Escrow Contract** | Creating escrows, releasing payouts, requesting refunds, and expiration handling. |
| [`commission-workflow.ts`](commission-workflow.ts) | **Commission Agreements** | Initializing agreements, artist acceptance, multi-milestone approvals, and cancellation. |
| [`dispute-resolution.ts`](dispute-resolution.ts) | **Dispute Arbiter** | Opening disputes, full client/artist resolution, partial basis-point splits, and auto-resolve. |
| [`campaign-donation-flow.ts`](campaign-donation-flow.ts) | **Campaigns & Donations** | Creating fundraising campaigns, multi-currency donations, anonymous gifts, and withdrawals. |
| [`error-handling.ts`](error-handling.ts) | **Error Handling & Resilience** | Parsing Soroban `ScError`, transaction simulation failures, retry logic, and gas estimations. |
| [`campaign-workflow.ts`](campaign-workflow.ts) | **Legacy Reference** | Baseline campaign workflow reference implementation. |

---

## Getting Started

### Prerequisites

* Node.js v18.0.0 or higher
* npm / yarn / pnpm
* `@stellar/stellar-sdk` v12.0.0+

```bash
npm install @stellar/stellar-sdk
```

### Environment Configuration

Create a `.env` file or export environment variables:

```bash
export STELLAR_RPC_URL="https://soroban-testnet.stellar.org"
export STELLAR_NETWORK_PASSPHRASE="Test SDF Network ; September 2015"
export ESCROW_CONTRACT_ID="CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAMDR4"
export COMMISSION_CONTRACT_ID="CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAFCT4"
export DISPUTE_CONTRACT_ID="CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAHK3M"
export CONFIG_CONTRACT_ID="CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAD2KM"
export USDC_TOKEN_ID="CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAUSDC"
```

### Running Examples

Execute any example using `ts-node` or `tsx`:

```bash
npx ts-node sdk/examples/escrow-workflow.ts
npx ts-node sdk/examples/commission-workflow.ts
npx ts-node sdk/examples/dispute-resolution.ts
npx ts-node sdk/examples/campaign-donation-flow.ts
npx ts-node sdk/examples/error-handling.ts
```

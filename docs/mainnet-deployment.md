# Mainnet Deployment Process

This document outlines the process for deploying the smart contract to the mainnet. Following these steps is crucial for a secure and traceable deployment.

## Wallet Requirements

For mainnet deployment, it is **mandatory** to use a secure wallet. The following options are recommended:

*   **Multi-sig wallet**: A multi-signature wallet requires multiple parties to approve transactions, providing a higher level of security.
*   **Hardware wallet**: A hardware wallet stores your private keys offline, protecting them from online threats.

## Pre-Deployment Checklist

Before deploying to the mainnet, ensure the following steps have been completed:

*   [ ] **Audit complete**: The contract has been audited by a reputable third party, and all critical and high-severity findings have been addressed.
*   [ ] **Tests passing**: All automated tests are passing, including unit tests, integration tests, and any formal verification checks.
*   [ ] **WASM hash verified**: The hash of the compiled WASM file has been verified to match the expected hash. This ensures that the correct version of the contract is deployed.

## Post-Deployment Verification

After deploying the contract, perform the following steps to verify the deployment:

1.  **Confirm contract on-chain**: Use a block explorer to confirm that the contract has been successfully deployed to the mainnet.
2.  **Check initial state**: Verify that the initial state of the contract is as expected.
3.  **Perform a test transaction**: Conduct a small, low-risk transaction to ensure the contract is functioning correctly.

## Emergency Rollback Plan

In the event of a critical issue during or after deployment, the following rollback plan should be initiated:

1.  **Freeze the contract**: If the contract includes a freeze function, use it to pause all activity.
2.  **Notify the community**: Inform users and stakeholders of the issue and the steps being taken to address it.
3.  **Deploy a new contract**: If the issue cannot be resolved with the current contract, deploy a new, corrected version.
4.  **Migrate data**: If necessary, migrate any data from the old contract to the new one.
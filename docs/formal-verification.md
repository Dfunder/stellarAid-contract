# Formal Verification Report

This document outlines the formal verification of critical invariants in the smart contract. The goal of this verification is to provide mathematical proof of the correctness of these invariants.

## Tooling

The formal verification was conducted using [Halmos](https://github.com/aumetra/halmos), a symbolic testing tool for Rust.

## Verified Invariants

The following invariants have been formally verified:

*   **`raised_amount >= sum_of_released_milestones`**: The total amount of funds raised by the campaign is always greater than or equal to the sum of the amounts released for all milestones. This ensures that the contract never releases more funds than it has raised.
*   **`donor_refund_total <= donor_contributed_total`**: The total amount of refunds issued to a donor is always less than or equal to the total amount that the donor has contributed. This prevents a donor from receiving a refund for more than they have donated.
*   **`contract_balance >= unreleased_funds`**: The balance of the contract is always greater than or equal to the amount of funds that have not yet been released for milestones. This ensures that the contract always has enough funds to cover the remaining milestone payments.

## Test Implementation

The formal verification tests are implemented in the `campaign/src/test/formal_verification_tests.rs` file. These tests use `proptest` to generate a wide range of inputs and `halmos` to symbolically execute the contract and verify the invariants.
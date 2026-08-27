//! Config-contract validation helpers (closes #590).
//!
//! Contracts that call `config_contract` for fee/admin/token settings should
//! validate the address before invoking it.  This module provides typed
//! wrappers that:
//!
//! 1. Invoke the config contract with the expected selector.
//! 2. If the call fails (bad address, missing function), emit a
//!    `config_lookup_failed` event and return a safe fallback value.
//! 3. Allow callers to *require* a valid result (hard failure path) or
//!    gracefully fall back to defaults (soft failure path).

#![allow(unused)]

use soroban_sdk::{symbol_short, Address, Env, InvokeError, Symbol, TryFromVal, Val};

fn try_call<T>(env: &Env, config_contract: &Address, selector: &Symbol) -> Option<T>
where
    T: TryFromVal<Env, Val>,
{
    env.try_invoke_contract::<T, InvokeError>(
        config_contract,
        selector,
        soroban_sdk::vec![env],
    )
    .ok()
    .and_then(Result::ok)
}

// ── Typed wrappers ───────────────────────────────────────────────────────────

/// Attempt to read the platform fee in basis points from `config_contract`.
///
/// Returns `Ok(fee_bps)` on success.  On failure, emits a
/// `config_lookup_failed` event with `topic = "get_fee_b"` and returns
/// `Err(())` so the caller can decide whether to use a fallback.
///
/// Closes #590.
pub fn try_get_fee_bps(env: &Env, config_contract: &Address) -> Result<u32, ()> {
    let result: Option<u32> = env.try_invoke_contract::<u32, soroban_sdk::Error>(
        config_contract,
        &symbol_short!("get_fee_b"),
        soroban_sdk::vec![env],
    ).ok().and_then(|r| r.ok());

    match result {
    match try_call(env, config_contract, &symbol_short!("get_fee_b")) {
        Some(v) => Ok(v),
        None => {
            emit_config_lookup_failed(env, config_contract, "get_fee_b");
            Err(())
        }
    }
}

/// Attempt to read the platform USDC token address from `config_contract`.
///
/// Closes #590.
pub fn try_get_usdc(env: &Env, config_contract: &Address) -> Result<Address, ()> {
    let result: Option<Address> = env.try_invoke_contract::<Address, soroban_sdk::Error>(
        config_contract,
        &symbol_short!("get_usdc"),
        soroban_sdk::vec![env],
    ).ok().and_then(|r| r.ok());

    match result {
    match try_call(env, config_contract, &symbol_short!("get_usdc")) {
        Some(v) => Ok(v),
        None => {
            emit_config_lookup_failed(env, config_contract, "get_usdc");
            Err(())
        }
    }
}

/// Attempt to read the platform admin address from `config_contract`.
///
/// Closes #590.
pub fn try_get_admin(env: &Env, config_contract: &Address) -> Result<Address, ()> {
    let result: Option<Address> = env.try_invoke_contract::<Address, soroban_sdk::Error>(
        config_contract,
        &symbol_short!("get_adm"),
        soroban_sdk::vec![env],
    ).ok().and_then(|r| r.ok());

    match result {
    match try_call(env, config_contract, &symbol_short!("get_adm")) {
        Some(v) => Ok(v),
        None => {
            emit_config_lookup_failed(env, config_contract, "get_adm");
            Err(())
        }
    }
}

/// Attempt to read the platform wallet address from `config_contract`.
///
/// Closes #590.
pub fn try_get_platform_wallet(env: &Env, config_contract: &Address) -> Result<Address, ()> {
    let result: Option<Address> = env.try_invoke_contract::<Address, soroban_sdk::Error>(
        config_contract,
        &symbol_short!("get_pw"),
        soroban_sdk::vec![env],
    ).ok().and_then(|r| r.ok());

    match result {
    match try_call(env, config_contract, &symbol_short!("get_pw")) {
        Some(v) => Ok(v),
        None => {
            emit_config_lookup_failed(env, config_contract, "get_pw");
            Err(())
        }
    }
}

// ── Event helper ─────────────────────────────────────────────────────────────

fn emit_config_lookup_failed(env: &Env, config_contract: &Address, selector: &str) {
    env.events().publish(
        (symbol_short!("cfg_fail"),),
        (config_contract.clone(), Symbol::new(env, selector)),
    );
}

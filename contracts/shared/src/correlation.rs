//! Inter-contract event correlation toolkit (closes #661).
//!
//! A single logical operation (escrow creation, commission settlement, dispute
//! arbitration, campaign funding, …) frequently spans several contracts and
//! several transactions. Contracts that take part in such an operation emit an
//! extra *correlation event* carrying a stable, replay-safe identifier. Off-chain
//! indexers join those events on the identifier — and on the shared operation
//! key that travels in the payload — to reconstruct the full cross-contract
//! trace.
//!
//! ## Design
//!
//! * **Deterministic.** Identifiers are SHA-256 digests over the operation’s
//!   stable inputs (`scope` + `parts`), so replaying an operation yields the
//!   same id. Indexers can safely deduplicate replayed events.
//! * **Scoped.** The `scope` (e.g. `"escrow"`, `"agr"`, `"dispute"`) prevents
//!   two domains from claiming the same id for different operations.
//! * **Chainable.** [`CorrelationId::link`] encodes causal order: a child
//!   operation’s id is derived from its parent’s, so a consumer can rebuild the
//!   causation DAG without trusting event arrival order.
//! * **Non-invasive.** Correlation events use a reserved third topic element
//!   (`corr`), so the primary event schema documented in `docs/EVENTS.md` is
//!   unchanged.

use soroban_sdk::{contracttype, Bytes, BytesN, Env, Symbol};

/// A 256-bit correlation identifier.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CorrelationId {
    /// Deterministic SHA-256 digest identifying the correlated operation.
    pub id: BytesN<32>,
}

impl CorrelationId {
    /// Derives an identifier from a `scope` and an ordered list of canonical
    /// parts. Parts must be the operation’s stable keys (commission id,
    /// campaign id, client/artist addresses, …). Ordering matters.
    pub fn derive(env: &Env, scope: &Bytes, parts: &[&Bytes]) -> Self {
        let mut buf = Bytes::new(env);
        buf.append(scope);
        for part in parts {
            let len = part.len() as u32;
            buf.append(&Bytes::from_array(env, &len.to_be_bytes()));
            buf.append(part);
        }
        CorrelationId {
            id: env.crypto().sha256(&buf).into(),
        }
    }

    /// Chained identifier for causal ordering: `sha256(parent ∥ child)`.
    ///
    /// Emitting a chained id tells indexers that `child` causally followed
    /// `parent`, even when both events arrive from different contracts.
    pub fn link(env: &Env, parent: &CorrelationId, child: &CorrelationId) -> Self {
        let mut buf = Bytes::new(env);
        buf.append(&Bytes::from_array(env, &parent.id.to_array()));
        buf.append(&Bytes::from_array(env, &child.id.to_array()));
        CorrelationId {
            id: env.crypto().sha256(&buf).into(),
        }
    }

    /// The identifier as an opaque byte slice, suitable for event payloads.
    pub fn to_bytes(&self, env: &Env) -> Bytes {
        Bytes::from_slice(env, &self.id.to_array())
    }
}

/// Canonical scope bytes for a platform domain (e.g. `"escrow"`, `"agr"`).
pub fn scope(env: &Env, domain: &str) -> Bytes {
    Bytes::from_slice(env, domain.as_bytes())
}

/// Emits `(domain, action, "corr")` with payload `(correlation_id, key)`.
///
/// The reserved third topic element lets consumers subscribe to correlation
/// events without touching the contract’s primary event schema. The `key` is
/// the shared operation key that other correlated events carry too, enabling
/// joins across contract boundaries.
pub fn publish<S>(env: &Env, domain: S, action: S, id: &CorrelationId, key: &Bytes)
where
    S: Into<Symbol>,
{
    env.events().publish(
        (domain.into(), action.into(), Symbol::new(env, "corr")),
        (id.id.clone(), key.clone()),
    );
}
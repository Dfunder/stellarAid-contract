//! Contract interaction DSL (#663).
//!
//! Builder-style steps that wrap raw contract calls in named, assertable
//! operations so scenarios read as a linear description of the flow.

use escrow::storage::CommissionStatus;
use soroban_sdk::{Address, Bytes};

use crate::assertions;
use crate::fixtures::World;

/// Calculates a 0.01%-precision fee given basis points and amount.
pub fn fee_for(amount: i128, fee_bps: u32) -> i128 {
    amount * fee_bps as i128 / 10_000
}

/// Typed steps over a deployed [`World`].
pub struct Steps<'a> {
    pub world: &'a World<'a>,
}

impl<'a> Steps<'a> {
    pub fn new(world: &'a World<'a>) -> Self {
        Self { world }
    }

    /// Create and fund a locked escrow.
    pub fn create(
        &mut self,
        commission_id: &Bytes,
        client: &Address,
        artist: &Address,
        amount: i128,
    ) -> &mut Self {
        self.world
            .escrow
            .create_escrow(commission_id, client, artist, &amount, &self.world.config_stub_addr);
        assertions::assert_escrow_status(
            self.world.environment,
            &self.world.escrow,
            commission_id,
            CommissionStatus::Locked,
        );
        self
    }

    /// Release the payout (admin) and verify the resulting state.
    pub fn release(&mut self, commission_id: &Bytes) -> &mut Self {
        self.world
            .escrow
            .release_payment(commission_id, &self.world.config_stub_addr);
        assertions::assert_escrow_status(
            self.world.environment,
            &self.world.escrow,
            commission_id,
            CommissionStatus::Released,
        );
        assertions::assert_balance(self.world.environment, &self.world.escrow_addr, 0);
        self
    }

    /// Begin the two-party atomic commit (#656).
    pub fn begin_atomic(&mut self, commission_id: &Bytes) -> &mut Self {
        self.world.escrow.begin_atomic_commit(commission_id);
        self
    }

    /// Confirm one participant of the atomic commit.
    pub fn confirm_atomic(&mut self, commission_id: &Bytes, party: &Address) -> &mut Self {
        self.world
            .escrow
            .confirm_atomic_step(commission_id, party);
        self
    }

    /// Migrate the escrowed payout to the commission contract atomically.
    pub fn migrate_atomically(
        &self,
        commission_id: &Bytes,
        commission: &Address,
    ) -> escrow::AtomicCommitMarker {
        let marker = self.world.escrow.atomic_escrow_to_commission(
            commission_id,
            &self.world.config_stub_addr,
            commission,
        );
        assert_eq!(marker.state, escrow::AtomicCommitState::Settled);
        assertions::assert_escrow_status(
            self.world.environment,
            &self.world.escrow,
            commission_id,
            CommissionStatus::Released,
        );
        assertions::assert_balance(self.world.environment, &self.world.escrow_addr, 0);
        marker
    }
}
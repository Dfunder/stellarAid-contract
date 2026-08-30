//! Scenario scripting (#663).
//!
//! A tiny orchestrator that sequences named steps over a [`World`] and keeps a
//! human-readable log of what has run, so a failed assertion shows the full
//! path taken to reach it.

use soroban_sdk::Address;

use crate::Environment;
use crate::fixtures::World;

pub struct Scenario<'a> {
    world: World<'a>,
    log: std::vec::Vec<&'static str>,
}

impl<'a> Scenario<'a> {
    /// Deploy fresh platform-config + escrow contracts and return a scenario
    /// positioned to run steps against them.
    pub fn begin(
        environment: &'a Environment,
        admin: &Address,
        platform_wallet: &Address,
        fee_bps: u32,
    ) -> Self {
        let world = World::deploy(environment, admin, platform_wallet, fee_bps);
        Self {
            world,
            log: std::vec::Vec::new(),
        }
    }

    /// Record a milestone in the scenario log.
    pub fn step(&mut self, name: &'static str) -> &mut Self {
        self.log.push(name);
        self
    }

    /// Access the deployed world.
    pub fn world(&self) -> &World<'a> {
        &self.world
    }

    /// Consume the scenario, returning the world and logging the completed
    /// steps to the test framework for diagnosis.
    pub fn finish(self) -> World<'a> {
        self.world
    }

    /// The ordered list of steps executed so far.
    pub fn steps(&self) -> &[&'static str] {
        &self.log
    }
}
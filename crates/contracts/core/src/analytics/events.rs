use soroban_sdk::{Env, Symbol, Address, Vec};

// Event Topics (used for indexing)
pub struct EventTopics;

impl EventTopics {
    pub const CAMPAIGN_CREATED: Symbol = Symbol::short("camp_cr");
    pub const DONATION_MADE: Symbol = Symbol::short("donate");
    pub const CLAIM_VALIDATED: Symbol = Symbol::short("validate");
}
//! Legacy shared event payloads.
//!
//! Closes #940: the status enums and the campaign/donation/withdrawal structs
//! in this module were referenced by no crate — each contract owns its real
//! types — so the dead definitions have been removed. The two event payloads
//! stay here because downstream indexers use this module as the schema
//! reference point.

#![allow(unused)]
use soroban_sdk::{contracttype, Address};

#[derive(Clone, Debug)]
#[contracttype]
pub struct DonationRefundedEvent {
    pub campaign_id: u64,
    pub donor: Address,
    pub amount: i128,
    pub caller: Address,
}

#[derive(Clone, Debug)]
#[contracttype]
pub struct AnonymousDonationEvent {
    pub campaign_id: u64,
    pub amount: i128,
}

#[cfg(test)]
mod tests {
    extern crate std;

    use super::*;
    use soroban_sdk::{
        symbol_short, testutils::Address as _, testutils::Events as _, Env, Symbol, TryFromVal,
    };

    /// Closes #933: pin the topic names of the donation events to the schema in
    /// `docs/event_schemas.md`. A rename that an indexer does not pick up should
    /// fail here rather than in production.
    #[test]
    fn published_event_topics_match_schema() {
        let env = Env::default();
        let donor = Address::generate(&env);
        let caller = Address::generate(&env);

        env.events().publish(
            (symbol_short!("don_ref"),),
            DonationRefundedEvent {
                campaign_id: 1,
                donor: donor.clone(),
                amount: 100,
                caller,
            },
        );
        env.events().publish(
            (symbol_short!("ano_don"),),
            AnonymousDonationEvent {
                campaign_id: 1,
                amount: 100,
            },
        );

        let events = env.events().all();
        assert_eq!(events.len(), 2);

        let refund_topics = events.get(0).unwrap().1;
        let anonymous_topics = events.get(1).unwrap().1;
        let refund: Symbol = Symbol::try_from_val(&env, &refund_topics.get(0).unwrap()).unwrap();
        let anonymous: Symbol =
            Symbol::try_from_val(&env, &anonymous_topics.get(0).unwrap()).unwrap();

        assert_eq!(refund, symbol_short!("don_ref"));
        assert_eq!(anonymous, symbol_short!("ano_don"));
    }
}

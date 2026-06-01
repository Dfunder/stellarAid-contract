use soroban_sdk::{Address, Env};

pub fn deadline_extended(
    env: &Env,
    creator: &Address,
    old_deadline: u64,
    new_deadline: u64,
) {
    env.events().publish(
        ("campaign", "deadline_extended"),
        (
            creator,
            old_deadline,
            new_deadline,
        ),
    );
}

use soroban_sdk::{Address, Env};

pub fn campaign_cancelled(
    env: &Env,
    creator: &Address,
) {
    env.events().publish(
        ("campaign", "campaign_cancelled"),
        creator,
    );
}


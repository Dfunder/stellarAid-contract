use soroban_sdk::{Address, Env, symbol_short};

pub fn admin_rotated(env: &Env, new_admin: Address) {
    env.events().publish(
        (symbol_short!("admin_rotated"),),
        new_admin,
    );
}

pub fn signer_added(env: &Env, signer: Address) {
    env.events().publish(
        (symbol_short!("signer_added"),),
        signer,
    );
}
#![no_std]
use soroban_sdk::{contract, contractimpl, symbol_short, token, Address, Env, String, Symbol, Vec};

pub mod errors;
pub mod types;

#[cfg(test)]
mod test;

use errors::SubscriptionError;
use types::{DataKey, PaymentKind, PaymentRecord, Subscription, SubscriptionStatus, Tier};

#[contract]
pub struct SubscriptionContract;

fn has_admin(env: &Env) -> bool {
    env.storage().instance().has(&DataKey::Admin)
}

fn require_admin(env: &Env) -> Result<(), SubscriptionError> {
    let admin: Address = env
        .storage()
        .instance()
        .get(&DataKey::Admin)
        .ok_or(SubscriptionError::NotInitialized)?;
    admin.require_auth();
    Ok(())
}

fn require_initialized(env: &Env) -> Result<(), SubscriptionError> {
    if has_admin(env) {
        Ok(())
    } else {
        Err(SubscriptionError::NotInitialized)
    }
}

fn get_u32(env: &Env, key: &DataKey) -> u32 {
    env.storage().instance().get(key).unwrap_or(0)
}

fn load_tier(env: &Env, tier_id: u32) -> Result<Tier, SubscriptionError> {
    env.storage()
        .persistent()
        .get(&DataKey::Tier(tier_id))
        .ok_or(SubscriptionError::TierNotFound)
}

fn load_subscription(env: &Env, subscriber: &Address) -> Result<Subscription, SubscriptionError> {
    env.storage()
        .persistent()
        .get(&DataKey::Subscription(subscriber.clone()))
        .ok_or(SubscriptionError::NoSubscription)
}

fn save_subscription(env: &Env, subscription: &Subscription) {
    env.storage().persistent().set(
        &DataKey::Subscription(subscription.subscriber.clone()),
        subscription,
    );
}

fn credit_of(env: &Env, account: &Address) -> i128 {
    env.storage()
        .persistent()
        .get(&DataKey::Credit(account.clone()))
        .unwrap_or(0)
}

fn set_credit(env: &Env, account: &Address, amount: i128) {
    env.storage()
        .persistent()
        .set(&DataKey::Credit(account.clone()), &amount);
}

/// Charge a period against the subscriber's prepaid credit. Renewals are driven
/// off this balance rather than a live transfer so that `renew` can be called by
/// anyone once a period ends, without the subscriber signing each time.
fn charge(env: &Env, subscriber: &Address, price: i128) -> Result<(), SubscriptionError> {
    let credit = credit_of(env, subscriber);
    if credit < price {
        return Err(SubscriptionError::InsufficientCredit);
    }
    set_credit(env, subscriber, credit - price);
    Ok(())
}

fn record_payment(env: &Env, subscriber: &Address, record: PaymentRecord) {
    let key = DataKey::Payments(subscriber.clone());
    let mut payments: Vec<PaymentRecord> = env
        .storage()
        .persistent()
        .get(&key)
        .unwrap_or_else(|| Vec::new(env));
    let limit = get_u32(env, &DataKey::HistoryLimit);
    while payments.len() >= limit {
        payments.pop_front();
    }
    payments.push_back(record);
    env.storage().persistent().set(&key, &payments);
}

/// Last ledger on which benefits still apply. A renewing subscription keeps its
/// entitlements through the grace window; a cancelled one stops dead at the end
/// of the period it already paid for.
fn coverage_end(env: &Env, subscription: &Subscription) -> u32 {
    if subscription.auto_renew {
        subscription.period_end_ledger + get_u32(env, &DataKey::GraceLedgers)
    } else {
        subscription.period_end_ledger
    }
}

fn is_active(env: &Env, subscription: &Subscription) -> bool {
    subscription.status != SubscriptionStatus::Expired
        && env.ledger().sequence() <= coverage_end(env, subscription)
}

#[contractimpl]
impl SubscriptionContract {
    pub fn initialize(
        env: Env,
        admin: Address,
        token: Address,
        grace_ledgers: u32,
        history_limit: u32,
    ) -> Result<(), SubscriptionError> {
        if has_admin(&env) {
            return Err(SubscriptionError::AlreadyInitialized);
        }
        if history_limit == 0 {
            return Err(SubscriptionError::InvalidAmount);
        }
        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage().instance().set(&DataKey::Token, &token);
        env.storage()
            .instance()
            .set(&DataKey::GraceLedgers, &grace_ledgers);
        env.storage()
            .instance()
            .set(&DataKey::HistoryLimit, &history_limit);
        env.events()
            .publish((symbol_short!("init"),), (admin, token, grace_ledgers));
        Ok(())
    }

    // ── Tiers and benefits ─────────────────────────────────────────────────

    pub fn create_tier(
        env: Env,
        tier_id: u32,
        name: String,
        price: i128,
        period_ledgers: u32,
        benefits: Vec<Symbol>,
    ) -> Result<(), SubscriptionError> {
        require_admin(&env)?;
        if env.storage().persistent().has(&DataKey::Tier(tier_id)) {
            return Err(SubscriptionError::TierExists);
        }
        if price <= 0 {
            return Err(SubscriptionError::InvalidPrice);
        }
        if period_ledgers == 0 {
            return Err(SubscriptionError::InvalidPeriod);
        }
        let tier = Tier {
            tier_id,
            name,
            price,
            period_ledgers,
            benefits,
            active: true,
        };
        env.storage()
            .persistent()
            .set(&DataKey::Tier(tier_id), &tier);
        env.events().publish(
            (symbol_short!("tier_new"),),
            (tier_id, price, period_ledgers),
        );
        Ok(())
    }

    /// Retire or re-open a tier. Existing subscribers keep renewing on a
    /// retired tier only if it is re-activated; new sign-ups are blocked.
    pub fn set_tier_active(env: Env, tier_id: u32, active: bool) -> Result<(), SubscriptionError> {
        require_admin(&env)?;
        let mut tier = load_tier(&env, tier_id)?;
        tier.active = active;
        env.storage()
            .persistent()
            .set(&DataKey::Tier(tier_id), &tier);
        env.events()
            .publish((symbol_short!("tier_set"),), (tier_id, active));
        Ok(())
    }

    pub fn get_tier(env: Env, tier_id: u32) -> Result<Tier, SubscriptionError> {
        load_tier(&env, tier_id)
    }

    /// Whether an account's current tier grants a named entitlement. Returns
    /// false once coverage lapses, so callers need only this one check.
    pub fn has_benefit(env: Env, subscriber: Address, benefit: Symbol) -> bool {
        let Ok(subscription) = load_subscription(&env, &subscriber) else {
            return false;
        };
        if !is_active(&env, &subscription) {
            return false;
        }
        match load_tier(&env, subscription.tier_id) {
            Ok(tier) => tier.benefits.contains(&benefit),
            Err(_) => false,
        }
    }

    // ── Prepaid credit ─────────────────────────────────────────────────────

    pub fn deposit(env: Env, subscriber: Address, amount: i128) -> Result<i128, SubscriptionError> {
        require_initialized(&env)?;
        subscriber.require_auth();
        if amount <= 0 {
            return Err(SubscriptionError::InvalidAmount);
        }
        let balance = credit_of(&env, &subscriber) + amount;
        set_credit(&env, &subscriber, balance);

        let token_address: Address = env.storage().instance().get(&DataKey::Token).unwrap();
        token::Client::new(&env, &token_address).transfer(
            &subscriber,
            &env.current_contract_address(),
            &amount,
        );

        env.events()
            .publish((symbol_short!("deposit"),), (subscriber, amount, balance));
        Ok(balance)
    }

    pub fn withdraw(
        env: Env,
        subscriber: Address,
        amount: i128,
    ) -> Result<i128, SubscriptionError> {
        require_initialized(&env)?;
        subscriber.require_auth();
        if amount <= 0 {
            return Err(SubscriptionError::InvalidAmount);
        }
        let credit = credit_of(&env, &subscriber);
        if credit < amount {
            return Err(SubscriptionError::InsufficientCredit);
        }
        set_credit(&env, &subscriber, credit - amount);

        let token_address: Address = env.storage().instance().get(&DataKey::Token).unwrap();
        token::Client::new(&env, &token_address).transfer(
            &env.current_contract_address(),
            &subscriber,
            &amount,
        );

        env.events()
            .publish((symbol_short!("withdraw"),), (subscriber, amount));
        Ok(credit - amount)
    }

    pub fn get_credit(env: Env, subscriber: Address) -> i128 {
        credit_of(&env, &subscriber)
    }

    // ── Subscription lifecycle ─────────────────────────────────────────────

    pub fn subscribe(
        env: Env,
        subscriber: Address,
        tier_id: u32,
        auto_renew: bool,
    ) -> Result<(), SubscriptionError> {
        require_initialized(&env)?;
        subscriber.require_auth();

        if let Ok(existing) = load_subscription(&env, &subscriber) {
            if is_active(&env, &existing) {
                return Err(SubscriptionError::AlreadySubscribed);
            }
        }
        let tier = load_tier(&env, tier_id)?;
        if !tier.active {
            return Err(SubscriptionError::TierInactive);
        }
        charge(&env, &subscriber, tier.price)?;

        let ledger = env.ledger().sequence();
        let subscription = Subscription {
            subscriber: subscriber.clone(),
            tier_id,
            status: SubscriptionStatus::Active,
            started_ledger: ledger,
            period_end_ledger: ledger + tier.period_ledgers,
            renewals: 0,
            total_paid: tier.price,
            auto_renew,
        };
        save_subscription(&env, &subscription);
        record_payment(
            &env,
            &subscriber,
            PaymentRecord {
                sequence: 1,
                tier_id,
                amount: tier.price,
                kind: PaymentKind::Initial,
                ledger,
                period_end_ledger: subscription.period_end_ledger,
            },
        );

        env.events().publish(
            (symbol_short!("subbed"),),
            (subscriber, tier_id, tier.price),
        );
        Ok(())
    }

    /// Charge the next period. Permissionless by design: once a period ends the
    /// renewal is due, and anyone (a keeper, the platform) can trigger it — the
    /// money comes from credit the subscriber already authorised.
    pub fn renew(env: Env, subscriber: Address) -> Result<u32, SubscriptionError> {
        require_initialized(&env)?;
        let mut subscription = load_subscription(&env, &subscriber)?;

        if subscription.status == SubscriptionStatus::Expired {
            return Err(SubscriptionError::NoSubscription);
        }
        if !subscription.auto_renew {
            return Err(SubscriptionError::NotRenewable);
        }
        let ledger = env.ledger().sequence();
        if ledger <= subscription.period_end_ledger {
            return Err(SubscriptionError::RenewalNotDue);
        }
        if ledger > subscription.period_end_ledger + get_u32(&env, &DataKey::GraceLedgers) {
            return Err(SubscriptionError::GraceExpired);
        }
        let tier = load_tier(&env, subscription.tier_id)?;
        if !tier.active {
            return Err(SubscriptionError::TierInactive);
        }
        charge(&env, &subscriber, tier.price)?;

        // Periods are contiguous: renewing late does not shorten or shift the
        // billing cycle, it just closes the gap the grace window covered.
        subscription.period_end_ledger += tier.period_ledgers;
        subscription.renewals += 1;
        subscription.total_paid += tier.price;
        save_subscription(&env, &subscription);
        record_payment(
            &env,
            &subscriber,
            PaymentRecord {
                sequence: subscription.renewals + 1,
                tier_id: subscription.tier_id,
                amount: tier.price,
                kind: PaymentKind::Renewal,
                ledger,
                period_end_ledger: subscription.period_end_ledger,
            },
        );

        env.events().publish(
            (symbol_short!("renewed"),),
            (
                subscriber,
                subscription.tier_id,
                subscription.period_end_ledger,
            ),
        );
        Ok(subscription.period_end_ledger)
    }

    /// Cancel auto-renewal. The subscriber keeps their benefits for the period
    /// they have already paid for.
    pub fn cancel(env: Env, subscriber: Address) -> Result<u32, SubscriptionError> {
        require_initialized(&env)?;
        subscriber.require_auth();
        let mut subscription = load_subscription(&env, &subscriber)?;
        if subscription.status == SubscriptionStatus::Expired {
            return Err(SubscriptionError::NoSubscription);
        }
        subscription.status = SubscriptionStatus::Cancelled;
        subscription.auto_renew = false;
        save_subscription(&env, &subscription);

        env.events().publish(
            (symbol_short!("cancelled"),),
            (subscriber, subscription.period_end_ledger),
        );
        Ok(subscription.period_end_ledger)
    }

    /// Permissionless: mark a subscription expired once coverage has run out,
    /// so a lapsed account stops reading as active.
    pub fn lapse(env: Env, subscriber: Address) -> Result<(), SubscriptionError> {
        require_initialized(&env)?;
        let mut subscription = load_subscription(&env, &subscriber)?;
        if subscription.status == SubscriptionStatus::Expired {
            return Err(SubscriptionError::NoSubscription);
        }
        if is_active(&env, &subscription) {
            return Err(SubscriptionError::StillActive);
        }
        subscription.status = SubscriptionStatus::Expired;
        subscription.auto_renew = false;
        save_subscription(&env, &subscription);

        env.events().publish((symbol_short!("lapsed"),), subscriber);
        Ok(())
    }

    pub fn get_subscription(
        env: Env,
        subscriber: Address,
    ) -> Result<Subscription, SubscriptionError> {
        load_subscription(&env, &subscriber)
    }

    pub fn is_active(env: Env, subscriber: Address) -> bool {
        match load_subscription(&env, &subscriber) {
            Ok(subscription) => is_active(&env, &subscription),
            Err(_) => false,
        }
    }

    /// True while the subscription is past its paid period but still inside the
    /// grace window where a renewal can still be taken.
    pub fn in_grace(env: Env, subscriber: Address) -> bool {
        match load_subscription(&env, &subscriber) {
            Ok(subscription) => {
                let ledger = env.ledger().sequence();
                subscription.status != SubscriptionStatus::Expired
                    && subscription.auto_renew
                    && ledger > subscription.period_end_ledger
                    && ledger <= coverage_end(&env, &subscription)
            }
            Err(_) => false,
        }
    }

    pub fn get_payments(env: Env, subscriber: Address) -> Vec<PaymentRecord> {
        env.storage()
            .persistent()
            .get(&DataKey::Payments(subscriber))
            .unwrap_or_else(|| Vec::new(&env))
    }
}

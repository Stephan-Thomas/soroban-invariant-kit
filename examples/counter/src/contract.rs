//! Toy counter Soroban smart contract for end-to-end invariant validation.

use soroban_sdk::{contract, contractimpl, contracttype, Env};

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DataKey {
    Counter,
}

#[contract]
pub struct CounterContract;

#[contractimpl]
impl CounterContract {
    /// Returns the current counter value, defaulting to 0.
    pub fn get(env: Env) -> i64 {
        env.storage()
            .instance()
            .get(&DataKey::Counter)
            .unwrap_or(0)
    }

    /// Increments the counter by `amount` and returns the new value.
    pub fn increment(env: Env, amount: u32) -> i64 {
        let current = Self::get(env.clone());
        let updated = current.checked_add(amount as i64).expect("overflow");
        env.storage().instance().set(&DataKey::Counter, &updated);
        updated
    }

    /// Decrements the counter by `amount` (capped at 0) and returns the new value.
    pub fn decrement(env: Env, amount: u32) -> i64 {
        let current = Self::get(env.clone());
        let amount_i64 = amount as i64;
        let updated = if current >= amount_i64 {
            current - amount_i64
        } else {
            0
        };
        env.storage().instance().set(&DataKey::Counter, &updated);
        updated
    }

    /// Resets the counter back to 0.
    pub fn reset(env: Env) -> i64 {
        let updated = 0i64;
        env.storage().instance().set(&DataKey::Counter, &updated);
        updated
    }
}

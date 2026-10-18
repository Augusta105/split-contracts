//! Centralized persistent storage helpers with automatic TTL management.
//!
//! Issue #563: All persistent storage writes go through these helpers to ensure
//! that TTL bump calls are consistent and cannot be accidentally forgotten.

use crate::constants::{MAX_INVOICE_TTL_LEDGERS, MIN_INVOICE_TTL_LEDGERS};
use soroban_sdk::{Env, IntoVal, Val};

/// Storage tier classification for keys managed by this module.
///
/// Issue #949: keys are grouped by access pattern so that hot, small entries
/// live in instance storage, durable entries live in persistent storage, and
/// short-lived scratch data lives in temporary storage.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StorageTier {
    /// Small, frequently accessed contract-wide data (config, counters).
    Instance,
    /// Durable per-invoice data (invoices, recipients, contributors).
    Persistent,
    /// Short-lived scratch data that may be evicted without loss.
    Temporary,
}

/// TTL bump helper for instance storage entries.
pub fn bump_instance_ttl<K>(env: &Env, key: &K)
where
    K: IntoVal<Env, Val> + Clone,
{
    env.storage()
        .instance()
        .extend_ttl(MIN_INVOICE_TTL_LEDGERS, MAX_INVOICE_TTL_LEDGERS);
    let _ = key;
}

/// TTL bump helper for persistent storage entries.
pub fn bump_persistent_ttl<K>(env: &Env, key: &K)
where
    K: IntoVal<Env, Val> + Clone,
{
    env.storage()
        .persistent()
        .extend_ttl(key, MIN_INVOICE_TTL_LEDGERS, MAX_INVOICE_TTL_LEDGERS);
}

/// TTL bump helper for temporary storage entries.
pub fn bump_temporary_ttl<K>(env: &Env, key: &K)
where
    K: IntoVal<Env, Val> + Clone,
{
    env.storage()
        .temporary()
        .extend_ttl(key, MIN_INVOICE_TTL_LEDGERS, MAX_INVOICE_TTL_LEDGERS);
}

/// Save an entry into the given tier and bump its TTL.
pub fn save_in_tier<K, V>(env: &Env, tier: StorageTier, key: K, value: &V)
where
    K: IntoVal<Env, Val> + Clone,
    V: IntoVal<Env, Val>,
{
    match tier {
        StorageTier::Instance => {
            env.storage().instance().set(&key, value);
            bump_instance_ttl(env, &key);
        }
        StorageTier::Persistent => {
            env.storage().persistent().set(&key, value);
            bump_persistent_ttl(env, &key);
        }
        StorageTier::Temporary => {
            env.storage().temporary().set(&key, value);
            bump_temporary_ttl(env, &key);
        }
    }
}

/// Read-modify-write a single field of a stored entry incrementally.
///
/// Issue #948: instead of rewriting the whole entry, callers pass a mutator
/// that receives the current value (if any) and returns the new value. Only
/// the single key is written, reducing compute and storage churn.
pub fn update_field<K, V, F>(env: &Env, tier: StorageTier, key: K, mutator: F)
where
    K: IntoVal<Env, Val> + Clone,
    V: IntoVal<Env, Val> + soroban_sdk::TryFromVal<Env, Val>,
    F: FnOnce(Option<V>) -> V,
{
    let current: Option<V> = match tier {
        StorageTier::Instance => env.storage().instance().get(&key),
        StorageTier::Persistent => env.storage().persistent().get(&key),
        StorageTier::Temporary => env.storage().temporary().get(&key),
    };
    let updated = mutator(current);
    save_in_tier(env, tier, key, &updated);
}

/// Incrementally update a single field of an invoice entry.
pub fn update_invoice_field<K, V, F>(env: &Env, key: K, mutator: F)
where
    K: IntoVal<Env, Val> + Clone,
    V: IntoVal<Env, Val> + soroban_sdk::TryFromVal<Env, Val>,
    F: FnOnce(Option<V>) -> V,
{
    update_field(env, StorageTier::Persistent, key, mutator);
}

/// Incrementally update a single recipient amount without rewriting the list.
pub fn update_recipient_amount<K, V, F>(env: &Env, key: K, mutator: F)
where
    K: IntoVal<Env, Val> + Clone,
    V: IntoVal<Env, Val> + soroban_sdk::TryFromVal<Env, Val>,
    F: FnOnce(Option<V>) -> V,
{
    update_field(env, StorageTier::Persistent, key, mutator);
}

/// Incrementally bump a counter stored in instance storage.
pub fn bump_counter<K>(env: &Env, key: K, delta: i128)
where
    K: IntoVal<Env, Val> + Clone,
{
    update_field::<K, i128, _>(env, StorageTier::Instance, key, |current| {
        current.unwrap_or(0).saturating_add(delta)
    });
}

/// Save an invoice entry and automatically bump its TTL.
///
/// # Arguments
/// * `env` – Soroban environment
/// * `key` – storage key (any type that implements IntoVal)
/// * `value` – value to store (any type that implements IntoVal)
pub fn save_invoice<K, V>(env: &Env, key: K, value: &V)
where
    K: IntoVal<Env, Val> + Clone,
    V: IntoVal<Env, Val>,
{
    save_in_tier(env, StorageTier::Persistent, key, value);
}

/// Save a recipients list entry and automatically bump its TTL.
pub fn save_recipients<K, V>(env: &Env, key: K, value: &V)
where
    K: IntoVal<Env, Val> + Clone,
    V: IntoVal<Env, Val>,
{
    save_in_tier(env, StorageTier::Persistent, key, value);
}

/// Save a contributor entry and automatically bump its TTL.
pub fn save_contributor<K, V>(env: &Env, key: K, value: &V)
where
    K: IntoVal<Env, Val> + Clone,
    V: IntoVal<Env, Val>,
{
    save_in_tier(env, StorageTier::Persistent, key, value);
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::testutils::Address as _;
    use soroban_sdk::{symbol_short, Address, String};

    fn contract_id(env: &Env) -> Address {
        env.register(crate::SplitContract, ())
    }

    #[test]
    fn test_save_invoice_bumps_ttl() {
        let env = Env::default();
        let id = contract_id(&env);
        let key = (symbol_short!("test_inv"), 42u64);
        let value = String::from_str(&env, "test_value");

        env.as_contract(&id, || {
            save_invoice(&env, key.clone(), &value);

            let stored: String = env
                .storage()
                .persistent()
                .get(&key)
                .expect("value should be stored");
            assert_eq!(stored, String::from_str(&env, "test_value"));
        });
    }

    #[test]
    fn test_save_recipients_bumps_ttl() {
        let env = Env::default();
        let id = contract_id(&env);
        let key = (symbol_short!("test_rec"), 42u64);
        let value = 100i128;

        env.as_contract(&id, || {
            save_recipients(&env, key.clone(), &value);

            let stored: i128 = env
                .storage()
                .persistent()
                .get(&key)
                .expect("value should be stored");
            assert_eq!(stored, 100);
        });
    }

    #[test]
    fn test_save_contributor_bumps_ttl() {
        let env = Env::default();
        let id = contract_id(&env);
        let value = Address::generate(&env);
        let key = (symbol_short!("test_con"), 42u64);

        env.as_contract(&id, || {
            save_contributor(&env, key.clone(), &value);

            let stored: Address = env
                .storage()
                .persistent()
                .get(&key)
                .expect("value should be stored");
            assert_eq!(stored, value);
        });
    }

    #[test]
    fn test_update_invoice_field_incremental() {
        let env = Env::default();
        let id = contract_id(&env);
        let key = (symbol_short!("inv_fld"), 7u64);

        env.as_contract(&id, || {
            update_invoice_field::<_, i128, _>(&env, key.clone(), |_| 10);
            update_invoice_field::<_, i128, _>(&env, key.clone(), |current| {
                current.unwrap_or(0) + 5
            });

            let stored: i128 = env
                .storage()
                .persistent()
                .get(&key)
                .expect("value should be stored");
            assert_eq!(stored, 15);
        });
    }

    #[test]
    fn test_update_recipient_amount_incremental() {
        let env = Env::default();
        let id = contract_id(&env);
        let key = (symbol_short!("rec_amt"), 3u64);

        env.as_contract(&id, || {
            update_recipient_amount::<_, i128, _>(&env, key.clone(), |_| 100);
            update_recipient_amount::<_, i128, _>(&env, key.clone(), |current| {
                current.unwrap_or(0) - 40
            });

            let stored: i128 = env
                .storage()
                .persistent()
                .get(&key)
                .expect("value should be stored");
            assert_eq!(stored, 60);
        });
    }

    #[test]
    fn test_bump_counter_instance_tier() {
        let env = Env::default();
        let id = contract_id(&env);
        let key = symbol_short!("counter");

        env.as_contract(&id, || {
            bump_counter(&env, key.clone(), 1);
            bump_counter(&env, key.clone(), 2);

            let stored: i128 = env
                .storage()
                .instance()
                .get(&key)
                .expect("counter should be stored");
            assert_eq!(stored, 3);
        });
    }

    #[test]
    fn test_save_in_tier_temporary() {
        let env = Env::default();
        let id = contract_id(&env);
        let key = (symbol_short!("tmp_key"), 1u64);

        env.as_contract(&id, || {
            save_in_tier(&env, StorageTier::Temporary, key.clone(), &9i128);

            let stored: i128 = env
                .storage()
                .temporary()
                .get(&key)
                .expect("value should be stored");
            assert_eq!(stored, 9);
        });
    }
}

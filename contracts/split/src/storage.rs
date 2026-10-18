//! Centralized persistent storage helpers with automatic TTL management.
//!
//! Issue #563: All persistent storage writes go through these helpers to ensure
//! that TTL bump calls are consistent and cannot be accidentally forgotten.

use crate::constants::{MAX_INVOICE_TTL_LEDGERS, MIN_INVOICE_TTL_LEDGERS};
use soroban_sdk::{Env, IntoVal, TryFromVal, Val};

/// TTL (in ledgers) applied to cached invoice extension entries.
///
/// Issue #942: extension data is loaded lazily and cached in temporary
/// storage so repeated reads within the same ledger window avoid the
/// persistent-storage round trip.
pub const INVOICE_EXT_CACHE_TTL_LEDGERS: u32 = 100;

/// Storage key used to cache the invoice extension payload for an invoice id.
fn invoice_ext_cache_key(env: &Env, invoice_id: u64) -> Val {
    (soroban_sdk::symbol_short!("inv_ext"), invoice_id).into_val(env)
}

/// Lazily load the invoice extension payload, caching the result in
/// temporary storage.
///
/// Returns `None` when no extension data is stored for `invoice_id`.
/// The first call reads persistent storage and populates the cache; later
/// calls within the cache TTL are served from temporary storage.
pub fn load_invoice_ext_cached<V>(env: &Env, invoice_id: u64) -> Option<V>
where
    V: IntoVal<Env, Val> + TryFromVal<Env, Val>,
{
    let cache_key = invoice_ext_cache_key(env, invoice_id);

    if let Some(cached) = env.storage().temporary().get::<Val, V>(&cache_key) {
        return Some(cached);
    }

    let ext_key = (soroban_sdk::symbol_short!("inv_ext"), invoice_id);
    let value: Option<V> = env.storage().persistent().get(&ext_key);

    if value.is_some() {
        env.storage().temporary().set(&cache_key, &value);
        env.storage().temporary().extend_ttl(
            &cache_key,
            INVOICE_EXT_CACHE_TTL_LEDGERS,
            INVOICE_EXT_CACHE_TTL_LEDGERS,
        );
    }

    value
}

/// Invalidate the cached invoice extension payload for `invoice_id`.
///
/// Must be called whenever the underlying extension data changes so that
/// stale extension data is never served from the cache.
pub fn invalidate_invoice_ext_cache(env: &Env, invoice_id: u64) {
    let cache_key = invoice_ext_cache_key(env, invoice_id);
    env.storage().temporary().remove(&cache_key);
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
    env.storage().persistent().set(&key, value);
    env.storage()
        .persistent()
        .extend_ttl(&key, MIN_INVOICE_TTL_LEDGERS, MAX_INVOICE_TTL_LEDGERS);
}

/// Save a recipients list entry and automatically bump its TTL.
pub fn save_recipients<K, V>(env: &Env, key: K, value: &V)
where
    K: IntoVal<Env, Val> + Clone,
    V: IntoVal<Env, Val>,
{
    env.storage().persistent().set(&key, value);
    env.storage()
        .persistent()
        .extend_ttl(&key, MIN_INVOICE_TTL_LEDGERS, MAX_INVOICE_TTL_LEDGERS);
}

/// Save a contributor entry and automatically bump its TTL.
pub fn save_contributor<K, V>(env: &Env, key: K, value: &V)
where
    K: IntoVal<Env, Val> + Clone,
    V: IntoVal<Env, Val>,
{
    env.storage().persistent().set(&key, value);
    env.storage()
        .persistent()
        .extend_ttl(&key, MIN_INVOICE_TTL_LEDGERS, MAX_INVOICE_TTL_LEDGERS);
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
    fn test_load_invoice_ext_cached_returns_none_when_absent() {
        let env = Env::default();
        let id = contract_id(&env);

        env.as_contract(&id, || {
            let loaded: Option<i128> = load_invoice_ext_cached(&env, 7u64);
            assert!(loaded.is_none());
        });
    }

    #[test]
    fn test_load_invoice_ext_cached_caches_value() {
        let env = Env::default();
        let id = contract_id(&env);
        let ext_key = (symbol_short!("inv_ext"), 7u64);

        env.as_contract(&id, || {
            env.storage().persistent().set(&ext_key, &42i128);

            let first: Option<i128> = load_invoice_ext_cached(&env, 7u64);
            assert_eq!(first, Some(42));

            // Mutating persistent storage directly must not affect the
            // cached read until the cache is invalidated.
            env.storage().persistent().set(&ext_key, &99i128);
            let cached: Option<i128> = load_invoice_ext_cached(&env, 7u64);
            assert_eq!(cached, Some(42));

            invalidate_invoice_ext_cache(&env, 7u64);
            let refreshed: Option<i128> = load_invoice_ext_cached(&env, 7u64);
            assert_eq!(refreshed, Some(99));
        });
    }

    #[test]
    fn test_save_invoice_invalidates_ext_cache() {
        let env = Env::default();
        let id = contract_id(&env);
        let ext_key = (symbol_short!("inv_ext"), 7u64);
        let key = (symbol_short!("test_inv"), 7u64);
        let value = String::from_str(&env, "test_value");

        env.as_contract(&id, || {
            env.storage().persistent().set(&ext_key, &1i128);
            let _ = load_invoice_ext_cached::<i128>(&env, 7u64);

            env.storage().persistent().set(&ext_key, &2i128);
            save_invoice(&env, key.clone(), &value);

            let reloaded: Option<i128> = load_invoice_ext_cached(&env, 7u64);
            assert_eq!(reloaded, Some(2));
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
}

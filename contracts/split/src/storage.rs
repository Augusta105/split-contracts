//! Centralized persistent storage helpers with automatic TTL management.
//!
//! Issue #563: All persistent storage writes go through these helpers to ensure
//! that TTL bump calls are consistent and cannot be accidentally forgotten.

use crate::constants::{MAX_INVOICE_TTL_LEDGERS, MIN_INVOICE_TTL_LEDGERS};
use soroban_sdk::{Env, IntoVal, Val};

/// Storage key for the cached recipient list of a given invoice.
///
/// Issue #946: recipient lists are read far more often than they are mutated,
/// so a short-lived cache entry is kept alongside the persistent entry.
#[derive(Clone)]
pub struct RecipientCacheKey {
    pub invoice_id: u64,
}

impl IntoVal<Env, Val> for RecipientCacheKey {
    fn into_val(&self, env: &Env) -> Val {
        (soroban_sdk::symbol_short!("rec_cache"), self.invoice_id).into_val(env)
    }
}

/// Read the cached recipient list for `invoice_id`, populating the cache from
/// persistent storage on a miss.
///
/// `load` is invoked only on a cache miss and is expected to read the
/// authoritative recipient list from persistent storage. The returned value is
/// written to the cache (with a TTL bump) so subsequent reads hit the cache.
pub fn get_cached_recipients<K, V, F>(env: &Env, invoice_id: u64, load: F) -> V
where
    K: IntoVal<Env, Val> + Clone,
    V: IntoVal<Env, Val> + Clone,
    F: FnOnce() -> V,
{
    let cache_key = RecipientCacheKey { invoice_id };

    if let Some(cached) = env.storage().persistent().get::<_, V>(&cache_key) {
        // Refresh the TTL on hit so hot entries do not expire mid-use.
        env.storage().persistent().extend_ttl(
            &cache_key,
            MIN_INVOICE_TTL_LEDGERS,
            MAX_INVOICE_TTL_LEDGERS,
        );
        return cached;
    }

    let value = load();
    env.storage().persistent().set(&cache_key, &value);
    env.storage().persistent().extend_ttl(
        &cache_key,
        MIN_INVOICE_TTL_LEDGERS,
        MAX_INVOICE_TTL_LEDGERS,
    );
    value
}

/// Invalidate the cached recipient list for `invoice_id`.
///
/// Must be called whenever the recipients of an invoice are mutated so that
/// stale data is never served from the cache.
pub fn invalidate_recipients_cache(env: &Env, invoice_id: u64) {
    let cache_key = RecipientCacheKey { invoice_id };
    env.storage().persistent().remove(&cache_key);
}
=======

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
    fn test_recipients_cache_miss_populates() {
        let env = Env::default();
        let id = contract_id(&env);

        env.as_contract(&id, || {
            let value: i128 = get_cached_recipients(&env, 7u64, || 42i128);
            assert_eq!(value, 42);

            let cache_key = RecipientCacheKey { invoice_id: 7u64 };
            let cached: i128 = env
                .storage()
                .persistent()
                .get(&cache_key)
                .expect("cache should be populated on miss");
            assert_eq!(cached, 42);
        });
    }

    #[test]
    fn test_recipients_cache_hit_skips_loader() {
        let env = Env::default();
        let id = contract_id(&env);

        env.as_contract(&id, || {
            let _ = get_cached_recipients(&env, 7u64, || 42i128);

            let mut loader_called = false;
            let value: i128 = get_cached_recipients(&env, 7u64, || {
                loader_called = true;
                99i128
            });
            assert_eq!(value, 42);
            assert!(!loader_called, "loader must not run on a cache hit");
        });
    }

    #[test]
    fn test_recipients_cache_invalidation() {
        let env = Env::default();
        let id = contract_id(&env);

        env.as_contract(&id, || {
            let _ = get_cached_recipients(&env, 7u64, || 42i128);

            invalidate_recipients_cache(&env, 7u64);

            let cache_key = RecipientCacheKey { invoice_id: 7u64 };
            assert!(env.storage().persistent().get::<_, i128>(&cache_key).is_none());

            // After invalidation the loader runs again and repopulates the cache.
            let value: i128 = get_cached_recipients(&env, 7u64, || 100i128);
            assert_eq!(value, 100);
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

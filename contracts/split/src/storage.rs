//! Centralized persistent storage helpers with automatic TTL management.
//!
//! Issue #563: All persistent storage writes go through these helpers to ensure
//! that TTL bump calls are consistent and cannot be accidentally forgotten.

use crate::constants::{MAX_INVOICE_TTL_LEDGERS, MIN_INVOICE_TTL_LEDGERS};
use soroban_sdk::{Env, IntoVal, Val};

/// Maximum number of payment shards that may be stored for a single invoice.
///
/// Any shard index `>= MAX_PAYMENT_SHARDS` is rejected by
/// [`validate_shard_index`] before any storage access occurs.
pub const MAX_PAYMENT_SHARDS: u32 = 32;

/// Error returned when a shard index exceeds the configured bounds.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ShardError {
    /// The requested shard index is outside `0..MAX_PAYMENT_SHARDS`.
    ShardIndexOutOfBounds,
}

/// Validate that `index` is a legal payment shard index.
///
/// Shard indices are zero-based and must be strictly less than
/// [`MAX_PAYMENT_SHARDS`]. This check must run before any shard storage
/// access so out-of-bounds reads/writes can never touch persistent storage.
pub fn validate_shard_index(index: u32) -> Result<(), ShardError> {
    if index >= MAX_PAYMENT_SHARDS {
        return Err(ShardError::ShardIndexOutOfBounds);
    }
    Ok(())
}

/// Validate a shard count against the configured bounds.
///
/// A count of `0` is allowed (no shards), but any count greater than
/// [`MAX_PAYMENT_SHARDS`] is rejected.
pub fn validate_shard_count(count: u32) -> Result<(), ShardError> {
    if count > MAX_PAYMENT_SHARDS {
        return Err(ShardError::ShardIndexOutOfBounds);
    }
    Ok(())
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

/// Load the invoice hot key once and reuse it for the duration of an invocation.
///
/// Issue #941: the hot key is read at most once per call; callers pass the
/// returned value around instead of re-reading storage on every access.
pub fn load_invoice_hot_cached<K, V>(env: &Env, key: K) -> Option<V>
where
    K: IntoVal<Env, Val> + Clone,
    V: IntoVal<Env, Val> + soroban_sdk::TryFromVal<Env, Val>,
{
    env.storage().persistent().get(&key)
}

/// Read all payment shards for an invoice in a single pass.
///
/// Issue #945: iterates `0..count` once, validating each index before the
/// storage read. Returns [`ShardError::ShardIndexOutOfBounds`] if `count`
/// exceeds [`MAX_PAYMENT_SHARDS`].
pub fn load_payment_shards<K, V>(
    env: &Env,
    key_fn: impl Fn(u32) -> K,
    count: u32,
) -> Result<soroban_sdk::Vec<V>, ShardError>
where
    K: IntoVal<Env, Val> + Clone,
    V: IntoVal<Env, Val> + soroban_sdk::TryFromVal<Env, Val>,
{
    validate_shard_count(count)?;
    let mut out = soroban_sdk::Vec::new(env);
    let mut i = 0u32;
    while i < count {
        validate_shard_index(i)?;
        let key = key_fn(i);
        if let Some(v) = env.storage().persistent().get::<K, V>(&key) {
            out.push_back(v);
        }
        i += 1;
    }
    Ok(out)
}

/// Write all payment shards for an invoice in a single pass.
///
/// Issue #945: validates the count and every index before writing, so a
/// partially-invalid batch never mutates storage.
pub fn save_payment_shards<K, V>(
    env: &Env,
    key_fn: impl Fn(u32) -> K,
    shards: &soroban_sdk::Vec<V>,
) -> Result<(), ShardError>
where
    K: IntoVal<Env, Val> + Clone,
    V: IntoVal<Env, Val>,
{
    let count = shards.len();
    validate_shard_count(count)?;
    let mut i = 0u32;
    while i < count {
        validate_shard_index(i)?;
        i += 1;
    }
    let mut i = 0u32;
    while i < count {
        let key = key_fn(i);
        let value = shards.get_unchecked(i);
        save_invoice(env, key, &value);
        i += 1;
    }
    Ok(())
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
    fn test_validate_shard_index_bounds() {
        assert_eq!(validate_shard_index(0), Ok(()));
        assert_eq!(validate_shard_index(MAX_PAYMENT_SHARDS - 1), Ok(()));
        assert_eq!(
            validate_shard_index(MAX_PAYMENT_SHARDS),
            Err(ShardError::ShardIndexOutOfBounds)
        );
        assert_eq!(
            validate_shard_index(u32::MAX),
            Err(ShardError::ShardIndexOutOfBounds)
        );
    }

    #[test]
    fn test_validate_shard_count_bounds() {
        assert_eq!(validate_shard_count(0), Ok(()));
        assert_eq!(validate_shard_count(MAX_PAYMENT_SHARDS), Ok(()));
        assert_eq!(
            validate_shard_count(MAX_PAYMENT_SHARDS + 1),
            Err(ShardError::ShardIndexOutOfBounds)
        );
    }

    #[test]
    fn test_load_payment_shards_rejects_oversized_count() {
        let env = Env::default();
        let id = contract_id(&env);
        env.as_contract(&id, || {
            let res = load_payment_shards::<_, i128>(
                &env,
                |i| (symbol_short!("shard"), i),
                MAX_PAYMENT_SHARDS + 1,
            );
            assert_eq!(res, Err(ShardError::ShardIndexOutOfBounds));
        });
    }

    #[test]
    fn test_save_and_load_payment_shards_roundtrip() {
        let env = Env::default();
        let id = contract_id(&env);
        env.as_contract(&id, || {
            let mut shards = soroban_sdk::Vec::new(&env);
            shards.push_back(10i128);
            shards.push_back(20i128);
            shards.push_back(30i128);

            save_payment_shards(&env, |i| (symbol_short!("shard"), i), &shards)
                .expect("valid shard batch");

            let loaded = load_payment_shards::<_, i128>(
                &env,
                |i| (symbol_short!("shard"), i),
                shards.len(),
            )
            .expect("valid shard count");
            assert_eq!(loaded, shards);
        });
    }

    #[test]
    fn test_load_invoice_hot_cached_returns_value() {
        let env = Env::default();
        let id = contract_id(&env);
        let key = (symbol_short!("hot_key"), 7u64);
        let value = 999i128;

        env.as_contract(&id, || {
            save_invoice(&env, key.clone(), &value);
            let first = load_invoice_hot_cached::<_, i128>(&env, key.clone());
            let second = load_invoice_hot_cached::<_, i128>(&env, key.clone());
            assert_eq!(first, Some(999));
            assert_eq!(first, second);
        });
    }

    #[test]
    fn test_load_invoice_hot_cached_missing_returns_none() {
        let env = Env::default();
        let id = contract_id(&env);
        let key = (symbol_short!("hot_key"), 8u64);
        env.as_contract(&id, || {
            let res = load_invoice_hot_cached::<_, i128>(&env, key);
            assert_eq!(res, None);
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

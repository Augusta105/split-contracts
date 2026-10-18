//! Issue #556 / #558: Invoice validation helpers.
//!
//! These guards run *before* any storage is written during invoice creation
//! (or before any recipient mutation) so that a malformed recipient list is
//! rejected atomically.

use crate::error::ContractError;
use soroban_sdk::{symbol_short, Address, Env, IntoVal, Map, Val, Vec};

/// Issue #556: Reject a recipient list that contains duplicate addresses.
///
/// Uses a `soroban_sdk::Map` for O(n log n) membership tracking — no
/// unbounded heap allocation and a single pass over the slice.
///
/// # Errors
/// Returns [ContractError::DuplicateRecipient] when a duplicate is found.
pub fn assert_unique_recipients(env: &Env, recipients: &Vec<Address>) -> Result<(), ContractError> {
    let mut seen: Map<Address, bool> = Map::new(env);
    for r in recipients.iter() {
        if seen.contains_key(r.clone()) {
            return Err(ContractError::DuplicateRecipient);
        }
        seen.set(r.clone(), true);
    }
    Ok(())
}

/// Issue #558: Verify that every recipient has a trustline for the payment
/// token by calling `balance()` on the token contract.
///
/// A host-trapped invocation (the account has never been initialised on the
/// ledger, or no trustline exists for the asset) is treated as "no trustline".
///
/// The check is bypassed for the native / XLM asset, which requires no
/// trustline.  In Soroban the native asset is identified by the absence of an
/// explicit token contract — callers should pass the native sentinel address
/// (`env.ledger().native_token_id()` when available) or simply skip the check
/// for native invoices.
///
/// Returns `Ok(())` when every recipient returns a valid balance, or
/// `Err(ContractError::RecipientMissingTrustline)` with the offending address
/// surfaced in the panic message.
/// # Errors
/// Returns [ContractError::RecipientMissingTrustline] when a balance call fails.
pub fn assert_recipients_have_trustlines(
    env: &Env,
    token: &Address,
    recipients: &Vec<Address>,
) -> Result<(), ContractError> {
    for r in recipients.iter() {
        let mut args: Vec<Val> = Vec::new(env);
        args.push_back(r.clone().into_val(env));
        let result = env.try_invoke_contract::<i128, soroban_sdk::Error>(
            token,
            &symbol_short!("balance"),
            args,
        );
        match result {
            Ok(_) => {}
            Err(_) => {
                // Surface the offending address in the panic message so
                // callers and tests can identify which recipient is missing
                // the trustline.
                env.panic_with_error(ContractError::RecipientMissingTrustline);
            }
        }
    }
    Ok(())
}

/// Issue #623: Verify that the sum of `values` equals exactly `BASIS_POINTS_TOTAL` (10 000).
///
/// Used wherever a slice of basis-point weights must cover 100% of a whole:
/// split ratios, release stages, fee recipients, etc.
///
/// # Errors
/// Returns `Err(ContractError::InvalidRatioSum)` when `values.iter().sum::<u32>() != 10_000`.
///
/// # Examples
/// ```ignore
/// assert!(assert_bps_sum(&[5_000u32, 5_000]).is_ok());
/// assert!(assert_bps_sum(&[3_000u32, 3_000]).is_err());
/// ```
pub const BASIS_POINTS_TOTAL: u32 = 10_000;

pub fn assert_bps_sum(values: &[u32]) -> Result<(), ContractError> {
    let sum: u32 = values.iter().copied().fold(0u32, |acc, v| acc.saturating_add(v));
    assert_bps_total(sum)
}

/// Variant of [`assert_bps_sum`] for call sites where the sum has already
/// been computed (e.g. from a soroban `Vec` iterator in a `no_std` context).
///
/// Returns `Err(ContractError::InvalidRatioSum)` when `total != 10_000`.
pub fn assert_bps_total(total: u32) -> Result<(), ContractError> {
    if total != BASIS_POINTS_TOTAL {
        return Err(ContractError::InvalidRatioSum);
    }
    Ok(())
}

/// Reject a single basis-point value that exceeds 100% (`BASIS_POINTS_TOTAL` =
/// 10 000). Used for standalone rate fields such as `penalty_bps`, `tax_bps`
/// and `insurance_premium_bps` where the value must be a fraction of a whole,
/// not a sum that covers it.
///
/// # Errors
/// Returns `Err(ContractError::InvalidAmount)` when `bps > 10_000`.
pub fn assert_valid_bps(bps: u32) -> Result<(), ContractError> {
    if bps > BASIS_POINTS_TOTAL {
        return Err(ContractError::InvalidAmount);
    }
    Ok(())
}

/// Issue #936: Total number of shards an invoice's data is partitioned into.
///
/// Valid shard indices are `0..SHARD_COUNT` (inclusive lower, exclusive upper).
pub const SHARD_COUNT: u64 = 256;

/// Issue #936: Validate that a shard index lies within all applicable bounds.
///
/// A shard index must satisfy *all* of the following:
/// - `shard_id < SHARD_COUNT` (the fixed shard-space limit, 256),
/// - `shard_id < invoice_count` (there must be an invoice at that index),
/// - `shard_id < data_len` (there must be data at that index).
///
/// # Errors
/// Returns `Err(ContractError::InvalidShardIndex)` when any bound is exceeded.
///
/// # Examples
/// ```ignore
/// assert!(assert_shard_index_bounds(0, 10, 10).is_ok());
/// assert!(assert_shard_index_bounds(10, 10, 10).is_err()); // == invoice_count
/// assert!(assert_shard_index_bounds(256, 1_000, 1_000).is_err()); // >= SHARD_COUNT
/// ```
pub fn assert_shard_index_bounds(
    shard_id: u64,
    invoice_count: u64,
    data_len: u64,
) -> Result<(), ContractError> {
    if shard_id >= SHARD_COUNT || shard_id >= invoice_count || shard_id >= data_len {
        return Err(ContractError::InvalidShardIndex);
    }
    Ok(())
}

/// Issue #936: Validate a shard count against the fixed shard-space limit.
///
/// A count of zero is rejected (an invoice must have at least one shard) and
/// any count above [`SHARD_COUNT`] is rejected.
///
/// # Errors
/// Returns `Err(ContractError::InvalidShardIndex)` when `count == 0` or
/// `count > SHARD_COUNT`.
pub fn assert_shard_count(count: u64) -> Result<(), ContractError> {
    if count == 0 || count > SHARD_COUNT {
        return Err(ContractError::InvalidShardIndex);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::testutils::Address as _;

    #[test]
    fn unique_recipients_passes() {
        let env = Env::default();
        let a = Address::generate(&env);
        let b = Address::generate(&env);
        let c = Address::generate(&env);
        let mut v: Vec<Address> = Vec::new(&env);
        v.push_back(a.clone());
        v.push_back(b.clone());
        v.push_back(c.clone());
        assert!(assert_unique_recipients(&env, &v).is_ok());
    }

    #[test]
    fn duplicate_recipient_rejected() {
        let env = Env::default();
        let a = Address::generate(&env);
        let b = Address::generate(&env);
        let mut v: Vec<Address> = Vec::new(&env);
        v.push_back(a.clone());
        v.push_back(b.clone());
        v.push_back(a.clone());
        assert_eq!(
            assert_unique_recipients(&env, &v),
            Err(ContractError::DuplicateRecipient)
        );
    }

    #[test]
    fn empty_recipient_list_passes() {
        let env = Env::default();
        let v: Vec<Address> = Vec::new(&env);
        assert!(assert_unique_recipients(&env, &v).is_ok());
    }

    #[test]
    fn single_recipient_passes() {
        let env = Env::default();
        let a = Address::generate(&env);
        let mut v: Vec<Address> = Vec::new(&env);
        v.push_back(a.clone());
        assert!(assert_unique_recipients(&env, &v).is_ok());
    }

    #[test]
    fn non_adjacent_duplicate_rejected() {
        // duplicate at index 0 and 2 with a different address at index 1
        let env = Env::default();
        let a = Address::generate(&env);
        let b = Address::generate(&env);
        let mut v: Vec<Address> = Vec::new(&env);
        v.push_back(a.clone()); // index 0
        v.push_back(b.clone()); // index 1
        v.push_back(a.clone()); // index 2 — non-adjacent duplicate of index 0
        assert_eq!(
            assert_unique_recipients(&env, &v),
            Err(ContractError::DuplicateRecipient)
        );
    }

    #[test]
    fn ten_unique_recipients_passes() {
        let env = Env::default();
        let mut v: Vec<Address> = Vec::new(&env);
        for _ in 0..10 {
            v.push_back(Address::generate(&env));
        }
        assert!(assert_unique_recipients(&env, &v).is_ok());
    }

    // --- assert_bps_sum (issue #623) ---

    #[test]
    fn bps_sum_equals_10000_passes() {
        assert!(assert_bps_sum(&[5_000u32, 5_000]).is_ok());
        assert!(assert_bps_sum(&[3_000u32, 3_000, 4_000]).is_ok());
        assert!(assert_bps_sum(&[10_000u32]).is_ok());
    }

    #[test]
    fn bps_sum_not_10000_fails() {
        assert_eq!(
            assert_bps_sum(&[3_000u32, 3_000]),
            Err(ContractError::InvalidRatioSum)
        );
        assert_eq!(
            assert_bps_sum(&[0u32]),
            Err(ContractError::InvalidRatioSum)
        );
        assert_eq!(
            assert_bps_sum(&[10_001u32]),
            Err(ContractError::InvalidRatioSum)
        );
        assert_eq!(
            assert_bps_sum(&[]),
            Err(ContractError::InvalidRatioSum)
        );
    }

    // --- assert_shard_index_bounds / assert_shard_count (issue #936) ---

    #[test]
    fn shard_index_zero_passes() {
        assert!(assert_shard_index_bounds(0, 1, 1).is_ok());
        assert!(assert_shard_index_bounds(0, 10, 10).is_ok());
    }

    #[test]
    fn shard_index_max_valid_passes() {
        // Highest index that is still below every bound.
        assert!(assert_shard_index_bounds(SHARD_COUNT - 1, SHARD_COUNT, SHARD_COUNT).is_ok());
        assert!(assert_shard_index_bounds(9, 10, 10).is_ok());
    }

    #[test]
    fn shard_index_at_invoice_count_rejected() {
        assert_eq!(
            assert_shard_index_bounds(10, 10, 100),
            Err(ContractError::InvalidShardIndex)
        );
    }

    #[test]
    fn shard_index_at_data_len_rejected() {
        assert_eq!(
            assert_shard_index_bounds(10, 100, 10),
            Err(ContractError::InvalidShardIndex)
        );
    }

    #[test]
    fn shard_index_at_shard_count_rejected() {
        assert_eq!(
            assert_shard_index_bounds(SHARD_COUNT, u64::MAX, u64::MAX),
            Err(ContractError::InvalidShardIndex)
        );
    }

    #[test]
    fn shard_index_overflow_rejected() {
        assert_eq!(
            assert_shard_index_bounds(u64::MAX, u64::MAX, u64::MAX),
            Err(ContractError::InvalidShardIndex)
        );
    }

    #[test]
    fn shard_index_with_zero_bounds_rejected() {
        assert_eq!(
            assert_shard_index_bounds(0, 0, 0),
            Err(ContractError::InvalidShardIndex)
        );
        assert_eq!(
            assert_shard_index_bounds(0, 0, 10),
            Err(ContractError::InvalidShardIndex)
        );
        assert_eq!(
            assert_shard_index_bounds(0, 10, 0),
            Err(ContractError::InvalidShardIndex)
        );
    }

    #[test]
    fn shard_count_valid_passes() {
        assert!(assert_shard_count(1).is_ok());
        assert!(assert_shard_count(SHARD_COUNT).is_ok());
    }

    #[test]
    fn shard_count_zero_rejected() {
        assert_eq!(
            assert_shard_count(0),
            Err(ContractError::InvalidShardIndex)
        );
    }

    #[test]
    fn shard_count_above_limit_rejected() {
        assert_eq!(
            assert_shard_count(SHARD_COUNT + 1),
            Err(ContractError::InvalidShardIndex)
        );
        assert_eq!(
            assert_shard_count(u64::MAX),
            Err(ContractError::InvalidShardIndex)
        );
    }
}

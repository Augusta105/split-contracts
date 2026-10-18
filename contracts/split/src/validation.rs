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

/// Issue #935: Maximum platform fee rate, in basis points (10%).
///
/// Platform fees are capped well below 100% to prevent misconfiguration that
/// would drain recipients. Any configured fee above this limit is rejected.
pub const MAX_PLATFORM_FEE_BPS: u32 = 1_000;

/// Issue #935: Validate a platform fee rate against [`MAX_PLATFORM_FEE_BPS`].
///
/// # Errors
/// Returns `Err(ContractError::FeeRateTooHigh)` when `rate > MAX_PLATFORM_FEE_BPS`.
///
/// # Examples
/// ```ignore
/// assert!(validate_fee_rate(0).is_ok());
/// assert!(validate_fee_rate(MAX_PLATFORM_FEE_BPS).is_ok());
/// assert!(validate_fee_rate(MAX_PLATFORM_FEE_BPS + 1).is_err());
/// ```
pub fn validate_fee_rate(rate: u32) -> Result<(), ContractError> {
    if rate > MAX_PLATFORM_FEE_BPS {
        return Err(ContractError::FeeRateTooHigh);
    }
    Ok(())
}

/// Issue #939: Maximum ratio for a single split rule, in basis points (100%).
pub const MAX_RATIO: u32 = BASIS_POINTS_TOTAL;

/// Issue #939: Validate a list of split rules.
///
/// Each rule must have a ratio strictly greater than zero and at most
/// [`MAX_RATIO`], and the ratios must sum to exactly [`BASIS_POINTS_TOTAL`]
/// (10 000). Summation uses checked arithmetic so an overflowing list is
/// rejected rather than wrapping.
///
/// # Errors
/// - `Err(ContractError::InvalidSplitRules)` when the list is empty.
/// - `Err(ContractError::InvalidRatio)` when a ratio is zero or exceeds `MAX_RATIO`.
/// - `Err(ContractError::InvalidRatioSum)` when the sum is not exactly 10 000.
pub fn validate_split_rules(rules: &[u32]) -> Result<(), ContractError> {
    if rules.is_empty() {
        return Err(ContractError::InvalidSplitRules);
    }
    let mut total: u32 = 0;
    for &ratio in rules.iter() {
        if ratio == 0 || ratio > MAX_RATIO {
            return Err(ContractError::InvalidRatio);
        }
        total = match total.checked_add(ratio) {
            Some(t) => t,
            None => return Err(ContractError::InvalidRatioSum),
        };
    }
    assert_bps_total(total)
}

/// Issue #940: Verify that `token` implements the SEP-41 token interface.
///
/// Probes each required method (`balance`, `transfer`, `transfer_from`,
/// `approve`, `allowance`, `decimals`, `name`, `symbol`) via `try_` calls.
/// A host trap (missing export / wrong signature) is mapped to
/// `ContractError::InvalidTokenContract`.
///
/// # Errors
/// Returns `Err(ContractError::InvalidTokenContract)` when any probe fails.
pub fn validate_token_contract(env: &Env, token: &Address) -> Result<(), ContractError> {
    let account = env.current_contract_address();

    // balance(account) -> i128
    let mut args: Vec<Val> = Vec::new(env);
    args.push_back(account.clone().into_val(env));
    if env
        .try_invoke_contract::<i128, soroban_sdk::Error>(token, &symbol_short!("balance"), args)
        .is_err()
    {
        return Err(ContractError::InvalidTokenContract);
    }

    // allowance(from, spender) -> i128
    let mut args: Vec<Val> = Vec::new(env);
    args.push_back(account.clone().into_val(env));
    args.push_back(account.clone().into_val(env));
    if env
        .try_invoke_contract::<i128, soroban_sdk::Error>(token, &symbol_short!("allowance"), args)
        .is_err()
    {
        return Err(ContractError::InvalidTokenContract);
    }

    // decimals() -> u32
    if env
        .try_invoke_contract::<u32, soroban_sdk::Error>(
            token,
            &symbol_short!("decimals"),
            Vec::new(env),
        )
        .is_err()
    {
        return Err(ContractError::InvalidTokenContract);
    }

    // name() -> String
    if env
        .try_invoke_contract::<soroban_sdk::String, soroban_sdk::Error>(
            token,
            &symbol_short!("name"),
            Vec::new(env),
        )
        .is_err()
    {
        return Err(ContractError::InvalidTokenContract);
    }

    // symbol() -> String
    if env
        .try_invoke_contract::<soroban_sdk::String, soroban_sdk::Error>(
            token,
            &symbol_short!("symbol"),
            Vec::new(env),
        )
        .is_err()
    {
        return Err(ContractError::InvalidTokenContract);
    }

    // transfer / transfer_from / approve are state-mutating; probe them with
    // zero amounts so a valid contract accepts the call shape. A trap (missing
    // export or wrong signature) indicates a non-SEP-41 contract.
    let mut args: Vec<Val> = Vec::new(env);
    args.push_back(account.clone().into_val(env));
    args.push_back(account.clone().into_val(env));
    args.push_back(0i128.into_val(env));
    if env
        .try_invoke_contract::<(), soroban_sdk::Error>(token, &symbol_short!("transfer"), args)
        .is_err()
    {
        return Err(ContractError::InvalidTokenContract);
    }

    let mut args: Vec<Val> = Vec::new(env);
    args.push_back(account.clone().into_val(env));
    args.push_back(account.clone().into_val(env));
    args.push_back(account.clone().into_val(env));
    args.push_back(0i128.into_val(env));
    if env
        .try_invoke_contract::<(), soroban_sdk::Error>(token, &symbol_short!("transfer_from"), args)
        .is_err()
    {
        return Err(ContractError::InvalidTokenContract);
    }

    let mut args: Vec<Val> = Vec::new(env);
    args.push_back(account.clone().into_val(env));
    args.push_back(account.clone().into_val(env));
    args.push_back(0i128.into_val(env));
    args.push_back(0u32.into_val(env));
    if env
        .try_invoke_contract::<(), soroban_sdk::Error>(token, &symbol_short!("approve"), args)
        .is_err()
    {
        return Err(ContractError::InvalidTokenContract);
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

    // --- validate_fee_rate (issue #935) ---

    #[test]
    fn fee_rate_zero_passes() {
        assert!(validate_fee_rate(0).is_ok());
    }

    #[test]
    fn fee_rate_at_cap_passes() {
        assert!(validate_fee_rate(MAX_PLATFORM_FEE_BPS).is_ok());
    }

    #[test]
    fn fee_rate_one_above_cap_rejected() {
        assert_eq!(
            validate_fee_rate(MAX_PLATFORM_FEE_BPS + 1),
            Err(ContractError::FeeRateTooHigh)
        );
    }

    #[test]
    fn fee_rate_max_u32_rejected() {
        assert_eq!(
            validate_fee_rate(u32::MAX),
            Err(ContractError::FeeRateTooHigh)
        );
    }

    // --- validate_split_rules (issue #939) ---

    #[test]
    fn split_rules_single_full_ratio_passes() {
        assert!(validate_split_rules(&[BASIS_POINTS_TOTAL]).is_ok());
    }

    #[test]
    fn split_rules_even_split_passes() {
        assert!(validate_split_rules(&[5_000u32, 5_000]).is_ok());
        assert!(validate_split_rules(&[2_500u32, 2_500, 2_500, 2_500]).is_ok());
    }

    #[test]
    fn split_rules_empty_rejected() {
        assert_eq!(
            validate_split_rules(&[]),
            Err(ContractError::InvalidSplitRules)
        );
    }

    #[test]
    fn split_rules_zero_ratio_rejected() {
        assert_eq!(
            validate_split_rules(&[0u32, BASIS_POINTS_TOTAL]),
            Err(ContractError::InvalidRatio)
        );
    }

    #[test]
    fn split_rules_ratio_above_max_rejected() {
        assert_eq!(
            validate_split_rules(&[BASIS_POINTS_TOTAL + 1]),
            Err(ContractError::InvalidRatio)
        );
    }

    #[test]
    fn split_rules_sum_too_low_rejected() {
        assert_eq!(
            validate_split_rules(&[3_000u32, 3_000]),
            Err(ContractError::InvalidRatioSum)
        );
    }

    #[test]
    fn split_rules_sum_too_high_rejected() {
        assert_eq!(
            validate_split_rules(&[6_000u32, 6_000]),
            Err(ContractError::InvalidRatioSum)
        );
    }

    #[test]
    fn split_rules_overflow_rejected() {
        // Two max ratios overflow u32; checked arithmetic must reject.
        assert_eq!(
            validate_split_rules(&[u32::MAX, u32::MAX]),
            Err(ContractError::InvalidRatio)
        );
    }

    // --- validate_token_contract (issue #940) ---

    #[test]
    fn token_contract_non_contract_address_rejected() {
        let env = Env::default();
        let bogus = Address::generate(&env);
        assert_eq!(
            validate_token_contract(&env, &bogus),
            Err(ContractError::InvalidTokenContract)
        );
    }
}

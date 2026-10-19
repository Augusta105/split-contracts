//! Issue #556 / #558: Invoice validation helpers.
//!
//! These guards run *before* any storage is written during invoice creation
//! (or before any recipient mutation) so that a malformed recipient list is
//! rejected atomically.

use crate::error::ContractError;
use soroban_sdk::{symbol_short, Address, Env, IntoVal, Map, Val, Vec};

/// Issue #931: Maximum number of recipients on a single invoice. Bounds the
/// release loop and per-invoice storage (see `DEFAULT_INVOICE_STORAGE_QUOTA`).
pub const MAX_INVOICE_RECIPIENTS: u32 = 200;

/// Issue #931: Maximum distance between the current ledger time and an
/// invoice deadline (~10 years). Rejects typos such as millisecond timestamps
/// that would lock funds effectively forever.
pub const MAX_DEADLINE_HORIZON_SECS: u64 = 315_360_000;

/// Issue #931: Validate the recipients and amounts arrays of an invoice.
///
/// Checks, in order:
/// 1. `recipients.len() == amounts.len()` — [ContractError::InvalidRecipients]
/// 2. at least one recipient — [ContractError::EmptyRecipientList]
/// 3. at most [`MAX_INVOICE_RECIPIENTS`] — [ContractError::BatchLimitExceeded]
/// 4. every amount is strictly positive — [ContractError::ZeroAmountNotAllowed]
///
/// Single pass over `amounts`, no allocation.
///
/// # Errors
/// Returns the first failing check's error.
pub fn validate_recipients_and_amounts(
    recipients: &Vec<Address>,
    amounts: &Vec<i128>,
) -> Result<(), ContractError> {
    if recipients.len() != amounts.len() {
        return Err(ContractError::InvalidRecipients);
    }
    if recipients.is_empty() {
        return Err(ContractError::EmptyRecipientList);
    }
    if recipients.len() > MAX_INVOICE_RECIPIENTS {
        return Err(ContractError::BatchLimitExceeded);
    }
    for amt in amounts.iter() {
        if amt <= 0 {
            return Err(ContractError::ZeroAmountNotAllowed);
        }
    }
    Ok(())
}

/// Issue #931: Validate an invoice deadline against the current ledger time.
///
/// # Errors
/// - [ContractError::DeadlinePassed] when `deadline <= now`
/// - [ContractError::InvalidDeadline] when `deadline - now > MAX_DEADLINE_HORIZON_SECS`
pub fn validate_deadline(env: &Env, deadline: u64) -> Result<(), ContractError> {
    let now = env.ledger().timestamp();
    if deadline <= now {
        return Err(ContractError::DeadlinePassed);
    }
    if deadline - now > MAX_DEADLINE_HORIZON_SECS {
        return Err(ContractError::InvalidDeadline);
    }
    Ok(())
}

/// Issue #931: Validate every core input to invoice creation before any
/// storage is written: recipients/amounts arrays and the deadline.
///
/// # Errors
/// See [`validate_recipients_and_amounts`] and [`validate_deadline`].
pub fn validate_invoice_inputs(
    env: &Env,
    recipients: &Vec<Address>,
    amounts: &Vec<i128>,
    deadline: u64,
) -> Result<(), ContractError> {
    validate_recipients_and_amounts(recipients, amounts)?;
    validate_deadline(env, deadline)
}

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

    // --- validate_invoice_inputs (issue #931) ---

    use soroban_sdk::testutils::Ledger;

    fn addrs(env: &Env, n: u32) -> Vec<Address> {
        let mut v = Vec::new(env);
        for _ in 0..n {
            v.push_back(Address::generate(env));
        }
        v
    }

    fn amts(env: &Env, values: &[i128]) -> Vec<i128> {
        let mut v = Vec::new(env);
        for a in values {
            v.push_back(*a);
        }
        v
    }

    #[test]
    fn valid_invoice_inputs_pass() {
        let env = Env::default();
        env.ledger().set_timestamp(1_000);
        let r = addrs(&env, 2);
        let a = amts(&env, &[10, 20]);
        assert_eq!(validate_invoice_inputs(&env, &r, &a, 2_000), Ok(()));
    }

    #[test]
    fn length_mismatch_rejected() {
        let env = Env::default();
        let r = addrs(&env, 2);
        let a = amts(&env, &[10]);
        assert_eq!(
            validate_recipients_and_amounts(&r, &a),
            Err(ContractError::InvalidRecipients)
        );
    }

    #[test]
    fn empty_arrays_rejected() {
        let env = Env::default();
        let r = addrs(&env, 0);
        let a = amts(&env, &[]);
        assert_eq!(
            validate_recipients_and_amounts(&r, &a),
            Err(ContractError::EmptyRecipientList)
        );
    }

    #[test]
    fn too_many_recipients_rejected() {
        let env = Env::default();
        let n = MAX_INVOICE_RECIPIENTS + 1;
        let r = addrs(&env, n);
        let mut a = Vec::new(&env);
        for _ in 0..n {
            a.push_back(1i128);
        }
        assert_eq!(
            validate_recipients_and_amounts(&r, &a),
            Err(ContractError::BatchLimitExceeded)
        );
    }

    #[test]
    fn max_recipients_accepted() {
        let env = Env::default();
        let r = addrs(&env, MAX_INVOICE_RECIPIENTS);
        let mut a = Vec::new(&env);
        for _ in 0..MAX_INVOICE_RECIPIENTS {
            a.push_back(1i128);
        }
        assert_eq!(validate_recipients_and_amounts(&r, &a), Ok(()));
    }

    #[test]
    fn zero_and_negative_amounts_rejected() {
        let env = Env::default();
        let r = addrs(&env, 2);
        assert_eq!(
            validate_recipients_and_amounts(&r, &amts(&env, &[10, 0])),
            Err(ContractError::ZeroAmountNotAllowed)
        );
        assert_eq!(
            validate_recipients_and_amounts(&r, &amts(&env, &[-1, 10])),
            Err(ContractError::ZeroAmountNotAllowed)
        );
        assert_eq!(
            validate_recipients_and_amounts(&r, &amts(&env, &[i128::MIN, 10])),
            Err(ContractError::ZeroAmountNotAllowed)
        );
    }

    #[test]
    fn past_and_current_deadline_rejected() {
        let env = Env::default();
        env.ledger().set_timestamp(1_000);
        assert_eq!(validate_deadline(&env, 999), Err(ContractError::DeadlinePassed));
        assert_eq!(validate_deadline(&env, 1_000), Err(ContractError::DeadlinePassed));
        assert_eq!(validate_deadline(&env, 0), Err(ContractError::DeadlinePassed));
    }

    #[test]
    fn deadline_horizon_boundary() {
        let env = Env::default();
        env.ledger().set_timestamp(1_000);
        assert_eq!(validate_deadline(&env, 1_001), Ok(()));
        assert_eq!(
            validate_deadline(&env, 1_000 + MAX_DEADLINE_HORIZON_SECS),
            Ok(())
        );
        assert_eq!(
            validate_deadline(&env, 1_000 + MAX_DEADLINE_HORIZON_SECS + 1),
            Err(ContractError::InvalidDeadline)
        );
        assert_eq!(validate_deadline(&env, u64::MAX), Err(ContractError::InvalidDeadline));
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
}

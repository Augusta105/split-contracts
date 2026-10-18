//! # Event naming convention
//!
//! All split-contracts events follow a consistent topic layout:
//! `(symbol_short!("split"), <action>, invoice_id?)`.
//!
//! - `<action>` is an 8-char symbol identifying the lifecycle stage
//!   (`created`, `paid`, `released`, `refunded`, `st_chg`, …).
//! - `invoice_id` is included as the third topic for per-invoice events
//!   so indexers can filter by invoice without inspecting event data.
//!
//! ## When to call `next_seq`
//!
//! Events that represent discrete, countable occurrences on a single invoice
//! should include an auto-incrementing `event_seq` (fetched via `next_seq`)
//! as the last field in the event data. This gives indexers a stable,
//! per-invoice ordering key. Do **not** call `next_seq` for:
//! - global/contract-level events with no `invoice_id`
//! - events that already contain a unique identifier (e.g. `action_id`,
//!   `milestone_number`, `new_id`)
//!
//! ## `symbol_short!` vs `Symbol::new`
//!
//! Prefer `symbol_short!("abbr")` for event action topics because they are
//! short, fixed strings. Use `Symbol::new(env, "LongName")` only when the
//! symbol exceeds the short-macro length limit or must be constructed
//! dynamically.

use crate::storage_keys::ev_seq_key;
use crate::types::{DisputeOutcome, FeeSplit, InvoicePhase, InvoiceStatus, OverfundingPolicy, RepScore, TimelockAction};
use soroban_sdk::{contracttype, symbol_short, Address, BytesN, Env, String, Vec};

// ---------------------------------------------------------------------------
// Event sequence helper (per-invoice, temporary-storage counter)
// ---------------------------------------------------------------------------

/// Fetch and increment the per-invoice event sequence counter.
/// Lives in `storage::temporary` so it resets between transactions.
fn next_seq(env: &Env, invoice_id: u64) -> u64 {
    let key = ev_seq_key(invoice_id);
    let seq: u64 = env.storage().temporary().get(&key).unwrap_or(0) + 1;
    env.storage().temporary().set(&key, &seq);
    seq
}

// ---------------------------------------------------------------------------
// Events
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// Existing events
// ---------------------------------------------------------------------------

/// Emitted when a new invoice is created.
/// Topics: (split, created, invoice_id)
/// Data: (creator, total, event_seq)
pub fn invoice_created(
    env: &Env,
    invoice_id: u64,
    creator: &Address,
    total: i128,
    cross_chain_ref: &Option<String>,
) {
    let event_seq = next_seq(env, invoice_id);
    env.events().publish(
        (symbol_short!("split"), symbol_short!("created"), invoice_id),
        (creator.clone(), total, cross_chain_ref.clone(), event_seq),
    );
}

/// Emitted at invoice creation when `forward_to` is configured, making
/// surplus-forwarding visible to indexers without waiting for a release.
/// Topics: (split, fwd_cfg, invoice_id)
/// Data: forward_to
pub fn forward_configured(env: &Env, invoice_id: u64, forward_to: &Address) {
    env.events().publish(
        (symbol_short!("split"), symbol_short!("fwd_cfg"), invoice_id),
        forward_to.clone(),
    );
}

/// Emitted when a payment is received toward an invoice.
/// Topics: (split, paid, invoice_id)
/// Data: (payer, amount, token, event_seq)
pub fn payment_received(env: &Env, invoice_id: u64, payer: &Address, amount: i128, token: &Address) {
    let event_seq = next_seq(env, invoice_id);
    env.events().publish(
        (symbol_short!("split"), symbol_short!("paid"), invoice_id),
        (payer.clone(), amount, token.clone(), event_seq),
    );
}

pub fn payment_committed(env: &Env, invoice_id: u64, payer: &Address, commit_ledger: u32) {
    let event_seq = next_seq(env, invoice_id);
    env.events().publish(
        (symbol_short!("split"), symbol_short!("pay_cmt"), invoice_id),
        (payer.clone(), commit_ledger, event_seq),
    );
}

/// Emitted when a confidential (Pedersen-committed) payment is settled via
/// `reveal_confidential_payment`. Deliberately omits the amount — that is the
/// entire point of a confidential payment — even though the amount is visible
/// elsewhere on-chain after settlement (e.g. the token transfer, `funded`).
/// Topics: (split, conf_rev, invoice_id)
/// Data: (payer, event_seq)
pub fn confidential_payment_revealed(env: &Env, invoice_id: u64, payer: &Address) {
    let event_seq = next_seq(env, invoice_id);
    env.events().publish(
        (symbol_short!("split"), symbol_short!("conf_rev"), invoice_id),
        (payer.clone(), event_seq),
    );
}

pub fn milestone_released(env: &Env, invoice_id: u64, milestone_bps: u32, amount_released: i128) {
    let event_seq = next_seq(env, invoice_id);
    env.events().publish(
        (
            symbol_short!("split"),
            symbol_short!("mile_rel"),
            invoice_id,
        ),
        (milestone_bps, amount_released, event_seq),
    );
}

pub fn surplus_claimed(env: &Env, invoice_id: u64, payer: &Address, amount: i128) {
    let event_seq = next_seq(env, invoice_id);
    env.events().publish(
        (symbol_short!("split"), symbol_short!("surplus"), invoice_id),
        (payer.clone(), amount, event_seq),
    );
}

/// Emitted when an invoice is fully funded and funds are released.
/// Topics: (split, released, invoice_id)
/// Data: (recipients, event_seq)
pub fn invoice_released(env: &Env, invoice_id: u64, recipients: &Vec<Address>) {
    let event_seq = next_seq(env, invoice_id);
    env.events().publish(
        (
            symbol_short!("split"),
            symbol_short!("released"),
            invoice_id,
        ),
        (recipients.clone(), event_seq),
    );
}

/// Emitted when an invoice is refunded.
/// Topics: (split, refunded, invoice_id)
/// Data: (invoice_id, amount, event_seq)
pub fn invoice_refunded(env: &Env, invoice_id: u64, amount: i128) {
    let event_seq = next_seq(env, invoice_id);
    env.events().publish(
        (
            symbol_short!("split"),
            symbol_short!("refunded"),
            invoice_id,
        ),
        (invoice_id, amount, event_seq),
    );
}

/// Emitted when a release-condition preimage is verified.
/// Topics: (split, cond_ok, invoice_id)
/// Data: preimage_hash
pub fn condition_verified(env: &Env, invoice_id: u64, preimage_hash: &BytesN<32>) {
    let event_seq = next_seq(env, invoice_id);
    env.events().publish(
        (symbol_short!("split"), symbol_short!("cond_ok"), invoice_id),
        (preimage_hash.clone(), event_seq),
    );
}

/// Emitted when an invoice expires.
/// Topics: (split, expired, invoice_id)
/// Data: (deadline, funded, creator)
pub fn invoice_expired(env: &Env, invoice_id: u64, deadline: u64, funded: i128, creator: &Address) {
    env.events().publish(
        (
            symbol_short!("split"),
            symbol_short!("expired"),
            invoice_id,
        ),
        (deadline, funded, creator.clone()),
    );
}

/// Emitted when a recipient is added to an invoice whitelist.
/// Topics: (split, rcp_wl, invoice_id)
/// Data: address
pub fn recipient_whitelisted(env: &Env, invoice_id: u64, address: &Address) {
    let event_seq = next_seq(env, invoice_id);
    env.events().publish(
        (symbol_short!("split"), symbol_short!("rcp_wl"), invoice_id),
        (address.clone(), event_seq),
    );
}

/// Emitted when a recipient is removed from an invoice whitelist.
/// Topics: (split, rcp_rl, invoice_id)
/// Data: address
pub fn recipient_removed_from_whitelist(env: &Env, invoice_id: u64, address: &Address) {
    let event_seq = next_seq(env, invoice_id);
    env.events().publish(
        (symbol_short!("split"), symbol_short!("rcp_rl"), invoice_id),
        (address.clone(), event_seq),
    );
}

/// Emitted when rebate is accrued for a creator.
/// Topics: (split, rbt_acr, creator)
/// Data: (amount, tier_bps)
pub fn rebate_accrued(env: &Env, creator: &Address, amount: i128, tier_bps: u32) {
    env.events().publish(
        (
            symbol_short!("split"),
            symbol_short!("rbt_acr"),
            creator.clone(),
        ),
        (amount, tier_bps),
    );
}

/// Emitted once per payer when their refund is transferred.
/// Topics: (split, pay_ref, invoice_id)
/// Data: (payer, amount)
pub fn payer_refunded(env: &Env, invoice_id: u64, payer: &Address, amount: i128) {
    let event_seq = next_seq(env, invoice_id);
    env.events().publish(
        (symbol_short!("split"), symbol_short!("pay_ref"), invoice_id),
        (payer.clone(), amount, event_seq),
    );
}

/// Emitted when a recipient is added to a pending invoice.
/// Topics: (split, add_rec, invoice_id)
/// Data: (recipient, amount)
pub fn recipient_added(env: &Env, invoice_id: u64, recipient: &Address, amount: i128) {
    let event_seq = next_seq(env, invoice_id);
    env.events().publish(
        (symbol_short!("split"), symbol_short!("add_rec"), invoice_id),
        (recipient.clone(), amount, event_seq),
    );
}

/// Emitted when the creator adjusts recipient split amounts.
/// Topics: (split, adj_spl, invoice_id)
/// Data: creator
pub fn split_adjusted(env: &Env, invoice_id: u64, creator: &Address) {
    let event_seq = next_seq(env, invoice_id);
    env.events().publish(
        (symbol_short!("split"), symbol_short!("adj_spl"), invoice_id),
        (creator.clone(), event_seq),
    );
}

/// Emitted when a recipient is removed and their share redistributed among
/// the remaining recipients (issue #423).
/// Topics: (split, rebalance, invoice_id)
/// Data: (removed_address, redistributed_amount)
pub fn recipients_rebalanced(
    env: &Env,
    invoice_id: u64,
    removed_address: &Address,
    redistributed_amount: i128,
) {
    let event_seq = next_seq(env, invoice_id);
    env.events().publish(
        (
            symbol_short!("split"),
            symbol_short!("rebalance"),
            invoice_id,
        ),
        (removed_address.clone(), redistributed_amount, event_seq),
    );
}

/// Emitted when an invoice is archived to instance storage.
/// Topics: (split, archived, invoice_id)
/// Data: (invoice_id, event_seq)
pub fn invoice_archived(env: &Env, invoice_id: u64) {
    let event_seq = next_seq(env, invoice_id);
    env.events().publish(
        (
            symbol_short!("split"),
            symbol_short!("archived"),
            invoice_id,
        ),
        (invoice_id, event_seq),
    );
}

/// Emitted when a delegate is assigned to an invoice.
/// Topics: (split, delegated, invoice_id)
/// Data: (delegate, event_seq)
pub fn delegate_set(env: &Env, invoice_id: u64, delegate: &Address) {
    let event_seq = next_seq(env, invoice_id);
    env.events().publish(
        (
            symbol_short!("split"),
            symbol_short!("delegated"),
            invoice_id,
        ),
        (delegate.clone(), event_seq),
    );
}

/// Emitted when a delegate is revoked from an invoice.
/// Topics: (split, revoked, invoice_id)
/// Data: (revoker, ledger_sequence)
pub fn delegate_revoked(env: &Env, invoice_id: u64, revoker: &Address) {
    env.events().publish(
        (symbol_short!("split"), symbol_short!("revoked"), invoice_id),
        (revoker.clone(), env.ledger().sequence()),
    );
}

/// Emitted when an invoice is partially released.
/// Topics: (split, part_rel, invoice_id)
/// Data: (recipients, event_seq)
pub fn invoice_partially_released(env: &Env, invoice_id: u64, recipients: &Vec<Address>) {
    let event_seq = next_seq(env, invoice_id);
    env.events().publish(
        (
            symbol_short!("split"),
            symbol_short!("part_rel"),
            invoice_id,
        ),
        (recipients.clone(), event_seq),
    );
}

/// Emitted when a payment reminder is triggered.
/// Topics: (split, reminder, invoice_id)
/// Data: who
pub fn payment_reminder(env: &Env, invoice_id: u64, who: &Address) {
    env.events().publish(
        (
            symbol_short!("split"),
            symbol_short!("reminder"),
            invoice_id,
        ),
        who.clone(),
    );
}

/// Emitted when a payment is matched via memo.
/// Topics: (split, matched, invoice_id)
/// Data: (payer, memo)
pub fn payment_matched(env: &Env, invoice_id: u64, memo: u64, payer: &Address) {
    env.events().publish(
        (symbol_short!("split"), symbol_short!("matched"), invoice_id),
        (memo, payer.clone()),
    );
}

/// Emitted when an invoice is cloned.
/// Topics: (cloned, source_id, new_id)
/// Data: ledger_sequence
pub fn invoice_cloned(env: &Env, source_id: u64, new_id: u64) {
    env.events().publish(
        (symbol_short!("cloned"), source_id, new_id),
        (env.ledger().sequence(),),
    );
}

/// Emitted when an invoice is paused.
/// Topics: (split, paused, invoice_id)
/// Data: (creator, reason, auto_resume_at)
pub fn invoice_paused(
    env: &Env,
    invoice_id: u64,
    creator: &Address,
    reason: &String,
    auto_resume_at: &Option<u64>,
) {
    env.events().publish(
        (symbol_short!("split"), symbol_short!("paused"), invoice_id),
        (creator.clone(), reason.clone(), *auto_resume_at),
    );
}

/// Emitted whenever an invoice's `frozen` flag transitions to true.
/// Topics: (split, frozen, invoice_id)
/// Data: (creator, ledger)
pub fn invoice_frozen(env: &Env, invoice_id: u64, creator: &Address) {
    env.events().publish(
        (symbol_short!("split"), symbol_short!("frozen"), invoice_id),
        (creator.clone(), env.ledger().sequence()),
    );
}

/// Emitted when an invoice is resumed.
/// Topics: (split, resumed, invoice_id)
/// Data: creator
pub fn invoice_resumed(env: &Env, invoice_id: u64, creator: &Address) {
    env.events().publish(
        (symbol_short!("split"), symbol_short!("resumed"), invoice_id),
        creator.clone(),
    );
}

/// Emitted when a paused invoice is automatically resumed because
/// `auto_resume_at` has passed (checked lazily on the next `pay()` call).
/// Distinct from `invoice_resumed`, which is only for manual `resume_invoice`.
/// Topics: (split, auto_res, invoice_id)
/// Data: auto_resume_at
pub fn invoice_auto_resumed(env: &Env, invoice_id: u64, auto_resume_at: u64) {
    env.events().publish(
        (symbol_short!("split"), symbol_short!("auto_res"), invoice_id),
        auto_resume_at,
    );
}

/// Emitted when an invoice is force resumed.
/// Topics: (split, forced, invoice_id)
/// Data: admin_addr
pub fn invoice_force_resumed(env: &Env, invoice_id: u64, admin_addr: &Address) {
    env.events().publish(
        (symbol_short!("split"), symbol_short!("forced"), invoice_id),
        admin_addr.clone(),
    );
}

/// Emitted when the per-invoice contributor allowlist gating is toggled on
/// (first entry added, list goes None -> Some) or off (last entry removed,
/// list goes Some -> None).
/// Topics: (split, al_tog, invoice_id)
/// Data: (creator, enabled)
pub fn contributor_allowlist_toggled(env: &Env, invoice_id: u64, creator: &Address, enabled: bool) {
    env.events().publish(
        (symbol_short!("split"), symbol_short!("al_tog"), invoice_id),
        (creator.clone(), enabled),
    );
}

/// Emitted when a pending payout is claimed by a recipient (issue #209).
/// Topics: (split, pend_pay, invoice_id)
/// Data: (recipient, amount)
pub fn pending_payout_claimed(env: &Env, invoice_id: u64, recipient: &Address, amount: i128) {
    env.events().publish(
        (
            symbol_short!("split"),
            symbol_short!("pend_pay"),
            invoice_id,
        ),
        (recipient.clone(), amount),
    );
}

/// Issue #410: emitted when an invoice is renewed.
pub fn invoice_renewed(env: &Env, old_id: u64, new_id: u64, carried_amount: i128) {
    env.events().publish(
        (symbol_short!("split"), symbol_short!("renewed"), old_id),
        (new_id, carried_amount),
    );
}

/// Issue #412: emitted when a payer rates a released invoice.
pub fn invoice_rated(env: &Env, invoice_id: u64, payer: &Address, score: u32) {
    env.events().publish(
        (symbol_short!("split"), symbol_short!("rated"), invoice_id),
        (payer.clone(), score),
    );
}

/// Issue #413: emitted when a payer hits the payment rate limit.
pub fn rate_limit_hit(env: &Env, invoice_id: u64, payer: &Address, next_allowed_ledger: u32) {
    env.events().publish(
        (symbol_short!("split"), symbol_short!("rl_hit"), invoice_id),
        (payer.clone(), next_allowed_ledger),
    );
}

pub fn nft_gate_set(env: &Env, contract: &Option<Address>, admin: &Address) {
    env.events().publish(
        (symbol_short!("split"), symbol_short!("nft_set")),
        (contract.clone(), admin.clone()),
    );
}

pub fn action_queued(env: &Env, action_id: u64, action: &TimelockAction, admin: &Address) {
    env.events().publish(
        (symbol_short!("split"), symbol_short!("tl_queue"), action_id),
        (action.clone(), admin.clone()),
    );
}

pub fn action_executed(env: &Env, action_id: u64, action: &TimelockAction) {
    env.events().publish(
        (symbol_short!("split"), symbol_short!("tl_exec"), action_id),
        action.clone(),
    );
}

pub fn action_cancelled(env: &Env, action_id: u64, action: &TimelockAction, admin: &Address) {
    env.events().publish(
        (symbol_short!("split"), symbol_short!("tl_cncl"), action_id),
        (action.clone(), admin.clone()),
    );
}

pub fn invoice_admin_frozen(env: &Env, invoice_id: u64, admin: &Address, reason: &String) {
    env.events().publish(
        (symbol_short!("split"), symbol_short!("adm_frz"), invoice_id),
        (admin.clone(), reason.clone()),
    );
}

pub fn invoice_admin_unfrozen(env: &Env, invoice_id: u64, admin: &Address) {
    env.events().publish(
        (symbol_short!("split"), symbol_short!("adm_unf"), invoice_id),
        admin.clone(),
    );
}

pub fn batch_archived(env: &Env, count: u32, ids: &Vec<u64>) {
    env.events().publish(
        (symbol_short!("split"), symbol_short!("bat_arc")),
        (count, ids.clone()),
    );
}

pub fn partial_refund_issued(
    env: &Env,
    invoice_id: u64,
    creator: &Address,
    bps: u32,
    amount: i128,
) {
    env.events().publish(
        (symbol_short!("split"), symbol_short!("prt_ref"), invoice_id),
        (creator.clone(), bps, amount),
    );
}

/// Issue #276: Emitted when cumulative platform volume crosses a milestone threshold.
/// Topics: (split, plt_v_ms, milestone_number)
/// Data: (total_volume, invoice_count, ledger)
pub fn platform_volume_milestone(
    env: &Env,
    total_volume: i128,
    invoice_count: u64,
    milestone_number: i128,
) {
    env.events().publish(
        (
            symbol_short!("split"),
            symbol_short!("plt_v_ms"),
            milestone_number,
        ),
        (total_volume, invoice_count, env.ledger().sequence()),
    );
}

/// Issue #276: Emitted when a creator's lifetime volume crosses a milestone threshold.
/// Topics: (split, cr_v_ms, creator)
/// Data: (total_volume, invoice_count, milestone_number, ledger)
pub fn creator_volume_milestone(
    env: &Env,
    creator: &Address,
    total_volume: i128,
    invoice_count: u64,
    milestone_number: i128,
) {
    env.events().publish(
        (
            symbol_short!("split"),
            symbol_short!("cr_v_ms"),
            creator.clone(),
        ),
        (
            total_volume,
            invoice_count,
            milestone_number,
            env.ledger().sequence(),
        ),
    );
}

/// Issue #297: Emitted when the circuit breaker is activated.
/// Topics: (split, cb_act)
/// Data: reason
pub fn circuit_breaker_activated(env: &Env, reason: &String) {
    env.events().publish(
        (symbol_short!("split"), symbol_short!("cb_act")),
        reason.clone(),
    );
}

/// Issue #297: Emitted when the circuit breaker is deactivated.
/// Topics: (split, cb_deact)
/// Data: ()
pub fn circuit_breaker_deactivated(env: &Env) {
    env.events()
        .publish((symbol_short!("split"), symbol_short!("cb_dact")), ());
}

/// Issue #296: Emitted when a fee waiver is granted to a creator.
/// Topics: (split, fw_grant, creator)
/// Data: ()
pub fn fee_waiver_granted(env: &Env, creator: &Address) {
    env.events().publish(
        (
            symbol_short!("split"),
            symbol_short!("fw_grant"),
            creator.clone(),
        ),
        (),
    );
}

/// Issue #296: Emitted when a fee waiver is revoked from a creator.
/// Topics: (split, fw_rev, creator)
/// Data: ()
pub fn fee_waiver_revoked(env: &Env, creator: &Address) {
    env.events().publish(
        (
            symbol_short!("split"),
            symbol_short!("fw_rev"),
            creator.clone(),
        ),
        (),
    );
}

/// Issue #285: Emitted when fee tiers are updated.
/// Topics: (split, fee_tiers_updated)
/// Data: count of tiers
pub fn fee_tiers_updated(env: &Env, tier_count: u32) {
    env.events().publish(
        (symbol_short!("split"), symbol_short!("fee_upd")),
        tier_count,
    );
}

/// Issue #285: Emitted when a fee tier is applied at release time.
/// Topics: (split, fee_tier_applied, creator)
/// Data: (tier_index, fee_bps, creator_volume)
#[allow(dead_code)]
pub fn fee_tier_applied(
    env: &Env,
    creator: &Address,
    tier_index: u32,
    fee_bps: u32,
    creator_volume: u64,
) {
    env.events().publish(
        (
            symbol_short!("split"),
            symbol_short!("fee_app"),
            creator.clone(),
        ),
        (tier_index, fee_bps, creator_volume),
    );
}

/// Issue #299: Emitted when creator stats are updated.
/// Topics: (split, creator_stats_updated, creator)
/// Data: (total_invoices, total_raised, total_released, total_payers, avg_funding_time)
#[allow(dead_code)]
pub fn creator_stats_updated(
    env: &Env,
    creator: &Address,
    total_invoices: u32,
    total_raised: u64,
    total_released: u64,
    total_payers: u32,
    avg_funding_time_ledgers: u32,
) {
    env.events().publish(
        (
            symbol_short!("split"),
            symbol_short!("stats_upd"),
            creator.clone(),
        ),
        (
            total_invoices,
            total_raised,
            total_released,
            total_payers,
            avg_funding_time_ledgers,
        ),
    );
}

/// Issue #283: Unified state transition event emitted on every invoice status change.
///
/// # Indexer Guide
/// Indexers can reconstruct the full invoice lifecycle by filtering events with
/// topic[1] == "st_chg". Each event carries:
///   - `from`: the previous status (as a Symbol: "none", "pending", "released", "refunded", "cancelled")
///   - `to`: the new status (same encoding)
///   - `actor`: the address that triggered the transition
///   - `ledger`: the ledger sequence number at transition time
///
/// To build a per-invoice state machine, collect all "st_chg" events for a given
/// `invoice_id` ordered by ledger, then replay `from -> to` pairs.
///
/// Topics: (split, st_chg, invoice_id)
/// Data: (from_status, to_status, actor, ledger)
pub fn invoice_state_changed(
    env: &Env,
    invoice_id: u64,
    from_status: Option<&InvoiceStatus>,
    to_status: &InvoiceStatus,
    actor: &Address,
) {
    let from_sym = match from_status {
        None => symbol_short!("none"),
        Some(InvoiceStatus::Pending) => symbol_short!("pending"),
        Some(InvoiceStatus::Released) => symbol_short!("released"),
        Some(InvoiceStatus::Refunded) => symbol_short!("refunded"),
        Some(InvoiceStatus::Expired) => symbol_short!("expired"),
        Some(InvoiceStatus::Cancelled) => symbol_short!("cancld"),
        Some(InvoiceStatus::Disputed) => symbol_short!("disputed"),
        Some(InvoiceStatus::PartiallyReleased) => symbol_short!("part_rel"),
        Some(InvoiceStatus::Finalised) => symbol_short!("finald"),
        Some(InvoiceStatus::Deleted) => symbol_short!("deleted"),
        Some(InvoiceStatus::PayoutInProgress) => symbol_short!("pay_prg"),
    };
    let to_sym = match to_status {
        InvoiceStatus::Pending => symbol_short!("pending"),
        InvoiceStatus::Released => symbol_short!("released"),
        InvoiceStatus::Refunded => symbol_short!("refunded"),
        InvoiceStatus::Expired => symbol_short!("expired"),
        InvoiceStatus::Cancelled => symbol_short!("cancld"),
        InvoiceStatus::Disputed => symbol_short!("disputed"),
        InvoiceStatus::PartiallyReleased => symbol_short!("part_rel"),
        InvoiceStatus::Finalised => symbol_short!("finald"),
        InvoiceStatus::Deleted => symbol_short!("deleted"),
        InvoiceStatus::PayoutInProgress => symbol_short!("pay_prg"),
    };
    env.events().publish(
        (symbol_short!("split"), symbol_short!("st_chg"), invoice_id),
        (from_sym, to_sym, actor.clone(), env.ledger().sequence()),
    );
}

/// Issue #309: Emitted when a payer is added/removed from an invoice's allowlist.
/// Topics: (split, al_upd, invoice_id)
/// Data: (creator, payer, added)
pub fn allowlist_updated(
    env: &Env,
    invoice_id: u64,
    creator: &Address,
    payer: &Address,
    added: bool,
) {
    env.events().publish(
        (symbol_short!("split"), symbol_short!("al_upd"), invoice_id),
        (creator.clone(), payer.clone(), added),
    );
}

/// Emitted when a creator clears an invoice's entire payer allowlist,
/// transitioning it from restricted to open (allowed_payers set to None).
/// Topics: (split, al_open, invoice_id)
/// Data: (creator, ledger)
pub fn allowlist_removed(env: &Env, invoice_id: u64, creator: &Address) {
    env.events().publish(
        (symbol_short!("split"), symbol_short!("al_open"), invoice_id),
        (creator.clone(), env.ledger().sequence()),
    );
}

/// Issue #308: Emitted when a payer claims their per-payer refund.
/// Topics: (split, ref_clm, invoice_id)
/// Data: (payer, amount)
pub fn refund_claimed(env: &Env, invoice_id: u64, payer: &Address, amount: i128) {
    env.events().publish(
        (symbol_short!("split"), symbol_short!("ref_clm"), invoice_id),
        (payer.clone(), amount),
    );
}

/// Issue #310: Emitted when an upgrade is proposed via the timelock.
/// Topics: (split, upg_prop)
/// Data: (new_wasm_hash, eligible_at)
pub fn upgrade_proposed(env: &Env, new_wasm_hash: &soroban_sdk::BytesN<32>, eligible_at: u64) {
    env.events().publish(
        (symbol_short!("split"), symbol_short!("upg_prop")),
        (new_wasm_hash.clone(), eligible_at),
    );
}

/// Issue #310: Emitted when a pending upgrade is executed.
/// Topics: (split, upg_exec)
/// Data: new_wasm_hash
pub fn upgrade_executed(env: &Env, new_wasm_hash: &soroban_sdk::BytesN<32>) {
    env.events().publish(
        (symbol_short!("split"), symbol_short!("upg_exec")),
        new_wasm_hash.clone(),
    );
}

/// Issue #310: Emitted when a pending upgrade proposal is cancelled.
/// Topics: (split, upg_cncl)
/// Data: admin
pub fn upgrade_cancelled(env: &Env, admin: &Address) {
    env.events().publish(
        (symbol_short!("split"), symbol_short!("upg_cncl")),
        admin.clone(),
    );
}

/// Issue #328: Emitted when the contract is emergency-paused by an admin.
/// Topics: (split, ct_paused)
/// Data: (admin, ledger)
pub fn contract_paused(env: &Env, admin: &Address) {
    env.events().publish(
        (symbol_short!("split"), symbol_short!("ct_paused")),
        (admin.clone(), env.ledger().sequence()),
    );
}

/// Issue #328: Emitted when the contract is unpaused by an admin.
/// Topics: (split, ct_unpsd)
/// Data: (admin, ledger)
pub fn contract_unpaused(env: &Env, admin: &Address) {
    env.events().publish(
        (symbol_short!("split"), symbol_short!("ct_unpsd")),
        (admin.clone(), env.ledger().sequence()),
    );
}

/// Issue #327: Emitted when funds become releasable after the time-lock delay expires.
/// Topics: (split, fnd_unlk, invoice_id)
/// Data: unlock_ledger
pub fn funds_unlocked(env: &Env, invoice_id: u64, unlock_ledger: u32) {
    env.events().publish(
        (
            symbol_short!("split"),
            symbol_short!("fnd_unlk"),
            invoice_id,
        ),
        unlock_ledger,
    );
}

/// Issue #329: Emitted when a creator updates an invoice's off-chain metadata hash.
/// Topics: (split, meta_upd, invoice_id)
/// Data: (old_hash, new_hash, ledger)
pub fn metadata_updated(
    env: &Env,
    invoice_id: u64,
    old_hash: &Option<BytesN<32>>,
    new_hash: &BytesN<32>,
) {
    env.events().publish(
        (
            symbol_short!("split"),
            symbol_short!("meta_upd"),
            invoice_id,
        ),
        (old_hash.clone(), new_hash.clone(), env.ledger().sequence()),
    );
}

/// Issue #330: Emitted when a single recipient is paid via release_to_recipient.
/// Topics: (split, rec_paid, invoice_id)
/// Data: (recipient, amount, ledger)
pub fn recipient_paid(env: &Env, invoice_id: u64, recipient: &Address, amount: i128) {
    env.events().publish(
        (
            symbol_short!("split"),
            symbol_short!("rec_paid"),
            invoice_id,
        ),
        (recipient.clone(), amount, env.ledger().sequence()),
    );
}

/// Issue #333: Emitted when an invoice crosses a funding milestone (25%, 50%, 75%, 100%).
///
/// # Indexer Guide
/// Indexers can subscribe to milestone crossings by filtering events with
/// topic[1] == "milestone" (optionally narrowed further by topic[2] == invoice_id). Each
/// event carries:
///   - `milestone_bps`: the crossed threshold in basis points relative to the invoice total —
///     2500 = 25%, 5000 = 50%, 7500 = 75%, 10000 = 100%.
///   - `funded_amount`: the invoice's cumulative funded amount at the moment the threshold
///     was crossed (in the invoice's payment token's base units).
///   - `ledger`: the ledger sequence number at which the crossing was recorded.
///
/// Multiple events can be emitted in a single `pay()` call when a large payment
/// crosses several thresholds at once — do not assume one event per payment; instead
/// group by `invoice_id` and treat each `milestone_bps` as an independent crossing.
///
/// Topics: (split, milestone, invoice_id)
/// Data: (milestone_bps, funded_amount, ledger)
pub fn milestone_reached(env: &Env, invoice_id: u64, milestone_bps: u32, funded_amount: i128) {
    env.events().publish(
        (
            symbol_short!("split"),
            symbol_short!("milestone"),
            invoice_id,
        ),
        (milestone_bps, funded_amount, env.ledger().sequence()),
    );
}

/// Emitted when an invoice crosses an admin-configured funding checkpoint.
///
/// Checkpoints are contract-level thresholds expressed in basis points where
/// `10_000 = 100%`. A single payment can emit multiple checkpoint events when it
/// crosses several configured thresholds at once.
///
/// # Indexer Guide
/// Filter events with topic[1] == "fnd_chk" (topic[2
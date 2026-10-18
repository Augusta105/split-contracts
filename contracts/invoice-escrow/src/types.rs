//! Type definitions for the invoice-escrow contract.

use soroban_sdk::{contracttype, Address, BytesN};

/// Reason a refund was issued, recorded on the invoice and emitted in the
/// refund event so indexers can categorise refunds.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub enum RefundReason {
    /// Deadline passed without the invoice being fully funded.
    DeadlineExpired,
    /// Admin cancelled the invoice before release.
    AdminCancelled,
    /// Invoice was over-funded and the excess is being returned.
    Overfunded,
    /// Payer requested a refund before the deadline.
    PayerRequested,
    /// Refund issued as part of an error recovery path.
    ErrorRecovery,
}

/// Lifecycle state of an escrow invoice.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub enum EscrowStatus {
    /// Invoice has been created but no funds deposited yet.
    Pending,
    /// At least one deposit has been received; not yet fully funded.
    Active,
    /// Invoice is fully funded and funds have been released to recipients.
    Released,
    /// Deadline passed without full funding; payers have been refunded.
    Refunded,
    /// Admin explicitly cancelled the invoice before release.
    Cancelled,
}

/// Core escrow invoice record.
#[contracttype]
#[derive(Clone, Debug)]
pub struct EscrowInvoice {
    /// Address that created the invoice.
    pub creator: Address,
    /// Token contract address (e.g. USDC).
    pub token: Address,
    /// Total amount required to fully fund this invoice.
    pub total_amount: i128,
    /// Amount deposited so far.
    pub funded_amount: i128,
    /// Unix timestamp after which refunds are permitted if not fully funded.
    pub deadline: u64,
    /// Current lifecycle status.
    pub status: EscrowStatus,
    /// Reason recorded for the most recent refund, if any.
    pub refund_reason: Option<RefundReason>,
}

/// Emitted when funds are refunded to a payer.
///
/// Event topics: `(escrow, refunded)`
/// Event data: `EscrowRefunded { invoice_id, payer, amount, reason }`
#[contracttype]
#[derive(Clone, Debug)]
pub struct EscrowRefunded {
    /// The ID of the invoice that was refunded.
    pub invoice_id: u64,
    /// The address that received the refund.
    pub payer: Address,
    /// The amount of tokens refunded.
    pub amount: i128,
    /// The reason the refund was issued.
    pub reason: RefundReason,
}

/// Emitted when the admin initiates a two-step admin authority transfer.
///
/// Event topics: `(escrow, adm_prop)`
/// Event data: `(current_admin, proposed_admin, ledger)`
#[contracttype]
#[derive(Clone, Debug)]
pub struct AdminTransferProposedEvent {
    /// The current admin initiating the transfer.
    pub current_admin: Address,
    /// The new admin being proposed.
    pub proposed_admin: Address,
    /// Ledger sequence at which the proposal was made.
    pub ledger: u32,
}

/// Emitted when the proposed admin accepts the role.
///
/// Event topics: `(escrow, adm_accpt)`
/// Event data: `(new_admin, ledger)`
#[contracttype]
#[derive(Clone, Debug)]
pub struct AdminTransferAcceptedEvent {
    /// The address that accepted the admin role.
    pub new_admin: Address,
    /// Ledger sequence at which acceptance occurred.
    pub ledger: u32,
}

/// Emitted when the current admin cancels a pending transfer.
///
/// Event topics: `(escrow, adm_cncl)`
/// Event data: `(admin, cancelled_pending, ledger)`
#[contracttype]
#[derive(Clone, Debug)]
pub struct AdminTransferCancelledEvent {
    /// The admin who cancelled the pending transfer.
    pub admin: Address,
    /// The pending admin address that was cancelled.
    pub cancelled_pending: Address,
    /// Ledger sequence at which cancellation occurred.
    pub ledger: u32,
}

// ──────────────────────────────────────────────────────────────────────
// Escrow release event
// ──────────────────────────────────────────────────────────────────────

/// Emitted after funds are successfully transferred to a recipient during
/// a release call.
///
/// Event topics: `(escrow, released)`
/// Event data: `EscrowReleased { invoice_id, recipient, amount }`
#[contracttype]
#[derive(Clone, Debug)]
pub struct EscrowReleased {
    /// The ID of the invoice that was released.
    pub invoice_id: u64,
    /// The address that received the released funds.
    pub recipient: Address,
    /// The amount of tokens transferred to the recipient.
    pub amount: i128,
}

// ──────────────────────────────────────────────────────────────────────
// Payer Blacklist types
// ──────────────────────────────────────────────────────────────────────

/// Entry in the payer blacklist.
///
/// When a payer is blacklisted by the admin, an entry is created with
/// `finalised: false`. The payer can submit an appeal during the
/// `APPEAL_WINDOW_LEDGERS` window. After the window closes (or the payer
/// submits an appeal), the admin calls `finalise_blacklist` with either
/// `uphold: true` (ban stays) or `uphold: false` (payer reinstated).
#[contracttype]
#[derive(Clone, Debug)]
pub struct BlacklistEntry {
    /// Unix timestamp when the blacklist entry was created.
    pub banned_at: u64,
    /// Optional appeal hash submitted by the payer (32-byte hash).
    pub appeal_hash: Option<BytesN<32>>,
    /// Whether the blacklist entry has been finalised by the admin.
    pub finalised: bool,
    /// Whether the ban was upheld after finalisation. Only meaningful
    /// when `finalised` is `true`.
    pub upheld: bool,
    /// Hash of the reason for blacklisting (provided by admin).
    pub reason_hash: BytesN<32>,
}

// ──────────────────────────────────────────────────────────────────────
// Error recovery suggestions
// ──────────────────────────────────────────────────────────────────────

/// Returns an actionable recovery suggestion for a given contract error.
///
/// The returned string is a short, human-readable hint describing how a
/// caller can recover from the error (e.g. retry, top up funds, wait for
/// the deadline). Unknown errors fall back to a generic suggestion.
pub fn get_recovery_suggestion(error: &crate::Error) -> &'static str {
    use crate::Error;
    match error {
        Error::NotInitialized => "Contract is not initialized; call initialize first.",
        Error::AlreadyInitialized => "Contract is already initialized; no action needed.",
        Error::Unauthorized => "Caller is not authorized; use the admin or invoice creator.",
        Error::InvoiceNotFound => "Invoice does not exist; verify the invoice ID.",
        Error::InvalidAmount => "Amount must be positive; provide a valid amount.",
        Error::InvalidStatus => "Invoice is in the wrong state; check its current status.",
        Error::DeadlineNotReached => "Deadline has not passed; wait until the deadline.",
        Error::DeadlinePassed => "Deadline has passed; refunds are now available.",
        Error::AlreadyFullyFunded => "Invoice is fully funded; no further deposits needed.",
        Error::InsufficientFunds => "Insufficient balance; top up the payer account.",
        Error::PayerBlacklisted => "Payer is blacklisted; submit an appeal or use another payer.",
        Error::TransferFailed => "Token transfer failed; retry or verify token allowance.",
        Error::Overflow => "Arithmetic overflow; reduce the amount and retry.",
        _ => "Unexpected error; retry the operation or contact support.",
    }
}

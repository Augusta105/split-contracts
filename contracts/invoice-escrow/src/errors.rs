//! Error variants for the invoice-escrow contract.
//!
//! Discriminants are stable — never reorder, only append.

use soroban_sdk::contracterror;

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum Error {
    /// Caller is not the contract admin.
    NotAdmin = 1,
    /// No pending admin transfer exists to accept or cancel.
    NoPendingAdmin = 2,
    /// Caller is not the pending admin (for accept_admin).
    Unauthorized = 3,
    /// Contract has already been initialized.
    AlreadyInitialized = 4,
    /// Contract has not been initialized yet.
    NotInitialized = 5,
    /// Invoice ID was not found in storage.
    InvoiceNotFound = 6,
    /// The invoice deadline has already passed.
    DeadlinePassed = 7,
    /// The invoice is in a state that does not allow this operation.
    InvalidStatus = 8,
    /// Amount must be greater than zero.
    InvalidAmount = 9,
    /// Total amount must be greater than zero.
    InvalidTotalAmount = 10,
    /// Deposit would exceed the invoice total; partial overflow not accepted.
    OverFunded = 11,
    /// The invoice is not yet fully funded; release is not permitted.
    InsufficientFunding = 12,
    /// The deadline has not yet passed; refund is not permitted.
    DeadlineNotPassed = 13,
    /// Recipient and amount vectors must be the same length and non-empty.
    InvalidRecipients = 14,
    /// Payer is blacklisted and cannot interact with invoices.
    PayerBlacklisted = 15,
    /// The blacklist appeal window has expired.
    AppealWindowExpired = 16,
    /// The payer is not blacklisted; cannot submit appeal or finalise.
    NotBlacklisted = 17,
    /// The blacklist entry has already been finalised.
    AlreadyFinalised = 18,
    /// Caller is not the blacklisted payer.
    NotBlacklistedPayer = 19,
    /// Co-signer list is empty, has duplicates, exceeds the maximum, or the
    /// threshold is outside `1..=signers.len()`.
    InvalidCosignerConfig = 20,
    /// Caller is not one of the invoice's configured co-signers.
    NotCosigner = 21,
    /// Co-signer has already approved or rejected this invoice.
    AlreadyVoted = 22,
    /// Release requires co-signer approval that has not yet reached threshold.
    ApprovalRequired = 23,
    /// Co-signers rejected the release; threshold can no longer be reached.
    ApprovalRejected = 24,
    /// Co-signer has not voted, so there is nothing to revoke.
    NotVoted = 25,
    /// Co-signer configuration cannot change once voting has started.
    CosignersLocked = 26,
    /// No co-signer approval is configured for this invoice.
    NoCosigners = 27,
    /// Oracle condition is malformed (non-positive threshold, zero staleness
    /// window, or fallback identical to the primary oracle).
    InvalidOracleConfig = 28,
    /// No oracle could be reached or none returned a price for the asset.
    OracleUnavailable = 29,
    /// The oracle price is older than the condition's staleness window.
    OracleStalePrice = 30,
    /// The oracle returned a non-positive price or a future timestamp.
    OracleInvalidPrice = 31,
    /// The oracle price does not satisfy the release condition.
    OracleConditionNotMet = 32,
    /// No oracle condition is configured for this invoice.
    NoOracleCondition = 33,
    /// Schedule is malformed: `release_at` not in the future, or the ledger
    /// delay exceeds the maximum / overflows.
    InvalidSchedule = 34,
    /// The scheduled release time has not been reached yet.
    ReleaseNotYetDue = 35,
    /// The scheduled minimum ledger count has not elapsed yet.
    ScheduleLedgerNotReached = 36,
    /// No active release schedule exists for this invoice.
    ScheduleNotFound = 37,
    /// The refund grace period is still running; refund is not permitted yet.
    RefundGraceActive = 38,
    /// Grace period exceeds `MAX_REFUND_GRACE_SECS`.
    InvalidGracePeriod = 39,
    /// The refund grace period has already elapsed or been waived.
    RefundGraceElapsed = 40,
}

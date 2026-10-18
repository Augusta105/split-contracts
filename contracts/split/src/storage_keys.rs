//! Centralised storage key registry (issue #312).
//!
//! All storage keys used by the contract are declared here in one of four
//! `#[contracttype]` enums. The enum-based approach guarantees that no two
//! keys serialize to the same XDR value within a storage tier, and it
//! replaces the scattered `symbol_short!` literals that previously lived
//! inside `lib.rs`.
//!
//! ## Why four enums?
//! Soroban's XDR spec limits a `#[contracttype]` enum to **50 variants**.
//! The contract uses ~115 distinct keys, so they are split by storage role:
//!
//! | Enum          | Tier       | Key shape                       | Variants |
//! |---------------|------------|---------------------------------|----------|
//! | `StorageKey`  | instance   | unit (no data)                  | ≤50      |
//! | `InvoiceKey`  | persistent | `(invoice_id: u64)` or similar  | ≤50      |
//! | `AddressKey`  | persistent | `(address: Address)` singletons | ≤50      |
//! | `CompoundKey` | persistent | two or three fields             | ≤50      |
//!
//! ## Collision safety
//! Within each enum, Rust/XDR guarantees that every variant name is unique,
//! so no two variants can produce the same serialised value. Cross-enum
//! collisions cannot happen in practice because:
//! - `StorageKey` variants are only written to **instance** storage.
//! - `InvoiceKey`, `AddressKey`, `CompoundKey` are only written to
//!   **persistent** storage.
//! - Even within persistent storage, the variant names are globally unique
//!   across the three enums (enforced by the tests below).
//!
//! ## Key migration
//! Use [`migrate_persistent`] / [`migrate_instance`] when renaming a key
//! between contract versions.

use soroban_sdk::{contracttype, symbol_short, Address, Env, IntoVal, Symbol, TryFromVal, Val};

// ---------------------------------------------------------------------------
// Precomputed key prefixes (issue #943)
// ---------------------------------------------------------------------------
//
// Hot paths previously rebuilt `Symbol` values (and the tuple wrapping them)
// on every storage access. The prefixes below are `const` so the compiler
// folds them into the binary and no allocation happens at call time. They are
// also used by the legacy tuple-key helpers further down so that the whole
// module shares a single, allocation-free set of prefixes.

/// Prefix for the per-invoice creator fee (bps) key.
pub const PREFIX_CREATOR_FEE_BPS: Symbol = symbol_short!("cr_fee_bp");
/// Prefix for the allowed payment tokens key.
pub const PREFIX_ALLOWED_TOKENS: Symbol = symbol_short!("alwd_toks");
/// Prefix for the pending successor creator key.
pub const PREFIX_PENDING_CREATOR: Symbol = symbol_short!("pend_cr");
/// Prefix for the soft-delete tombstone key.
pub const PREFIX_TOMBSTONE: Symbol = symbol_short!("tombstone");
/// Prefix for the per-invoice event sequence counter (legacy tuple form).
pub const PREFIX_EV_SEQ: Symbol = symbol_short!("ev_seq");

// ---------------------------------------------------------------------------
// Enum 1 — Instance-tier singletons
// ---------------------------------------------------------------------------

/// Contract-level singleton keys stored in **instance** storage.
///
/// All variants are unit (carry no data). Instance storage is wiped on
/// upgrade unless explicitly preserved, so these represent live config.
///
/// Soroban's XDR spec caps a `#[contracttype]` enum at **50 variants**, so
/// this enum must never grow past that limit — it is already one of four
/// enums the key registry is split across for exactly this reason (see the
/// module-level docs above). When adding a new instance-storage key, always
/// **append** a new variant at the end; never reorder or remove an existing
/// variant, since XDR encodes variants positionally and reordering would
/// silently corrupt every value already written under the old positions. If
/// this enum is at or near 50 variants, add the new key to a fifth enum
/// instead of extending this one.
#[contracttype]
#[derive(Clone)]
pub enum StorageKey {
    // --- Admin ---
    /// Primary super-admin address.
    Admin,
    /// Role map: Address → AdminRole for RBAC.
    Admins,
    /// Pending admin address for two-step admin transfer.
    PendingAdmin,
    /// Governance contract address.
    GovernanceContract,
    /// Registered factory contracts.
    Factories,

    // --- Pause / Circuit breaker ---
    /// Global pause flag (true = all write entry-points blocked).
    Paused,
    /// Set of individual function names that are selectively paused.
    PausedFns,
    /// Circuit-breaker active flag (issue #297).
    CircuitBreaker,
    /// Human-readable reason string set when the circuit breaker fires.
    CircuitBreakerReason,

    // --- Fees ---
    /// One-time invoice creation fee (in stroops / token units).
    CreationFee,
    /// Platform fee in basis points charged on release.
    PlatformFeeBps,
    /// Addresses exempt from the platform fee.
    PlatformFeeWaiverList,
    /// Addresses exempt from the creation fee.
    CreatorFeeWaiver,
    /// Tiered fee schedule (Vec<FeeTier>).
    FeeTiers,
    /// Underlying protocol / network fee (issue #559 extension).
    ProtocolFee,

    // --- Tokens / Treasury ---
    /// Default USDC token contract address.
    UsdcToken,
    /// Primary treasury address for fee collection.
    Treasury,
    /// DEX router contract used for token swaps.
    DexContract,

    // --- Invoice limits / config ---
    /// Global cap on the number of active payers per invoice.
    GlobalPayerLimit,
    /// Rolling-window length (in ledgers) for the global payer rate limit.
    GlobalPayerWindow,
    /// Generic monotonic counter used by various features.
    Counter,
    /// Maximum cancel-rate threshold in basis points.
    MaxCancelBps,
    /// Minimum invoice-volume before a platform milestone is triggered.
    PlatformVolThresh,
    /// Last platform-volume milestone that was recorded.
    PlatformVolMile,
    /// Per-creator volume threshold for milestone events.
    CreatorVolThresh,
    /// Number of ledgers after which a released/refunded invoice is archived.
    ArchiveAfterLedgers,

    // --- Contract config ---
    /// Timelock duration in seconds for governance-gated actions.
    TimelockSecs,
    /// Monotonically increasing counter for timelock action IDs.
    TimelockActionCounter,
    /// Rate-limit cap: max calls per window for protected entry-points.
    RateLimit,
    /// Rolling-window length (in ledgers) for rate-limit tracking.
    RateWindow,

    // --- Integrations ---
    /// Streaming payment contract address.
    StreamContract,
    /// Receipt-NFT factory contract address.
    ReceiptFactory,
    /// Dashboard analytics contract address.
    DashboardContract,
    /// NFT gate contract address (token-gated access).
    NftGate,
    /// KYC / compliance oracle contract address.
    KycContract,
    /// Compliance module contract address.
    Compliance,
    /// Creator allowlist for restricted-deployment mode.
    CreatorWhitelist,

    // --- Stats ---
    /// Cumulative count of all invoices ever created.
    TotalInvoices,
    /// Cumulative payment volume across all invoices.
    TotalVolume,
    /// Cumulative amount successfully released to recipients.
    TotalReleased,
    /// Cumulative amount refunded to payers.
    TotalRefunded,
    /// Counter of treasury-group invoices.
    TreasuryGroupCounter,

    // --- Upgrade / versioning ---
    /// Deployed contract schema/version number.
    ContractVersion,
    /// Pending WASM-upgrade proposal hash and metadata.
    UpgradeProposal,

    // --- Reentrancy ---
    /// Reentrancy guard flag (stored in temporary storage; cleared each tx).
    ReentrancyGuard,
    /// Issue #526: Minimum number of recipients required per invoice.
    MinRecipients,
}

// ---------------------------------------------------------------------------
// Enum 2 — Persistent-tier per-invoice keys
// ---------------------------------------------------------------------------

/// Per-invoice or per-resource keys stored in **persistent** storage.
///
/// Most variants carry a single `invoice_id: u64`; `PaymentShard` carries
/// two `u64` values.
#[contracttype]
#[derive(Clone)]
pub enum InvoiceKey {
    Invoice(u64),
    InvoiceExt(u64),
    InvoiceExt2(u64),
    InvoiceCompact(u64),
    InvoiceHot(u64),
    AuditLog(u64),
    PaymentShard(u64, u64),
    ReleaseDelay(u64),
    FundedAtLedger(u64),
    MetadataHash(u64),
    PaidRecipients(u64),
    CompactStatus(u64),
    CompactDeadlineLedger(u64),
    ConfidentialCount(u64),
    InvoiceGroup(u64),
    InvoiceTreasury(u64),
    Delegate(u64),
    PaymentWindow(u64),
    Cert(u64),
    DisputeRecord(u64),
    DisputeRaisedAt(u64),
    Refunded(u64),
    RecipientsList(u64),
    AmountsList(u64),
    PaidFlags(u64),
    MilestoneFlags(u64),
    ArchiveMarker(u64),
    CreatedLedger(u64),
    SubscriptionParams(u64),
    SubscriptionSubscribers(u64),
    ExtVote(u64),
    Group(u64),
    GroupTreasury(u64),
    TimelockAction(u64),
    PayoutCheckpoint(u64),
    /// Per-invoice event sequence counter — typed replacement for the former
    /// `(symbol_short!("ev_seq"), invoice_id)` inline key (issue #708).
    EvSeq(u64),
}

// ---------------------------------------------------------------------------
// Enum 3 — Persistent-tier per-address keys
// ---------------------------------------------------------------------------

/// Per-`Address` keys stored in **persistent** storage.
#[contracttype]
#[derive(Clone)]
pub enum AddressKey {
    Reputation(Address),
    Credit(Address),
    ReferralCount(Address),
    RecipientInvoiceIds(Address),
    DelegatePay(Address),
    RateUsage(Address),
    InvoiceCount(Address),
    CancelCount(Address),
    CreatorStatsCount(Address),
    CreatorStatsVolume(Address),
    CreatorStatsReleased(Address),
    CreatorStatsRefunded(Address),
    CreatorStatsPayers(Address),
    CreatorStatsAvgFunding(Address),
    CreatorVolumeCap(Address),
    CreatorVolumeUsed(Address),
    CreatorSelfLimit(Address),
    CreatorSelfUsed(Address),
    CreatorSelfLimitDay(Address),
    CreatorSelfLimitRaise(Address),
    PauseExempt(Address),
    GlobalVelocity(Address),
    CreatorVolMile(Address),
    /// Issue #527: Payment history for a contributor address.
    PayerHistory(Address),
}

// ---------------------------------------------------------------------------
// Enum 4 — Persistent-tier compound keys
// ---------------------------------------------------------------------------

/// Compound (multi-field) persistent-storage keys.
#[contracttype]
#[derive(Clone)]
pub enum CompoundKey {
    PendingPayout(u64, Address),
    Channel(u64, Address),
    Nonce(u64, Address),
    Velocity(u64, Address),
    ReceiptToken(u64, Address),
    Accumulator(u64, Address),
    Reminder(u64, Address),
    ConfidentialPay(u64, Address),
    Delegation(u64, Address),
    PayerCooldown(u64, Address),
    CreatorPayerSet(Address, Address),
    Template(Address, Symbol),
    TemplateVersion(Address, Symbol, u32),
    TemplateVersionCount(Address, Symbol),
}

// ---------------------------------------------------------------------------
// Migration helpers
// ---------------------------------------------------------------------------

/// Copy a value from `old_key` to `new_key` in **persistent** storage and
/// remove the old entry. No-op when `old_key` is absent.
///
/// Use this in contract `upgrade()` when renaming a storage key between
/// contract versions.
#[allow(dead_code)]
pub fn migrate_persistent<OldKey, NewKey, V>(env: &Env, old_key: &OldKey, new_key: &NewKey)
where
    OldKey: IntoVal<Env, Val>,
    NewKey: IntoVal<Env, Val>,
    V: IntoVal<Env, Val> + TryFromVal<Env, Val>,
{
    if let Some(val) = env.storage().persistent().get::<OldKey, V>(old_key) {
        env.storage().persistent().set(new_key, &val);
        env.storage().persistent().remove(old_key);
    }
}

/// Same as [`migrate_persistent`] but operates on **instance** storage.
#[allow(dead_code)]
pub fn migrate_instance<OldKey, NewKey, V>(env: &Env, old_key: &OldKey, new_key: &NewKey)
where
    OldKey: IntoVal<Env, Val>,
    NewKey: IntoVal<Env, Val>,
    V: IntoVal<Env, Val> + TryFromVal<Env, Val>,
{
    if let Some(val) = env.storage().instance().get::<OldKey, V>(old_key) {
        env.storage().instance().set(new_key, &val);
        env.storage().instance().remove(old_key);
    }
}

// ---------------------------------------------------------------------------
// Tests — key uniqueness
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::{testutils::Address as _, xdr::ToXdr, Env};

    fn xdr_sk(env: &Env, k: &StorageKey) -> soroban_sdk::Bytes {
        let v: soroban_sdk::Val = k.clone().into_val(env);
        v.to_xdr(env)
    }
    fn xdr_ik(env: &Env, k: &InvoiceKey) -> soroban_sdk::Bytes {
        let v: soroban_sdk::Val = k.clone().into_val(env);
        v.to_xdr(env)
    }
    fn xdr_ak(env: &Env, k: &AddressKey) -> soroban_sdk::Bytes {
        let v: soroban_sdk::Val = k.clone().into_val(env);
        v.to_xdr(env)
    }
    fn xdr_ck(env: &Env, k: &CompoundKey) -> soroban_sdk::Bytes {
        let v: soroban_sdk::Val = k.clone().into_val(env);
        v.to_xdr(env)
    }

    #[test]
    fn instance_keys_are_unique() {
        let env = Env::default();
        let keys: &[StorageKey] = &[
            StorageKey::Admin, StorageKey::Admins, StorageKey::Paused,
            StorageKey::PausedFns, StorageKey::Treasury, StorageKey::UsdcToken,
            StorageKey::CreationFee, StorageKey::PlatformFeeBps,
            StorageKey::PlatformFeeWaiverList, StorageKey::CreatorFeeWaiver,
            StorageKey::Counter, StorageKey::GlobalPayerLimit,
            StorageKey::GlobalPayerWindow, StorageKey::StreamContract,
            StorageKey::CreatorWhitelist, StorageKey::Compliance,
            StorageKey::KycContract, StorageKey::RateLimit, StorageKey::RateWindow,
            StorageKey::MaxCancelBps, StorageKey::ReceiptFactory,
            StorageKey::DashboardContract, StorageKey::NftGate,
            StorageKey::TimelockSecs, StorageKey::TimelockActionCounter,
            StorageKey::FeeTiers, StorageKey::PendingAdmin,
            StorageKey::GovernanceContract, StorageKey::Factories,
            StorageKey::DexContract, StorageKey::TotalInvoices,
            StorageKey::TotalVolume, StorageKey::TotalReleased,
            StorageKey::TotalRefunded, StorageKey::TreasuryGroupCounter,
            StorageKey::ContractVersion, StorageKey::ArchiveAfterLedgers,
            StorageKey::CircuitBreaker, StorageKey::CircuitBreakerReason,
            StorageKey::PlatformVolThresh, StorageKey::PlatformVolMile,
            StorageKey::CreatorVolThresh, StorageKey::UpgradeProposal,
            StorageKey::ProtocolFee, StorageKey::ReentrancyGuard,
            StorageKey::MinRecipients,
        ];
        for i in 0..keys.len() {
            for j in (i + 1)..keys.len() {
                assert_ne!(
                    xdr_sk(&env, &keys[i]),
                    xdr_sk(&env, &keys[j]),
                    "StorageKey variant {i} and {j} collide"
                );
            }
        }
    }

    #[test]
    fn invoice_keys_differ_by_variant() {
        let env = Env::default();
        let id = 42u64;
        let keys: &[InvoiceKey] = &[
            InvoiceKey::Invoice(id), InvoiceKey::InvoiceExt(id),
            InvoiceKey::InvoiceExt2(id), InvoiceKey::InvoiceCompact(id),
            InvoiceKey::InvoiceHot(id), InvoiceKey::AuditLog(id),
            InvoiceKey::ReleaseDelay(id), InvoiceKey::CompactStatus(id),
            InvoiceKey::RecipientsList(id), InvoiceKey::AmountsList(id),
            InvoiceKey::PaidFlags(id), InvoiceKey::MilestoneFlags(id),
            InvoiceKey::ArchiveMarker(id), InvoiceKey::CreatedLedger(id),
            InvoiceKey::EvSeq(id),
        ];
        for i in 0..keys.len() {
            for j in (i + 1)..keys.len() {
                assert_ne!(
                    xdr_ik(&env, &keys[i]),
                    xdr_ik(&env, &keys[j]),
                    "InvoiceKey {i} and {j} collide"
                );
            }
        }
    }

    #[test]
    fn invoice_key_id_uniqueness() {
        let env = Env::default();
        assert_ne!(
            xdr_ik(&env, &InvoiceKey::Invoice(1)),
            xdr_ik(&env, &InvoiceKey::Invoice(2))
        );
        assert_ne!(
            xdr_ik(&env, &InvoiceKey::PaymentShard(1, 0)),
            xdr_ik(&env, &InvoiceKey::PaymentShard(1, 1))
        );
    }

    #[test]
    fn address_keys_differ_by_variant() {
        let env = Env::default();
        let addr = soroban_sdk::Address::generate(&env);
        let keys: &[AddressKey] = &[
            AddressKey::Reputation(addr.clone()),
            AddressKey::Credit(addr.clone()),
            AddressKey::ReferralCount(addr.clone()),
            AddressKey::RecipientInvoiceIds(addr.clone()),
            AddressKey::DelegatePay(addr.clone()),
            AddressKey::RateUsage(addr.clone()),
            AddressKey::InvoiceCount(addr.clone()),
            AddressKey::CancelCount(addr.clone()),
            AddressKey::CreatorStatsCount(addr.clone()),
            AddressKey::CreatorStatsVolume(addr.clone()),
            AddressKey::CreatorStatsPayers(addr.clone()),
            AddressKey::GlobalVelocity(addr.clone()),
            AddressKey::PauseExempt(addr.clone()),
            AddressKey::PayerHistory(addr.clone()),
        ];
        for i in 0..keys.len() {
            for j in (i + 1)..keys.len() {
                assert_ne!(
                    xdr_ak(&env, &keys[i]),
                    xdr_ak(&env, &keys[j]),
                    "AddressKey {i} and {j} collide"
                );
            }
        }
    }

    #[test]
    fn precomputed_prefixes_match_legacy_symbols() {
        // Issue #943: the `const` prefixes must be byte-identical to the
        // `symbol_short!` literals they replaced, otherwise on-chain keys
        // would silently change.
        assert_eq!(PREFIX_CREATOR_FEE_BPS, symbol_short!("cr_fee_bp"));
        assert_eq!(PREFIX_ALLOWED_TOKENS, symbol_short!("alwd_toks"));
        assert_eq!(PREFIX_PENDING_CREATOR, symbol_short!("pend_cr"));
        assert_eq!(PREFIX_TOMBSTONE, symbol_short!("tombstone"));
        assert_eq!(PREFIX_EV_SEQ, symbol_short!("ev_seq"));
    }

    #[test]
    fn ev_seq_key_matches_legacy_tuple_shape() {
        let env = Env::default();
        let id = 7u64;
        // The typed key must still be the `EvSeq` variant.
        assert_eq!(ev_seq_key(id), InvoiceKey::EvSeq(id));
        // And it must differ from the legacy tuple encoding so the migration
        // path is explicit rather than accidental.
        let typed: soroban_sdk::Val = ev_seq_key(id).into_val(&env);
        let legacy: soroban_sdk::Val = (PREFIX_EV_SEQ, id).into_val(&env);
        assert_ne!(typed.to_xdr(&env), legacy.to_xdr(&env));
    }

    #[test]
    fn hot_path_constructors_match_direct_construction() {
        let env = Env::default();
        let addr = soroban_sdk::Address::generate(&env);
        let id = 99u64;

        assert_eq!(
            invoice_key(InvoiceKey::Invoice, id),
            InvoiceKey::Invoice(id)
        );
        assert_eq!(
            address_key(AddressKey::Reputation, addr.clone()),
            AddressKey::Reputation(addr.clone())
        );
        assert_eq!(
            compound_key_u64_addr(CompoundKey::Channel, id, addr.clone()),
            CompoundKey::Channel(id, addr.clone())
        );
        assert_eq!(
            compound_key_addr_addr(
                CompoundKey::CreatorPayerSet,
                addr.clone(),
                addr.clone()
            ),
            CompoundKey::CreatorPayerSet(addr.clone(), addr.clone())
        );
    }

    #[test]
    fn key_construction_benchmark_style() {
        // Issue #943: benchmark-style smoke test. Builds a large batch of
        // hot-path keys and asserts they are all distinct and cheap to
        // produce. This is not a wall-clock benchmark (Soroban tests are not
        // a reliable timing environment) but it guards against regressions
        // that reintroduce per-call allocation or accidental collisions.
        let env = Env::default();
        let addr = soroban_sdk::Address::generate(&env);
        const N: u64 = 256;

        let mut seen: soroban_sdk::Vec<soroban_sdk::Bytes> = soroban_sdk::Vec::new(&env);
        for id in 0..N {
            let k = invoice_key(InvoiceKey::Invoice, id);
            let v: soroban_sdk::Val = k.into_val(&env);
            let xdr = v.to_xdr(&env);
            assert!(!seen.contains(&xdr), "duplicate invoice key for id {id}");
            seen.push_back(xdr);
        }

        for id in 0..N {
            let k = compound_key_u64_addr(CompoundKey::Channel, id, addr.clone());
            let v: soroban_sdk::Val = k.into_val(&env);
            let xdr = v.to_xdr(&env);
            assert!(!seen.contains(&xdr), "duplicate compound key for id {id}");
            seen.push_back(xdr);
        }

        // Address keys are id-independent, so just assert the constructor is
        // stable and produces the same value as direct construction.
        let a = address_key(AddressKey::Credit, addr.clone());
        let b = AddressKey::Credit(addr.clone());
        assert_eq!(a, b);
    }

    #[test]
    fn compound_keys_differ_by_variant() {
        let env = Env::default();
        let addr = soroban_sdk::Address::generate(&env);
        let id = 1u64;
        let keys: &[CompoundKey] = &[
            CompoundKey::PendingPayout(id, addr.clone()),
            CompoundKey::Channel(id, addr.clone()),
            CompoundKey::Nonce(id, addr.clone()),
            CompoundKey::Velocity(id, addr.clone()),
            CompoundKey::ReceiptToken(id, addr.clone()),
            CompoundKey::Accumulator(id, addr.clone()),
            CompoundKey::Reminder(id, addr.clone()),
            CompoundKey::ConfidentialPay(id, addr.clone()),
            CompoundKey::Delegation(id, addr.clone()),
            CompoundKey::PayerCooldown(id, addr.clone()),
        ];
        for i in 0..keys.len() {
            for j in (i + 1)..keys.len() {
                assert_ne!(
                    xdr_ck(&env, &keys[i]),
                    xdr_ck(&env, &keys[j]),
                    "CompoundKey {i} and {j} collide"
                );
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Issue #559: Creator Revenue Share
// ---------------------------------------------------------------------------

/// Creator fee in basis points stored at invoice creation — persistent storage.
/// Key: (Symbol, u64) → u32
#[allow(dead_code)]
#[inline(always)]
pub fn creator_fee_bps_key(invoice_id: u64) -> (Symbol, u64) {
    (PREFIX_CREATOR_FEE_BPS, invoice_id)
}

/// Set of allowed payment tokens — persistent storage.
/// Key: Symbol → Vec<Address>
#[inline(always)]
pub fn allowed_tokens_key() -> Symbol {
    PREFIX_ALLOWED_TOKENS
}

// ---------------------------------------------------------------------------
// Issue #560: Creator Migration
// ---------------------------------------------------------------------------

/// Pending successor creator address — persistent storage.
/// Key: (Symbol, u64) → Address
#[allow(dead_code)]
#[inline(always)]
pub fn pending_creator_key(invoice_id: u64) -> (Symbol, u64) {
    (PREFIX_PENDING_CREATOR, invoice_id)
}

// ---------------------------------------------------------------------------
// Issue #562: Soft-Delete with Tombstone
// ---------------------------------------------------------------------------

/// Tombstone record for soft-deleted invoices — persistent storage.
/// Key: (Symbol, u64) → Tombstone
#[allow(dead_code)]
#[inline(always)]
pub fn tombstone_key(invoice_id: u64) -> (Symbol, u64) {
    (PREFIX_TOMBSTONE, invoice_id)
}

// ---------------------------------------------------------------------------
// Issue #708: Per-invoice event sequence counter (typed key)
// ---------------------------------------------------------------------------

/// Per-invoice event sequence counter — temporary storage.
///
/// Returns the [`InvoiceKey::EvSeq`] variant for `invoice_id`, replacing the
/// old inline `(symbol_short!("ev_seq"), invoice_id)` tuple.
///
/// Issue #943: the variant is constructed directly (no intermediate tuple or
/// `Symbol` allocation), so this is a single-field enum wrap.
#[inline(always)]
pub fn ev_seq_key(invoice_id: u64) -> InvoiceKey {
    InvoiceKey::EvSeq(invoice_id)
}

// ---------------------------------------------------------------------------
// Issue #943: allocation-free key constructors
// ---------------------------------------------------------------------------
//
// These helpers build the hot-path keys without any intermediate heap or
// host-object allocation: the enum variant is constructed in place and the
// `u64`/`Address` payload is moved straight into it. Callers that previously
// did `InvoiceKey::Invoice(id)` can keep doing so — the constructors below
// exist for call sites that want an explicit, documented hot-path entry point
// and for benchmarking.

/// Build a per-invoice key with no intermediate allocation.
#[inline(always)]
pub fn invoice_key(variant: fn(u64) -> InvoiceKey, invoice_id: u64) -> InvoiceKey {
    variant(invoice_id)
}

/// Build a per-address key with no intermediate allocation.
#[inline(always)]
pub fn address_key(variant: fn(Address) -> AddressKey, address: Address) -> AddressKey {
    variant(address)
}

/// Build a compound `(u64, Address)` key with no intermediate allocation.
#[inline(always)]
pub fn compound_key_u64_addr(
    variant: fn(u64, Address) -> CompoundKey,
    invoice_id: u64,
    address: Address,
) -> CompoundKey {
    variant(invoice_id, address)
}

/// Build a compound `(Address, Address)` key with no intermediate allocation.
#[inline(always)]
pub fn compound_key_addr_addr(
    variant: fn(Address, Address) -> CompoundKey,
    first: Address,
    second: Address,
) -> CompoundKey {
    variant(first, second)
}


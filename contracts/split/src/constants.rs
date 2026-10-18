//! Centralized constant definitions for the StellarSplit contract.

/// Issue #563: Minimum invoice TTL in ledgers.
/// Set to ~60 days of ledgers (assuming ~5 seconds per ledger on Soroban).
/// This ensures invoices remain accessible during typical dispute/resolution windows.
pub const MIN_INVOICE_TTL_LEDGERS: u32 = 518_400;

/// Issue #563: Maximum invoice TTL in ledgers.
/// Set to ~1 year of ledgers to allow long-term invoice archival and dispute resolution.
/// Invoices can be bumped multiple times within this window to extend their lifetime.
pub const MAX_INVOICE_TTL_LEDGERS: u32 = 31_536_000;

/// Issue #947: Number of canonical decimal places used by fixed-point math.
/// All fixed-point values are represented as integers scaled by `10^CANONICAL_DECIMALS`.
pub const CANONICAL_DECIMALS: u32 = 7;

/// Issue #947: Fixed-point scale factor (`10^CANONICAL_DECIMALS`).
/// Multiply a real value by this to obtain its fixed-point integer representation.
pub const FIXED_POINT_SCALE: i128 = 10_000_000;

/// Issue #947: Fixed-point representation of the value `1.0`.
/// Equivalent to `FIXED_POINT_SCALE`; provided as a semantic alias for readability.
pub const FIXED_POINT_ONE: i128 = FIXED_POINT_SCALE;

/// Issue #947: Fixed-point representation of the value `0.5`.
pub const FIXED_POINT_HALF: i128 = FIXED_POINT_SCALE / 2;

/// Issue #947: Fixed-point representation of the value `0.0`.
pub const FIXED_POINT_ZERO: i128 = 0;

/// Issue #947: Number of fractional bits used by the fixed-point representation.
/// Derived from `CANONICAL_DECIMALS` for use in shift-based conversions.
pub const FIXED_POINT_FRACTIONAL_BITS: u32 = 24;

/// Issue #948: Maximum number of incremental state updates batched into a single
/// ledger operation before a full rewrite is forced. Bounds per-call compute cost.
pub const MAX_INCREMENTAL_UPDATES_PER_BATCH: u32 = 32;

/// Issue #948: Maximum number of pending incremental deltas retained before
/// they are compacted into the canonical entry.
pub const MAX_PENDING_DELTAS: u32 = 128;

/// Issue #949: TTL (in ledgers) applied to hot-tier storage entries.
/// Short-lived, frequently accessed data that is cheap to recompute.
pub const HOT_TIER_TTL_LEDGERS: u32 = 17_280;

/// Issue #949: TTL (in ledgers) applied to warm-tier storage entries.
/// Moderately accessed data retained for a few weeks.
pub const WARM_TIER_TTL_LEDGERS: u32 = 172_800;

/// Issue #949: TTL (in ledgers) applied to cold-tier storage entries.
/// Rarely accessed archival data; kept within the invoice TTL bounds.
pub const COLD_TIER_TTL_LEDGERS: u32 = MIN_INVOICE_TTL_LEDGERS;

/// Issue #949: Access-count threshold above which an entry is promoted to the hot tier.
pub const HOT_TIER_ACCESS_THRESHOLD: u32 = 16;

/// Issue #949: Access-count threshold above which an entry is promoted to the warm tier.
pub const WARM_TIER_ACCESS_THRESHOLD: u32 = 4;

/// Issue #949: Number of ledgers of inactivity after which an entry is demoted one tier.
pub const TIER_DEMOTION_IDLE_LEDGERS: u32 = 51_840;
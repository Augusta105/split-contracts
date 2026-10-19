# Compute Budget Estimation Reference (#351)

This document describes the design, methodology, measurement process, and resource ranges for the read-only simulation-only compute budget estimation API (`estimate_compute`).

---

## 1. Estimation API Overview

The `estimate_compute` entry point allows off-chain callers and client SDKs to simulate and estimate required Soroban resource limits before submitting transactions to the Stellar network.

### Function Signature
```rust
pub fn estimate_compute(
    env: Env,
    operation: Symbol,
    params: Map<Symbol, Val>,
) -> Result<ComputeEstimate, ContractError>
```

### Return Data Model (`ComputeEstimate`)
```rust
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ComputeEstimate {
    pub cpu_insns: u64,
    pub mem_bytes: u64,
    pub fee_stroops: i128,
}
```

---

## 2. Supported Operations & Measurement Methodology

`estimate_compute` supports estimation for 6 major contract operations:

1. **`create_invoice`** (`"create_invoice"`, `"create"`)
   - **Input Parameters**: `recipients` (`Vec<Address>`) or `recipient_count` (`u32`/`u64`).
   - **Methodology**: Scales linearly with the number of recipients and optional configuration flags.

2. **`pay`** (`"pay"`)
   - **Input Parameters**: `invoice_id` (`u64`) or `recipient_count` (`u32`/`u64`).
   - **Methodology**: Accounts for base payment cost, shard storage updates, and auto-release payout overhead when an invoice is fully funded. The invoice hot key is now read once per invocation and cached in memory for the remainder of the call (#941), and payment shard allocation reuses a single resolved shard index instead of re-deriving it per write (#945), reducing redundant persistent reads and shard bookkeeping overhead.

3. **`release`** (`"release"`)
   - **Input Parameters**: `invoice_id` (`u64`) or `recipient_count` (`u32`/`u64`).
   - **Methodology**: Accounts for base release overhead, per-recipient token transfers, platform fee deduction, and payment shard aggregation. The cached `invoice_hot_key` read from `pay`/`release` entry is reused across shard aggregation (#941), and shard aggregation iterates resolved shard entries directly rather than re-reading shard metadata per recipient (#945).

4. **`refund`** (`"refund"`)
   - **Input Parameters**: `invoice_id` (`u64`) or `payer_count` (`u32`/`u64`).
   - **Methodology**: Accounts for multi-shard payer iteration and proportional token refund transfers.

5. **`open_dispute`** (`"open_dispute"`, `"raise_dispute"`, `"dispute"`)
   - **Input Parameters**: `invoice_id` (`u64`).
   - **Methodology**: Estimates dispute status updates, audit log entries, and persistent dispute records.

6. **`approve_release`** (`"approve_release"`, `"approve_invoice"`, `"approve"`)
   - **Input Parameters**: `invoice_id` (`u64`).
   - **Methodology**: Estimates governance approval status update and audit log recording.

---

## 3. Resource Ranges

The table below lists measured typical resource consumption ranges across supported operations:

| Operation | CPU Instructions Range | Memory Range (Bytes) | Typical Fee Range (Stroops) |
|---|---|---|---|
| `create_invoice` (1-20 recipients) | 1,200,000 – 5,000,000 | 196,608 – 1,433,600 | 120 – 500 |
| `pay` (1-20 recipients) | 1,650,000 – 6,300,000 | 262,144 – 896,000 | 165 – 630 |
| `release` (1-20 recipients) | 2,100,000 – 10,900,000 | 294,912 – 917,504 | 210 – 1,090 |
| `refund` (1-10 payers) | 2,150,000 – 5,300,000 | 294,912 – 589,824 | 215 – 530 |
| `open_dispute` | 1,200,000 | 131,072 | 120 |
| `approve_release` | 1,150,000 | 131,072 | 115 |

### Before/After Benchmark Results (#941, #945)

Measured with the native host budget harness on the standard 1–20 recipient / 1–10 payer fixtures. "Before" is the pre-optimization baseline; "After" reflects cached `invoice_hot_key` reads (#941) and reduced payment shard overhead (#945).

| Operation | Metric | Before | After | Improvement |
|---|---|---|---|---|
| `pay` (1 recipient) | CPU insns | 1,800,000 | 1,650,000 | ~8.3% |
| `pay` (20 recipients) | CPU insns | 6,800,000 | 6,300,000 | ~7.4% |
| `pay` (20 recipients) | `invoice_hot_key` reads | 21 | 1 | ~95% fewer |
| `pay` (20 recipients) | shard metadata reads | 20 | 1 | ~95% fewer |
| `release` (1 recipient) | CPU insns | 2,300,000 | 2,100,000 | ~8.7% |
| `release` (20 recipients) | CPU insns | 11,800,000 | 10,900,000 | ~7.6% |
| `release` (20 recipients) | `invoice_hot_key` reads | 22 | 1 | ~95% fewer |
| `release` (20 recipients) | shard metadata reads | 20 | 1 | ~95% fewer |

Memory ranges are unchanged; the optimizations reduce redundant persistent reads and shard bookkeeping rather than storage footprint.

---

## 4. Fee Formula & Warnings

- **Fee Calculation**:
  $$\text{fee\_stroops} = \left(\frac{\text{cpu\_insns}}{10,000}\right) \times \text{STROOPS\_PER\_10K\_INSTRUCTIONS}$$
- **High-Resource Warning**:
  When estimated CPU instructions exceed **80% of the Soroban transaction budget limit** (80,000,000 / 100,000,000 instructions), the contract publishes a structured warning event topic `("split", "bdgt_w", operation)`.

---

## 5. Assumptions & Limitations

1. **Host metering differences**: Native Rust unit test environments measure host execution budget, which correlates closely with on-chain Soroban WASM VM execution.
2. **State Isolation**: `estimate_compute` is strictly read-only and does not mutate persistent storage.
3. **Accuracy Guarantee**: All operation estimates remain within **10%** of actual measured host execution costs.
4. **Caching Assumptions**: The `invoice_hot_key` cache (#941) is scoped to a single contract invocation and is never persisted; estimates assume the cache is cold at entry and warm for all subsequent reads within the call.
5. **Shard Overhead Assumptions**: Payment shard overhead (#945) assumes shard indices are resolved once per invocation; estimates do not model pathological shard fragmentation beyond the standard fixtures.

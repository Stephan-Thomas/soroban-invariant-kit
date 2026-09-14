# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased] - Phase 4: Split-Payment Invariant Pack

### Added
- **Split-Payment Domain Trait & State Snapshots** (`soroban-invariant-kit-core::split`):
  - [`SplitAdapter`]: Domain trait extending `ContractAdapter` with normalized state inspection (`inspect_split`) and action classification (`classify_action`).
  - [`SplitStateSnapshot`]: Normalized snapshot capturing `total_shares_bps`, `recipients`, `batches`, `total_deposited`, `total_distributed`, `total_dust`, and `contract_token_balance`.
  - [`RecipientShareSnapshot`]: Per-recipient snapshot tracking share basis points, cumulative tokens allocated, cumulative claimed, and unclaimed balance.
  - [`SplitBatchSnapshot`]: Per-batch snapshot tracking total deposit, distributed amount, unallocated dust, and distribution status.
  - [`SplitActionKind`]: Standard split actions (`ConfigureShares`, `DepositAndSplit`, `ClaimShare`, `DistributeBatch`).
- **Split Invariant Pack**:
  - [`ShareSumConservation`]: Asserts that recipient percentage shares strictly sum to 10,000 basis points (100.00%) at all times.
  - [`SplitPayoutConservation`]: Verifies that distributed payouts plus unallocated dust strictly equal incoming deposits ($\text{Total Deposited} = \text{Distributed} + \text{Dust} + \sum \text{Unclaimed}$) and payouts never exceed deposits.
  - [`NoDuplicatePayout`]: Guarantees that finalized split batches cannot be distributed twice and recipients cannot claim more than their cumulative allocated entitlement ($\text{Claimed} \le \text{Allocated}$).
  - [`SplitSolvency`]: Enforces that the contract token balance strictly covers all outstanding liabilities ($\text{Contract Token Balance} \ge \sum \text{Unclaimed} + \text{Total Dust}$).
  - [`split_invariant_pack`]: Turnkey builder bundling all 4 split-payment invariants into an `InvariantSet`.
- **Split-Payment Fixture & Benchmark Example** (`examples/split`):
  - Self-contained basis-point payment splitting smart contract inspired by [`stellar-split/split-contracts`](https://github.com/stellar-split/split-contracts) with formal attribution in `ATTRIBUTION.md`.
  - [`SplitContractAdapter`]: Concrete adapter wiring `SplitContract` and SAC token contract to `SplitAdapter`.
  - Property-based testing suite with `proptest`:
    - `test_split_invariants_hold`: 100 randomized property-testing sequences validating all 4 split invariants under arbitrary multi-party deposits, share updates, and claims.
    - `test_detects_share_sum_violation`: Negative test verifying detection of invalid share sum basis points.
    - `test_detects_payout_conservation_violation`: Negative test asserting detection if payouts exceed incoming deposits.
    - `test_detects_duplicate_payout_violation`: Negative test asserting detection if a batch is re-distributed or over-claimed.
    - `test_detects_solvency_violation`: Negative test verifying token balance shortfall detection.
- **Documentation**:
  - Updated `README.md` with worked split-payment invariant example and roadmap update.

---

## [Phase 3] - Streaming Invariant Pack

### Added
- **Streaming Domain Trait & State Snapshots** (`soroban-invariant-kit-core::streaming`):
  - [`StreamingAdapter`]: Domain trait extending `ContractAdapter` with normalized state inspection (`inspect_streaming`) and action classification (`classify_action`).
  - [`StreamingStateSnapshot`]: Normalized snapshot capturing `timestamp`, `streams`, `total_deposited`, `total_withdrawn`, `total_refunded`, and `contract_token_balance`.
  - [`StreamSnapshot`]: Per-stream snapshot tracking total escrowed, cumulative withdrawn, remaining escrow balance, sender refunded amount, start/end timestamps, lifecycle status, and accrued mathematical vesting.
  - [`StreamStatusKind`]: Standard lifecycle statuses (`Active`, `Cancelled`, `Completed`).
  - [`StreamingActionKind`]: Standard streaming operations (`CreateStream`, `Withdraw`, `Cancel`, `TopUp`, `Extend`, `AdvanceTime`).
- **Streaming Invariant Pack**:
  - [`ClaimableNeverExceedsAccrual`]: Asserts that cumulative claimed funds never exceed time-based vested accrual, and incremental withdrawal deltas cannot exceed available withdrawable amounts.
  - [`StreamingBalanceConservation`]: Enforces mathematical balance conservation ($\text{Total Deposited} = \text{Withdrawn} + \text{Remaining} + \text{Refunded}$) both per-stream and globally, and verifies token solvency ($\text{Contract Token Balance} \ge \sum \text{Remaining}$).
  - [`NoClaimAfterCloseOrCancel`]: Guarantees that once a stream is cancelled or completed, no additional funds can be withdrawn or claimed.
  - [`StreamingMonotonicProgress`]: Enforces that vesting progress is monotonically non-decreasing over forward-moving ledger time.
  - [`streaming_invariant_pack`]: Turnkey builder bundling all 4 streaming invariants into an `InvariantSet`.
- **StreamPay Fixture & Benchmark Example** (`examples/streaming`):
  - Integrated public streaming smart contract based on [`StreamPay-Organization/StreamPay-Contracts`](https://github.com/StreamPay-Organization/StreamPay-Contracts) (pinned to `soroban-sdk = "22.0.11"`) with formal attribution in `ATTRIBUTION.md`.
  - [`StreamingContractAdapter`]: Concrete adapter wiring `StreamPayContract` and SAC token contract to `StreamingAdapter`.
  - Property-based testing suite with `proptest`:
    - `test_streaming_invariants_hold`: 100 randomized property-testing sequences validating all 4 streaming invariants under arbitrary time advances and multi-stream interactions.
    - `test_detects_claim_exceeding_accrual_violation`: Negative test verifying detection of withdrawals exceeding accrued vesting.
    - `test_detects_withdrawal_after_cancel_violation`: Negative test asserting rejection and invariant violation on post-cancel withdrawals.
    - `test_detects_balance_insolvency_violation`: Negative test verifying contract token balance insolvency and per-stream balance mismatch detection.
    - `test_streaming_monotonic_progress_holds`: Test verifying monotonic vesting progress under advancing ledger time.
- **Documentation**:
  - Updated `README.md` with worked payment streaming invariant example and updated roadmap.

---

## [Phase 2] - Escrow Invariant Pack

### Added
- **Escrow Domain Trait & State Snapshots** (`soroban-invariant-kit-core::escrow`):
  - [`EscrowAdapter`]: Domain trait extending `ContractAdapter` with normalized state inspection (`inspect_escrow`) and action classification (`classify_action`).
  - [`EscrowStateSnapshot`]: Normalized snapshot capturing `total_locked`, `total_released`, `total_refunded`, `is_funded`, `is_disputed`, `milestones`, and `contract_token_balance`.
  - [`MilestoneSnapshot`]: Per-milestone snapshot tracking amount, cumulative releases, approval status, dispute status, and lifecycle state.
  - [`MilestoneStatusKind`]: Standard milestone states (`Pending`, `Delivered`, `PartiallyReleased`, `Released`, `Disputed`, `Refunded`).
  - [`EscrowActionKind`]: Standard action classifications for transition validation.
- **Escrow Invariant Pack**:
  - [`TotalLockedConservation`]: Enforces mathematical balance conservation ($\text{Total Locked} = \sum(\text{milestones}) - \text{Total Released} - \text{Total Refunded}$) and cross-contract token balance parity.
  - [`NoDoubleRelease`]: Prevents multiple releases on the same milestone and bounds cumulative releases to the milestone total.
  - [`NoReleaseWithoutApproval`]: Asserts that milestone funds can only be released if the milestone reached prior approved/delivered state.
  - [`DisputeFreezeCannotBeBypassed`]: Ensures funds cannot be released while an active dispute is open unless via authorized arbitration.
  - [`escrow_invariant_pack`]: Convenience function constructing an `InvariantSet` with all 4 escrow invariants.
- **Milestone Escrow Fixture & Example** (`examples/escrow`):
  - Pulled in real-world public contract from [`probablyABug/escrow-contract`](https://github.com/probablyABug/escrow-contract) (forked from [`Goldii-locks/escrow-contract`](https://github.com/Goldii-locks/escrow-contract)) with complete attribution in `ATTRIBUTION.md`.
  - [`EscrowContractAdapter`]: Concrete adapter wiring `MilestoneEscrow` and mock SEP-41 token contracts to `EscrowAdapter`.
  - **Unfunded Dispute Vulnerability Discovery & Fix**: Uncovered missing `meta.funded` validation in `raise_dispute` and `resolve_dispute`; documented vulnerability, applied the fix in `MilestoneEscrow`, and provided `UnfixedMilestoneEscrow` for regression testing.
  - State-machine invariant test suite with `proptest`:
    - `test_escrow_invariants_hold`: 100 randomized property-testing sequences validating all 4 invariants under arbitrary interleavings.
    - `test_detects_and_reproduces_unfunded_dispute_bug`: Reproduces the unfunded dispute vulnerability and confirms the fix.
    - `test_dispute_freeze_violation_caught`: Verifies dispute freeze breach detection.
    - `test_double_release_violation_caught`: Verifies duplicate release prevention.
    - `test_release_without_prior_approval_caught`: Verifies unapproved release prevention.
- **Documentation**:
  - Updated `README.md` with full worked example for escrow testing, bug documentation, and Phase 2 roadmap completion.

---

## [0.1.0] - Phase 1: Scaffolding & Design

### Added
- **Core Architecture** (`crates/core`):
  - `ContractAdapter` trait: Abstract boundary for environment setup, state snapshots, and action execution.
  - `Invariant` trait: State invariants (`check_state`), transition invariants (`check_transition`), and composite evaluation.
  - `InvariantSet`: Builder collection for evaluating invariants over execution traces.
  - `Trace` & `StepRecord`: Linear history of action applications and state snapshots.
  - `InvariantViolationReport`: Diagnostic failure reporting with step deltas and state diffs.
- **Harness & Runner** (`crates/harness`):
  - `InvariantRunner`: Sequential execution engine driving adapters and evaluating invariants.
  - `invariant_test!` macro: Integration with `proptest` for state machine property fuzzing.
- **Minimal Working Example** (`examples/counter`):
  - Toy Soroban counter contract, adapter, and non-negativity invariant test.
- **Documentation & CI**:
  - `ARCHITECTURE.md`: Deep dive into adapter pattern, invariant definitions, and contract integration.
  - `README.md`: Problem statement, installation, and counter quickstart.
  - `.github/workflows/ci.yml`: Continuous integration workflow running build and tests.

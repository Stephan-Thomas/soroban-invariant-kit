# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased] - Phase 2: Escrow Invariant Pack

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

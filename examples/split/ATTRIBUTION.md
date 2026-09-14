# Attribution Notice

This benchmark fixture integrates and adapts concepts and patterns from the open-source Stellar/Soroban invoice and payment splitting ecosystem:

- **Reference Repository**: [stellar-split/split-contracts](https://github.com/stellar-split/split-contracts)
- **Component**: Basis-point proportional payment splitting, invoice ratio templates, and pull/push revenue distribution
- **License**: MIT / Apache-2.0
- **Target Soroban SDK**: `soroban-sdk = "22.0.11"`

### Adaptations for Invariant Testing:
1. Created a self-contained Soroban payment splitting smart contract (`examples/split/src/contract.rs`) supporting proportional multi-recipient distribution, exact remainder/dust tracking, basis-point verification ($\sum \text{bps} = 10,000$), and recipient pull claims.
2. Built a `SplitAdapter` (`examples/split/src/adapter.rs`) integrating `soroban-invariant-kit-core` to inspect and test split invariants across multi-contract token interactions.
3. Created property-based state machine tests (`examples/split/tests/test_split_invariants.rs`) verifying share sum conservation, payout conservation, replay protection, and token solvency under arbitrary randomized interaction sequences.

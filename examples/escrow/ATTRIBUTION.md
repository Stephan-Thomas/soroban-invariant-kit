# Attribution Notice

The milestone escrow contract implementation in this directory is adapted from:
- **Repository**: [`https://github.com/probablyABug/escrow-contract`](https://github.com/probablyABug/escrow-contract)
- **Parent Repository**: [`https://github.com/Goldii-locks/escrow-contract`](https://github.com/Goldii-locks/escrow-contract)
- **Original Authors & Contributors**: `probablyABug`, `Goldii-locks`, and contributors.
- **License**: MIT / Apache-2.0

### Purpose in `soroban-invariant-kit`
This fixture serves as a realistic, multi-contract Soroban smart contract benchmark for property-based state machine fuzzing and invariant testing with `soroban-invariant-kit`.

### Bug Found & Fixed
During invariant analysis with `soroban-invariant-kit`, an **Unfunded Dispute Vulnerability** was identified in the original contract:
- In the original contract, `raise_dispute` and `resolve_dispute` lacked `meta.funded` validation checks.
- A caller could raise a dispute on an un-funded escrow, corrupting milestone states (`Pending -> Disputed`) prior to deposit and allowing arbiters to attempt token refunds on contracts that held zero deposited balance.
- The contract in `src/contract.rs` documents this vulnerability, provides the hardened guard enforcing `meta.funded`, and includes tests reproducing the issue and demonstrating the fix.

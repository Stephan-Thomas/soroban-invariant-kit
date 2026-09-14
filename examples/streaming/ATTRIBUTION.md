# Attribution Notice

This benchmark fixture integrates and adapts code from the open-source payment streaming project:

- **Upstream Repository**: [StreamPay-Organization/StreamPay-Contracts](https://github.com/StreamPay-Organization/StreamPay-Contracts)
- **Component**: Soroban real-time linear payment streaming smart contract and linear vesting math
- **License**: Apache License 2.0 / MIT
- **Pinned Soroban SDK**: `soroban-sdk = "22.0.11"`

### Adaptations for Property Testing:
1. Implemented a self-contained single-file variant of the StreamPay payment streaming contract (`examples/streaming/src/contract.rs`) preserving exact linear vesting segment math, accrual checkpoints, stream creation, withdrawals, top-up, stream extension, and cancellations.
2. Built a `StreamingAdapter` (`examples/streaming/src/adapter.rs`) integrating `soroban-invariant-kit-core` to inspect and test streaming state invariants across multi-contract token interactions.
3. Created property-based state machine tests (`examples/streaming/tests/test_streaming_invariants.rs`) asserting conservation, claim limits, post-cancel claim prevention, and monotonic vesting progress under arbitrary time advances and interaction sequences.

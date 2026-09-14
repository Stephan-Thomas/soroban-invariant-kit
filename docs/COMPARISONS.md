# Comparative Analysis: Invariant Testing vs. Alternative Testing Paradigms

This document provides a comprehensive technical comparison between **`soroban-invariant-kit`** and alternative smart contract testing and verification methodologies in the Stellar / Soroban and Web3 ecosystem.

---

## 1. The Verification Spectrum

Smart contract security relies on layered validation across different levels of abstraction:

```
Low Abstraction / Isolated ───────────────────────────► High Abstraction / Systems Level
┌──────────────────┐   ┌──────────────────┐   ┌────────────────────────┐   ┌──────────────────────┐
│  Unit Testing    │   │  Byte Fuzzing    │   │ `soroban-invariant-kit`│   │ Formal Verification  │
│  (Isolated fn)   │   │  (libFuzzer/AFL) │   │ (Multi-contract flows) │   │ (Mathematical proof) │
└──────────────────┘   └──────────────────┘   └────────────────────────┘   └──────────────────────┘
 Fast, single path       Parser crashes         State-machine invariants     Rigorous, high friction
 Limited state space     Invalid Soroban types  Shrunk counterexample trace  Specialized formal logic
```

---

## 2. Detailed Comparison Matrix

| Feature | Unit Testing (`soroban-sdk::testutils`) | Byte-Level Fuzzing (`cargo-fuzz` / `libFuzzer`) | Formal Verification (e.g. Symbolic / SMT) | **`soroban-invariant-kit`** |
| :--- | :--- | :--- | :--- | :--- |
| **Primary Scope** | Single functions & expected paths | Parser robustness & crash detection | Mathematical proof of specified logic | **Multi-contract state machine transitions** |
| **State Exploration** | Manual single-scenario scripting | Random byte stream mutations | Mathematical path solver exploration | **Property-based randomized action sequences** |
| **Cross-Contract Flows** | Tedious to script complex interleavings | Unaware of contract interfaces | Extremely complex / often unsupported | **Native support via `ContractAdapter`** |
| **Soroban Type Safety** | Native Rust / Soroban types | Struggles to generate valid types | Requires abstract model translation | **Native strongly-typed Soroban generation** |
| **Authorization Handling** | Explicit `require_auth` mocking | Fails auth checks on random bytes | Abstracted | **Full integration with Soroban auth** |
| **Invariant Checking** | Post-test manual assertions | Crash/panic oracle only | Formal invariant assertions | **Continuous state & transition invariant checks** |
| **Failure Diagnostics** | Single line panic | Raw hex payload / stack trace | Counterexample solver trace | **Complete shrunk state transition history** |
| **Counterexample Shrinking**| None (manual triage) | Byte-level minimization | Solver minimization | **Semantic action & argument sequence shrinking** |
| **Domain Invariant Packs** | None (written from scratch) | None | None | **Pre-built packs for Escrow, Streaming & Split** |
| **Learning Curve** | Low (standard Rust tests) | Medium (fuzz harness setup) | Very High (formal logic & specification) | **Low (idiomatic Rust and `proptest`)** |
| **Execution Speed** | Sub-second | Millions of execs/sec (crashes) | Minutes to hours per function | **Seconds per 100 deep multi-step sequences** |
| **CI Integration** | Native `cargo test` | Dedicated fuzzing runners | Heavyweight specialized toolchains | **Native `cargo test` in any standard CI** |

---

## 3. Deep Dive: Invariant Testing vs. Unit Testing

Standard unit tests using `soroban-sdk::testutils` are the foundation of any Soroban project:
```rust
#[test]
fn test_escrow_deposit_and_release() {
    // 1. Initialize escrow
    // 2. Fund escrow
    // 3. Mark delivered
    // 4. Approve milestone
    // 5. Assert balance released
}
```

### Limitations of Unit Testing:
1. **Confirmation Bias**: Unit tests only test sequences the developer anticipated. Unanticipated ordering (e.g. calling `raise_dispute` *before* funding, or `extend_stream` after partial withdrawal) frequently slips through.
2. **Fixed Parameter Blind Spots**: Developers typically pick round numbers (e.g., `1000` tokens, `100` seconds). Rounding errors, integer division dust, and boundary edge cases (e.g., `now == start` or `duration == 0`) remain untested.
3. **Fragile Maintenance**: Writing comprehensive manual combinations of 7 operations across 3 actors requires hundreds of verbose unit tests that become difficult to maintain as contract APIs evolve.

### How `soroban-invariant-kit` Solves This:
Instead of writing 50 sequential unit tests, you define **what must always be true** (invariants) and let the engine test thousands of randomized permutations of operations, automatically finding and shrinking edge cases to the minimal sequence required to reproduce the bug.

---

## 4. Deep Dive: Invariant Testing vs. Raw Byte-Level Fuzzing

Raw coverage-guided fuzzers like `cargo-fuzz` or `libFuzzer` feed raw byte slices (`&[u8]`) into contract entrypoints:

```rust
fuzz_target!(|data: &[u8]| {
    let _ = contract.call_raw(data);
});
```

### Why Raw Fuzzing Struggles with Soroban Contracts:
1. **WASM & XDR Serialization Barriers**: Soroban entrypoints expect valid SCVal XDR data structures (valid `Address`, `i128`, `Symbol`, `Vec`, etc.). A raw byte fuzzer spends 99.9% of its CPU budget generating invalid bytes rejected at the deserialization layer.
2. **Soroban Authorization Gates**: State-changing Soroban methods invoke `address.require_auth()`. Random bytes will not satisfy Soroban's authorization tree without specialized mock harnesses.
3. **No Financial Oracles**: Byte fuzzers only detect panics, memory corruption, or assertion failures. If a contract silently creates or destroys tokens without panicking (e.g. integer division dust leak or double-crediting an internal balance), a raw fuzzer reports complete success.

### How `soroban-invariant-kit` Solves This:
`soroban-invariant-kit` operates at the semantic domain level using strongly typed abstract actions. Generated actions always conform to valid Soroban contract inputs and proper auth contexts, while domain invariant packs evaluate balance conservation, solvency, and non-double-spend oracles after every single transition.

---

## 5. Case Study: The Unfunded Dispute Vulnerability

During Phase 2 of `soroban-invariant-kit` development, the toolkit was benchmarked against [`probablyABug/escrow-contract`](https://github.com/probablyABug/escrow-contract) (forked from `Goldii-locks/escrow-contract`).

### The Bug
The upstream contract had thorough unit test coverage for all standard happy paths (`fund -> mark_delivered -> approve`). However:
- `raise_dispute` and `resolve_dispute` did not check `meta.funded`.
- A client could call `raise_dispute` on an unfunded escrow.
- When `resolve_dispute(..., release_to_freelancer: false)` was invoked, it attempted to transfer refund tokens that had never been deposited.

### How `soroban-invariant-kit` Caught It
When generating randomized state-machine sequences:
1. Action sequence: `[RaiseDispute { milestone_id: 0 }, ResolveDispute { milestone_id: 0, release_to_freelancer: false }]`
2. `TotalLockedConservation` invariant was triggered immediately:
   ```
   INVARIANT VIOLATION: TotalLockedConservation
   Reason: Balance conservation broken: unfunded escrow attempted refund transition
   Failed at step: #2
   Trace:
     [Step 0 - Initial] State: EscrowStateSnapshot { total_locked: 0, is_funded: false }
     [Step 1] Action: RaiseDispute { milestone_id: 0 } => Result: Ok
     [Step 2] Action: ResolveDispute { milestone_id: 0, release_to_freelancer: false } => Result: Ok
   ```
The fuzzer shrank the sequence to the minimal 2 steps and provided an instant, reproducible diagnostic trace.

---

## 6. Summary: When to Use What

- Use **Unit Tests** for basic sanity checks, initial TDD, and simple regression tests.
- Use **`soroban-invariant-kit`** for any contract that holds, transfers, or escrows financial assets (Tokens, Escrows, Streaming, Revenue Splitting, AMMs, Lending).
- Use **Formal Verification** for mission-critical core primitives with static, mathematically proven specifications where multi-month audit timelines are acceptable.

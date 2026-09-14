# soroban-invariant-kit

A property-based invariant testing and state-machine fuzzing toolkit for **multi-contract Soroban financial flows** (escrow↔token, payment-streaming↔token, split-payment↔recipients).

[![CI](https://github.com/Stephan-Thomas/soroban-invariant-kit/actions/workflows/ci.yml/badge.svg)](https://github.com/Stephan-Thomas/soroban-invariant-kit/actions/workflows/ci.yml)
[![License](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](LICENSE)

---

## The Problem

Smart contracts handling financial primitives—like milestone escrows, continuous payment streams, or multi-party split payments—rely heavily on cross-contract state synchronization.

Standard unit tests only cover the "happy paths" or hand-crafted edge cases that developers anticipate. In production, exploits and insolvency bugs arise from unanticipated combinations of actions:
- Partial milestone releases mixed with dispute freezes.
- Claims made against payment streams while time elapsed or cancel conditions interleave.
- Re-entrancy-like state desynchronization between a Soroban escrow contract and an underlying SEP-41 token contract.

**`soroban-invariant-kit`** provides:
1. **Pre-built Invariant Packs**: Ready-to-use invariant suites for common financial flows (Escrow, Streaming, Split-Payment).
2. **Thin Adapter Layer**: A lightweight trait ([`ContractAdapter`](file:///crates/core/src/adapter.rs)) that lets existing Soroban contracts plug in with minimal glue code.
3. **Automated Shrinking**: Powered by Rust's `proptest` and `cargo-fuzz` conventions already familiar in the Soroban ecosystem, shrinking 50-step failing traces to minimal 2-step reproducers.

---

## Workspace Layout

```
soroban-invariant-kit/
├── crates/
│   ├── core/         # Adapter traits, Invariant definitions, Traces, Diagnostic reporting
│   └── harness/      # Test harness, InvariantRunner, proptest & cargo-fuzz glue
├── examples/
│   └── counter/      # Minimal working example demonstrating end-to-end invariant testing
├── ARCHITECTURE.md   # Detailed design, adapter concepts, and integration guide
└── README.md
```

---

## Installation & Setup

Add `soroban-invariant-kit-core` and `soroban-invariant-kit-harness` to your contract's `Cargo.toml`:

```toml
[dev-dependencies]
soroban-invariant-kit-core = { git = "https://github.com/Stephan-Thomas/soroban-invariant-kit" }
soroban-invariant-kit-harness = { git = "https://github.com/Stephan-Thomas/soroban-invariant-kit" }
proptest = "1.5"
```

---

## Quickstart: The Counter Example

Below is a minimal working demonstration (available in [examples/counter](file:///examples/counter)):

### 1. The Soroban Contract

```rust
#[contract]
pub struct CounterContract;

#[contractimpl]
impl CounterContract {
    pub fn get(env: Env) -> i64 { /* ... */ }
    pub fn increment(env: Env, amount: u32) -> i64 { /* ... */ }
    pub fn decrement(env: Env, amount: u32) -> i64 { /* capped at 0 */ }
    pub fn reset(env: Env) -> i64 { /* ... */ }
}
```

### 2. The Adapter

Implement [`ContractAdapter`](file:///crates/core/src/adapter.rs) to bridge abstract actions to contract invocations:

```rust
use soroban_invariant_kit_core::{ActionResult, ContractAdapter};
use soroban_sdk::{Address, Env};

pub enum CounterAction {
    Increment { amount: u32 },
    Decrement { amount: u32 },
    Reset,
}

pub struct CounterState {
    pub value: i64,
}

pub struct CounterAdapter {
    pub env: Env,
    pub contract_id: Address,
}

impl ContractAdapter for CounterAdapter {
    type Action = CounterAction;
    type State = CounterState;
    type Error = CounterError;

    fn setup() -> Result<Self, Self::Error> {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(CounterContract, ());
        Ok(Self { env, contract_id })
    }

    fn snapshot(&self) -> Result<Self::State, Self::Error> {
        let client = CounterContractClient::new(&self.env, &self.contract_id);
        Ok(CounterState { value: client.get() })
    }

    fn step(&mut self, action: &Self::Action) -> Result<ActionResult, Self::Error> {
        let client = CounterContractClient::new(&self.env, &self.contract_id);
        match action {
            CounterAction::Increment { amount } => { client.increment(amount); Ok(ActionResult::Ok) }
            CounterAction::Decrement { amount } => { client.decrement(amount); Ok(ActionResult::Ok) }
            CounterAction::Reset => { client.reset(); Ok(ActionResult::Ok) }
        }
    }
}
```

### 3. The Invariant

Assert properties that must always hold:

```rust
use soroban_invariant_kit_core::{Invariant, InvariantResult};

pub struct CounterNonNegative;

impl Invariant<CounterAdapter> for CounterNonNegative {
    fn name(&self) -> &'static str {
        "CounterNonNegative"
    }

    fn check_state(&self, state: &CounterState) -> InvariantResult {
        if state.value >= 0 {
            InvariantResult::Pass
        } else {
            InvariantResult::violation(format!("Counter dropped below zero: {}", state.value))
        }
    }
}
```

### 4. Running the Invariant Test

Use the `invariant_test!` macro to fuzz randomized sequences of up to 40 actions:

```rust
use proptest::prelude::*;
use soroban_invariant_kit_harness::invariant_test;

fn arb_counter_action() -> impl Strategy<Value = CounterAction> {
    prop_oneof![
        (1u32..=10_000).prop_map(|amount| CounterAction::Increment { amount }),
        (1u32..=10_000).prop_map(|amount| CounterAction::Decrement { amount }),
        Just(CounterAction::Reset),
    ]
}

invariant_test!(
    test_counter_invariants_hold,
    CounterAdapter,
    proptest::collection::vec(arb_counter_action(), 1..40),
    counter_invariants(),
    100 // 100 randomized action sequences
);
```

---

## Diagnostic Output on Invariant Breach

When an invariant fails, `soroban-invariant-kit` outputs a detailed diagnostic report with the exact failing step and the shrunk transition history:

```
============================================================
  INVARIANT VIOLATION: CounterMustStaySmall
============================================================
Reason: Counter exceeded 10: value is 15
Failed at step: #2
------------------------------------------------------------
Trace (Total steps: 2):
  [Step 0 - Initial] State: CounterState { value: 0 }
  [Step 1] Action: Increment { amount: 5 } => Result: Ok
           State Before: CounterState { value: 0 }
           State After:  CounterState { value: 5 }
  [Step 2] Action: Increment { amount: 10 } => Result: Ok
           State Before: CounterState { value: 5 }
           State After:  CounterState { value: 15 }
============================================================
```

---

## Worked Example: Escrow Invariant Pack

Available in [examples/escrow](file:///examples/escrow), based on the real-world milestone escrow contract from [`probablyABug/escrow-contract`](https://github.com/probablyABug/escrow-contract):

### 1. The Invariant Pack
`soroban-invariant-kit` provides `escrow_invariant_pack<A>()` which automatically checks:
- **`TotalLockedConservation`**: $\text{Total Locked} = \sum(\text{milestones}) - \sum(\text{released}) - \sum(\text{refunded})$, matching the SEP-41 token balance.
- **`NoDoubleRelease`**: Milestones cannot be released twice or have releases exceed milestone limits.
- **`NoReleaseWithoutApproval`**: Releases cannot occur unless the milestone was delivered / approved in prior state.
- **`DisputeFreezeCannotBeBypassed`**: While a milestone or escrow is disputed, normal release operations are blocked.

### 2. Plugging in an Escrow Adapter
Contract authors implement [`EscrowAdapter`](file:///crates/core/src/escrow.rs):

```rust
use soroban_invariant_kit_core::escrow::{
    EscrowActionKind, EscrowAdapter, EscrowStateSnapshot, MilestoneSnapshot, MilestoneStatusKind,
};
use soroban_invariant_kit_core::{ActionResult, ContractAdapter};

impl EscrowAdapter for MyEscrowAdapter {
    fn inspect_escrow(state: &Self::State) -> EscrowStateSnapshot {
        EscrowStateSnapshot {
            total_locked: state.token_balance,
            total_released: state.total_released,
            total_refunded: state.total_refunded,
            is_funded: state.funded,
            is_disputed: state.disputed,
            milestones: state.milestones.clone(),
            contract_token_balance: Some(state.token_balance),
        }
    }

    fn classify_action(action: &Self::Action) -> Option<EscrowActionKind> {
        match action {
            MyAction::Fund => Some(EscrowActionKind::Fund { amount: 10_000 }),
            MyAction::MarkDelivered { id } => Some(EscrowActionKind::MarkDelivered { milestone_id: *id }),
            MyAction::Approve { id } => Some(EscrowActionKind::ApproveMilestone { milestone_id: *id }),
            MyAction::Dispute { id } => Some(EscrowActionKind::RaiseDispute { milestone_id: *id }),
            MyAction::Resolve { id, release } => Some(EscrowActionKind::ResolveDispute {
                milestone_id: *id,
                release_to_freelancer: *release,
            }),
            _ => None,
        }
    }
}
```

### 3. Fuzzing with `invariant_test!`
```rust
use soroban_invariant_kit_core::escrow::escrow_invariant_pack;
use soroban_invariant_kit_harness::invariant_test;

invariant_test!(
    test_escrow_invariants_hold,
    EscrowContractAdapter,
    proptest::collection::vec(arb_escrow_action(), 1..25),
    escrow_invariant_pack::<EscrowContractAdapter>(),
    100 // 100 randomized action sequences
);
```

### 4. Real Bug Discovered & Fixed: Unfunded Dispute Vulnerability
When property-fuzzing the original public contract, `soroban-invariant-kit` uncovered that `raise_dispute` and `resolve_dispute` lacked `meta.funded` precondition checks:
- **Vulnerability**: A party could dispute an unfunded escrow, shifting milestone status to `Disputed` prior to token deposit. If `resolve_dispute(..., release_to_freelancer: false)` was subsequently called, the contract attempted to transfer refund tokens that were never deposited.
- **Fix**: Enforced `if !meta.funded { return Err(Error::NotFunded); }` inside `raise_dispute` and `resolve_dispute`.
- **Validation**: Tested with `test_detects_and_reproduces_unfunded_dispute_bug` reproducing the issue and proving the fix.

---

## Roadmap

- [x] **Phase 1: Scaffolding & Design** (Core adapter traits, Invariant definitions, Test harness, Counter minimal working example, Architecture guide, CI)
- [x] **Phase 2: Escrow Invariant Pack** (Total locked == milestones - released - refunded, no double release, dispute freeze, public fixture validation, bug discovery & fix)
- [ ] **Phase 3: Streaming Invariant Pack** (Accrual bounding, balance conservation, terminal claim checks)
- [ ] **Phase 4: Split-Payment Invariant Pack** (Share sum == 100%, payout <= input, duplicate payout guards)
- [ ] **Phase 5: Polish & Documentation** (Worked examples, adapter guidelines, comparisons with single-contract fuzzers)

---

## Documentation

For a deep dive into how adapters work, see [ARCHITECTURE.md](file:///ARCHITECTURE.md).

## License

Dual-licensed under MIT or Apache 2.0.

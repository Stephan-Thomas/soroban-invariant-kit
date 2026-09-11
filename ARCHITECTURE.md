# Architecture & Design of `soroban-invariant-kit`

`soroban-invariant-kit` is a stateful property-testing and invariant-fuzzing toolkit designed for multi-contract financial flows on Soroban (such as escrow↔token, payment-streaming↔token, and payment splitter↔recipients).

This document details the architectural layers, core abstractions, and how Soroban contract authors integrate their systems.

---

## 1. System Overview

Invariant-based testing operates on a simple but powerful premise: **financial systems have safety invariants that must hold under any sequence of valid or invalid actions.**

In conventional unit tests, developers test known, anticipated transitions:
`deploy -> deposit(100) -> release(50) -> assert balance == 50`.

In reality, smart contract exploits occur when unanticipated sequences interact across multiple contracts:
`deposit(100) -> dispute() -> partial_release(20) -> cancel() -> withdraw() ...`

```
┌────────────────────────────────────────────────────────────────────────┐
│                        Execution & Fuzz Engine                         │
│                    (proptest / cargo-fuzz / custom)                    │
└──────────────────────────────────┬─────────────────────────────────────┘
                                   │ Generates Action Sequence
                                   ▼
┌────────────────────────────────────────────────────────────────────────┐
│                          The Adapter Layer                             │
│                     trait ContractAdapter<A, S>                        │
│  - Wraps simulated soroban_sdk::Env & multi-contract deployments        │
│  - Maps abstract Action enum into concrete contract calls               │
│  - Captures immutable State snapshots before & after each step         │
└──────────────────┬───────────────────────────────┬─────────────────────┘
                   │ Invokes Contract Calls        │ State Snapshot
                   ▼                               ▼
     ┌───────────────────────────┐    ┌──────────────────────────────────┐
     │ Soroban Contracts & Mocks │    │      The Invariant Engine        │
     │  - Escrow Contract        │    │    trait Invariant<A: Adapter>   │
     │  - Token Contract (SEP-41)│    │                                  │
     │  - Streaming Contract     │    │ Checks:                          │
     │  - Splitter Contract      │    │  1. Single-state invariants      │
     └───────────────────────────┘    │  2. Transition delta invariants  │
                                      │  3. Multi-contract consistency   │
                                      └────────────────┬─────────────────┘
                                                       │
                                          On Invariant Violation
                                                       ▼
                                      ┌──────────────────────────────────┐
                                      │    Diagnostic Shrinking Report   │
                                      │  - Offending invariant name      │
                                      │  - Step # and action delta       │
                                      │  - Shrunk minimal trace          │
                                      └──────────────────────────────────┘
```

---

## 2. Core Concepts

### What is an "Adapter"?

An **Adapter** is a lightweight bridge implementing [`ContractAdapter`](file:///crates/core/src/adapter.rs). It isolates your contracts' specific ABI, address setup, and authorization rules from the invariant test runner.

The adapter answers three questions:
1. **How is the test environment initialized?** (`setup() -> Result<Self, Self::Error>`)
   - Spawns a fresh `soroban_sdk::Env`.
   - Registers contracts (e.g. escrow, mock token, accounts).
   - Mocks authentication (`env.mock_all_auths()`).
2. **What can happen in the system?** (`type Action: Debug + Clone`)
   - An enum of operations the test engine can perform (e.g. `Deposit { amount }`, `ApproveMilestone { idx }`, `Refund`).
3. **What is the state of the system?** (`type State: Debug + Clone`, `snapshot() -> Result<Self::State, Self::Error>`)
   - A unified, queryable snapshot capturing contract storage fields and related token balances.
4. **How is an action executed?** (`step(&mut self, action: &Self::Action) -> Result<ActionResult, Self::Error>`)
   - Dispatches the action to contract clients and records whether it succeeded (`ActionResult::Ok`) or reverted (`ActionResult::Reverted(reason)`).

By decoupling the test harness from concrete contract ABIs, pre-built invariant packs (e.g., escrow packs, token packs) can evaluate properties across *any* contract that implements the corresponding adapter trait!

### What is an "Invariant"?

An **Invariant** is a property that must **never** be violated, regardless of what sequence of operations has executed.

`soroban-invariant-kit` classifies invariants into three complementary checks:

1. **State Invariants (`check_state(&self, state: &State) -> InvariantResult`)**:
   Properties verifiable from a single state snapshot in isolation.
   - Example: *The contract's recorded locked balance must never be negative.*
   - Example: *The sum of all splitter recipient shares must always equal exactly 100%.*
2. **Transition Invariants (`check_transition(&self, before: &State, after: &State, action: &Action, result: &ActionResult) -> InvariantResult`)**:
   Properties relating state changes to the action performed.
   - Example: *An escrow milestone release must decrement the locked balance by exactly the milestone amount.*
   - Example: *A streaming claim must never release more tokens than have accrued based on elapsed ledger time.*
3. **Multi-Contract Consistency Invariants**:
   Cross-contract balance parity checks.
   - Example: *The escrow contract's internal `total_locked` must equal `token.balance(escrow_address)` at all times.*

---

## 3. How Contract Authors Plug In Their Contracts

Plugging a Soroban contract into `soroban-invariant-kit` takes four straightforward steps:

### Step 1: Define Actions and State Snapshot

Create an abstract action enum and a state snapshot struct capturing relevant values:

```rust
#[derive(Debug, Clone)]
pub enum EscrowAction {
    Deposit { amount: i128 },
    ReleaseMilestone { milestone_idx: u32 },
    Dispute,
    ResolveDispute { refund_buyer: bool },
}

#[derive(Debug, Clone)]
pub struct EscrowState {
    pub total_locked: i128,
    pub token_balance: i128,
    pub disputed: bool,
    pub released_total: i128,
}
```

### Step 2: Implement `ContractAdapter`

```rust
use soroban_invariant_kit_core::{ActionResult, ContractAdapter};
use soroban_sdk::{Address, Env};

pub struct MyEscrowAdapter {
    pub env: Env,
    pub escrow_id: Address,
    pub token_id: Address,
}

impl ContractAdapter for MyEscrowAdapter {
    type Action = EscrowAction;
    type State = EscrowState;
    type Error = MyAdapterError;

    fn setup() -> Result<Self, Self::Error> {
        let env = Env::default();
        env.mock_all_auths();

        // Deploy contracts
        let token_id = env.register_stellar_asset_contract_v2(Address::generate(&env)).address();
        let escrow_id = env.register(MyEscrowContract, ());

        Ok(Self { env, escrow_id, token_id })
    }

    fn snapshot(&self) -> Result<Self::State, Self::Error> {
        let escrow_client = MyEscrowContractClient::new(&self.env, &self.escrow_id);
        let token_client = token::Client::new(&self.env, &self.token_id);

        Ok(EscrowState {
            total_locked: escrow_client.get_locked_amount(),
            token_balance: token_client.balance(&self.escrow_id),
            disputed: escrow_client.is_disputed(),
            released_total: escrow_client.get_released_amount(),
        })
    }

    fn step(&mut self, action: &Self::Action) -> Result<ActionResult, Self::Error> {
        let client = MyEscrowContractClient::new(&self.env, &self.escrow_id);
        match action {
            EscrowAction::Deposit { amount } => {
                match client.try_deposit(amount) {
                    Ok(Ok(())) => Ok(ActionResult::Ok),
                    Ok(Err(e)) => Ok(ActionResult::Reverted(format!("{:?}", e))),
                    Err(e) => Ok(ActionResult::Reverted(format!("{:?}", e))),
                }
            }
            EscrowAction::ReleaseMilestone { milestone_idx } => {
                match client.try_release(milestone_idx) {
                    Ok(Ok(())) => Ok(ActionResult::Ok),
                    Ok(Err(e)) => Ok(ActionResult::Reverted(format!("{:?}", e))),
                    Err(e) => Ok(ActionResult::Reverted(format!("{:?}", e))),
                }
            }
            // ... other actions
        }
    }
}
```

### Step 3: Attach Invariants

Use standard invariant packs (or author contract-specific invariants):

```rust
pub fn escrow_invariants() -> InvariantSet<MyEscrowAdapter> {
    InvariantSet::new()
        .with(LockedMatchesTokenBalance)
        .with(NoReleaseDuringDispute)
        .with(TotalReleasedPlusLockedEqualsDeposited)
}
```

### Step 4: Run with `proptest` or `cargo-fuzz`

Use the `invariant_test!` macro to generate random action sequences and verify the invariants:

```rust
use soroban_invariant_kit_harness::invariant_test;
use proptest::prelude::*;

fn arb_escrow_action() -> impl Strategy<Value = EscrowAction> {
    prop_oneof![
        (1i128..=50_000).prop_map(|amount| EscrowAction::Deposit { amount }),
        (0u32..5).prop_map(|idx| EscrowAction::ReleaseMilestone { milestone_idx: idx }),
        Just(EscrowAction::Dispute),
    ]
}

invariant_test!(
    test_escrow_invariants,
    MyEscrowAdapter,
    proptest::collection::vec(arb_escrow_action(), 1..50),
    escrow_invariants(),
    100 // 100 randomized test traces
);
```

When an invariant is breached, the test harness leverages `proptest`'s shrinking algorithm to reduce a 50-step sequence down to the minimal 2-step bug reproducer, printing a clear diagnostic report.

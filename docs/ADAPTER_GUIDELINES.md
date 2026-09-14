# Adapter Implementation Best Practices Guide

This guide provides practical recommendations and patterns for developers building **`ContractAdapter`** implementations to test Soroban smart contracts with `soroban-invariant-kit`.

---

## 1. What is an Adapter?

An **adapter** acts as the translation layer between the generic invariant testing engine and your concrete Soroban smart contracts:

```
┌───────────────────────────────────────────────────────────┐
│              `soroban-invariant-kit-harness`               │
│         (Generates actions, evaluates invariant set)       │
└─────────────────────────────┬─────────────────────────────┘
                              │
                    Step & Snapshot Calls
                              │
┌─────────────────────────────▼─────────────────────────────┐
│                    `ContractAdapter`                      │
│   • Translates fuzzer action into Soroban client calls    │
│   • Maps contract storage & token state to snapshots      │
│   • Captures reverts as structured `ActionResult`         │
└─────────────────────────────┬─────────────────────────────┘
                              │
                  Direct Rust Client Calls
                              │
┌─────────────────────────────▼─────────────────────────────┐
│           Soroban Environment (`soroban_sdk::Env`)         │
│   • Target Contract under test (Escrow, Stream, Split)    │
│   • Stellar Asset Contracts (SAC) & Test Tokens           │
│   • Simulated Auth & Ledger Timestamps                    │
└───────────────────────────────────────────────────────────┘
```

---

## 2. Seven Steps to Building an Adapter

### Step 1: Define Abstract Actions
Define an enum representing the state-changing operations your contract supports:

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MyContractAction {
    Deposit { amount: i128 },
    Withdraw { amount: i128 },
    UpdateSettings { parameter: u32 },
    AdvanceTime { seconds: u64 },
}
```

> [!TIP]
> Keep actions abstract and clean. Include an `AdvanceTime` or `AdvanceLedger` action whenever time-dependent logic (timelocks, vesting schedules, auto-release deadlines) is present.

---

### Step 2: Define an Immutable State Snapshot
Define a snapshot struct capturing all observable state necessary to verify invariants:

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MyContractState {
    pub contract_balance: i128,
    pub total_deposited: i128,
    pub total_withdrawn: i128,
    pub is_paused: bool,
    pub user_balances: Vec<(Address, i128)>,
}
```

> [!IMPORTANT]
> The snapshot must be **read-only and non-invasive**. Capturing a snapshot must never mutate contract storage or advance the ledger.

---

### Step 3: Implement `ContractAdapter::setup()`
In `setup()`, instantiate the Soroban environment, configure mock authorizations, register token contracts, and initialize your target contract:

```rust
impl ContractAdapter for MyAdapter {
    type Action = MyContractAction;
    type State = MyContractState;
    type Error = MyAdapterError;

    fn setup() -> Result<Self, Self::Error> {
        let env = Env::default();
        // 1. Mock all authorizations for simulated tests
        env.mock_all_auths();

        // 2. Generate participant addresses
        let admin = Address::generate(&env);
        let user = Address::generate(&env);

        // 3. Register standard SAC token contract & mint test balance
        let token_id = env.register_stellar_asset_contract_v2(admin.clone()).address();
        let token_admin = token::StellarAssetClient::new(&env, &token_id);
        token_admin.mint(&user, &10_000_000);

        // 4. Register and initialize contract under test
        let contract_id = env.register(MyContract, ());
        let client = MyContractClient::new(&env, &contract_id);
        client.initialize(&admin, &token_id);

        Ok(Self { env, contract_id, token_id, admin, user })
    }
}
```

---

### Step 4: Implement `ContractAdapter::snapshot()`
Query the contract client and any external contracts (e.g. SEP-41 token balance) to build your state snapshot:

```rust
    fn snapshot(&self) -> Result<Self::State, Self::Error> {
        let client = MyContractClient::new(&self.env, &self.contract_id);
        let token_client = token::Client::new(&self.env, &self.token_id);

        let contract_balance = token_client.balance(&self.contract_id);
        let total_deposited = client.get_total_deposited();
        let total_withdrawn = client.get_total_withdrawn();
        let is_paused = client.is_paused();

        Ok(MyContractState {
            contract_balance,
            total_deposited,
            total_withdrawn,
            is_paused,
            user_balances: vec![(self.user.clone(), token_client.balance(&self.user))],
        })
    }
```

---

### Step 5: Implement `ContractAdapter::step()`
Translate each incoming action into a client call. Use Soroban's `try_*` methods to catch expected contract errors cleanly:

```rust
    fn step(&mut self, action: &Self::Action) -> Result<ActionResult, Self::Error> {
        let client = MyContractClient::new(&self.env, &self.contract_id);

        match action {
            MyContractAction::Deposit { amount } => {
                match client.try_deposit(&self.user, amount) {
                    Ok(Ok(())) => Ok(ActionResult::Ok),
                    Ok(Err(contract_err)) => Ok(ActionResult::Reverted(format!("{:?}", contract_err))),
                    Err(host_err) => Ok(ActionResult::Reverted(format!("{:?}", host_err))),
                }
            }
            MyContractAction::Withdraw { amount } => {
                match client.try_withdraw(&self.user, amount) {
                    Ok(Ok(())) => Ok(ActionResult::Ok),
                    Ok(Err(contract_err)) => Ok(ActionResult::Reverted(format!("{:?}", contract_err))),
                    Err(host_err) => Ok(ActionResult::Reverted(format!("{:?}", host_err))),
                }
            }
            MyContractAction::AdvanceTime { seconds } => {
                self.env.ledger().with_mut(|l| {
                    l.timestamp = l.timestamp.saturating_add(*seconds);
                });
                Ok(ActionResult::Ok)
            }
            _ => Ok(ActionResult::Skipped("Unimplemented action".into())),
        }
    }
```

---

### Step 6: Plug in a Domain Invariant Pack
To reuse pre-built invariant packs, implement the corresponding domain adapter trait:

| Domain | Adapter Trait | Pre-built Invariant Pack |
| :--- | :--- | :--- |
| **Milestone Escrow** | [`EscrowAdapter`](file:///crates/core/src/escrow.rs) | [`escrow_invariant_pack::<A>()`](file:///crates/core/src/escrow.rs) |
| **Payment Streaming** | [`StreamingAdapter`](file:///crates/core/src/streaming.rs) | [`streaming_invariant_pack::<A>()`](file:///crates/core/src/streaming.rs) |
| **Payment Splitting** | [`SplitAdapter`](file:///crates/core/src/split.rs) | [`split_invariant_pack::<A>()`](file:///crates/core/src/split.rs) |

Or define custom invariants using the [`Invariant`](file:///crates/core/src/invariant.rs) trait:

```rust
pub struct SolvencyInvariant;

impl Invariant<MyAdapter> for SolvencyInvariant {
    fn name(&self) -> &'static str { "SolvencyInvariant" }
    
    fn check_state(&self, state: &MyContractState) -> InvariantResult {
        let outstanding_liabilities = state.total_deposited - state.total_withdrawn;
        if state.contract_balance < outstanding_liabilities {
            InvariantResult::violation(format!(
                "Insolvency: token balance {} < liabilities {}",
                state.contract_balance, outstanding_liabilities
            ))
        } else {
            InvariantResult::Pass
        }
    }
}
```

---

### Step 7: Write Property-Based Tests
Use `proptest` and `invariant_test!` to run 100+ multi-step transitions:

```rust
use proptest::prelude::*;
use soroban_invariant_kit_harness::invariant_test;

fn arb_action() -> impl Strategy<Value = MyContractAction> {
    prop_oneof![
        (1_000i128..=50_000i128).prop_map(|amount| MyContractAction::Deposit { amount }),
        (1_000i128..=50_000i128).prop_map(|amount| MyContractAction::Withdraw { amount }),
        (1u64..=3_600u64).prop_map(|seconds| MyContractAction::AdvanceTime { seconds }),
    ]
}

invariant_test!(
    test_my_contract_invariants,
    MyAdapter,
    proptest::collection::vec(arb_action(), 1..25),
    my_invariants(),
    100
);
```

---

## 3. Common Soroban SDK Gotchas & Solutions

### 1. WASM Entrypoint Export Collisions
Inside `#[contractimpl]`, all `pub fn` functions are treated as WASM contract entrypoints by the Soroban SDK macro.
- **Problem**: If you define an internal storage helper with `pub fn read_state(&self, env: &Env)` inside `#[contractimpl]`, compilation fails because references cannot cross the WASM ABI boundary.
- **Solution**: Keep all internal storage helpers private (`fn read_state(...)`) or define them in a standalone module outside `#[contractimpl]`.

### 2. Duplicate `#[contractimpl]` Symbols
If you define multiple `#[contractimpl]` blocks on the same contract struct in the same module, the generated macros produce duplicate WASM symbol definitions (`__initialize`, etc.).
- **Solution**: Keep a single `#[contractimpl]` per struct, or wrap alternative/mock variants in their own submodule (e.g. `pub mod mock { ... }`).

### 3. Vector Macro Shadowing (`soroban_sdk::vec` vs `std::vec`)
Importing `soroban_sdk::vec` shadows the standard Rust `vec![]` macro:
- `soroban_sdk::vec![&env, val1, val2]` requires `&Env` as its first argument.
- Standard `vec![val1, val2]` creates a `std::vec::Vec`.
- **Solution**: Use `use soroban_sdk::vec as sdk_vec;` or explicit `std::vec![...]`.

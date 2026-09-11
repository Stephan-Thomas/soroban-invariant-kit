//! Adapter implementation connecting CounterContract to soroban-invariant-kit.

use soroban_invariant_kit_core::{ActionResult, ContractAdapter};
use soroban_sdk::{Address, Env};
use crate::contract::{CounterContract, CounterContractClient};

/// Actions supported by the Counter contract adapter during fuzzing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CounterAction {
    Increment { amount: u32 },
    Decrement { amount: u32 },
    Reset,
}

/// Observable state snapshot of the Counter contract.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CounterState {
    pub value: i64,
}

#[derive(thiserror::Error, Debug)]
pub enum CounterError {
    #[error("Adapter failure: {0}")]
    General(String),
}

/// Adapter encapsulating the Soroban environment and CounterContract instance.
pub struct CounterAdapter {
    pub env: Env,
    pub contract_id: Address,
}

impl CounterAdapter {
    /// Helper to construct a contract client on demand.
    pub fn client(&self) -> CounterContractClient<'_> {
        CounterContractClient::new(&self.env, &self.contract_id)
    }
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
        let value = self.client().get();
        Ok(CounterState { value })
    }

    fn step(&mut self, action: &Self::Action) -> Result<ActionResult, Self::Error> {
        match action {
            CounterAction::Increment { amount } => {
                self.client().increment(amount);
                Ok(ActionResult::Ok)
            }
            CounterAction::Decrement { amount } => {
                self.client().decrement(amount);
                Ok(ActionResult::Ok)
            }
            CounterAction::Reset => {
                self.client().reset();
                Ok(ActionResult::Ok)
            }
        }
    }
}

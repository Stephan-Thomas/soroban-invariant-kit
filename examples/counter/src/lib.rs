//! # example-counter
//!
//! Minimal working example of a Soroban smart contract tested with `soroban-invariant-kit`.

pub mod adapter;
pub mod contract;
pub mod invariants;

pub use adapter::{CounterAction, CounterAdapter, CounterError, CounterState};
pub use contract::{CounterContract, CounterContractClient};
pub use invariants::{counter_invariants, CounterNonNegative, IncrementMatchesAmount, ResetSetsToZero};

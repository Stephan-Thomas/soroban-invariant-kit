//! Milestone escrow example and invariant testing benchmark.

pub mod adapter;
pub mod contract;

pub use adapter::{
    EscrowAction, EscrowAdapterError, EscrowAdapterState, EscrowContractAdapter,
    UnfixedEscrowContractAdapter,
};
pub use contract::unfixed::UnfixedMilestoneEscrow;
pub use contract::{Milestone, MilestoneEscrow, MilestoneStatus};

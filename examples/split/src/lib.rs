pub mod adapter;
pub mod contract;

pub use adapter::{SplitAction, SplitAdapterState, SplitContractAdapter};
pub use contract::{
    DataKey, RecipientShare, SplitBatch, SplitContract, SplitContractClient, SplitError,
};

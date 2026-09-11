//! Error types for soroban-invariant-kit.

use thiserror::Error;

#[derive(Error, Debug)]
pub enum KitError {
    #[error("Adapter setup failed: {0}")]
    SetupFailed(String),

    #[error("Adapter state snapshot extraction failed: {0}")]
    SnapshotFailed(String),

    #[error("Adapter execution failed fatally: {0}")]
    ExecutionFailed(String),

    #[error("Invariant check error: {0}")]
    InvariantError(String),
}

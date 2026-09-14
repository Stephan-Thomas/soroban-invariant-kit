//! # soroban-invariant-kit-core
//!
//! Core adapter traits, invariant definitions, state transitions, and diagnostic reporting
//! for invariant-testing and fuzzing Soroban smart contracts.
//!
//! ## Key Abstractions
//! - [`ContractAdapter`]: Interface abstracting Soroban environment setup, action dispatch, and state snapshots.
//! - [`Invariant`]: Trait defining state and transition invariant checks.
//! - [`InvariantSet`]: Collection of invariants validated against adapter executions.
//! - [`Trace`]: Sequence of state transitions for failure reproduction and shrinking.
//! - [`InvariantViolationReport`]: Diagnostic output when an invariant fails.

pub mod adapter;
pub mod error;
pub mod escrow;
pub mod invariant;
pub mod report;
pub mod streaming;
pub mod transition;

pub use adapter::{ActionResult, ContractAdapter};
pub use error::KitError;
pub use escrow::{
    escrow_invariant_pack, DisputeFreezeCannotBeBypassed, EscrowActionKind, EscrowAdapter,
    EscrowStateSnapshot, MilestoneSnapshot, MilestoneStatusKind, NoDoubleRelease,
    NoReleaseWithoutApproval, TotalLockedConservation,
};
pub use invariant::{Invariant, InvariantResult, InvariantSet, InvariantViolation};
pub use report::InvariantViolationReport;
pub use streaming::{
    streaming_invariant_pack, ClaimableNeverExceedsAccrual, NoClaimAfterCloseOrCancel,
    StreamSnapshot, StreamStatusKind, StreamingActionKind, StreamingAdapter,
    StreamingBalanceConservation, StreamingMonotonicProgress, StreamingStateSnapshot,
};
pub use transition::{StepRecord, Trace};

/// Prelude containing commonly used traits and types.
pub mod prelude {
    pub use crate::adapter::{ActionResult, ContractAdapter};
    pub use crate::error::KitError;
    pub use crate::escrow::{
        escrow_invariant_pack, DisputeFreezeCannotBeBypassed, EscrowActionKind, EscrowAdapter,
        EscrowStateSnapshot, MilestoneSnapshot, MilestoneStatusKind, NoDoubleRelease,
        NoReleaseWithoutApproval, TotalLockedConservation,
    };
    pub use crate::invariant::{Invariant, InvariantResult, InvariantSet, InvariantViolation};
    pub use crate::report::InvariantViolationReport;
    pub use crate::streaming::{
        streaming_invariant_pack, ClaimableNeverExceedsAccrual, NoClaimAfterCloseOrCancel,
        StreamSnapshot, StreamStatusKind, StreamingActionKind, StreamingAdapter,
        StreamingBalanceConservation, StreamingMonotonicProgress, StreamingStateSnapshot,
    };
    pub use crate::transition::{StepRecord, Trace};
}

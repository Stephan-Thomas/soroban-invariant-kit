//! Streaming adapter trait, normalized snapshots, and pre-built financial invariants.
//!
//! This module provides the [`StreamingAdapter`] trait and domain invariants for
//! real-time linear payment streaming and token vesting flows on Soroban:
//! - [`ClaimableNeverExceedsAccrual`]: Guarantees that cumulative claimed/withdrawn
//!   amounts never exceed time-based accrued vesting.
//! - [`StreamingBalanceConservation`]: Verifies that total deposited tokens equal
//!   cumulative withdrawn plus remaining balance plus refunded tokens at all times.
//! - [`NoClaimAfterCloseOrCancel`]: Ensures no withdrawals may take place once a stream
//!   is cancelled or completed.
//! - [`StreamingMonotonicProgress`]: Enforces that vesting progress is monotonically
//!   non-decreasing as ledger time advances.
//! - [`streaming_invariant_pack`]: Bundles all streaming invariants into a ready-to-use [`InvariantSet`].

use crate::adapter::{ActionResult, ContractAdapter};
use crate::invariant::{Invariant, InvariantResult, InvariantSet};

/// Status of an individual payment stream.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreamStatusKind {
    /// Active and vesting over the defined time window.
    Active,
    /// Cancelled before completion; remaining unvested tokens refunded.
    Cancelled,
    /// Fully vested and entirely withdrawn.
    Completed,
}

/// Normalized snapshot of an individual payment stream.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StreamSnapshot {
    /// Unique identifier of the stream.
    pub id: u64,
    /// Total committed / escrowed amount for this stream.
    pub total: i128,
    /// Cumulative amount withdrawn / claimed by the recipient so far.
    pub withdrawn: i128,
    /// Remaining unwithdrawn balance still held in the stream escrow.
    pub remaining: i128,
    /// Amount refunded back to the sender upon cancellation (0 if active/completed).
    pub refunded_sender: i128,
    /// Ledger timestamp at which vesting begins.
    pub start_time: u64,
    /// Ledger timestamp at which vesting completes.
    pub end_time: u64,
    /// Current lifecycle status of the stream.
    pub status: StreamStatusKind,
    /// Total amount vested mathematically at the snapshot timestamp.
    pub accrued_vested: i128,
}

/// Normalized snapshot of a streaming contract's overall state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StreamingStateSnapshot {
    /// Current ledger timestamp.
    pub timestamp: u64,
    /// List of all streams tracked by the contract.
    pub streams: Vec<StreamSnapshot>,
    /// Cumulative total tokens deposited across all streams.
    pub total_deposited: i128,
    /// Cumulative total tokens withdrawn across all streams.
    pub total_withdrawn: i128,
    /// Cumulative total tokens refunded across all streams.
    pub total_refunded: i128,
    /// Underlying token balance held by the streaming contract, if observed.
    pub contract_token_balance: Option<i128>,
}

/// High-level classification of actions that can occur in a payment streaming system.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StreamingActionKind {
    /// Creation of a new stream.
    CreateStream {
        id: u64,
        amount: i128,
        start_time: u64,
        end_time: u64,
    },
    /// Withdrawal of vested tokens by the recipient.
    Withdraw { id: u64, amount: i128 },
    /// Cancellation of an active stream.
    Cancel { id: u64 },
    /// Adding more funds to an active stream.
    TopUp { id: u64, amount: i128 },
    /// Extending the vesting window end time.
    Extend { id: u64, new_end: u64 },
    /// Simulating the passage of time on the ledger.
    AdvanceTime { seconds: u64 },
    /// Other custom action.
    Other,
}

/// Adapter trait bridging concrete Soroban streaming contracts to normalized streaming checks.
pub trait StreamingAdapter: ContractAdapter {
    /// Extracts a [`StreamingStateSnapshot`] from the adapter's concrete state.
    fn inspect_streaming(state: &Self::State) -> StreamingStateSnapshot;

    /// Optionally classifies an action into standard streaming actions for transition checks.
    fn classify_action(_action: &Self::Action) -> Option<StreamingActionKind> {
        None
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Invariant 1: ClaimableNeverExceedsAccrual
// ─────────────────────────────────────────────────────────────────────────────

/// Invariant: Cumulative withdrawn amounts can never exceed accrued vesting at any timestamp.
///
/// Mathematical invariant:
/// `stream.withdrawn <= stream.accrued_vested <= stream.total`
///
/// Additionally checks:
/// - Incremental withdrawals in state transitions cannot exceed the withdrawable accrual
///   available before the withdrawal.
pub struct ClaimableNeverExceedsAccrual;

impl<A: StreamingAdapter> Invariant<A> for ClaimableNeverExceedsAccrual {
    fn name(&self) -> &'static str {
        "ClaimableNeverExceedsAccrual"
    }

    fn description(&self) -> &'static str {
        "Withdrawn amount can never exceed mathematically accrued vesting, and accrual cannot exceed total"
    }

    fn check_state(&self, state: &A::State) -> InvariantResult {
        let streaming = A::inspect_streaming(state);

        for s in &streaming.streams {
            if s.withdrawn < 0 {
                return InvariantResult::violation(format!(
                    "Stream {} has negative withdrawn amount: {}",
                    s.id, s.withdrawn
                ));
            }
            if s.accrued_vested < 0 {
                return InvariantResult::violation(format!(
                    "Stream {} has negative accrued_vested amount: {}",
                    s.id, s.accrued_vested
                ));
            }
            if s.accrued_vested > s.total {
                return InvariantResult::violation(format!(
                    "Stream {} accrued_vested ({}) exceeds stream total ({})",
                    s.id, s.accrued_vested, s.total
                ));
            }
            if s.withdrawn > s.accrued_vested {
                return InvariantResult::violation(format!(
                    "Over-withdrawal detected: stream {} withdrawn ({}) exceeds accrued_vested ({}) [total: {}]",
                    s.id, s.withdrawn, s.accrued_vested, s.total
                ));
            }
        }

        InvariantResult::Pass
    }

    fn check_transition(
        &self,
        before: &A::State,
        after: &A::State,
        action: &A::Action,
        result: &ActionResult,
    ) -> InvariantResult {
        let before_streaming = A::inspect_streaming(before);
        let after_streaming = A::inspect_streaming(after);

        for s_after in &after_streaming.streams {
            if let Some(s_before) = before_streaming.streams.iter().find(|s| s.id == s_after.id) {
                let withdrawn_delta = s_after.withdrawn - s_before.withdrawn;
                if withdrawn_delta > 0 {
                    // Maximum withdrawable amount prior to this step or at the current timestamp
                    let available_accrual = s_after.accrued_vested - s_before.withdrawn;
                    if withdrawn_delta > available_accrual {
                        return InvariantResult::violation(format!(
                            "Withdrawal delta ({}) on stream {} exceeds available accrual ({})",
                            withdrawn_delta, s_after.id, available_accrual
                        ));
                    }
                }
            }
        }

        // If an explicit Withdraw action was attempted that exceeded available accrual, it must fail
        if let Some(StreamingActionKind::Withdraw { id, amount }) = A::classify_action(action) {
            if let Some(s_before) = before_streaming.streams.iter().find(|s| s.id == id) {
                let available = (s_before.accrued_vested - s_before.withdrawn).max(0);
                if amount > available && result.is_ok() {
                    return InvariantResult::violation(format!(
                        "Withdraw action succeeded for amount {} on stream {}, exceeding available withdrawable accrual {}",
                        amount, id, available
                    ));
                }
            }
        }

        InvariantResult::Pass
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Invariant 2: StreamingBalanceConservation
// ─────────────────────────────────────────────────────────────────────────────

/// Invariant: Total deposited tokens must strictly equal cumulative withdrawn
/// plus remaining escrowed tokens plus refunded tokens across all streams.
///
/// Mathematical invariant:
/// - Per-stream: `stream.total == stream.withdrawn + stream.remaining + stream.refunded_sender`
/// - Global: `total_deposited == total_withdrawn + sum(remaining) + total_refunded`
/// - Solvency: `contract_token_balance >= sum(remaining)`
pub struct StreamingBalanceConservation;

impl<A: StreamingAdapter> Invariant<A> for StreamingBalanceConservation {
    fn name(&self) -> &'static str {
        "StreamingBalanceConservation"
    }

    fn description(&self) -> &'static str {
        "Total deposited tokens must equal withdrawn + remaining + refunded (per stream and globally), and contract token balance must cover remaining liabilities"
    }

    fn check_state(&self, state: &A::State) -> InvariantResult {
        let streaming = A::inspect_streaming(state);

        let mut sum_totals: i128 = 0;
        let mut sum_withdrawn: i128 = 0;
        let mut sum_remaining: i128 = 0;
        let mut sum_refunded: i128 = 0;

        for s in &streaming.streams {
            if s.total < 0 || s.withdrawn < 0 || s.remaining < 0 || s.refunded_sender < 0 {
                return InvariantResult::violation(format!(
                    "Stream {} has negative financial field: total={}, withdrawn={}, remaining={}, refunded={}",
                    s.id, s.total, s.withdrawn, s.remaining, s.refunded_sender
                ));
            }

            let expected_total = s
                .withdrawn
                .checked_add(s.remaining)
                .and_then(|v| v.checked_add(s.refunded_sender));

            match expected_total {
                Some(expected) => {
                    if s.total != expected {
                        return InvariantResult::violation(format!(
                            "Per-stream balance conservation broken on stream {}: total ({}) != withdrawn ({}) + remaining ({}) + refunded ({}) [sum: {}]",
                            s.id, s.total, s.withdrawn, s.remaining, s.refunded_sender, expected
                        ));
                    }
                }
                None => {
                    return InvariantResult::violation(format!(
                        "Overflow calculating expected balance for stream {}",
                        s.id
                    ));
                }
            }

            sum_totals = match sum_totals.checked_add(s.total) {
                Some(v) => v,
                None => return InvariantResult::violation("Overflow in sum_totals"),
            };
            sum_withdrawn = match sum_withdrawn.checked_add(s.withdrawn) {
                Some(v) => v,
                None => return InvariantResult::violation("Overflow in sum_withdrawn"),
            };
            sum_remaining = match sum_remaining.checked_add(s.remaining) {
                Some(v) => v,
                None => return InvariantResult::violation("Overflow in sum_remaining"),
            };
            sum_refunded = match sum_refunded.checked_add(s.refunded_sender) {
                Some(v) => v,
                None => return InvariantResult::violation("Overflow in sum_refunded"),
            };
        }

        // Global conservation check
        if streaming.total_deposited != sum_totals {
            return InvariantResult::violation(format!(
                "Global total_deposited ({}) != sum of stream totals ({})",
                streaming.total_deposited, sum_totals
            ));
        }

        if streaming.total_withdrawn != sum_withdrawn {
            return InvariantResult::violation(format!(
                "Global total_withdrawn ({}) != sum of stream withdrawals ({})",
                streaming.total_withdrawn, sum_withdrawn
            ));
        }

        if streaming.total_refunded != sum_refunded {
            return InvariantResult::violation(format!(
                "Global total_refunded ({}) != sum of stream refunds ({})",
                streaming.total_refunded, sum_refunded
            ));
        }

        let global_accounted = streaming
            .total_withdrawn
            .checked_add(sum_remaining)
            .and_then(|v| v.checked_add(streaming.total_refunded));

        match global_accounted {
            Some(accounted) => {
                if streaming.total_deposited != accounted {
                    return InvariantResult::violation(format!(
                        "Global conservation broken: total_deposited ({}) != total_withdrawn ({}) + remaining ({}) + total_refunded ({}) [sum: {}]",
                        streaming.total_deposited, streaming.total_withdrawn, sum_remaining, streaming.total_refunded, accounted
                    ));
                }
            }
            None => {
                return InvariantResult::violation("Overflow in global balance conservation calculation");
            }
        }

        // Token solvency check: contract token balance must at least equal active remaining liabilities
        if let Some(token_bal) = streaming.contract_token_balance {
            if token_bal < sum_remaining {
                return InvariantResult::violation(format!(
                    "Contract token balance insolvency: token balance ({}) < outstanding stream liabilities ({})",
                    token_bal, sum_remaining
                ));
            }
        }

        InvariantResult::Pass
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Invariant 3: NoClaimAfterCloseOrCancel
// ─────────────────────────────────────────────────────────────────────────────

/// Invariant: Once a stream is cancelled or completed, its withdrawn amount
/// cannot increase, and no further withdrawals or claims can occur.
pub struct NoClaimAfterCloseOrCancel;

impl<A: StreamingAdapter> Invariant<A> for NoClaimAfterCloseOrCancel {
    fn name(&self) -> &'static str {
        "NoClaimAfterCloseOrCancel"
    }

    fn description(&self) -> &'static str {
        "Terminated streams (cancelled or completed) cannot disburse additional funds or be claimed again"
    }

    fn check_state(&self, state: &A::State) -> InvariantResult {
        let streaming = A::inspect_streaming(state);

        for s in &streaming.streams {
            match s.status {
                StreamStatusKind::Cancelled => {
                    if s.remaining != 0 {
                        return InvariantResult::violation(format!(
                            "Cancelled stream {} still has non-zero remaining balance: {}",
                            s.id, s.remaining
                        ));
                    }
                }
                StreamStatusKind::Completed => {
                    if s.remaining != 0 {
                        return InvariantResult::violation(format!(
                            "Completed stream {} still has non-zero remaining balance: {}",
                            s.id, s.remaining
                        ));
                    }
                    if s.withdrawn != s.total {
                        return InvariantResult::violation(format!(
                            "Completed stream {} withdrawn ({}) does not equal total ({})",
                            s.id, s.withdrawn, s.total
                        ));
                    }
                }
                StreamStatusKind::Active => {}
            }
        }

        InvariantResult::Pass
    }

    fn check_transition(
        &self,
        before: &A::State,
        after: &A::State,
        action: &A::Action,
        result: &ActionResult,
    ) -> InvariantResult {
        let before_streaming = A::inspect_streaming(before);
        let after_streaming = A::inspect_streaming(after);

        for s_before in &before_streaming.streams {
            if let Some(s_after) = after_streaming.streams.iter().find(|s| s.id == s_before.id) {
                // Check if withdrawn amount increased on a previously terminated stream
                if matches!(s_before.status, StreamStatusKind::Cancelled | StreamStatusKind::Completed) {
                    if s_after.withdrawn > s_before.withdrawn {
                        return InvariantResult::violation(format!(
                            "Post-termination claim detected: stream {} had status {:?} (withdrawn: {}), but withdrawn amount increased to {}",
                            s_before.id, s_before.status, s_before.withdrawn, s_after.withdrawn
                        ));
                    }

                    // Status immutability: terminated status cannot revert to Active
                    if s_after.status == StreamStatusKind::Active {
                        return InvariantResult::violation(format!(
                            "Terminated stream {} improperly reverted to Active status",
                            s_before.id
                        ));
                    }
                }
            }
        }

        // If an explicit Withdraw action was attempted on a terminated stream, it must fail
        if let Some(StreamingActionKind::Withdraw { id, .. }) = A::classify_action(action) {
            if let Some(s_before) = before_streaming.streams.iter().find(|s| s.id == id) {
                if matches!(s_before.status, StreamStatusKind::Cancelled | StreamStatusKind::Completed) && result.is_ok() {
                    return InvariantResult::violation(format!(
                        "Withdraw action succeeded on terminated stream {} (status: {:?})",
                        id, s_before.status
                    ));
                }
            }
        }

        InvariantResult::Pass
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Invariant 4: StreamingMonotonicProgress
// ─────────────────────────────────────────────────────────────────────────────

/// Invariant: Accrued vesting must be monotonically non-decreasing over forward-moving
/// ledger time for unmodified active streams.
pub struct StreamingMonotonicProgress;

impl<A: StreamingAdapter> Invariant<A> for StreamingMonotonicProgress {
    fn name(&self) -> &'static str {
        "StreamingMonotonicProgress"
    }

    fn description(&self) -> &'static str {
        "Accrued vesting is monotonically non-decreasing as ledger time advances"
    }

    fn check_transition(
        &self,
        before: &A::State,
        after: &A::State,
        _action: &A::Action,
        _result: &ActionResult,
    ) -> InvariantResult {
        let before_streaming = A::inspect_streaming(before);
        let after_streaming = A::inspect_streaming(after);

        // Only enforce forward progress if time advances or stays constant
        if after_streaming.timestamp >= before_streaming.timestamp {
            for s_before in &before_streaming.streams {
                if s_before.status == StreamStatusKind::Active {
                    if let Some(s_after) = after_streaming.streams.iter().find(|s| s.id == s_before.id) {
                        // If stream remains active and end_time wasn't shortened
                        if s_after.status == StreamStatusKind::Active && s_after.end_time >= s_before.end_time {
                            if s_after.accrued_vested < s_before.accrued_vested {
                                return InvariantResult::violation(format!(
                                    "Monotonicity violation on stream {}: accrued_vested decreased from {} to {} despite time advancing from {} to {}",
                                    s_before.id, s_before.accrued_vested, s_after.accrued_vested, before_streaming.timestamp, after_streaming.timestamp
                                ));
                            }
                        }
                    }
                }
            }
        }

        InvariantResult::Pass
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Invariant Pack Builder
// ─────────────────────────────────────────────────────────────────────────────

/// Constructs a complete [`InvariantSet`] containing all standard payment streaming invariants.
///
/// Includes:
/// - [`ClaimableNeverExceedsAccrual`]
/// - [`StreamingBalanceConservation`]
/// - [`NoClaimAfterCloseOrCancel`]
/// - [`StreamingMonotonicProgress`]
pub fn streaming_invariant_pack<A: StreamingAdapter>() -> InvariantSet<A> {
    InvariantSet::new()
        .with(ClaimableNeverExceedsAccrual)
        .with(StreamingBalanceConservation)
        .with(NoClaimAfterCloseOrCancel)
        .with(StreamingMonotonicProgress)
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockStreamingAdapter;

    #[derive(Debug, Clone)]
    struct MockAction(StreamingActionKind);

    #[derive(Debug, Clone)]
    struct MockState(StreamingStateSnapshot);

    #[derive(thiserror::Error, Debug)]
    #[error("mock error")]
    struct MockError;

    impl ContractAdapter for MockStreamingAdapter {
        type Action = MockAction;
        type State = MockState;
        type Error = MockError;

        fn setup() -> Result<Self, Self::Error> {
            Ok(Self)
        }

        fn snapshot(&self) -> Result<Self::State, Self::Error> {
            unimplemented!()
        }

        fn step(&mut self, _action: &Self::Action) -> Result<ActionResult, Self::Error> {
            Ok(ActionResult::Ok)
        }
    }

    impl StreamingAdapter for MockStreamingAdapter {
        fn inspect_streaming(state: &Self::State) -> StreamingStateSnapshot {
            state.0.clone()
        }

        fn classify_action(action: &Self::Action) -> Option<StreamingActionKind> {
            Some(action.0.clone())
        }
    }

    fn sample_stream(id: u64) -> StreamSnapshot {
        StreamSnapshot {
            id,
            total: 10_000,
            withdrawn: 2_000,
            remaining: 8_000,
            refunded_sender: 0,
            start_time: 100,
            end_time: 1_100,
            status: StreamStatusKind::Active,
            accrued_vested: 3_000,
        }
    }

    fn sample_state(streams: Vec<StreamSnapshot>, timestamp: u64) -> MockState {
        let total_deposited = streams.iter().map(|s| s.total).sum();
        let total_withdrawn = streams.iter().map(|s| s.withdrawn).sum();
        let total_refunded = streams.iter().map(|s| s.refunded_sender).sum();
        let total_remaining: i128 = streams.iter().map(|s| s.remaining).sum();

        MockState(StreamingStateSnapshot {
            timestamp,
            streams,
            total_deposited,
            total_withdrawn,
            total_refunded,
            contract_token_balance: Some(total_remaining),
        })
    }

    #[test]
    fn test_claimable_never_exceeds_accrual_passes() {
        let invariant = ClaimableNeverExceedsAccrual;
        let state = sample_state(vec![sample_stream(1)], 400);
        let res = Invariant::<MockStreamingAdapter>::check_state(&invariant, &state);
        assert!(res.is_pass());
    }

    #[test]
    fn test_claimable_never_exceeds_accrual_catches_over_withdrawal() {
        let invariant = ClaimableNeverExceedsAccrual;
        let mut s = sample_stream(1);
        s.withdrawn = 5_000;
        s.accrued_vested = 3_000;
        s.remaining = 5_000;
        let state = sample_state(vec![s], 400);
        let res = Invariant::<MockStreamingAdapter>::check_state(&invariant, &state);
        assert!(res.is_violation());
        assert!(res.reason().unwrap().contains("Over-withdrawal detected"));
    }

    #[test]
    fn test_balance_conservation_passes() {
        let invariant = StreamingBalanceConservation;
        let state = sample_state(vec![sample_stream(1)], 400);
        let res = Invariant::<MockStreamingAdapter>::check_state(&invariant, &state);
        assert!(res.is_pass());
    }

    #[test]
    fn test_balance_conservation_catches_mismatch() {
        let invariant = StreamingBalanceConservation;
        let mut s = sample_stream(1);
        s.remaining = 7_000; // 2_000 + 7_000 != 10_000
        let state = sample_state(vec![s], 400);
        let res = Invariant::<MockStreamingAdapter>::check_state(&invariant, &state);
        assert!(res.is_violation());
        assert!(res.reason().unwrap().contains("Per-stream balance conservation broken"));
    }

    #[test]
    fn test_balance_conservation_catches_insolvency() {
        let invariant = StreamingBalanceConservation;
        let mut s = sample_state(vec![sample_stream(1)], 400);
        s.0.contract_token_balance = Some(5_000); // remaining liabilities are 8_000
        let res = Invariant::<MockStreamingAdapter>::check_state(&invariant, &s);
        assert!(res.is_violation());
        assert!(res.reason().unwrap().contains("token balance insolvency"));
    }

    #[test]
    fn test_no_claim_after_close_or_cancel() {
        let invariant = NoClaimAfterCloseOrCancel;
        let mut s1 = sample_stream(1);
        s1.status = StreamStatusKind::Cancelled;
        s1.remaining = 0;
        s1.refunded_sender = 7_000;
        s1.withdrawn = 3_000;
        let state_before = sample_state(vec![s1.clone()], 500);

        let mut s2 = s1.clone();
        s2.withdrawn = 4_000; // attempted withdrawal after cancellation!
        let state_after = sample_state(vec![s2], 600);

        let action = MockAction(StreamingActionKind::Withdraw { id: 1, amount: 1_000 });
        let res = Invariant::<MockStreamingAdapter>::check_transition(
            &invariant,
            &state_before,
            &state_after,
            &action,
            &ActionResult::Ok,
        );
        assert!(res.is_violation());
        assert!(res.reason().unwrap().contains("Post-termination claim detected"));
    }

    #[test]
    fn test_streaming_monotonic_progress() {
        let invariant = StreamingMonotonicProgress;
        let s1 = sample_stream(1); // accrued_vested = 3_000
        let state_before = sample_state(vec![s1], 400);

        let mut s2 = sample_stream(1);
        s2.accrued_vested = 2_500; // decreased!
        let state_after = sample_state(vec![s2], 500);

        let action = MockAction(StreamingActionKind::AdvanceTime { seconds: 100 });
        let res = Invariant::<MockStreamingAdapter>::check_transition(
            &invariant,
            &state_before,
            &state_after,
            &action,
            &ActionResult::Ok,
        );
        assert!(res.is_violation());
        assert!(res.reason().unwrap().contains("Monotonicity violation"));
    }

    #[test]
    fn test_streaming_invariant_pack_builder() {
        let pack = streaming_invariant_pack::<MockStreamingAdapter>();
        assert_eq!(pack.len(), 4);
    }
}

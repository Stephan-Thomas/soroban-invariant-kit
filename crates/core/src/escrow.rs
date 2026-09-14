//! Escrow adapter trait, normalized snapshots, and pre-built financial invariants.
//!
//! This module provides the [`EscrowAdapter`] trait and domain invariants for
//! multi-contract milestone escrow flows on Soroban:
//! - [`TotalLockedConservation`]: Verifies that total locked funds equal the sum of
//!   milestone amounts minus released and refunded amounts at all times.
//! - [`NoDoubleRelease`]: Guarantees that no milestone can be released more than once.
//! - [`NoReleaseWithoutApproval`]: Ensures funds cannot be released without prior approval state.
//! - [`DisputeFreezeCannotBeBypassed`]: Enforces that funds cannot be released during an active dispute.
//! - [`escrow_invariant_pack`]: Bundles all escrow invariants into a ready-to-use [`InvariantSet`].

use crate::adapter::{ActionResult, ContractAdapter};
use crate::invariant::{Invariant, InvariantResult, InvariantSet};

/// Status of a milestone within an escrow contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MilestoneStatusKind {
    /// Initial state, waiting for delivery or approval.
    Pending,
    /// Delivered by the freelancer, pending review.
    Delivered,
    /// Partially approved / released.
    PartiallyReleased,
    /// Fully approved and released.
    Released,
    /// Currently disputed.
    Disputed,
    /// Refunded back to the funder.
    Refunded,
}

/// Normalized snapshot of an individual milestone.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MilestoneSnapshot {
    /// Unique index or identifier of the milestone.
    pub id: u32,
    /// Total committed amount for this milestone.
    pub amount: i128,
    /// Cumulative amount released to the freelancer so far.
    pub released_amount: i128,
    /// Whether the milestone has reached an approved or delivered state eligible for release.
    pub is_approved: bool,
    /// Whether the milestone has been fully released.
    pub is_released: bool,
    /// Whether the milestone has been refunded.
    pub is_refunded: bool,
    /// Whether this specific milestone is currently in dispute.
    pub is_disputed: bool,
    /// Detailed milestone status.
    pub status: MilestoneStatusKind,
}

/// Normalized snapshot of an escrow system's overall state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EscrowStateSnapshot {
    /// Currently locked balance held in the escrow contract.
    pub total_locked: i128,
    /// Cumulative amount released across all milestones.
    pub total_released: i128,
    /// Cumulative amount refunded across all milestones.
    pub total_refunded: i128,
    /// Whether the escrow has been funded.
    pub is_funded: bool,
    /// Whether the escrow or any milestone is currently under active dispute.
    pub is_disputed: bool,
    /// List of milestones defined in the escrow.
    pub milestones: Vec<MilestoneSnapshot>,
    /// Underlying SEP-41 token balance held by the escrow contract, if observed.
    pub contract_token_balance: Option<i128>,
}

/// High-level classification of actions that can occur in an escrow system.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EscrowActionKind {
    /// Initial funding / deposit of escrow balance.
    Fund { amount: i128 },
    /// Milestone marked delivered by the worker.
    MarkDelivered { milestone_id: u32 },
    /// Approval of a milestone (full or partial).
    ApproveMilestone { milestone_id: u32 },
    /// Partial release of milestone funds.
    ApprovePartial { milestone_id: u32, amount: i128 },
    /// Auto-release claim after delivery timeout.
    ClaimAutoRelease { milestone_id: u32 },
    /// Dispute raised on a milestone or the contract.
    RaiseDispute { milestone_id: u32 },
    /// Dispute resolution by arbiter.
    ResolveDispute {
        milestone_id: u32,
        release_to_freelancer: bool,
    },
    /// Emergency pause or unpause.
    SetPaused(bool),
    /// Cancellation of the escrow.
    Cancel,
    /// Other custom action.
    Other,
}

/// Adapter trait bridging concrete Soroban escrow contracts to normalized escrow checks.
pub trait EscrowAdapter: ContractAdapter {
    /// Extracts an [`EscrowStateSnapshot`] from the adapter's concrete state.
    fn inspect_escrow(state: &Self::State) -> EscrowStateSnapshot;

    /// Optionally classifies an action into standard escrow actions for transition checks.
    fn classify_action(_action: &Self::Action) -> Option<EscrowActionKind> {
        None
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Invariant 1: TotalLockedConservation
// ─────────────────────────────────────────────────────────────────────────────

/// Invariant: Total locked funds must equal the sum of milestone amounts
/// minus cumulative released and refunded amounts.
///
/// Mathematical invariant:
/// `total_locked == sum(milestones) - total_released - total_refunded`
///
/// Additionally verifies:
/// - `total_locked >= 0`
/// - If `contract_token_balance` is provided, `contract_token_balance == total_locked`
pub struct TotalLockedConservation;

impl<A: EscrowAdapter> Invariant<A> for TotalLockedConservation {
    fn name(&self) -> &'static str {
        "TotalLockedConservation"
    }

    fn description(&self) -> &'static str {
        "Total locked funds must equal sum(milestone amounts) - sum(released) - sum(refunded) and match token balance"
    }

    fn check_state(&self, state: &A::State) -> InvariantResult {
        let escrow = A::inspect_escrow(state);

        if !escrow.is_funded {
            if escrow.total_locked != 0 {
                return InvariantResult::violation(format!(
                    "Unfunded escrow has non-zero total_locked: {}",
                    escrow.total_locked
                ));
            }
            if escrow.total_released != 0 {
                return InvariantResult::violation(format!(
                    "Unfunded escrow has non-zero total_released: {}",
                    escrow.total_released
                ));
            }
            return InvariantResult::Pass;
        }

        if escrow.total_locked < 0 {
            return InvariantResult::violation(format!(
                "Total locked balance is negative: {}",
                escrow.total_locked
            ));
        }

        let total_milestone_amounts: i128 = escrow.milestones.iter().map(|m| m.amount).sum();
        let expected_locked = total_milestone_amounts
            .checked_sub(escrow.total_released)
            .and_then(|v| v.checked_sub(escrow.total_refunded));

        match expected_locked {
            Some(expected) => {
                if escrow.total_locked != expected {
                    return InvariantResult::violation(format!(
                        "Balance conservation broken: total_locked ({}) != sum(milestones) ({}) - released ({}) - refunded ({}) [expected: {}]",
                        escrow.total_locked, total_milestone_amounts, escrow.total_released, escrow.total_refunded, expected
                    ));
                }
            }
            None => {
                return InvariantResult::violation(
                    "Overflow occurred computing expected locked balance",
                );
            }
        }

        // Verify cross-contract token balance parity if available
        if let Some(token_bal) = escrow.contract_token_balance {
            if token_bal != escrow.total_locked {
                return InvariantResult::violation(format!(
                    "Token contract balance ({}) does not match escrow total_locked ({})",
                    token_bal, escrow.total_locked
                ));
            }
        }

        InvariantResult::Pass
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Invariant 2: NoDoubleRelease
// ─────────────────────────────────────────────────────────────────────────────

/// Invariant: A milestone cannot be released more than once, and cumulative
/// releases can never exceed the milestone amount.
pub struct NoDoubleRelease;

impl<A: EscrowAdapter> Invariant<A> for NoDoubleRelease {
    fn name(&self) -> &'static str {
        "NoDoubleRelease"
    }

    fn description(&self) -> &'static str {
        "A milestone cannot be released more than once; released_amount cannot exceed milestone amount"
    }

    fn check_state(&self, state: &A::State) -> InvariantResult {
        let escrow = A::inspect_escrow(state);
        for m in &escrow.milestones {
            if m.released_amount > m.amount {
                return InvariantResult::violation(format!(
                    "Milestone {} released_amount ({}) exceeds total amount ({})",
                    m.id, m.released_amount, m.amount
                ));
            }
            if m.is_released && m.released_amount != m.amount {
                return InvariantResult::violation(format!(
                    "Milestone {} marked is_released but released_amount ({}) != amount ({})",
                    m.id, m.released_amount, m.amount
                ));
            }
        }
        InvariantResult::Pass
    }

    fn check_transition(
        &self,
        before: &A::State,
        after: &A::State,
        _action: &A::Action,
        _result: &ActionResult,
    ) -> InvariantResult {
        let before_escrow = A::inspect_escrow(before);
        let after_escrow = A::inspect_escrow(after);

        for m_before in &before_escrow.milestones {
            if let Some(m_after) = after_escrow.milestones.iter().find(|m| m.id == m_before.id) {
                // If previously fully released, released amount cannot increase
                if m_before.is_released && m_after.released_amount > m_before.released_amount {
                    return InvariantResult::violation(format!(
                        "Double-release detected: milestone {} was already released ({} released), but released amount increased to {}",
                        m_before.id, m_before.released_amount, m_after.released_amount
                    ));
                }

                // If previously refunded, released amount cannot increase
                if m_before.is_refunded && m_after.released_amount > m_before.released_amount {
                    return InvariantResult::violation(format!(
                        "Invalid release on refunded milestone: milestone {} was refunded, but released amount increased from {} to {}",
                        m_before.id, m_before.released_amount, m_after.released_amount
                    ));
                }

                // Terminal status cannot revert
                if m_before.is_released && !m_after.is_released {
                    return InvariantResult::violation(format!(
                        "Milestone {} reverted from terminal released status",
                        m_before.id
                    ));
                }
            }
        }

        InvariantResult::Pass
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Invariant 3: NoReleaseWithoutApproval
// ─────────────────────────────────────────────────────────────────────────────

/// Invariant: A milestone can only release funds if it was already in an
/// approved/delivered state in the preceding state.
pub struct NoReleaseWithoutApproval;

impl<A: EscrowAdapter> Invariant<A> for NoReleaseWithoutApproval {
    fn name(&self) -> &'static str {
        "NoReleaseWithoutApproval"
    }

    fn description(&self) -> &'static str {
        "Milestone funds cannot be released unless the milestone was in a prior approved/delivered state"
    }

    fn check_transition(
        &self,
        before: &A::State,
        after: &A::State,
        action: &A::Action,
        result: &ActionResult,
    ) -> InvariantResult {
        let before_escrow = A::inspect_escrow(before);
        let after_escrow = A::inspect_escrow(after);

        // Check each milestone for releases
        for m_after in &after_escrow.milestones {
            if let Some(m_before) = before_escrow.milestones.iter().find(|m| m.id == m_after.id) {
                let delta = m_after.released_amount - m_before.released_amount;
                if delta > 0 {
                    // Check if the prior state was approved or if this is an authorized dispute resolution
                    let is_dispute_resolution = matches!(
                        A::classify_action(action),
                        Some(EscrowActionKind::ResolveDispute { .. })
                    );

                    if !m_before.is_approved && !is_dispute_resolution {
                        return InvariantResult::violation(format!(
                            "Release without prior approval: milestone {} released {} (from {} to {}) but before.is_approved was false (status was {:?})",
                            m_after.id, delta, m_before.released_amount, m_after.released_amount, m_before.status
                        ));
                    }
                }
            }
        }

        // Also check if an unapproved release action succeeded
        if let Some(EscrowActionKind::ApproveMilestone { milestone_id }) = A::classify_action(action) {
            if let Some(m_before) = before_escrow.milestones.iter().find(|m| m.id == milestone_id) {
                if !m_before.is_approved && result.is_ok() {
                    // Action succeeded even though milestone wasn't delivered/approved
                    return InvariantResult::violation(format!(
                        "ApproveMilestone succeeded on milestone {} which was not in approved state (status: {:?})",
                        milestone_id, m_before.status
                    ));
                }
            }
        }

        InvariantResult::Pass
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Invariant 4: DisputeFreezeCannotBeBypassed
// ─────────────────────────────────────────────────────────────────────────────

/// Invariant: While an escrow or milestone is in dispute, no normal release
/// can occur. Only an authorized dispute resolution may disburse or refund funds.
pub struct DisputeFreezeCannotBeBypassed;

impl<A: EscrowAdapter> Invariant<A> for DisputeFreezeCannotBeBypassed {
    fn name(&self) -> &'static str {
        "DisputeFreezeCannotBeBypassed"
    }

    fn description(&self) -> &'static str {
        "Dispute freeze cannot be bypassed: no milestone release may occur while in dispute"
    }

    fn check_transition(
        &self,
        before: &A::State,
        after: &A::State,
        action: &A::Action,
        result: &ActionResult,
    ) -> InvariantResult {
        let before_escrow = A::inspect_escrow(before);
        let after_escrow = A::inspect_escrow(after);

        let is_dispute_resolution = matches!(
            A::classify_action(action),
            Some(EscrowActionKind::ResolveDispute { .. })
        );

        // Check if any milestone that was disputed released funds without dispute resolution
        for m_before in &before_escrow.milestones {
            if m_before.is_disputed {
                if let Some(m_after) = after_escrow.milestones.iter().find(|m| m.id == m_before.id) {
                    if m_after.released_amount > m_before.released_amount && !is_dispute_resolution {
                        return InvariantResult::violation(format!(
                            "Dispute freeze bypassed: milestone {} released funds ({} -> {}) while in dispute without dispute resolution",
                            m_before.id, m_before.released_amount, m_after.released_amount
                        ));
                    }
                }
            }
        }

        // If the contract action was an attempt to approve/release a disputed milestone, it must not succeed
        if let Some(action_kind) = A::classify_action(action) {
            match action_kind {
                EscrowActionKind::ApproveMilestone { milestone_id }
                | EscrowActionKind::ApprovePartial { milestone_id, .. }
                | EscrowActionKind::ClaimAutoRelease { milestone_id } => {
                    if let Some(m_before) = before_escrow.milestones.iter().find(|m| m.id == milestone_id) {
                        if m_before.is_disputed && result.is_ok() {
                            return InvariantResult::violation(format!(
                                "Dispute freeze bypassed: action {:?} succeeded on disputed milestone {}",
                                action_kind, milestone_id
                            ));
                        }
                    }
                }
                _ => {}
            }
        }

        InvariantResult::Pass
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Invariant Pack Builder
// ─────────────────────────────────────────────────────────────────────────────

/// Constructs a complete [`InvariantSet`] containing all standard escrow invariants.
///
/// Includes:
/// - [`TotalLockedConservation`]
/// - [`NoDoubleRelease`]
/// - [`NoReleaseWithoutApproval`]
/// - [`DisputeFreezeCannotBeBypassed`]
pub fn escrow_invariant_pack<A: EscrowAdapter>() -> InvariantSet<A> {
    InvariantSet::new()
        .with(TotalLockedConservation)
        .with(NoDoubleRelease)
        .with(NoReleaseWithoutApproval)
        .with(DisputeFreezeCannotBeBypassed)
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockEscrowAdapter;

    #[derive(Debug, Clone)]
    struct MockAction(EscrowActionKind);

    #[derive(Debug, Clone)]
    struct MockState(EscrowStateSnapshot);

    #[derive(thiserror::Error, Debug)]
    #[error("mock error")]
    struct MockError;

    impl ContractAdapter for MockEscrowAdapter {
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

    impl EscrowAdapter for MockEscrowAdapter {
        fn inspect_escrow(state: &Self::State) -> EscrowStateSnapshot {
            state.0.clone()
        }

        fn classify_action(action: &Self::Action) -> Option<EscrowActionKind> {
            Some(action.0.clone())
        }
    }

    fn sample_snapshot(locked: i128, released: i128, refunded: i128) -> EscrowStateSnapshot {
        EscrowStateSnapshot {
            total_locked: locked,
            total_released: released,
            total_refunded: refunded,
            is_funded: true,
            is_disputed: false,
            milestones: vec![
                MilestoneSnapshot {
                    id: 0,
                    amount: 5000,
                    released_amount: released,
                    is_approved: true,
                    is_released: released == 5000,
                    is_refunded: false,
                    is_disputed: false,
                    status: if released == 5000 {
                        MilestoneStatusKind::Released
                    } else {
                        MilestoneStatusKind::Delivered
                    },
                },
                MilestoneSnapshot {
                    id: 1,
                    amount: 5000,
                    released_amount: 0,
                    is_approved: false,
                    is_released: false,
                    is_refunded: refunded == 5000,
                    is_disputed: false,
                    status: if refunded == 5000 {
                        MilestoneStatusKind::Refunded
                    } else {
                        MilestoneStatusKind::Pending
                    },
                },
            ],
            contract_token_balance: Some(locked),
        }
    }

    #[test]
    fn test_total_locked_conservation_holds() {
        let pack = escrow_invariant_pack::<MockEscrowAdapter>();

        // Total = 10000, locked = 10000, released = 0, refunded = 0
        let s0 = MockState(sample_snapshot(10000, 0, 0));
        assert!(pack.check_initial(&s0).is_empty());

        // Milestone 0 released: locked = 5000, released = 5000, refunded = 0
        let s1 = MockState(sample_snapshot(5000, 5000, 0));
        assert!(pack.check_initial(&s1).is_empty());

        // Milestone 1 refunded: locked = 0, released = 5000, refunded = 5000
        let s2 = MockState(sample_snapshot(0, 5000, 5000));
        assert!(pack.check_initial(&s2).is_empty());
    }

    #[test]
    fn test_total_locked_conservation_detects_leak() {
        let inv: Box<dyn Invariant<MockEscrowAdapter>> = Box::new(TotalLockedConservation);

        // Leaked 100 tokens: locked is 4900 instead of 5000
        let bad_state = MockState(sample_snapshot(4900, 5000, 0));
        let res = inv.check_state(&bad_state);
        assert!(res.is_violation());
    }

    #[test]
    fn test_no_double_release_detects_violation() {
        let inv: Box<dyn Invariant<MockEscrowAdapter>> = Box::new(NoDoubleRelease);

        let before = MockState(sample_snapshot(5000, 5000, 0)); // Milestone 0 already released (5000)
        let mut bad_snap = sample_snapshot(5000, 5000, 0);
        bad_snap.milestones[0].released_amount = 6000; // Increased released amount!
        let after = MockState(bad_snap);

        let action = MockAction(EscrowActionKind::ApproveMilestone { milestone_id: 0 });
        let res = inv.check_transition(&before, &after, &action, &ActionResult::Ok);
        assert!(res.is_violation());
    }

    #[test]
    fn test_no_release_without_approval_detects_violation() {
        let inv: Box<dyn Invariant<MockEscrowAdapter>> = Box::new(NoReleaseWithoutApproval);

        let before = MockState(sample_snapshot(10000, 0, 0)); // Milestone 1 is_approved == false
        let mut after_snap = sample_snapshot(5000, 0, 0);
        after_snap.milestones[1].released_amount = 5000; // Released unapproved milestone 1!
        let after = MockState(after_snap);

        let action = MockAction(EscrowActionKind::ApproveMilestone { milestone_id: 1 });
        let res = inv.check_transition(&before, &after, &action, &ActionResult::Ok);
        assert!(res.is_violation());
    }

    #[test]
    fn test_dispute_freeze_detects_violation() {
        let inv: Box<dyn Invariant<MockEscrowAdapter>> = Box::new(DisputeFreezeCannotBeBypassed);

        let mut before_snap = sample_snapshot(10000, 0, 0);
        before_snap.is_disputed = true;
        before_snap.milestones[0].is_disputed = true;
        let before = MockState(before_snap);

        let mut after_snap = sample_snapshot(5000, 5000, 0);
        after_snap.milestones[0].released_amount = 5000;
        let after = MockState(after_snap);

        // Regular approve action instead of dispute resolution
        let action = MockAction(EscrowActionKind::ApproveMilestone { milestone_id: 0 });
        let res = inv.check_transition(&before, &after, &action, &ActionResult::Ok);
        assert!(res.is_violation());
    }
}

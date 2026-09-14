//! Split-payment adapter trait, normalized snapshots, and pre-built financial invariants.
//!
//! This module provides the [`SplitAdapter`] trait and domain invariants for
//! revenue-sharing, royalty splitting, and multi-party payment distribution on Soroban:
//! - [`ShareSumConservation`]: Asserts that recipient percentage shares strictly sum
//!   to 10,000 basis points (100.00%).
//! - [`SplitPayoutConservation`]: Verifies that distributed payouts plus dust strictly
//!   equal incoming deposits at all times.
//! - [`NoDuplicatePayout`]: Guarantees that finalized payment batches cannot be paid twice
//!   and recipients cannot claim more than their allocation.
//! - [`SplitSolvency`]: Enforces that contract token balances cover all outstanding
//!   unclaimed allocations and unallocated dust.
//! - [`split_invariant_pack`]: Bundles all split-payment invariants into a ready-to-use [`InvariantSet`].

use crate::adapter::{ActionResult, ContractAdapter};
use crate::invariant::{Invariant, InvariantResult, InvariantSet};

/// Basis points representing exactly 100.00% (10,000 bps).
pub const TOTAL_SHARE_BPS: u32 = 10_000;

/// Normalized snapshot of an individual recipient's share allocation and claims.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecipientShareSnapshot {
    /// Identifier or address of the recipient.
    pub recipient: String,
    /// Configured share of distributions in basis points (0..=10,000).
    pub share_bps: u32,
    /// Cumulative tokens allocated to this recipient across all deposits.
    pub total_allocated: i128,
    /// Cumulative tokens withdrawn/claimed by this recipient.
    pub total_claimed: i128,
    /// Currently claimable unwithdrawn balance for this recipient.
    pub unclaimed_balance: i128,
}

/// Normalized snapshot of a payment split batch / deposit event.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SplitBatchSnapshot {
    /// Unique identifier of the split batch.
    pub batch_id: u64,
    /// Total incoming token deposit for this batch.
    pub total_amount: i128,
    /// Cumulative tokens distributed/allocated to recipients for this batch.
    pub distributed_amount: i128,
    /// Remainder / dust tokens left undistributed due to integer division truncation.
    pub unallocated_dust: i128,
    /// Whether this batch has been fully executed and distributed.
    pub is_distributed: bool,
}

/// Normalized snapshot of the split payment contract state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SplitStateSnapshot {
    /// Total basis points configured across all recipients (expected 10,000).
    pub total_shares_bps: u32,
    /// List of recipient share configurations and balances.
    pub recipients: Vec<RecipientShareSnapshot>,
    /// List of split batches processed by the contract.
    pub batches: Vec<SplitBatchSnapshot>,
    /// Cumulative total tokens deposited across all batches.
    pub total_deposited: i128,
    /// Cumulative total tokens distributed across all recipients.
    pub total_distributed: i128,
    /// Cumulative unallocated dust tokens retained by the contract.
    pub total_dust: i128,
    /// Underlying token balance held by the split contract, if observed.
    pub contract_token_balance: Option<i128>,
}

/// High-level classification of actions in a split payment system.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SplitActionKind {
    /// Updating or configuring recipient share ratios.
    ConfigureShares { total_bps: u32 },
    /// Depositing tokens and triggering a proportional split across recipients.
    DepositAndSplit { batch_id: u64, amount: i128 },
    /// A recipient claiming/withdrawing their accumulated balance.
    ClaimShare { recipient: String, amount: i128 },
    /// Distributing a pending batch.
    DistributeBatch { batch_id: u64 },
    /// Other custom action.
    Other,
}

/// Adapter trait bridging concrete Soroban split-payment contracts to normalized checks.
pub trait SplitAdapter: ContractAdapter {
    /// Extracts a [`SplitStateSnapshot`] from the adapter's concrete state.
    fn inspect_split(state: &Self::State) -> SplitStateSnapshot;

    /// Optionally classifies an action into standard split actions for transition checks.
    fn classify_action(_action: &Self::Action) -> Option<SplitActionKind> {
        None
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Invariant 1: ShareSumConservation
// ─────────────────────────────────────────────────────────────────────────────

/// Invariant: The sum of configured recipient shares must strictly equal
/// 10,000 basis points (100.00%).
///
/// Mathematical invariant:
/// `sum(recipient.share_bps) == 10,000`
///
/// Under-allocation traps funds permanently; over-allocation guarantees contract insolvency.
pub struct ShareSumConservation;

impl<A: SplitAdapter> Invariant<A> for ShareSumConservation {
    fn name(&self) -> &'static str {
        "ShareSumConservation"
    }

    fn description(&self) -> &'static str {
        "Sum of configured recipient share basis points must strictly equal 10,000 (100.00%)"
    }

    fn check_state(&self, state: &A::State) -> InvariantResult {
        let split = A::inspect_split(state);

        // If no recipients are configured yet (initial unconfigured state), pass
        if split.recipients.is_empty() && split.total_shares_bps == 0 {
            return InvariantResult::Pass;
        }

        let mut sum_bps: u32 = 0;
        for r in &split.recipients {
            if r.share_bps > TOTAL_SHARE_BPS {
                return InvariantResult::violation(format!(
                    "Recipient {} has invalid share_bps {} > {}",
                    r.recipient, r.share_bps, TOTAL_SHARE_BPS
                ));
            }
            sum_bps = match sum_bps.checked_add(r.share_bps) {
                Some(s) => s,
                None => {
                    return InvariantResult::violation("Overflow summing recipient share basis points");
                }
            };
        }

        if sum_bps != TOTAL_SHARE_BPS {
            return InvariantResult::violation(format!(
                "Share sum broken: sum of recipient shares is {} bps, expected exactly {} bps (100.00%)",
                sum_bps, TOTAL_SHARE_BPS
            ));
        }

        if split.total_shares_bps != sum_bps {
            return InvariantResult::violation(format!(
                "Reported total_shares_bps ({}) != actual sum of recipient shares ({})",
                split.total_shares_bps, sum_bps
            ));
        }

        InvariantResult::Pass
    }

    fn check_transition(
        &self,
        _before: &A::State,
        after: &A::State,
        action: &A::Action,
        result: &ActionResult,
    ) -> InvariantResult {
        // If an explicit action attempted to set invalid shares and succeeded, catch it
        if let Some(SplitActionKind::ConfigureShares { total_bps }) = A::classify_action(action) {
            if total_bps != TOTAL_SHARE_BPS && result.is_ok() {
                return InvariantResult::violation(format!(
                    "ConfigureShares action succeeded with invalid total_bps {} (expected 10,000)",
                    total_bps
                ));
            }
        }

        // Also assert that the resulting state maintains share conservation
        <Self as Invariant<A>>::check_state(self, after)
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Invariant 2: SplitPayoutConservation
// ─────────────────────────────────────────────────────────────────────────────

/// Invariant: Total deposited tokens must strictly equal distributed tokens
/// plus unallocated dust plus unclaimed balances.
///
/// Mathematical invariant:
/// - Per-batch: `batch.total_amount == batch.distributed_amount + batch.unallocated_dust`
/// - Batch bounds: `batch.distributed_amount <= batch.total_amount`
/// - Global: `total_deposited == total_distributed + total_dust + sum(unclaimed_balance)`
pub struct SplitPayoutConservation;

impl<A: SplitAdapter> Invariant<A> for SplitPayoutConservation {
    fn name(&self) -> &'static str {
        "SplitPayoutConservation"
    }

    fn description(&self) -> &'static str {
        "Total deposited tokens must strictly equal distributed tokens + dust + unclaimed balances, and payouts cannot exceed deposits"
    }

    fn check_state(&self, state: &A::State) -> InvariantResult {
        let split = A::inspect_split(state);

        let mut sum_batch_total: i128 = 0;
        let mut sum_batch_distributed: i128 = 0;
        let mut sum_batch_dust: i128 = 0;

        for b in &split.batches {
            if b.total_amount < 0 || b.distributed_amount < 0 || b.unallocated_dust < 0 {
                return InvariantResult::violation(format!(
                    "Batch {} has negative financial amounts: total={}, distributed={}, dust={}",
                    b.batch_id, b.total_amount, b.distributed_amount, b.unallocated_dust
                ));
            }

            if b.distributed_amount > b.total_amount {
                return InvariantResult::violation(format!(
                    "Batch {} distributed_amount ({}) exceeds total_amount ({})",
                    b.batch_id, b.distributed_amount, b.total_amount
                ));
            }

            let expected_total = match b.distributed_amount.checked_add(b.unallocated_dust) {
                Some(v) => v,
                None => {
                    return InvariantResult::violation(format!(
                        "Overflow adding distributed and dust on batch {}",
                        b.batch_id
                    ));
                }
            };

            if b.is_distributed && b.total_amount != expected_total {
                return InvariantResult::violation(format!(
                    "Batch {} conservation broken: total ({}) != distributed ({}) + dust ({})",
                    b.batch_id, b.total_amount, b.distributed_amount, b.unallocated_dust
                ));
            }

            sum_batch_total = match sum_batch_total.checked_add(b.total_amount) {
                Some(v) => v,
                None => return InvariantResult::violation("Overflow summing batch totals"),
            };
            sum_batch_distributed = match sum_batch_distributed.checked_add(b.distributed_amount) {
                Some(v) => v,
                None => return InvariantResult::violation("Overflow summing batch distributed"),
            };
            sum_batch_dust = match sum_batch_dust.checked_add(b.unallocated_dust) {
                Some(v) => v,
                None => return InvariantResult::violation("Overflow summing batch dust"),
            };
        }

        // Check recipients
        let mut sum_unclaimed: i128 = 0;
        let mut sum_allocated: i128 = 0;
        let mut sum_claimed: i128 = 0;

        for r in &split.recipients {
            if r.total_allocated < 0 || r.total_claimed < 0 || r.unclaimed_balance < 0 {
                return InvariantResult::violation(format!(
                    "Recipient {} has negative balance: allocated={}, claimed={}, unclaimed={}",
                    r.recipient, r.total_allocated, r.total_claimed, r.unclaimed_balance
                ));
            }

            let expected_alloc = match r.total_claimed.checked_add(r.unclaimed_balance) {
                Some(v) => v,
                None => {
                    return InvariantResult::violation(format!(
                        "Overflow adding claimed and unclaimed for recipient {}",
                        r.recipient
                    ));
                }
            };

            if r.total_allocated != expected_alloc {
                return InvariantResult::violation(format!(
                    "Recipient {} balance mismatch: total_allocated ({}) != claimed ({}) + unclaimed ({})",
                    r.recipient, r.total_allocated, r.total_claimed, r.unclaimed_balance
                ));
            }

            sum_allocated = match sum_allocated.checked_add(r.total_allocated) {
                Some(v) => v,
                None => return InvariantResult::violation("Overflow summing recipient allocated"),
            };
            sum_claimed = match sum_claimed.checked_add(r.total_claimed) {
                Some(v) => v,
                None => return InvariantResult::violation("Overflow summing recipient claimed"),
            };
            sum_unclaimed = match sum_unclaimed.checked_add(r.unclaimed_balance) {
                Some(v) => v,
                None => return InvariantResult::violation("Overflow summing recipient unclaimed"),
            };
        }

        // Global conservation
        if split.total_deposited != sum_batch_total {
            return InvariantResult::violation(format!(
                "Global total_deposited ({}) != sum of batch totals ({})",
                split.total_deposited, sum_batch_total
            ));
        }

        if split.total_dust != sum_batch_dust {
            return InvariantResult::violation(format!(
                "Global total_dust ({}) != sum of batch dust ({})",
                split.total_dust, sum_batch_dust
            ));
        }

        let expected_global_accounted = split
            .total_distributed
            .checked_add(split.total_dust)
            .and_then(|v| v.checked_add(sum_unclaimed));

        match expected_global_accounted {
            Some(accounted) => {
                if split.total_deposited != accounted {
                    return InvariantResult::violation(format!(
                        "Global conservation broken: total_deposited ({}) != total_distributed ({}) + dust ({}) + unclaimed ({}) [accounted: {}]",
                        split.total_deposited, split.total_distributed, split.total_dust, sum_unclaimed, accounted
                    ));
                }
            }
            None => {
                return InvariantResult::violation("Overflow in global split conservation calculation");
            }
        }

        InvariantResult::Pass
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Invariant 3: NoDuplicatePayout
// ─────────────────────────────────────────────────────────────────────────────

/// Invariant: Finalized batches cannot be distributed twice, and recipients
/// cannot claim more than their cumulative allocated entitlement.
pub struct NoDuplicatePayout;

impl<A: SplitAdapter> Invariant<A> for NoDuplicatePayout {
    fn name(&self) -> &'static str {
        "NoDuplicatePayout"
    }

    fn description(&self) -> &'static str {
        "Finalized batches cannot be re-distributed and recipients cannot claim beyond allocated entitlement"
    }

    fn check_state(&self, state: &A::State) -> InvariantResult {
        let split = A::inspect_split(state);

        for r in &split.recipients {
            if r.total_claimed > r.total_allocated {
                return InvariantResult::violation(format!(
                    "Over-claim detected: recipient {} claimed {} exceeding allocated {}",
                    r.recipient, r.total_claimed, r.total_allocated
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
        let before_split = A::inspect_split(before);
        let after_split = A::inspect_split(after);

        // Check for batch re-distribution
        for b_before in &before_split.batches {
            if b_before.is_distributed {
                if let Some(b_after) = after_split.batches.iter().find(|b| b.batch_id == b_before.batch_id) {
                    if b_after.distributed_amount > b_before.distributed_amount {
                        return InvariantResult::violation(format!(
                            "Duplicate payout on batch {}: batch was already distributed (amount: {}), but distributed amount increased to {}",
                            b_before.batch_id, b_before.distributed_amount, b_after.distributed_amount
                        ));
                    }
                }
            }
        }

        // Check that unclaimed balances don't decrease by more than the claim increase
        for r_after in &after_split.recipients {
            if let Some(r_before) = before_split.recipients.iter().find(|r| r.recipient == r_after.recipient) {
                let claim_delta = r_after.total_claimed - r_before.total_claimed;
                if claim_delta > 0 {
                    if claim_delta > r_before.unclaimed_balance {
                        return InvariantResult::violation(format!(
                            "Recipient {} claimed {} which exceeds prior unclaimed balance {}",
                            r_after.recipient, claim_delta, r_before.unclaimed_balance
                        ));
                    }
                }
            }
        }

        InvariantResult::Pass
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Invariant 4: SplitSolvency
// ─────────────────────────────────────────────────────────────────────────────

/// Invariant: The contract's token balance must strictly cover all outstanding
/// unclaimed recipient allocations and unallocated dust.
///
/// Mathematical invariant:
/// `contract_token_balance >= sum(unclaimed_balance) + total_dust`
pub struct SplitSolvency;

impl<A: SplitAdapter> Invariant<A> for SplitSolvency {
    fn name(&self) -> &'static str {
        "SplitSolvency"
    }

    fn description(&self) -> &'static str {
        "Contract token balance must cover all outstanding liabilities (unclaimed recipient balances + retained dust)"
    }

    fn check_state(&self, state: &A::State) -> InvariantResult {
        let split = A::inspect_split(state);

        if let Some(token_bal) = split.contract_token_balance {
            let total_unclaimed: i128 = split.recipients.iter().map(|r| r.unclaimed_balance).sum();
            let total_liabilities = match total_unclaimed.checked_add(split.total_dust) {
                Some(v) => v,
                None => return InvariantResult::violation("Overflow calculating split liabilities"),
            };

            if token_bal < total_liabilities {
                return InvariantResult::violation(format!(
                    "Split contract insolvency: token balance ({}) < outstanding liabilities ({}) [unclaimed: {}, dust: {}]",
                    token_bal, total_liabilities, total_unclaimed, split.total_dust
                ));
            }
        }

        InvariantResult::Pass
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Invariant Pack Builder
// ─────────────────────────────────────────────────────────────────────────────

/// Constructs a complete [`InvariantSet`] containing all standard split payment invariants.
///
/// Includes:
/// - [`ShareSumConservation`]
/// - [`SplitPayoutConservation`]
/// - [`NoDuplicatePayout`]
/// - [`SplitSolvency`]
pub fn split_invariant_pack<A: SplitAdapter>() -> InvariantSet<A> {
    InvariantSet::new()
        .with(ShareSumConservation)
        .with(SplitPayoutConservation)
        .with(NoDuplicatePayout)
        .with(SplitSolvency)
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockSplitAdapter;

    #[derive(Debug, Clone)]
    struct MockAction(SplitActionKind);

    #[derive(Debug, Clone)]
    struct MockState(SplitStateSnapshot);

    #[derive(thiserror::Error, Debug)]
    #[error("mock error")]
    struct MockError;

    impl ContractAdapter for MockSplitAdapter {
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

    impl SplitAdapter for MockSplitAdapter {
        fn inspect_split(state: &Self::State) -> SplitStateSnapshot {
            state.0.clone()
        }

        fn classify_action(action: &Self::Action) -> Option<SplitActionKind> {
            Some(action.0.clone())
        }
    }

    fn sample_recipients() -> Vec<RecipientShareSnapshot> {
        vec![
            RecipientShareSnapshot {
                recipient: "alice".into(),
                share_bps: 6_000,
                total_allocated: 6_000,
                total_claimed: 2_000,
                unclaimed_balance: 4_000,
            },
            RecipientShareSnapshot {
                recipient: "bob".into(),
                share_bps: 4_000,
                total_allocated: 4_000,
                total_claimed: 1_000,
                unclaimed_balance: 3_000,
            },
        ]
    }

    fn sample_state(recipients: Vec<RecipientShareSnapshot>, total_shares: u32) -> MockState {
        let batches = vec![SplitBatchSnapshot {
            batch_id: 1,
            total_amount: 10_000,
            distributed_amount: 10_000,
            unallocated_dust: 0,
            is_distributed: true,
        }];

        let total_claimed: i128 = recipients.iter().map(|r| r.total_claimed).sum();
        let total_unclaimed: i128 = recipients.iter().map(|r| r.unclaimed_balance).sum();

        MockState(SplitStateSnapshot {
            total_shares_bps: total_shares,
            recipients,
            batches,
            total_deposited: 10_000,
            total_distributed: total_claimed,
            total_dust: 0,
            contract_token_balance: Some(total_unclaimed),
        })
    }

    #[test]
    fn test_share_sum_conservation_passes() {
        let inv = ShareSumConservation;
        let state = sample_state(sample_recipients(), 10_000);
        let res = Invariant::<MockSplitAdapter>::check_state(&inv, &state);
        assert!(res.is_pass());
    }

    #[test]
    fn test_share_sum_conservation_catches_under_allocation() {
        let inv = ShareSumConservation;
        let mut recs = sample_recipients();
        recs[0].share_bps = 5_000; // sum = 5,000 + 4,000 = 9,000 != 10,000
        let state = sample_state(recs, 9_000);
        let res = Invariant::<MockSplitAdapter>::check_state(&inv, &state);
        assert!(res.is_violation());
        assert!(res.reason().unwrap().contains("Share sum broken"));
    }

    #[test]
    fn test_payout_conservation_passes() {
        let inv = SplitPayoutConservation;
        let state = sample_state(sample_recipients(), 10_000);
        let res = Invariant::<MockSplitAdapter>::check_state(&inv, &state);
        assert!(res.is_pass());
    }

    #[test]
    fn test_payout_conservation_catches_mismatch() {
        let inv = SplitPayoutConservation;
        let mut state = sample_state(sample_recipients(), 10_000);
        state.0.batches[0].distributed_amount = 12_000; // distributed > total!
        let res = Invariant::<MockSplitAdapter>::check_state(&inv, &state);
        assert!(res.is_violation());
        assert!(res.reason().unwrap().contains("exceeds total_amount"));
    }

    #[test]
    fn test_no_duplicate_payout_catches_replay() {
        let inv = NoDuplicatePayout;
        let state_before = sample_state(sample_recipients(), 10_000);

        let mut state_after = state_before.clone();
        state_after.0.batches[0].distributed_amount = 20_000; // paid twice!

        let action = MockAction(SplitActionKind::DistributeBatch { batch_id: 1 });
        let res = Invariant::<MockSplitAdapter>::check_transition(
            &inv,
            &state_before,
            &state_after,
            &action,
            &ActionResult::Ok,
        );
        assert!(res.is_violation());
        assert!(res.reason().unwrap().contains("Duplicate payout on batch"));
    }

    #[test]
    fn test_solvency_catches_balance_deficit() {
        let inv = SplitSolvency;
        let mut state = sample_state(sample_recipients(), 10_000);
        state.0.contract_token_balance = Some(3_000); // liabilities are 7_000!
        let res = Invariant::<MockSplitAdapter>::check_state(&inv, &state);
        assert!(res.is_violation());
        assert!(res.reason().unwrap().contains("contract insolvency"));
    }

    #[test]
    fn test_split_pack_builder() {
        let pack = split_invariant_pack::<MockSplitAdapter>();
        assert_eq!(pack.len(), 4);
    }
}

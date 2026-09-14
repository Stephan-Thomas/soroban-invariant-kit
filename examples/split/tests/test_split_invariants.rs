//! Property-based state-machine invariant tests for SplitContract.

use example_split::adapter::{SplitAction, SplitContractAdapter};
use proptest::prelude::*;
use soroban_invariant_kit_core::split::{
    split_invariant_pack, NoDuplicatePayout, ShareSumConservation, SplitPayoutConservation,
    SplitSolvency,
};
use soroban_invariant_kit_core::{ActionResult, ContractAdapter, Invariant};
use soroban_invariant_kit_harness::invariant_test;

/// Generator for arbitrary payment splitting actions.
fn arb_split_action() -> impl Strategy<Value = SplitAction> {
    prop_oneof![
        (1_000i128..=100_000i128).prop_map(|amount| SplitAction::DepositAndSplit { amount }),
        (0usize..=2).prop_map(|recipient_idx| SplitAction::Claim { recipient_idx }),
        // Mix of valid and invalid share updates to stress-test contract validation & invariants
        prop_oneof![
            Just(SplitAction::UpdateShares {
                shares_bps: vec![5_000, 3_000, 2_000],
            }),
            Just(SplitAction::UpdateShares {
                shares_bps: vec![4_000, 4_000, 2_000],
            }),
            Just(SplitAction::UpdateShares {
                shares_bps: vec![3_334, 3_333, 3_333],
            }),
            Just(SplitAction::UpdateShares {
                shares_bps: vec![7_000, 2_000, 1_000],
            }),
            // Invalid split (sum != 10,000) that should revert in contract
            Just(SplitAction::UpdateShares {
                shares_bps: vec![5_000, 5_000, 1_000], // 11,000
            }),
        ],
    ]
}

// ─────────────────────────────────────────────────────────────────────────────
// Test 1: Property-Based Invariant Test on SplitContract
// ─────────────────────────────────────────────────────────────────────────────

invariant_test!(
    test_split_invariants_hold,
    SplitContractAdapter,
    proptest::collection::vec(arb_split_action(), 1..25),
    split_invariant_pack::<SplitContractAdapter>(),
    100
);

// ─────────────────────────────────────────────────────────────────────────────
// Test 2: Negative Test - Detects Share Sum Mismatch
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_detects_share_sum_violation() {
    let mut adapter = SplitContractAdapter::setup().unwrap();
    let invariant = ShareSumConservation;

    // Normal initial state is 10,000 bps
    let valid_state = adapter.snapshot().unwrap();
    let valid_res = Invariant::<SplitContractAdapter>::check_state(&invariant, &valid_state);
    assert!(valid_res.is_pass());

    // Attempting to configure shares with sum != 10,000 in the contract must revert
    let bad_action = SplitAction::UpdateShares {
        shares_bps: vec![4_000, 3_000, 2_000], // 9,000 bps
    };
    let step_res = adapter.step(&bad_action).unwrap();
    assert!(
        step_res.is_reverted(),
        "Contract must reject invalid share sum"
    );

    // Verify invariant flags any synthetic state where share sum != 10,000
    let mut bad_state = valid_state.clone();
    bad_state.snapshot.recipients[0].share_bps = 4_000; // sum = 4,000 + 3,000 + 2,000 = 9,000
    bad_state.snapshot.total_shares_bps = 9_000;

    let check_res = Invariant::<SplitContractAdapter>::check_state(&invariant, &bad_state);
    assert!(check_res.is_violation());
    assert!(check_res.reason().unwrap().contains("Share sum broken"));
}

// ─────────────────────────────────────────────────────────────────────────────
// Test 3: Negative Test - Detects Payout Conservation Violations
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_detects_payout_conservation_violation() {
    let mut adapter = SplitContractAdapter::setup().unwrap();
    let invariant = SplitPayoutConservation;

    // Deposit and split 10,000 tokens
    adapter
        .step(&SplitAction::DepositAndSplit { amount: 10_000 })
        .unwrap();

    let valid_state = adapter.snapshot().unwrap();
    let valid_res = Invariant::<SplitContractAdapter>::check_state(&invariant, &valid_state);
    assert!(valid_res.is_pass());

    // Case 1: Batch distributed amount leaked / inflated beyond total deposit
    let mut bad_state = valid_state.clone();
    bad_state.snapshot.batches[0].distributed_amount = 15_000; // deposit was only 10,000!

    let bad_res = Invariant::<SplitContractAdapter>::check_state(&invariant, &bad_state);
    assert!(bad_res.is_violation());
    assert!(bad_res
        .reason()
        .unwrap()
        .contains("exceeds total_amount"));

    // Case 2: Global balance unaccounted
    let mut bad_global = valid_state.clone();
    bad_global.snapshot.total_deposited = 20_000; // total deposited mismatch
    let bad_global_res = Invariant::<SplitContractAdapter>::check_state(&invariant, &bad_global);
    assert!(bad_global_res.is_violation());
    assert!(bad_global_res
        .reason()
        .unwrap()
        .contains("Global total_deposited"));
}

// ─────────────────────────────────────────────────────────────────────────────
// Test 4: Negative Test - Detects Duplicate Payout & Over-Claiming
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_detects_duplicate_payout_violation() {
    let mut adapter = SplitContractAdapter::setup().unwrap();
    let invariant = NoDuplicatePayout;

    // Deposit 10,000 tokens
    adapter
        .step(&SplitAction::DepositAndSplit { amount: 10_000 })
        .unwrap();

    let before_state = adapter.snapshot().unwrap();
    assert!(before_state.snapshot.batches[0].is_distributed);

    // Replay attack / mock duplication: batch distributed amount increases after finalization
    let mut after_state = before_state.clone();
    after_state.snapshot.batches[0].distributed_amount = 20_000;

    let action = SplitAction::DepositAndSplit { amount: 10_000 };
    let trans_res = Invariant::<SplitContractAdapter>::check_transition(
        &invariant,
        &before_state,
        &after_state,
        &action,
        &ActionResult::Ok,
    );
    assert!(trans_res.is_violation());
    assert!(trans_res
        .reason()
        .unwrap()
        .contains("Duplicate payout on batch"));

    // Case 2: Over-claim beyond total allocated
    let mut over_claim_state = before_state.clone();
    over_claim_state.snapshot.recipients[0].total_claimed =
        over_claim_state.snapshot.recipients[0].total_allocated + 500;
    let over_claim_res =
        Invariant::<SplitContractAdapter>::check_state(&invariant, &over_claim_state);
    assert!(over_claim_res.is_violation());
    assert!(over_claim_res.reason().unwrap().contains("Over-claim detected"));
}

// ─────────────────────────────────────────────────────────────────────────────
// Test 5: Negative Test - Detects Token Balance Insolvency
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_detects_solvency_violation() {
    let mut adapter = SplitContractAdapter::setup().unwrap();
    let invariant = SplitSolvency;

    // Deposit 50,000 tokens (50% = 25,000 to r0, 30% = 15,000 to r1, 20% = 10,000 to r2)
    adapter
        .step(&SplitAction::DepositAndSplit { amount: 50_000 })
        .unwrap();

    let valid_state = adapter.snapshot().unwrap();
    assert_eq!(valid_state.snapshot.contract_token_balance, Some(50_000));
    let valid_res = Invariant::<SplitContractAdapter>::check_state(&invariant, &valid_state);
    assert!(valid_res.is_pass());

    // Recipient 0 claims 25,000
    adapter.step(&SplitAction::Claim { recipient_idx: 0 }).unwrap();
    let after_claim = adapter.snapshot().unwrap();
    assert_eq!(after_claim.snapshot.contract_token_balance, Some(25_000));
    let claim_res = Invariant::<SplitContractAdapter>::check_state(&invariant, &after_claim);
    assert!(claim_res.is_pass());

    // Contract token balance artificially reduced / drained
    let mut insolvent_state = after_claim.clone();
    insolvent_state.snapshot.contract_token_balance = Some(10_000); // liabilities are 25,000!

    let insolv_res =
        Invariant::<SplitContractAdapter>::check_state(&invariant, &insolvent_state);
    assert!(insolv_res.is_violation());
    assert!(insolv_res
        .reason()
        .unwrap()
        .contains("Split contract insolvency"));
}

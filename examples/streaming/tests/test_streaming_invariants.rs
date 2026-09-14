//! Property-based state-machine invariant tests for StreamPay payment streaming.

use example_streaming::adapter::{StreamingAction, StreamingContractAdapter};
use proptest::prelude::*;
use soroban_invariant_kit_core::streaming::{
    streaming_invariant_pack, ClaimableNeverExceedsAccrual, NoClaimAfterCloseOrCancel,
    StreamingBalanceConservation, StreamingMonotonicProgress,
};
use soroban_invariant_kit_core::{ActionResult, ContractAdapter, Invariant};
use soroban_invariant_kit_harness::invariant_test;

/// Generator for arbitrary payment streaming actions.
fn arb_streaming_action() -> impl Strategy<Value = StreamingAction> {
    prop_oneof![
        (1_000i128..=50_000i128, 100u64..=5_000u64).prop_map(|(amount, duration)| {
            StreamingAction::CreateStream { amount, duration }
        }),
        (1u64..=4).prop_map(|stream_id| StreamingAction::Withdraw { stream_id }),
        (1u64..=4).prop_map(|stream_id| StreamingAction::Cancel { stream_id }),
        (1u64..=4, 500i128..=10_000i128).prop_map(|(stream_id, amount)| {
            StreamingAction::TopUp { stream_id, amount }
        }),
        (1u64..=4, 100u64..=2_000u64).prop_map(|(stream_id, extra_seconds)| {
            StreamingAction::ExtendStream {
                stream_id,
                extra_seconds,
            }
        }),
        (10u64..=1_000u64).prop_map(|seconds| StreamingAction::AdvanceLedgerTime { seconds }),
    ]
}

// ─────────────────────────────────────────────────────────────────────────────
// Test 1: Property-Based Invariant Test on StreamPay Contract
// ─────────────────────────────────────────────────────────────────────────────

invariant_test!(
    test_streaming_invariants_hold,
    StreamingContractAdapter,
    proptest::collection::vec(arb_streaming_action(), 1..25),
    streaming_invariant_pack::<StreamingContractAdapter>(),
    100
);

// ─────────────────────────────────────────────────────────────────────────────
// Test 2: Negative Test - Detects Claim Exceeding Accrued Vesting
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_detects_claim_exceeding_accrual_violation() {
    let mut adapter = StreamingContractAdapter::setup().unwrap();
    let invariant = ClaimableNeverExceedsAccrual;

    // Create a 1,000 second stream for 10,000 tokens
    let create_res = adapter
        .step(&StreamingAction::CreateStream {
            amount: 10_000,
            duration: 1_000,
        })
        .unwrap();
    assert!(create_res.is_ok());

    // Advance 100 seconds (10% vested = 1,000 tokens)
    adapter
        .step(&StreamingAction::AdvanceLedgerTime { seconds: 100 })
        .unwrap();

    let before_state = adapter.snapshot().unwrap();
    assert_eq!(before_state.snapshot.streams[0].accrued_vested, 1_000);
    assert_eq!(before_state.snapshot.streams[0].withdrawn, 0);

    // Normal withdraw succeeds for accrued amount
    let withdraw_res = adapter
        .step(&StreamingAction::Withdraw { stream_id: 1 })
        .unwrap();
    assert!(withdraw_res.is_ok());

    // Now construct a synthetic violation state where withdrawn exceeds accrued_vested
    let mut bad_state = adapter.snapshot().unwrap();
    bad_state.snapshot.streams[0].withdrawn = 5_000; // only 1,000 vested!

    let check_res = Invariant::<StreamingContractAdapter>::check_state(&invariant, &bad_state);
    assert!(check_res.is_violation());
    assert!(check_res
        .reason()
        .unwrap()
        .contains("Over-withdrawal detected"));
}

// ─────────────────────────────────────────────────────────────────────────────
// Test 3: Negative Test - Detects Withdrawal After Cancellation
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_detects_withdrawal_after_cancel_violation() {
    let mut adapter = StreamingContractAdapter::setup().unwrap();
    let invariant = NoClaimAfterCloseOrCancel;

    // Create stream
    adapter
        .step(&StreamingAction::CreateStream {
            amount: 20_000,
            duration: 2_000,
        })
        .unwrap();

    // Advance 500 seconds (25% vested = 5,000 tokens)
    adapter
        .step(&StreamingAction::AdvanceLedgerTime { seconds: 500 })
        .unwrap();

    // Cancel stream (recipient paid 5,000 vested, sender refunded 15,000 unvested)
    let cancel_res = adapter
        .step(&StreamingAction::Cancel { stream_id: 1 })
        .unwrap();
    assert!(cancel_res.is_ok());

    let state_after_cancel = adapter.snapshot().unwrap();
    assert_eq!(
        state_after_cancel.snapshot.streams[0].status,
        soroban_invariant_kit_core::StreamStatusKind::Cancelled
    );

    // Any subsequent withdraw attempt on the cancelled stream MUST be rejected by contract
    let subsequent_withdraw = adapter
        .step(&StreamingAction::Withdraw { stream_id: 1 })
        .unwrap();
    assert!(
        subsequent_withdraw.is_reverted(),
        "Contract must revert subsequent withdrawals on cancelled stream"
    );

    // Verify invariant detects any transition that attempts to increase withdrawn on cancelled stream
    let mut illegal_after_state = state_after_cancel.clone();
    illegal_after_state.snapshot.streams[0].withdrawn += 1_000;

    let action = StreamingAction::Withdraw { stream_id: 1 };
    let trans_res = Invariant::<StreamingContractAdapter>::check_transition(
        &invariant,
        &state_after_cancel,
        &illegal_after_state,
        &action,
        &ActionResult::Ok,
    );
    assert!(trans_res.is_violation());
    assert!(trans_res
        .reason()
        .unwrap()
        .contains("Post-termination claim detected"));
}

// ─────────────────────────────────────────────────────────────────────────────
// Test 4: Negative Test - Detects Balance Conservation & Insolvency Violations
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_detects_balance_insolvency_violation() {
    let mut adapter = StreamingContractAdapter::setup().unwrap();
    let invariant = StreamingBalanceConservation;

    // Create 2 streams: 10,000 each
    adapter
        .step(&StreamingAction::CreateStream {
            amount: 10_000,
            duration: 1_000,
        })
        .unwrap();
    adapter
        .step(&StreamingAction::CreateStream {
            amount: 10_000,
            duration: 1_000,
        })
        .unwrap();

    let valid_state = adapter.snapshot().unwrap();
    let valid_check = Invariant::<StreamingContractAdapter>::check_state(&invariant, &valid_state);
    assert!(valid_check.is_pass());

    // Case 1: Contract token balance leaked / insolvent
    let mut insolvent_state = valid_state.clone();
    insolvent_state.snapshot.contract_token_balance = Some(15_000); // liabilities are 20,000!

    let insolv_res =
        Invariant::<StreamingContractAdapter>::check_state(&invariant, &insolvent_state);
    assert!(insolv_res.is_violation());
    assert!(insolv_res
        .reason()
        .unwrap()
        .contains("Contract token balance insolvency"));

    // Case 2: Per-stream conservation broken (sum != total)
    let mut mismatch_state = valid_state.clone();
    mismatch_state.snapshot.streams[0].remaining = 5_000; // total is 10,000, withdrawn is 0 -> sum = 5,000 != 10,000

    let mismatch_res =
        Invariant::<StreamingContractAdapter>::check_state(&invariant, &mismatch_state);
    assert!(mismatch_res.is_violation());
    assert!(mismatch_res
        .reason()
        .unwrap()
        .contains("Per-stream balance conservation broken"));
}

// ─────────────────────────────────────────────────────────────────────────────
// Test 5: Monotonic Vesting Progress Test
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_streaming_monotonic_progress_holds() {
    let mut adapter = StreamingContractAdapter::setup().unwrap();
    let invariant = StreamingMonotonicProgress;

    adapter
        .step(&StreamingAction::CreateStream {
            amount: 100_000,
            duration: 10_000,
        })
        .unwrap();

    let s0 = adapter.snapshot().unwrap();

    // Advance 1,000 seconds
    adapter
        .step(&StreamingAction::AdvanceLedgerTime { seconds: 1_000 })
        .unwrap();
    let s1 = adapter.snapshot().unwrap();

    let action = StreamingAction::AdvanceLedgerTime { seconds: 1_000 };
    let res = Invariant::<StreamingContractAdapter>::check_transition(
        &invariant,
        &s0,
        &s1,
        &action,
        &ActionResult::Ok,
    );
    assert!(res.is_pass());

    // Synthetic non-monotonic glitch check
    let mut s_bad = s1.clone();
    s_bad.snapshot.streams[0].accrued_vested = s0.snapshot.streams[0].accrued_vested - 1;
    let bad_res = Invariant::<StreamingContractAdapter>::check_transition(
        &invariant,
        &s0,
        &s_bad,
        &action,
        &ActionResult::Ok,
    );
    assert!(bad_res.is_violation());
    assert!(bad_res.reason().unwrap().contains("Monotonicity violation"));
}

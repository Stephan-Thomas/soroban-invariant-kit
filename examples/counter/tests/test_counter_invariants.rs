use example_counter::{
    counter_invariants, CounterAction, CounterAdapter, CounterState,
};
use proptest::prelude::*;
use soroban_invariant_kit_core::{
    Invariant, InvariantResult, InvariantSet,
};
use soroban_invariant_kit_harness::{invariant_test, InvariantRunner};

/// Strategy to generate randomized CounterActions.
fn arb_counter_action() -> impl Strategy<Value = CounterAction> {
    prop_oneof![
        (1u32..=10_000).prop_map(|amount| CounterAction::Increment { amount }),
        (1u32..=10_000).prop_map(|amount| CounterAction::Decrement { amount }),
        Just(CounterAction::Reset),
    ]
}

// 1. Proptest-driven invariant verification using the invariant_test! macro
invariant_test!(
    test_counter_invariants_hold,
    CounterAdapter,
    proptest::collection::vec(arb_counter_action(), 1..40),
    counter_invariants(),
    100
);

// 2. Unit test verifying that an invariant failure is properly caught, isolated, and formatted
struct CounterMustStaySmall;

impl Invariant<CounterAdapter> for CounterMustStaySmall {
    fn name(&self) -> &'static str {
        "CounterMustStaySmall"
    }

    fn check_state(&self, state: &CounterState) -> InvariantResult {
        if state.value <= 10 {
            InvariantResult::Pass
        } else {
            InvariantResult::violation(format!("Counter exceeded 10: value is {}", state.value))
        }
    }
}

#[test]
fn test_detects_and_reports_invariant_violation() {
    let runner = InvariantRunner::<CounterAdapter>::default();
    let invariants = InvariantSet::new().with(CounterMustStaySmall);

    // Run actions that will breach the threshold
    let actions = vec![
        CounterAction::Increment { amount: 5 },
        CounterAction::Increment { amount: 10 }, // 5 + 10 = 15 > 10!
    ];

    let result = runner.run_sequence(&actions, &invariants);
    assert!(result.is_err(), "Expected an invariant violation to be detected");

    let report = result.unwrap_err();
    assert_eq!(report.invariant_name, "CounterMustStaySmall");
    assert_eq!(report.failing_step_index, Some(2));
    assert!(report.reason.contains("Counter exceeded 10"));

    // Check report formatting
    let formatted = format!("{}", report);
    assert!(formatted.contains("INVARIANT VIOLATION: CounterMustStaySmall"));
    assert!(formatted.contains("Failed at step: #2"));
    assert!(formatted.contains("Action: Increment { amount: 10 }"));
}

#[test]
fn test_initial_state_invariant_checked() {
    struct CounterMustStartAt999;

    impl Invariant<CounterAdapter> for CounterMustStartAt999 {
        fn name(&self) -> &'static str {
            "CounterMustStartAt999"
        }

        fn check_state(&self, state: &CounterState) -> InvariantResult {
            if state.value == 999 {
                InvariantResult::Pass
            } else {
                InvariantResult::violation(format!("Expected initial 999, got {}", state.value))
            }
        }
    }

    let runner = InvariantRunner::<CounterAdapter>::default();
    let invariants = InvariantSet::new().with(CounterMustStartAt999);

    let actions = vec![CounterAction::Increment { amount: 1 }];
    let result = runner.run_sequence(&actions, &invariants);
    assert!(result.is_err());

    let report = result.unwrap_err();
    assert_eq!(report.invariant_name, "CounterMustStartAt999");
    assert_eq!(report.failing_step_index, None); // initial state failure
}

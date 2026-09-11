//! Invariants asserted against the Counter contract.

use soroban_invariant_kit_core::{ActionResult, Invariant, InvariantResult, InvariantSet};
use crate::adapter::{CounterAction, CounterAdapter, CounterState};

/// Invariant 1: The counter value must never become negative.
pub struct CounterNonNegative;

impl Invariant<CounterAdapter> for CounterNonNegative {
    fn name(&self) -> &'static str {
        "CounterNonNegative"
    }

    fn description(&self) -> &'static str {
        "Ensures that the counter value is always >= 0 across all states."
    }

    fn check_state(&self, state: &CounterState) -> InvariantResult {
        if state.value >= 0 {
            InvariantResult::Pass
        } else {
            InvariantResult::violation(format!(
                "Counter value is negative: {}",
                state.value
            ))
        }
    }
}

/// Invariant 2: Incrementing must strictly increase the state by the specified amount.
pub struct IncrementMatchesAmount;

impl Invariant<CounterAdapter> for IncrementMatchesAmount {
    fn name(&self) -> &'static str {
        "IncrementMatchesAmount"
    }

    fn description(&self) -> &'static str {
        "Ensures that an Increment action increases the counter exactly by `amount`."
    }

    fn check_transition(
        &self,
        before: &CounterState,
        after: &CounterState,
        action: &CounterAction,
        result: &ActionResult,
    ) -> InvariantResult {
        if !result.is_ok() {
            return InvariantResult::Pass;
        }

        if let CounterAction::Increment { amount } = action {
            let expected = before.value + (*amount as i64);
            if after.value == expected {
                InvariantResult::Pass
            } else {
                InvariantResult::violation(format!(
                    "Increment({}): expected value {}, got {}",
                    amount, expected, after.value
                ))
            }
        } else {
            InvariantResult::Pass
        }
    }
}

/// Invariant 3: Reset must set counter to 0.
pub struct ResetSetsToZero;

impl Invariant<CounterAdapter> for ResetSetsToZero {
    fn name(&self) -> &'static str {
        "ResetSetsToZero"
    }

    fn description(&self) -> &'static str {
        "Ensures that a Reset action always transitions the counter value to 0."
    }

    fn check_transition(
        &self,
        _before: &CounterState,
        after: &CounterState,
        action: &CounterAction,
        result: &ActionResult,
    ) -> InvariantResult {
        if !result.is_ok() {
            return InvariantResult::Pass;
        }

        if let CounterAction::Reset = action {
            if after.value == 0 {
                InvariantResult::Pass
            } else {
                InvariantResult::violation(format!(
                    "Reset did not set counter to 0: current value is {}",
                    after.value
                ))
            }
        } else {
            InvariantResult::Pass
        }
    }
}

/// Returns the standard invariant suite for the Counter contract.
pub fn counter_invariants() -> InvariantSet<CounterAdapter> {
    InvariantSet::new()
        .with(CounterNonNegative)
        .with(IncrementMatchesAmount)
        .with(ResetSetsToZero)
}

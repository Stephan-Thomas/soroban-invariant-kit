//! Invariant traits and evaluation collections.
//!
//! An **invariant** is a critical system invariant that must always hold true:
//! 1. **State invariant**: Holds for every individual snapshot in isolation (e.g., `balance >= 0`).
//! 2. **Transition invariant**: Holds across consecutive states (e.g., `after.balance <= before.balance`).
//! 3. **Action postcondition**: Asserts property preservation for specific actions (e.g., `after.count == before.count + step`).

use crate::adapter::{ActionResult, ContractAdapter};

/// Outcome of evaluating an invariant.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InvariantResult {
    /// The invariant holds.
    Pass,
    /// The invariant was violated, with an explanation.
    Violation {
        reason: String,
        details: Option<String>,
    },
}

impl InvariantResult {
    #[inline]
    pub fn is_pass(&self) -> bool {
        matches!(self, InvariantResult::Pass)
    }

    #[inline]
    pub fn is_violation(&self) -> bool {
        matches!(self, InvariantResult::Violation { .. })
    }

    pub fn violation(reason: impl Into<String>) -> Self {
        Self::Violation {
            reason: reason.into(),
            details: None,
        }
    }

    pub fn violation_with_details(reason: impl Into<String>, details: impl Into<String>) -> Self {
        Self::Violation {
            reason: reason.into(),
            details: Some(details.into()),
        }
    }
}

/// A safety or financial invariant evaluated against an adapter.
pub trait Invariant<A: ContractAdapter>: Send + Sync {
    /// Concise, descriptive name for the invariant.
    fn name(&self) -> &'static str;

    /// Extended description explaining what the invariant guarantees.
    fn description(&self) -> &'static str {
        ""
    }

    /// Evaluates a single-state condition.
    ///
    /// Invoked immediately after initial adapter setup, as well as after every subsequent step.
    fn check_state(&self, _state: &A::State) -> InvariantResult {
        InvariantResult::Pass
    }

    /// Evaluates a transition condition across consecutive states.
    ///
    /// Invoked after every step.
    fn check_transition(
        &self,
        _before: &A::State,
        _after: &A::State,
        _action: &A::Action,
        _result: &ActionResult,
    ) -> InvariantResult {
        InvariantResult::Pass
    }

    /// Comprehensive check evaluating state and transition invariants.
    fn check(
        &self,
        before: &A::State,
        after: &A::State,
        action: &A::Action,
        result: &ActionResult,
    ) -> InvariantResult {
        let state_res = self.check_state(after);
        if !state_res.is_pass() {
            return state_res;
        }
        self.check_transition(before, after, action, result)
    }
}

/// A recorded invariant violation.
#[derive(Debug, Clone)]
pub struct InvariantViolation {
    pub invariant_name: &'static str,
    pub reason: String,
    pub details: Option<String>,
}

/// A collection of invariants checked together against an adapter.
pub struct InvariantSet<A: ContractAdapter> {
    invariants: Vec<Box<dyn Invariant<A>>>,
}

impl<A: ContractAdapter> Default for InvariantSet<A> {
    fn default() -> Self {
        Self::new()
    }
}

impl<A: ContractAdapter> InvariantSet<A> {
    pub fn new() -> Self {
        Self {
            invariants: Vec::new(),
        }
    }

    pub fn add<I: Invariant<A> + 'static>(&mut self, invariant: I) -> &mut Self {
        self.invariants.push(Box::new(invariant));
        self
    }

    pub fn with<I: Invariant<A> + 'static>(mut self, invariant: I) -> Self {
        self.add(invariant);
        self
    }

    pub fn len(&self) -> usize {
        self.invariants.len()
    }

    pub fn is_empty(&self) -> bool {
        self.invariants.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = &Box<dyn Invariant<A>>> {
        self.invariants.iter()
    }

    /// Checks all registered invariants against an initial state.
    pub fn check_initial(&self, state: &A::State) -> Vec<InvariantViolation> {
        let mut violations = Vec::new();
        for inv in &self.invariants {
            if let InvariantResult::Violation { reason, details } = inv.check_state(state) {
                violations.push(InvariantViolation {
                    invariant_name: inv.name(),
                    reason,
                    details,
                });
            }
        }
        violations
    }

    /// Checks all registered invariants against a state transition.
    pub fn check_step(
        &self,
        before: &A::State,
        after: &A::State,
        action: &A::Action,
        result: &ActionResult,
    ) -> Vec<InvariantViolation> {
        let mut violations = Vec::new();
        for inv in &self.invariants {
            if let InvariantResult::Violation { reason, details } =
                inv.check(before, after, action, result)
            {
                violations.push(InvariantViolation {
                    invariant_name: inv.name(),
                    reason,
                    details,
                });
            }
        }
        violations
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct DummyAdapter;
    #[derive(Debug, Clone, PartialEq, Eq)]
    struct DummyAction;
    #[derive(Debug, Clone, PartialEq, Eq)]
    struct DummyState(i32);
    #[derive(thiserror::Error, Debug)]
    #[error("dummy error")]
    struct DummyError;

    impl ContractAdapter for DummyAdapter {
        type Action = DummyAction;
        type State = DummyState;
        type Error = DummyError;

        fn setup() -> Result<Self, Self::Error> {
            Ok(Self)
        }

        fn snapshot(&self) -> Result<Self::State, Self::Error> {
            Ok(DummyState(0))
        }

        fn step(&mut self, _action: &Self::Action) -> Result<ActionResult, Self::Error> {
            Ok(ActionResult::Ok)
        }
    }

    struct NonNegativeInvariant;
    impl Invariant<DummyAdapter> for NonNegativeInvariant {
        fn name(&self) -> &'static str {
            "NonNegative"
        }

        fn check_state(&self, state: &DummyState) -> InvariantResult {
            if state.0 >= 0 {
                InvariantResult::Pass
            } else {
                InvariantResult::violation(format!("Negative state: {}", state.0))
            }
        }
    }

    #[test]
    fn test_invariant_set_evaluation() {
        let set = InvariantSet::<DummyAdapter>::new().with(NonNegativeInvariant);
        assert_eq!(set.len(), 1);
        assert!(!set.is_empty());

        let ok_violations = set.check_initial(&DummyState(10));
        assert!(ok_violations.is_empty());

        let bad_violations = set.check_initial(&DummyState(-5));
        assert_eq!(bad_violations.len(), 1);
        assert_eq!(bad_violations[0].invariant_name, "NonNegative");
        assert!(bad_violations[0].reason.contains("Negative state: -5"));
    }
}


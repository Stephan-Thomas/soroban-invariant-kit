//! Execution trace tracking for invariant evaluation and bug reproduction.

use std::fmt;
use crate::adapter::{ActionResult, ContractAdapter};

/// A single step in an execution trace.
pub struct StepRecord<A: ContractAdapter> {
    pub step_index: usize,
    pub action: A::Action,
    pub state_before: A::State,
    pub state_after: A::State,
    pub result: ActionResult,
}

impl<A: ContractAdapter> fmt::Debug for StepRecord<A> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("StepRecord")
            .field("step_index", &self.step_index)
            .field("action", &self.action)
            .field("state_before", &self.state_before)
            .field("state_after", &self.state_after)
            .field("result", &self.result)
            .finish()
    }
}

impl<A: ContractAdapter> Clone for StepRecord<A> {
    fn clone(&self) -> Self {
        Self {
            step_index: self.step_index,
            action: self.action.clone(),
            state_before: self.state_before.clone(),
            state_after: self.state_after.clone(),
            result: self.result.clone(),
        }
    }
}

/// A complete trace of an execution run from setup through sequential steps.
pub struct Trace<A: ContractAdapter> {
    pub initial_state: A::State,
    pub steps: Vec<StepRecord<A>>,
}

impl<A: ContractAdapter> fmt::Debug for Trace<A> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Trace")
            .field("initial_state", &self.initial_state)
            .field("steps", &self.steps)
            .finish()
    }
}

impl<A: ContractAdapter> Clone for Trace<A> {
    fn clone(&self) -> Self {
        Self {
            initial_state: self.initial_state.clone(),
            steps: self.steps.clone(),
        }
    }
}

impl<A: ContractAdapter> Trace<A> {
    pub fn new(initial_state: A::State) -> Self {
        Self {
            initial_state,
            steps: Vec::new(),
        }
    }

    pub fn add_step(
        &mut self,
        action: A::Action,
        state_before: A::State,
        state_after: A::State,
        result: ActionResult,
    ) {
        let step_index = self.steps.len() + 1;
        self.steps.push(StepRecord {
            step_index,
            action,
            state_before,
            state_after,
            result,
        });
    }

    pub fn len(&self) -> usize {
        self.steps.len()
    }

    pub fn is_empty(&self) -> bool {
        self.steps.is_empty()
    }

    pub fn last_step(&self) -> Option<&StepRecord<A>> {
        self.steps.last()
    }
}

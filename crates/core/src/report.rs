//! Diagnostic reports formatting invariant violations with shrunk traces.

use std::fmt;
use crate::adapter::ContractAdapter;
use crate::transition::Trace;

/// Formatted diagnostic report generated when an invariant fails.
pub struct InvariantViolationReport<A: ContractAdapter> {
    pub invariant_name: &'static str,
    pub reason: String,
    pub details: Option<String>,
    pub failing_step_index: Option<usize>,
    pub trace: Trace<A>,
}

impl<A: ContractAdapter> fmt::Debug for InvariantViolationReport<A> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("InvariantViolationReport")
            .field("invariant_name", &self.invariant_name)
            .field("reason", &self.reason)
            .field("details", &self.details)
            .field("failing_step_index", &self.failing_step_index)
            .field("trace", &self.trace)
            .finish()
    }
}

impl<A: ContractAdapter> Clone for InvariantViolationReport<A> {
    fn clone(&self) -> Self {
        Self {
            invariant_name: self.invariant_name,
            reason: self.reason.clone(),
            details: self.details.clone(),
            failing_step_index: self.failing_step_index,
            trace: self.trace.clone(),
        }
    }
}

impl<A: ContractAdapter> InvariantViolationReport<A> {
    pub fn new_initial(
        invariant_name: &'static str,
        reason: String,
        details: Option<String>,
        initial_state: A::State,
    ) -> Self {
        Self {
            invariant_name,
            reason,
            details,
            failing_step_index: None,
            trace: Trace::new(initial_state),
        }
    }

    pub fn new_step(
        invariant_name: &'static str,
        reason: String,
        details: Option<String>,
        failing_step_index: usize,
        trace: Trace<A>,
    ) -> Self {
        Self {
            invariant_name,
            reason,
            details,
            failing_step_index: Some(failing_step_index),
            trace,
        }
    }
}

impl<A: ContractAdapter> fmt::Display for InvariantViolationReport<A> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "\n============================================================")?;
        writeln!(f, "  INVARIANT VIOLATION: {}", self.invariant_name)?;
        writeln!(f, "============================================================")?;
        writeln!(f, "Reason: {}", self.reason)?;
        if let Some(details) = &self.details {
            writeln!(f, "Details: {}", details)?;
        }
        if let Some(idx) = self.failing_step_index {
            writeln!(f, "Failed at step: #{}", idx)?;
        } else {
            writeln!(f, "Failed at: Initial State (prior to actions)")?;
        }
        writeln!(f, "------------------------------------------------------------")?;
        writeln!(f, "Trace (Total steps: {}):", self.trace.len())?;
        writeln!(f, "  [Step 0 - Initial] State: {:?}", self.trace.initial_state)?;

        for step in &self.trace.steps {
            writeln!(
                f,
                "  [Step {}] Action: {:?} => Result: {:?}",
                step.step_index, step.action, step.result
            )?;
            writeln!(f, "           State Before: {:?}", step.state_before)?;
            writeln!(f, "           State After:  {:?}", step.state_after)?;
        }
        writeln!(f, "============================================================\n")?;
        Ok(())
    }
}

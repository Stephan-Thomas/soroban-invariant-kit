//! Sequential runner for invariant testing.

use soroban_invariant_kit_core::{
    ContractAdapter, InvariantSet, InvariantViolationReport, Trace,
};

/// Configuration options for invariant test runs.
#[derive(Debug, Clone)]
pub struct RunnerConfig {
    /// Maximum actions allowed per sequence run. Actions beyond this limit are ignored.
    pub max_steps: Option<usize>,
}

impl Default for RunnerConfig {
    fn default() -> Self {
        Self {
            max_steps: Some(250),
        }
    }
}

/// Orchestrates state machine execution and invariant validation.
pub struct InvariantRunner<A: ContractAdapter> {
    config: RunnerConfig,
    _marker: std::marker::PhantomData<A>,
}

impl<A: ContractAdapter> Default for InvariantRunner<A> {
    fn default() -> Self {
        Self::new(RunnerConfig::default())
    }
}

impl<A: ContractAdapter> InvariantRunner<A> {
    pub fn new(config: RunnerConfig) -> Self {
        Self {
            config,
            _marker: std::marker::PhantomData,
        }
    }

    /// Executes a sequence of actions on a freshly initialized adapter,
    /// checking all registered invariants after setup and each subsequent action step.
    pub fn run_sequence(
        &self,
        actions: &[A::Action],
        invariants: &InvariantSet<A>,
    ) -> Result<Trace<A>, InvariantViolationReport<A>> {
        let mut adapter = match A::setup() {
            Ok(a) => a,
            Err(e) => {
                panic!("Adapter setup failed: {}", e);
            }
        };

        let initial_state = match adapter.snapshot() {
            Ok(s) => s,
            Err(e) => {
                panic!("Adapter snapshot on setup failed: {}", e);
            }
        };

        let mut trace = Trace::new(initial_state.clone());

        // 1. Validate initial state invariants
        let initial_violations = invariants.check_initial(&initial_state);
        if let Some(v) = initial_violations.into_iter().next() {
            return Err(InvariantViolationReport::new_initial(
                v.invariant_name,
                v.reason,
                v.details,
                initial_state,
            ));
        }

        // 2. Iterate through actions
        let actions_to_run = match self.config.max_steps {
            Some(max) => &actions[..actions.len().min(max)],
            None => actions,
        };

        for (idx, action) in actions_to_run.iter().enumerate() {
            let before = match adapter.snapshot() {
                Ok(b) => b,
                Err(e) => panic!("Snapshot failed before step {}: {}", idx + 1, e),
            };

            let step_res = match adapter.step(action) {
                Ok(res) => res,
                Err(e) => panic!("Adapter step execution failed at step {}: {}", idx + 1, e),
            };

            let after = match adapter.snapshot() {
                Ok(a) => a,
                Err(e) => panic!("Snapshot failed after step {}: {}", idx + 1, e),
            };

            trace.add_step(action.clone(), before.clone(), after.clone(), step_res.clone());

            let violations = invariants.check_step(&before, &after, action, &step_res);
            if let Some(v) = violations.into_iter().next() {
                return Err(InvariantViolationReport::new_step(
                    v.invariant_name,
                    v.reason,
                    v.details,
                    idx + 1,
                    trace,
                ));
            }
        }

        Ok(trace)
    }
}

/// Asserts that all invariants hold across the action sequence, panicking with a detailed report on failure.
pub fn assert_invariants<A: ContractAdapter>(
    invariants: &InvariantSet<A>,
    actions: &[A::Action],
) {
    let runner = InvariantRunner::<A>::default();
    if let Err(report) = runner.run_sequence(actions, invariants) {
        panic!("{}", report);
    }
}

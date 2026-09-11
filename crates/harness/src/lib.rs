//! # soroban-invariant-kit-harness
//!
//! Test harness and property-testing glue connecting `soroban-invariant-kit-core`
//! with `proptest` and `cargo-fuzz`.
//!
//! ## Macros
//! - [`invariant_test!`]: Convenient macro for generating state-machine property tests.

pub mod arbitrary_glue;
pub mod runner;

#[cfg(feature = "arbitrary")]
pub use arbitrary_glue::*;
pub use runner::{assert_invariants, InvariantRunner, RunnerConfig};

/// Macro to define a proptest-driven invariant test.
///
/// Automatically instantiates a `proptest::test_runner::TestRunner`, samples random
/// sequences of actions from the provided strategy, runs the action transitions against
/// the adapter, and verifies all invariants on each step.
///
/// On invariant violation, proptest will automatically shrink the action sequence down
/// to the minimal failing reproducer and print a structured diagnostic trace.
///
/// # Example
/// ```rust,ignore
/// invariant_test!(
///     test_my_contract_invariants,
///     MyContractAdapter,
///     proptest::collection::vec(my_action_strategy(), 1..50),
///     my_invariants(),
///     100 // number of test cases (optional)
/// );
/// ```
#[macro_export]
macro_rules! invariant_test {
    ($test_name:ident, $adapter_ty:ty, $action_strategy:expr, $invariants_expr:expr $(, $cases:expr)?) => {
        #[test]
        fn $test_name() {
            let mut config = proptest::test_runner::Config::default();
            $( config.cases = $cases; )?
            let mut runner = proptest::test_runner::TestRunner::new(config);
            let invariants = $invariants_expr;
            let result = runner.run(&$action_strategy, |actions| {
                $crate::runner::assert_invariants::<$adapter_ty>(&invariants, &actions);
                Ok(())
            });
            if let Err(e) = result {
                panic!("Invariant test failure: {}", e);
            }
        }
    };
}

//! Integration glue for arbitrary / cargo-fuzz fuzz targets.

#[cfg(feature = "arbitrary")]
use arbitrary::{Arbitrary, Unstructured};
#[cfg(feature = "arbitrary")]
use soroban_invariant_kit_core::{ContractAdapter, InvariantSet, InvariantViolationReport, Trace};
#[cfg(feature = "arbitrary")]
use crate::runner::InvariantRunner;

/// Helper function to drive an invariant check from raw fuzzing bytes.
///
/// Converts random input bytes from libFuzzer / cargo-fuzz into a sequence of
/// typed adapter actions using [`arbitrary::Arbitrary`].
#[cfg(feature = "arbitrary")]
pub fn fuzz_from_bytes<'a, A>(
    data: &'a [u8],
    invariants: &InvariantSet<A>,
) -> Result<Trace<A>, InvariantViolationReport<A>>
where
    A: ContractAdapter,
    A::Action: Arbitrary<'a>,
{
    let mut u = Unstructured::new(data);
    let actions: Vec<A::Action> = match Vec::<A::Action>::arbitrary(&mut u) {
        Ok(acts) => acts,
        Err(_) => return Ok(Trace::new(A::setup().unwrap().snapshot().unwrap())),
    };

    let runner = InvariantRunner::<A>::default();
    runner.run_sequence(&actions, invariants)
}

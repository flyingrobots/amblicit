//! Guest memory limit sized from the input cap (issue #333).
//!
//! `fixture.memory-32mib` allocates 32 MiB and then returns a conforming
//! result: above the former 16 MiB limit and within the input-scaled one.
//! `fixture.memory-unbounded` allocates until the host denies growth.

use amblicit_provider_host_wasmtime::{
    ProviderHostFailureKind, ProviderHostLimits, ProviderHostPhase,
};

use super::lower_harness;

/// Oracle: the guest memory limit the application build used before #333.
const PREVIOUS_MEMORY_LIMIT: usize = 16 * 1024 * 1024;

/// Oracle: issue #333 sizes guest memory as the 1 MiB input cap times 64
/// bytes per input byte, which is 64 MiB.
const INPUT_SCALED_MEMORY_LIMIT: usize = 64 * 1024 * 1024;

const APPLICATION_BUILD: ProviderHostLimits = ProviderHostLimits::APPLICATION_BUILD;

#[test]
fn application_build_memory_is_sized_from_the_input_cap() {
    assert_eq!(
        APPLICATION_BUILD.max_wasm_memory_bytes,
        INPUT_SCALED_MEMORY_LIMIT
    );
}

#[test]
fn input_scaled_memory_admits_growth_above_the_previous_limit() {
    let harness = lower_harness("fixture.memory-32mib");
    let mut previous = APPLICATION_BUILD;
    previous.max_wasm_memory_bytes = PREVIOUS_MEMORY_LIMIT;
    let failure = harness
        .host
        .invoke_lowerer(
            &harness.prepared,
            &harness.request,
            harness.schema,
            previous,
        )
        .expect_err("the calibrated allocation exceeds the previous limit");
    assert_eq!(
        failure.kind(),
        ProviderHostFailureKind::ResourceLimitExceeded
    );

    let outcome = harness
        .host
        .invoke_lowerer(
            &harness.prepared,
            &harness.request,
            harness.schema,
            APPLICATION_BUILD,
        )
        .expect("the same allocation fits the input-scaled limit");
    assert_eq!(
        outcome
            .manifest()
            .expect("success manifest")
            .outputs()
            .len(),
        1
    );
}

#[test]
fn unbounded_guest_growth_is_refused_deterministically() {
    let harness = lower_harness("fixture.memory-unbounded");
    for _ in 0..2 {
        let failure = harness
            .host
            .invoke_lowerer(
                &harness.prepared,
                &harness.request,
                harness.schema,
                APPLICATION_BUILD,
            )
            .expect_err("a guest that never stops allocating cannot complete");
        assert_eq!(
            failure.kind(),
            ProviderHostFailureKind::ResourceLimitExceeded
        );
        assert_eq!(failure.phase(), ProviderHostPhase::Lower);
    }
}

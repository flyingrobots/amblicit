//! Fuel ceiling sized from the input cap (issue #326).
//!
//! `fixture.fuel-burn` runs a fixed loop and then returns a conforming result.
//! Its fuel need lies above the former 50,000,000 ceiling and far below the
//! input-scaled ceiling, so it separates the two policies.

use amblicit_provider_host_wasmtime::{
    ProviderHostFailureKind, ProviderHostLimits, ProviderHostPhase, ProviderReplayObservation,
};

use super::lower_harness;

/// Oracle: the fixed ceiling the application build used before issue #326.
const PREVIOUS_FUEL_CEILING: u64 = 50_000_000;

/// Oracle: issue #326 sizes fuel as the 1 MiB input cap times 4,096 fuel per
/// input byte, which is 2^32.
const INPUT_SCALED_FUEL_CEILING: u64 = 1 << 32;

const APPLICATION_BUILD: ProviderHostLimits = ProviderHostLimits::APPLICATION_BUILD;

#[test]
fn application_build_fuel_is_sized_from_the_input_cap() {
    assert_eq!(APPLICATION_BUILD.max_wasm_fuel, INPUT_SCALED_FUEL_CEILING);
}

#[test]
fn application_build_keeps_every_other_limit() {
    // Oracle: the application build's limits before issue #326
    // (crates/amblicit-cli/src/application_build.rs at da1574f1).
    let previous = ProviderHostLimits {
        max_input_bytes: 1024 * 1024,
        max_output_bytes: 3 * 1024 * 1024,
        max_diagnostic_bytes: 3 * 1024 * 1024,
        // Issue #333 sized guest memory from the input cap separately.
        max_wasm_memory_bytes: APPLICATION_BUILD.max_wasm_memory_bytes,
        max_table_elements: 10_000,
        max_instances: 100,
        max_memories: 8,
        max_tables: 8,
        max_wasm_fuel: APPLICATION_BUILD.max_wasm_fuel,
        max_hostcall_bytes: 4 * 1024 * 1024,
        max_host_diagnostic_bytes: 512,
    };
    assert_eq!(APPLICATION_BUILD, previous);
}

#[test]
fn input_scaled_fuel_admits_work_above_the_previous_ceiling() {
    let burn = lower_harness("fixture.fuel-burn");
    let mut previous = APPLICATION_BUILD;
    previous.max_wasm_fuel = PREVIOUS_FUEL_CEILING;
    let failure = burn
        .host
        .invoke_lowerer(&burn.prepared, &burn.request, burn.schema, previous)
        .expect_err("the calibrated work needs more than the previous ceiling");
    assert_eq!(failure.kind(), ProviderHostFailureKind::FuelExhausted);
    assert_eq!(failure.phase(), ProviderHostPhase::Lower);

    let outcome = burn
        .host
        .invoke_lowerer(
            &burn.prepared,
            &burn.request,
            burn.schema,
            APPLICATION_BUILD,
        )
        .expect("the same work completes within the input-scaled ceiling");
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
fn non_terminating_guest_exhausts_the_input_scaled_ceiling_deterministically() {
    let looping = lower_harness("fixture.loop");
    for _ in 0..2 {
        let failure = looping
            .host
            .invoke_lowerer(
                &looping.prepared,
                &looping.request,
                looping.schema,
                APPLICATION_BUILD,
            )
            .expect_err("a guest that never stops cannot complete");
        assert_eq!(failure.kind(), ProviderHostFailureKind::FuelExhausted);
        assert_eq!(failure.phase(), ProviderHostPhase::Lower);
    }
    let replay = looping
        .host
        .replay_lowerer(
            &looping.prepared,
            &looping.request,
            looping.schema,
            APPLICATION_BUILD,
        )
        .expect("fuel exhaustion replays as an equal rejection");
    assert!(matches!(
        replay.observation(),
        ProviderReplayObservation::Rejected(failure)
            if failure.kind() == ProviderHostFailureKind::FuelExhausted
    ));
}

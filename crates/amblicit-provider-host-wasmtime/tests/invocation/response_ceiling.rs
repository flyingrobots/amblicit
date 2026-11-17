//! Response limits sized from the verifier's input cap (issue #329).
//!
//! `fixture.large-package` returns a canonical artifact above the former
//! 65,536-byte response cap. `fixture.output-flood` returns 2 MiB, above
//! the new cap.

use amblicit_provider_host_wasmtime::{
    ProviderHostFailureKind, ProviderHostLimits, APPLICATION_BUILD_RESPONSE_LIMITS,
};
use amblicit_syntax::ProviderResponseLimits;

use super::lower_harness_with_response_limits;

/// Oracle: the response cap the application build used before issue #329.
const PREVIOUS_RESPONSE_CAP: u64 = 64 * 1024;

const APPLICATION_BUILD: ProviderHostLimits = ProviderHostLimits::APPLICATION_BUILD;

#[test]
fn application_build_response_cap_is_the_verifier_input_cap() {
    // Oracle: issue #329. A package is fed back to the verifier, so the cap
    // equals the 1 MiB verifier input cap, which is inside the 3 MiB output
    // ceiling.
    assert_eq!(
        APPLICATION_BUILD_RESPONSE_LIMITS,
        ProviderResponseLimits {
            max_output_count: 1,
            max_diagnostic_count: 8,
            max_total_response_bytes: 1024 * 1024,
        }
    );
    assert_eq!(
        APPLICATION_BUILD_RESPONSE_LIMITS.max_total_response_bytes,
        APPLICATION_BUILD.max_input_bytes
    );
    assert!(
        APPLICATION_BUILD_RESPONSE_LIMITS.max_total_response_bytes
            <= APPLICATION_BUILD.max_output_bytes
    );
}

#[test]
fn package_above_the_previous_cap_is_admitted() {
    let mut previous = APPLICATION_BUILD_RESPONSE_LIMITS;
    previous.max_total_response_bytes = PREVIOUS_RESPONSE_CAP;
    let refused = lower_harness_with_response_limits("fixture.large-package", previous);
    let failure = refused
        .host
        .invoke_lowerer(
            &refused.prepared,
            &refused.request,
            refused.schema,
            APPLICATION_BUILD,
        )
        .expect_err("the calibrated package is larger than the previous cap");
    assert_eq!(
        failure.kind(),
        ProviderHostFailureKind::ResponseLimitExceeded
    );

    let admitted = lower_harness_with_response_limits(
        "fixture.large-package",
        APPLICATION_BUILD_RESPONSE_LIMITS,
    );
    let outcome = admitted
        .host
        .invoke_lowerer(
            &admitted.prepared,
            &admitted.request,
            admitted.schema,
            APPLICATION_BUILD,
        )
        .expect("the same package fits the input-derived cap");
    let manifest = outcome.manifest().expect("success manifest");
    assert_eq!(manifest.outputs().len(), 1);
}

#[test]
fn response_above_the_new_cap_is_refused_deterministically() {
    let flood = lower_harness_with_response_limits(
        "fixture.output-flood",
        APPLICATION_BUILD_RESPONSE_LIMITS,
    );
    for _ in 0..2 {
        let failure = flood
            .host
            .invoke_lowerer(
                &flood.prepared,
                &flood.request,
                flood.schema,
                APPLICATION_BUILD,
            )
            .expect_err("a 2 MiB response exceeds the 1 MiB cap");
        assert_eq!(
            failure.kind(),
            ProviderHostFailureKind::ResponseLimitExceeded
        );
        assert!(failure.validation_report().is_some());
    }
}

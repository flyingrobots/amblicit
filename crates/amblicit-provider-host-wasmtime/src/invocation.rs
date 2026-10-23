use std::any::Any;

use amblicit_provider_schema::ProviderArtifactSchemaRegistry;
use amblicit_syntax::{
    validate_provider_lowering_result, validate_provider_verification_result,
    ProviderArtifactSchemaValidator, ProviderLoweringOutputKind, ProviderLoweringSuccess,
    ProviderVerificationOutputKind, ProviderVerificationSuccess, ValidatedProviderLoweringRequest,
    ValidatedProviderOutcome, ValidatedProviderVerificationRequest,
};
use wasmtime::{Store, Trap};

use super::conversion::{
    lowering_diagnostic_bytes, lowering_input_bytes, lowering_request, lowering_result,
    verification_diagnostic_bytes, verification_input_bytes, verification_request,
    verification_result,
};
use super::limits::InvocationLimiter;
use super::{
    InvocationState, PreparedProviderComponent, PreparedWorld, ProviderComponentHost,
    ProviderHostFailure, ProviderHostFailureKind, ProviderHostLimits, ProviderHostPhase,
};

pub type ValidatedProviderLoweringOutcome =
    ValidatedProviderOutcome<ProviderLoweringSuccess, ProviderLoweringOutputKind>;
pub type ValidatedProviderVerificationOutcome =
    ValidatedProviderOutcome<ProviderVerificationSuccess, ProviderVerificationOutputKind>;

/// Measure the exact WIT logical bytes charged to one validated lowerer input.
///
/// # Errors
///
/// Returns `InputLimitExceeded` if checked `u64` accounting overflows.
pub fn provider_lowering_input_bytes(
    validated: &ValidatedProviderLoweringRequest<'_>,
) -> Result<u64, ProviderHostFailure> {
    lowering_input_bytes(validated.request()).ok_or_else(input_overflow_failure)
}

/// Measure the exact WIT logical bytes charged to one validated verifier input.
///
/// # Errors
///
/// Returns `InputLimitExceeded` if checked `u64` accounting overflows.
pub fn provider_verification_input_bytes(
    validated: &ValidatedProviderVerificationRequest<'_>,
) -> Result<u64, ProviderHostFailure> {
    verification_input_bytes(validated.request()).ok_or_else(input_overflow_failure)
}

impl ProviderComponentHost {
    /// Instantiate a fresh lowerer store, invoke it once, and admit its result.
    ///
    /// # Errors
    ///
    /// Returns a stable host failure for input/host limits, instantiation,
    /// traps, malformed lifting, or pure response-envelope rejection.
    pub fn invoke_lowerer(
        &self,
        prepared: &PreparedProviderComponent<'_>,
        validated: &ValidatedProviderLoweringRequest<'_>,
        registry: &ProviderArtifactSchemaRegistry,
        limits: ProviderHostLimits,
    ) -> Result<ValidatedProviderLoweringOutcome, ProviderHostFailure> {
        verify_authority_binding(self, prepared, validated.schema_validator(), registry)?;
        preflight_limits(
            lowering_input_bytes(validated.request()),
            validated.request().limits.max_total_response_bytes,
            limits,
        )?;
        let PreparedWorld::Lowering(pre) = &prepared.world else {
            return Err(ProviderHostFailure::message(
                ProviderHostFailureKind::HostInvariantViolated,
                ProviderHostPhase::Lower,
                "verification component passed to lowerer invocation",
            ));
        };
        let mut store = new_store(self, limits)?;
        let instance = pre.instantiate(&mut store).map_err(|error| {
            classify_engine_error(
                &store,
                ProviderHostFailureKind::ComponentInstantiationFailed,
                ProviderHostPhase::Instantiate,
                &error,
                limits,
            )
        })?;
        let request = lowering_request(validated.request());
        let function = instance.func_lower();
        let (result,) = function.call(&mut store, (&request,)).map_err(|error| {
            classify_call_error(&store, ProviderHostPhase::Lower, &error, limits)
        })?;
        if store.data().limiter.denied().is_some() {
            return Err(resource_failure(ProviderHostPhase::Lower));
        }
        let result = lowering_result(result);
        enforce_diagnostic_limit(lowering_diagnostic_bytes(&result), limits)?;
        validate_provider_lowering_result(validated, &result)
            .map_err(ProviderHostFailure::validation)
    }

    /// Instantiate a fresh verifier store, invoke it once, and admit its result.
    ///
    /// # Errors
    ///
    /// Returns a stable host failure for input/host limits, instantiation,
    /// traps, malformed lifting, or pure response-envelope rejection.
    pub fn invoke_verifier(
        &self,
        prepared: &PreparedProviderComponent<'_>,
        validated: &ValidatedProviderVerificationRequest<'_>,
        registry: &ProviderArtifactSchemaRegistry,
        limits: ProviderHostLimits,
    ) -> Result<ValidatedProviderVerificationOutcome, ProviderHostFailure> {
        verify_authority_binding(self, prepared, validated.schema_validator(), registry)?;
        preflight_limits(
            verification_input_bytes(validated.request()),
            validated.request().limits.max_total_response_bytes,
            limits,
        )?;
        let PreparedWorld::Verification(pre) = &prepared.world else {
            return Err(ProviderHostFailure::message(
                ProviderHostFailureKind::HostInvariantViolated,
                ProviderHostPhase::Verify,
                "lowerer component passed to verifier invocation",
            ));
        };
        let mut store = new_store(self, limits)?;
        let instance = pre.instantiate(&mut store).map_err(|error| {
            classify_engine_error(
                &store,
                ProviderHostFailureKind::ComponentInstantiationFailed,
                ProviderHostPhase::Instantiate,
                &error,
                limits,
            )
        })?;
        let request = verification_request(validated.request());
        let function = instance.func_verify();
        let (result,) = function.call(&mut store, (&request,)).map_err(|error| {
            classify_call_error(&store, ProviderHostPhase::Verify, &error, limits)
        })?;
        if store.data().limiter.denied().is_some() {
            return Err(resource_failure(ProviderHostPhase::Verify));
        }
        let result = verification_result(result);
        enforce_diagnostic_limit(verification_diagnostic_bytes(&result), limits)?;
        validate_provider_verification_result(validated, &result)
            .map_err(ProviderHostFailure::validation)
    }
}

fn verify_authority_binding(
    host: &ProviderComponentHost,
    prepared: &PreparedProviderComponent<'_>,
    request_validator: &dyn ProviderArtifactSchemaValidator,
    registry: &ProviderArtifactSchemaRegistry,
) -> Result<(), ProviderHostFailure> {
    let request_registry =
        (request_validator as &dyn Any).downcast_ref::<ProviderArtifactSchemaRegistry>();
    if !wasmtime::Engine::same(&prepared.engine, &host.engine)
        || prepared.selected.manifest() != registry.manifest()
        || request_registry.is_none_or(|request_registry| !std::ptr::eq(request_registry, registry))
    {
        return Err(ProviderHostFailure::message(
            ProviderHostFailureKind::HostInvariantViolated,
            ProviderHostPhase::Preflight,
            "component, schema registry, and validated request authority do not match",
        ));
    }
    Ok(())
}

fn new_store(
    host: &ProviderComponentHost,
    limits: ProviderHostLimits,
) -> Result<Store<InvocationState>, ProviderHostFailure> {
    let mut store = Store::try_new(
        &host.engine,
        InvocationState {
            limiter: InvocationLimiter::new(limits),
        },
    )
    .map_err(|error| {
        bounded_engine_failure(
            ProviderHostFailureKind::HostInvariantViolated,
            ProviderHostPhase::Configure,
            &error,
            limits,
        )
    })?;
    store.limiter(|state| &mut state.limiter);
    store.set_fuel(limits.max_wasm_fuel).map_err(|error| {
        ProviderHostFailure::error(
            ProviderHostFailureKind::HostInvariantViolated,
            ProviderHostPhase::Configure,
            &error,
        )
    })?;
    store.set_hostcall_fuel(limits.max_hostcall_bytes);
    Ok(store)
}

fn preflight_limits(
    input_bytes: Option<u64>,
    requested_output_bytes: u64,
    limits: ProviderHostLimits,
) -> Result<(), ProviderHostFailure> {
    if input_bytes.is_none_or(|bytes| bytes > limits.max_input_bytes) {
        return Err(ProviderHostFailure::message(
            ProviderHostFailureKind::InputLimitExceeded,
            ProviderHostPhase::Preflight,
            "provider request exceeds the WIT logical input-byte limit",
        ));
    }
    if requested_output_bytes > limits.max_output_bytes {
        return Err(ProviderHostFailure::message(
            ProviderHostFailureKind::ResponseLimitExceeded,
            ProviderHostPhase::Preflight,
            "provider request authorizes more response bytes than the host limit",
        ));
    }
    Ok(())
}

fn input_overflow_failure() -> ProviderHostFailure {
    ProviderHostFailure::message(
        ProviderHostFailureKind::InputLimitExceeded,
        ProviderHostPhase::Preflight,
        "provider request WIT logical byte accounting overflowed",
    )
}

fn enforce_diagnostic_limit(
    diagnostic_bytes: Option<u64>,
    limits: ProviderHostLimits,
) -> Result<(), ProviderHostFailure> {
    if diagnostic_bytes.is_none_or(|bytes| bytes > limits.max_diagnostic_bytes) {
        return Err(ProviderHostFailure::message(
            ProviderHostFailureKind::DiagnosticLimitExceeded,
            ProviderHostPhase::ValidateResponse,
            "provider diagnostics exceed the host byte limit",
        ));
    }
    Ok(())
}

fn classify_call_error(
    store: &Store<InvocationState>,
    phase: ProviderHostPhase,
    error: &wasmtime::Error,
    limits: ProviderHostLimits,
) -> ProviderHostFailure {
    if store.data().limiter.denied().is_some() {
        return resource_failure(phase);
    }
    if let Some(trap) = error.downcast_ref::<Trap>() {
        let kind = match trap {
            Trap::OutOfFuel => ProviderHostFailureKind::FuelExhausted,
            _ => ProviderHostFailureKind::GuestTrap,
        };
        return bounded_engine_failure(kind, phase, error, limits);
    }
    let mut failure =
        ProviderHostFailure::error(ProviderHostFailureKind::MalformedResponse, phase, error);
    if failure
        .diagnostic
        .contains("fuel allocated for hostcalls has been exhausted")
    {
        failure.kind = ProviderHostFailureKind::ResponseLiftLimitExceeded;
    }
    truncate_engine_diagnostic(&mut failure, limits.max_host_diagnostic_bytes);
    failure
}

fn classify_engine_error(
    store: &Store<InvocationState>,
    fallback: ProviderHostFailureKind,
    phase: ProviderHostPhase,
    error: &wasmtime::Error,
    limits: ProviderHostLimits,
) -> ProviderHostFailure {
    if store.data().limiter.denied().is_some() {
        return resource_failure(phase);
    }
    if matches!(error.downcast_ref::<Trap>(), Some(Trap::OutOfFuel)) {
        return bounded_engine_failure(
            ProviderHostFailureKind::FuelExhausted,
            phase,
            error,
            limits,
        );
    }
    let mut failure = ProviderHostFailure::error(fallback, phase, error);
    // Wasmtime exposes count-limit exhaustion only through this pinned error
    // adapter. The exact runtime version and this classification are ratcheted.
    if failure.diagnostic.contains("resource limit exceeded:") {
        failure.kind = ProviderHostFailureKind::ResourceLimitExceeded;
    }
    truncate_engine_diagnostic(&mut failure, limits.max_host_diagnostic_bytes);
    failure
}

fn bounded_engine_failure(
    kind: ProviderHostFailureKind,
    phase: ProviderHostPhase,
    error: &wasmtime::Error,
    limits: ProviderHostLimits,
) -> ProviderHostFailure {
    let mut failure = ProviderHostFailure::error(kind, phase, error);
    truncate_engine_diagnostic(&mut failure, limits.max_host_diagnostic_bytes);
    failure
}

fn truncate_engine_diagnostic(failure: &mut ProviderHostFailure, limit: usize) {
    if failure.diagnostic.len() > limit {
        let mut end = limit;
        while !failure.diagnostic.is_char_boundary(end) {
            end -= 1;
        }
        failure.diagnostic.truncate(end);
    }
}

fn resource_failure(phase: ProviderHostPhase) -> ProviderHostFailure {
    ProviderHostFailure::message(
        ProviderHostFailureKind::ResourceLimitExceeded,
        phase,
        "provider guest resource growth was denied",
    )
}

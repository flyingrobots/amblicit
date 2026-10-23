//! Capability-denied Wasmtime host for external Edict provider components.

use std::fmt;
use std::fmt::Write as _;
use std::sync::Arc;

use amblicit_syntax::{ProviderInvocationKind, SelectedProviderComponent};
use sha2::{Digest, Sha256};
use wasmparser::{Parser, Payload};
use wasmtime::component::types::ComponentItem;
use wasmtime::component::{Component, Linker};
use wasmtime::{Config, Engine};

mod conversion;
mod invocation;
mod limits;
mod replay;

pub use invocation::{
    provider_lowering_input_bytes, provider_verification_input_bytes,
    ValidatedProviderLoweringOutcome, ValidatedProviderVerificationOutcome,
};
pub use limits::ProviderHostLimits;
pub use replay::{
    ProviderHostFailureIdentity, ProviderReplayFailure, ProviderReplayFailureKind,
    ProviderReplayObservation, ValidatedProviderReplay,
};

mod lowerer_bindings {
    wasmtime::component::bindgen!({
        path: "../../docs/abi/edict-target-provider.wit",
        world: "lowerer",
    });
}

mod verifier_bindings {
    wasmtime::component::bindgen!({
        path: "../../docs/abi/edict-target-provider.wit",
        world: "verifier",
    });
}

/// Top-level custom section carrying exact digest-covered contract identity.
pub const PROVIDER_CONTRACT_CUSTOM_SECTION: &str = "edict:target-provider-contract";

const DEFAULT_MAX_HOST_DIAGNOSTIC_BYTES: usize = 1_024;
const MAX_WASM_STACK_BYTES: usize = 512 * 1024;

/// Stable provider-host failure categories independent of Wasmtime internals.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderHostFailureKind {
    ComponentDigestMismatch,
    ComponentDecodeFailed,
    ComponentContractMismatch,
    ComponentInstantiationFailed,
    InputLimitExceeded,
    FuelExhausted,
    ResourceLimitExceeded,
    ResponseLiftLimitExceeded,
    GuestTrap,
    MalformedResponse,
    ResponseLimitExceeded,
    DiagnosticLimitExceeded,
    ResponseEnvelopeInvalid,
    HostInvariantViolated,
}

/// Stable phase in which the host rejected an invocation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderHostPhase {
    Configure,
    Preflight,
    Compile,
    Instantiate,
    Lower,
    Verify,
    ValidateResponse,
}

/// One bounded host-owned failure observation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderHostFailure {
    kind: ProviderHostFailureKind,
    phase: ProviderHostPhase,
    diagnostic: String,
    validation: Option<amblicit_syntax::ProviderInvocationValidationReport>,
}

impl ProviderHostFailure {
    #[must_use]
    pub const fn kind(&self) -> ProviderHostFailureKind {
        self.kind
    }

    #[must_use]
    pub const fn phase(&self) -> ProviderHostPhase {
        self.phase
    }

    #[must_use]
    pub fn diagnostic(&self) -> &str {
        &self.diagnostic
    }

    /// Return the nested pure response-validation report when available.
    #[must_use]
    pub const fn validation_report(
        &self,
    ) -> Option<&amblicit_syntax::ProviderInvocationValidationReport> {
        self.validation.as_ref()
    }

    fn message(kind: ProviderHostFailureKind, phase: ProviderHostPhase, message: &str) -> Self {
        Self {
            kind,
            phase,
            diagnostic: bounded_diagnostic(message, DEFAULT_MAX_HOST_DIAGNOSTIC_BYTES),
            validation: None,
        }
    }

    fn error(
        kind: ProviderHostFailureKind,
        phase: ProviderHostPhase,
        error: &dyn fmt::Display,
    ) -> Self {
        let mut diagnostic = BoundedDiagnostic::new(DEFAULT_MAX_HOST_DIAGNOSTIC_BYTES);
        let _ = write!(&mut diagnostic, "{error}");
        Self {
            kind,
            phase,
            diagnostic: diagnostic.finish(),
            validation: None,
        }
    }

    fn validation(report: amblicit_syntax::ProviderInvocationValidationReport) -> Self {
        use amblicit_syntax::ProviderInvocationValidationFailureKind as Kind;
        let kind = if report
            .failures
            .iter()
            .any(|failure| failure.kind == Kind::DiagnosticCountLimitExceeded)
        {
            ProviderHostFailureKind::DiagnosticLimitExceeded
        } else if report.failures.iter().any(|failure| {
            matches!(
                failure.kind,
                Kind::OutputCountLimitExceeded
                    | Kind::ResponseByteCountOverflow
                    | Kind::ResponseByteLimitExceeded
            )
        }) {
            ProviderHostFailureKind::ResponseLimitExceeded
        } else {
            ProviderHostFailureKind::ResponseEnvelopeInvalid
        };
        Self {
            kind,
            phase: ProviderHostPhase::ValidateResponse,
            diagnostic: "provider response failed pure envelope validation".to_owned(),
            validation: Some(report),
        }
    }
}

impl fmt::Display for ProviderHostFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "{:?} during {:?}: {}",
            self.kind, self.phase, self.diagnostic
        )
    }
}

impl std::error::Error for ProviderHostFailure {}

/// Resolver output consumed by the host without performing discovery itself.
#[derive(Debug, Clone)]
pub struct ResolvedProviderComponent<'a> {
    selected: SelectedProviderComponent<'a>,
    bytes: Arc<[u8]>,
}

impl<'a> ResolvedProviderComponent<'a> {
    #[must_use]
    pub const fn new(selected: SelectedProviderComponent<'a>, bytes: Arc<[u8]>) -> Self {
        Self { selected, bytes }
    }

    #[must_use]
    pub const fn selected(&self) -> SelectedProviderComponent<'a> {
        self.selected
    }

    #[must_use]
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
}

#[derive(Debug)]
struct InvocationState {
    limiter: limits::InvocationLimiter,
}

enum PreparedWorld {
    Lowering(lowerer_bindings::LowererPre<InvocationState>),
    Verification(verifier_bindings::VerifierPre<InvocationState>),
}

/// Digest-verified, identity-attested, capability-denied typed component.
pub struct PreparedProviderComponent<'a> {
    selected: SelectedProviderComponent<'a>,
    engine: Engine,
    world: PreparedWorld,
}

impl fmt::Debug for PreparedProviderComponent<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PreparedProviderComponent")
            .field("role", &self.selected.role())
            .field("invocation", &self.selected.invocation())
            .finish_non_exhaustive()
    }
}

/// Wasmtime-backed provider component host with one immutable engine.
#[derive(Debug)]
pub struct ProviderComponentHost {
    engine: Engine,
}

impl ProviderComponentHost {
    /// Build the narrow deterministic-compatible component engine.
    ///
    /// # Errors
    ///
    /// Returns `HostInvariantViolated` if Wasmtime rejects the host's fixed
    /// engine configuration.
    pub fn new() -> Result<Self, ProviderHostFailure> {
        let mut config = Config::new();
        config
            .wasm_component_model(true)
            .consume_fuel(true)
            .epoch_interruption(false)
            .wasm_simd(false)
            .wasm_relaxed_simd(false)
            .relaxed_simd_deterministic(true)
            .wasm_tail_call(false)
            .wasm_memory64(false)
            .wasm_multi_memory(false)
            .cranelift_nan_canonicalization(true)
            .memory_init_cow(false)
            .max_wasm_stack(MAX_WASM_STACK_BYTES);
        let engine = Engine::new(&config).map_err(|error| {
            ProviderHostFailure::error(
                ProviderHostFailureKind::HostInvariantViolated,
                ProviderHostPhase::Configure,
                &error,
            )
        })?;
        Ok(Self { engine })
    }

    /// Verify and type-check one resolver-supplied component without a store.
    ///
    /// Digest verification precedes all byte decoding. Exact contract
    /// attestation, zero callable or capability-bearing imports, exact export
    /// closure, and generated frozen WIT type checks all precede instantiation.
    ///
    /// # Errors
    ///
    /// Returns a stable digest, decode, or contract failure.
    pub fn prepare<'a>(
        &self,
        resolved: &ResolvedProviderComponent<'a>,
    ) -> Result<PreparedProviderComponent<'a>, ProviderHostFailure> {
        verify_component_digest(resolved)?;
        verify_contract_attestation(resolved.bytes(), resolved.selected.contract_identity())?;
        let component = Component::new(&self.engine, resolved.bytes()).map_err(|error| {
            ProviderHostFailure::error(
                ProviderHostFailureKind::ComponentDecodeFailed,
                ProviderHostPhase::Compile,
                &error,
            )
        })?;
        verify_component_surface(&self.engine, &component, resolved.selected.invocation())?;

        let mut linker = Linker::<InvocationState>::new(&self.engine);
        linker
            .define_unknown_imports_as_traps(&component)
            .map_err(|error| {
                ProviderHostFailure::error(
                    ProviderHostFailureKind::HostInvariantViolated,
                    ProviderHostPhase::Preflight,
                    &error,
                )
            })?;
        let instance_pre = linker.instantiate_pre(&component).map_err(|error| {
            ProviderHostFailure::error(
                ProviderHostFailureKind::ComponentContractMismatch,
                ProviderHostPhase::Preflight,
                &error,
            )
        })?;
        let world = match resolved.selected.invocation() {
            ProviderInvocationKind::Lowering => PreparedWorld::Lowering(
                lowerer_bindings::LowererPre::new(instance_pre).map_err(|error| {
                    ProviderHostFailure::error(
                        ProviderHostFailureKind::ComponentContractMismatch,
                        ProviderHostPhase::Preflight,
                        &error,
                    )
                })?,
            ),
            ProviderInvocationKind::Verification => PreparedWorld::Verification(
                verifier_bindings::VerifierPre::new(instance_pre).map_err(|error| {
                    ProviderHostFailure::error(
                        ProviderHostFailureKind::ComponentContractMismatch,
                        ProviderHostPhase::Preflight,
                        &error,
                    )
                })?,
            ),
        };
        Ok(PreparedProviderComponent {
            selected: resolved.selected,
            engine: self.engine.clone(),
            world,
        })
    }
}

fn verify_component_digest(
    resolved: &ResolvedProviderComponent<'_>,
) -> Result<(), ProviderHostFailure> {
    let actual = format!("sha256:{:x}", Sha256::digest(resolved.bytes()));
    if resolved.selected.resource().digest.as_deref() != Some(actual.as_str()) {
        return Err(ProviderHostFailure::message(
            ProviderHostFailureKind::ComponentDigestMismatch,
            ProviderHostPhase::Preflight,
            "resolved component bytes do not reproduce the manifest digest",
        ));
    }
    Ok(())
}

fn verify_contract_attestation(bytes: &[u8], expected: &str) -> Result<(), ProviderHostFailure> {
    let mut attestations = Vec::new();
    let mut depth = 0_u32;
    for payload in Parser::new(0).parse_all(bytes) {
        let payload = payload.map_err(|error| {
            ProviderHostFailure::error(
                ProviderHostFailureKind::ComponentDecodeFailed,
                ProviderHostPhase::Preflight,
                &error,
            )
        })?;
        match payload {
            Payload::CustomSection(section)
                if depth == 0 && section.name() == PROVIDER_CONTRACT_CUSTOM_SECTION =>
            {
                attestations.push(section.data());
            }
            Payload::ModuleSection { .. } | Payload::ComponentSection { .. } => {
                depth = depth.checked_add(1).ok_or_else(|| {
                    ProviderHostFailure::message(
                        ProviderHostFailureKind::ComponentDecodeFailed,
                        ProviderHostPhase::Preflight,
                        "component nesting depth overflowed",
                    )
                })?;
            }
            Payload::End(_) if depth > 0 => depth -= 1,
            _ => {}
        }
    }
    if attestations.as_slice() != [expected.as_bytes()] {
        return Err(ProviderHostFailure::message(
            ProviderHostFailureKind::ComponentContractMismatch,
            ProviderHostPhase::Preflight,
            "component must carry exactly one matching contract attestation",
        ));
    }
    Ok(())
}

fn verify_component_surface(
    engine: &Engine,
    component: &Component,
    invocation: ProviderInvocationKind,
) -> Result<(), ProviderHostFailure> {
    let component_type = component.component_type();
    for (name, import) in component_type.imports(engine) {
        let type_only_protocol = name == "edict:target-provider/protocol@1.0.0"
            && matches!(import.ty, ComponentItem::ComponentInstance(ref instance)
                if instance.exports(engine).all(|(_, export)| matches!(export.ty, ComponentItem::Type(_))));
        if !type_only_protocol {
            return Err(ProviderHostFailure::message(
                ProviderHostFailureKind::ComponentContractMismatch,
                ProviderHostPhase::Preflight,
                "provider component imports callable or unknown host authority",
            ));
        }
    }
    let expected_export = match invocation {
        ProviderInvocationKind::Lowering => "lower",
        ProviderInvocationKind::Verification => "verify",
    };
    let mut exports = component_type.exports(engine);
    if exports.len() != 1
        || exports
            .next()
            .is_none_or(|(name, _)| name != expected_export)
    {
        return Err(ProviderHostFailure::message(
            ProviderHostFailureKind::ComponentContractMismatch,
            ProviderHostPhase::Preflight,
            "provider component export closure does not match the frozen world",
        ));
    }
    Ok(())
}

fn bounded_diagnostic(message: &str, limit: usize) -> String {
    let mut diagnostic = BoundedDiagnostic::new(limit);
    let _ = diagnostic.write_str(message);
    diagnostic.finish()
}

struct BoundedDiagnostic {
    value: String,
    remaining: usize,
}

impl BoundedDiagnostic {
    fn new(limit: usize) -> Self {
        Self {
            value: String::with_capacity(limit.min(256)),
            remaining: limit,
        }
    }

    fn finish(self) -> String {
        self.value
    }
}

impl fmt::Write for BoundedDiagnostic {
    fn write_str(&mut self, value: &str) -> fmt::Result {
        if self.remaining == 0 {
            return Ok(());
        }
        let mut end = self.remaining.min(value.len());
        while !value.is_char_boundary(end) {
            end -= 1;
        }
        self.value.push_str(&value[..end]);
        self.remaining -= end;
        Ok(())
    }
}

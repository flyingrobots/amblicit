//! Canonical, bounded application-result projections derived from Edict Core.
//!
//! The projection is data. It does not execute application code, call a target,
//! or grant runtime authority. Emitters derive it from exact Core and Target IR;
//! verifiers reconstruct the authored Core result from the claimed projection.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use crate::core_ir::{
    core_type_fits, resolved_core_type, validate_core_module_type_integrity,
    CORE_APPLICATION_INPUT_LOCAL_ID,
};
use crate::{
    decode_canonical_cbor, digest_canonical_artifact, encode_canonical_cbor, CanonicalValue,
    CoreDigest, CoreExpr, CoreIntent, CoreModule, CoreNode, CoreType, LocalRef, TargetIrArtifact,
    TargetIrIntent, TargetIrPureBinding, TargetIrSemanticClosure, TargetIrStep,
};

/// Semantic schema identifier for result-projection values.
pub const RESULT_PROJECTION_API_VERSION: &str = "edict.result-projection/v1";

/// Domain used to compute result-projection artifact identities.
pub const RESULT_PROJECTION_DIGEST_DOMAIN: &str = "edict.result-projection.artifact/v1";

/// CDDL root published for result-projection artifacts.
pub const RESULT_PROJECTION_CDDL_ROOT: &str = "result-projection";

/// Maximum expression nodes in one projection artifact.
pub const MAX_RESULT_PROJECTION_NODES: usize = 256;

/// Maximum field-path segments in one source reference.
pub const MAX_RESULT_PROJECTION_PATH_SEGMENTS: usize = 32;

/// Maximum UTF-8 bytes in any coordinate, field, step, or path segment.
pub const MAX_RESULT_PROJECTION_TEXT_BYTES: usize = 1_024;

/// Maximum canonical bytes in one projection artifact.
pub const MAX_RESULT_PROJECTION_ARTIFACT_BYTES: usize = 64 * 1_024;

/// Untrusted result-projection candidate for one application operation.
///
/// Callers may construct candidates for encoding and verification. Only
/// [`ResultProjectionArtifact`] binds an accepted candidate to canonical bytes
/// and their domain-framed identity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResultProjection {
    /// Projection schema identifier.
    pub api_version: String,
    /// Fully qualified application operation coordinate.
    pub operation_coordinate: String,
    /// Fully qualified Core output type.
    pub output_type: String,
    /// Maximum canonical result bytes admitted by the Core budget.
    pub max_output_bytes: u64,
    /// Closed projection expression derived from the authored Core result.
    pub expression: ResultProjectionExpr,
}

/// Closed projection-expression language.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResultProjectionExpr {
    Record {
        fields: BTreeMap<String, ResultProjectionExpr>,
    },
    Source {
        source: ResultProjectionSource,
        path: Vec<String>,
    },
}

/// Declared data sources available to a result projection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResultProjectionSource {
    ApplicationInput,
    PureBinding { binding_id: String },
    CapabilityResult { step_id: String },
}

/// Emitted canonical bytes and their domain-framed identity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResultProjectionArtifact {
    projection: ResultProjection,
    canonical_bytes: Vec<u8>,
    digest: CoreDigest,
}

impl ResultProjectionArtifact {
    /// Borrow the validated projection value.
    #[must_use]
    pub const fn projection(&self) -> &ResultProjection {
        &self.projection
    }

    /// Borrow the exact canonical bytes bound to the projection.
    #[must_use]
    pub fn canonical_bytes(&self) -> &[u8] {
        &self.canonical_bytes
    }

    /// Return the domain-framed artifact identity.
    #[must_use]
    pub const fn digest(&self) -> CoreDigest {
        self.digest
    }
}

/// Projection accepted after independent semantic reconstruction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedResultProjection {
    projection: ResultProjection,
    digest: CoreDigest,
}

impl VerifiedResultProjection {
    /// Borrow the accepted projection.
    #[must_use]
    pub const fn projection(&self) -> &ResultProjection {
        &self.projection
    }

    /// Return the accepted domain-framed identity.
    #[must_use]
    pub const fn digest(&self) -> CoreDigest {
        self.digest
    }
}

/// Stable result-projection rejection categories.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResultProjectionFailureKind {
    MissingIntent,
    CoreTargetMismatch,
    TargetResultMismatch,
    InvalidApplicationInput,
    UndeclaredProjectionSource,
    UnknownCapabilityStep,
    UnsupportedExpression,
    OutputShapeMismatch,
    InvalidOutputBound,
    ProjectionLimitExceeded,
    InvalidCanonicalArtifact,
    DigestMismatch,
}

/// Structured result-projection failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResultProjectionFailure {
    kind: ResultProjectionFailureKind,
    subject: String,
}

impl ResultProjectionFailure {
    /// Return the stable rejection category.
    #[must_use]
    pub const fn kind(&self) -> ResultProjectionFailureKind {
        self.kind
    }

    /// Return the rejected semantic subject.
    #[must_use]
    pub fn subject(&self) -> &str {
        &self.subject
    }

    fn new(kind: ResultProjectionFailureKind, subject: impl Into<String>) -> Self {
        Self {
            kind,
            subject: subject.into(),
        }
    }
}

impl fmt::Display for ResultProjectionFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{:?}: {}", self.kind, self.subject)
    }
}

impl std::error::Error for ResultProjectionFailure {}

/// Derive and emit one canonical projection artifact.
///
/// # Errors
///
/// Returns a structured failure when Core, Target IR, result sources, output
/// shape, or representation bounds do not admit the closed projection subset.
pub fn emit_result_projection(
    core: &CoreModule,
    target_ir: &TargetIrArtifact,
    intent_name: &str,
) -> Result<ResultProjectionArtifact, ResultProjectionFailure> {
    let validated_core =
        validate_core_module_type_integrity(core).map_err(|integrity_failure| {
            failure(
                ResultProjectionFailureKind::CoreTargetMismatch,
                integrity_failure.path(),
            )
        })?;
    let core = validated_core.module();
    let expected_semantic_closure =
        crate::target_ir::semantic_closure_for_core(core).map_err(|error| {
            failure(
                ResultProjectionFailureKind::CoreTargetMismatch,
                format!("{error:?}"),
            )
        })?;
    let expected_semantic_closure = expected_semantic_closure.as_ref().ok_or_else(|| {
        failure(
            ResultProjectionFailureKind::CoreTargetMismatch,
            format!("{intent_name}.semanticClosure"),
        )
    })?;
    emit_result_projection_with_closure(core, target_ir, intent_name, expected_semantic_closure)
}

pub(crate) fn emit_result_projection_with_closure(
    core: &CoreModule,
    target_ir: &TargetIrArtifact,
    intent_name: &str,
    expected_semantic_closure: &TargetIrSemanticClosure,
) -> Result<ResultProjectionArtifact, ResultProjectionFailure> {
    let semantics =
        ProjectionSemantics::new(core, target_ir, intent_name, expected_semantic_closure)?;
    let expression = project_core_expression(&semantics, &semantics.core_intent.body.result)?;
    validate_projection_output_type(core, &semantics.core_intent.output)?;
    validate_output_shape(&semantics, &expression, &semantics.core_intent.output)?;
    let projection = ResultProjection {
        api_version: RESULT_PROJECTION_API_VERSION.to_owned(),
        operation_coordinate: format!("{}.{}", core.coordinate, intent_name),
        output_type: semantics.core_intent.output.clone(),
        max_output_bytes: semantics
            .core_intent
            .core_evaluation_budget
            .max_output_bytes,
        expression,
    };
    let canonical_bytes = encode_result_projection(&projection)?;
    let digest = digest_canonical_artifact(RESULT_PROJECTION_DIGEST_DOMAIN, &canonical_bytes)
        .map_err(|error| {
            failure(
                ResultProjectionFailureKind::InvalidCanonicalArtifact,
                error.to_string(),
            )
        })?;
    Ok(ResultProjectionArtifact {
        projection,
        canonical_bytes,
        digest,
    })
}

/// Independently verify claimed projection bytes against exact Core and Target IR.
///
/// # Errors
///
/// Returns a structured failure for malformed or non-canonical bytes, digest
/// mismatch, unsupported semantics, or failed reverse reconstruction.
pub fn verify_result_projection(
    core: &CoreModule,
    target_ir: &TargetIrArtifact,
    intent_name: &str,
    canonical_bytes: &[u8],
    claimed_digest: CoreDigest,
) -> Result<VerifiedResultProjection, ResultProjectionFailure> {
    let validated_core =
        validate_core_module_type_integrity(core).map_err(|integrity_failure| {
            failure(
                ResultProjectionFailureKind::CoreTargetMismatch,
                integrity_failure.path(),
            )
        })?;
    let core = validated_core.module();
    let projection = decode_result_projection(canonical_bytes)?;
    let digest = digest_canonical_artifact(RESULT_PROJECTION_DIGEST_DOMAIN, canonical_bytes)
        .map_err(|error| {
            failure(
                ResultProjectionFailureKind::InvalidCanonicalArtifact,
                error.to_string(),
            )
        })?;
    if digest != claimed_digest {
        return Err(failure(
            ResultProjectionFailureKind::DigestMismatch,
            "claimed projection digest",
        ));
    }

    let expected_semantic_closure =
        crate::target_ir::semantic_closure_for_core(core).map_err(|error| {
            failure(
                ResultProjectionFailureKind::CoreTargetMismatch,
                format!("{error:?}"),
            )
        })?;
    let expected_semantic_closure = expected_semantic_closure.as_ref().ok_or_else(|| {
        failure(
            ResultProjectionFailureKind::CoreTargetMismatch,
            format!("{intent_name}.semanticClosure"),
        )
    })?;
    let semantics =
        ProjectionSemantics::new(core, target_ir, intent_name, expected_semantic_closure)?;
    if projection.api_version != RESULT_PROJECTION_API_VERSION
        || projection.operation_coordinate != format!("{}.{}", core.coordinate, intent_name)
        || projection.output_type != semantics.core_intent.output
        || projection.max_output_bytes
            != semantics
                .core_intent
                .core_evaluation_budget
                .max_output_bytes
    {
        return Err(failure(
            ResultProjectionFailureKind::CoreTargetMismatch,
            "projection envelope",
        ));
    }
    let reconstructed = reconstruct_core_expression(&semantics, &projection.expression)?;
    if reconstructed != semantics.core_intent.body.result
        || reconstructed != semantics.target_intent.result
    {
        return Err(failure(
            ResultProjectionFailureKind::TargetResultMismatch,
            intent_name,
        ));
    }
    validate_projection_output_type(core, &semantics.core_intent.output)?;
    validate_output_shape(
        &semantics,
        &projection.expression,
        &semantics.core_intent.output,
    )?;
    Ok(VerifiedResultProjection { projection, digest })
}

/// Encode one projection using `edict.canonical-cbor/v1`.
///
/// # Errors
///
/// Returns a structured failure when the projection violates a representation
/// bound or cannot be represented canonically.
pub fn encode_result_projection(
    projection: &ResultProjection,
) -> Result<Vec<u8>, ResultProjectionFailure> {
    validate_projection_bounds(projection)?;
    let bytes = encode_canonical_cbor(&projection_value(projection)).map_err(|error| {
        failure(
            ResultProjectionFailureKind::InvalidCanonicalArtifact,
            error.to_string(),
        )
    })?;
    if bytes.len() > MAX_RESULT_PROJECTION_ARTIFACT_BYTES {
        return Err(limit_failure("artifact bytes"));
    }
    Ok(bytes)
}

/// Decode and validate one canonical projection value.
///
/// # Errors
///
/// Returns a structured failure for malformed, non-canonical, incomplete,
/// unsupported, or unbounded projection bytes.
pub fn decode_result_projection(bytes: &[u8]) -> Result<ResultProjection, ResultProjectionFailure> {
    if bytes.len() > MAX_RESULT_PROJECTION_ARTIFACT_BYTES {
        return Err(limit_failure("artifact bytes"));
    }
    let value = decode_canonical_cbor(bytes).map_err(|error| {
        failure(
            ResultProjectionFailureKind::InvalidCanonicalArtifact,
            error.to_string(),
        )
    })?;
    let projection = projection_from_value(value)?;
    if encode_result_projection(&projection)? != bytes {
        return Err(failure(
            ResultProjectionFailureKind::InvalidCanonicalArtifact,
            "projection bytes do not reproduce exactly",
        ));
    }
    Ok(projection)
}

/// Compute the domain-framed identity of one projection.
///
/// # Errors
///
/// Returns a structured failure when canonical encoding fails.
pub fn digest_result_projection(
    projection: &ResultProjection,
) -> Result<CoreDigest, ResultProjectionFailure> {
    let bytes = encode_result_projection(projection)?;
    digest_canonical_artifact(RESULT_PROJECTION_DIGEST_DOMAIN, &bytes).map_err(|error| {
        failure(
            ResultProjectionFailureKind::InvalidCanonicalArtifact,
            error.to_string(),
        )
    })
}

struct ProjectionSemantics<'a> {
    core: &'a CoreModule,
    core_intent: &'a CoreIntent,
    target_intent: &'a TargetIrIntent,
    input: LocalRef,
    pure_by_local: BTreeMap<String, (String, LocalRef)>,
    pure_by_id: BTreeMap<String, LocalRef>,
    capability_by_local: BTreeMap<String, (String, LocalRef)>,
    capability_by_step: BTreeMap<String, LocalRef>,
}

impl<'a> ProjectionSemantics<'a> {
    fn new(
        core: &'a CoreModule,
        target_ir: &'a TargetIrArtifact,
        intent_name: &str,
        expected_semantic_closure: &TargetIrSemanticClosure,
    ) -> Result<Self, ResultProjectionFailure> {
        let (core_intent, target_intent) =
            resolve_projection_intents(core, target_ir, intent_name)?;
        validate_projection_basis(
            core,
            target_ir,
            core_intent,
            target_intent,
            intent_name,
            expected_semantic_closure,
        )?;
        let input = resolve_application_input(core_intent, intent_name)?;
        let (pure_by_local, pure_by_id, capability_by_local, capability_by_step) =
            resolve_projection_sources(core_intent, target_intent, intent_name, &input)?;

        Ok(Self {
            core,
            core_intent,
            target_intent,
            input,
            pure_by_local,
            pure_by_id,
            capability_by_local,
            capability_by_step,
        })
    }
}

fn resolve_projection_intents<'a>(
    core: &'a CoreModule,
    target_ir: &'a TargetIrArtifact,
    intent_name: &str,
) -> Result<(&'a CoreIntent, &'a TargetIrIntent), ResultProjectionFailure> {
    let core_intent = core.intents.get(intent_name).ok_or_else(|| {
        failure(
            ResultProjectionFailureKind::MissingIntent,
            format!("Core intent {intent_name}"),
        )
    })?;
    let target_intent = target_ir.intents.get(intent_name).ok_or_else(|| {
        failure(
            ResultProjectionFailureKind::MissingIntent,
            format!("Target IR intent {intent_name}"),
        )
    })?;
    Ok((core_intent, target_intent))
}

fn validate_projection_basis(
    core: &CoreModule,
    target_ir: &TargetIrArtifact,
    core_intent: &CoreIntent,
    target_intent: &TargetIrIntent,
    intent_name: &str,
    expected_semantic_closure: &TargetIrSemanticClosure,
) -> Result<(), ResultProjectionFailure> {
    if target_ir.source_core_coordinate != core.coordinate
        || core_intent.required_operation_profile != target_intent.operation_profile
        || core_intent.core_evaluation_budget != target_intent.core_evaluation_budget
    {
        return Err(failure(
            ResultProjectionFailureKind::CoreTargetMismatch,
            intent_name,
        ));
    }
    if target_ir.semantic_closure.as_ref() != Some(expected_semantic_closure) {
        return Err(failure(
            ResultProjectionFailureKind::CoreTargetMismatch,
            format!("{intent_name}.semanticClosure"),
        ));
    }
    if core_intent.body.result != target_intent.result {
        return Err(failure(
            ResultProjectionFailureKind::TargetResultMismatch,
            intent_name,
        ));
    }
    if core_intent.core_evaluation_budget.max_output_bytes == 0 {
        return Err(failure(
            ResultProjectionFailureKind::InvalidOutputBound,
            intent_name,
        ));
    }
    Ok(())
}

fn resolve_application_input(
    core_intent: &CoreIntent,
    intent_name: &str,
) -> Result<LocalRef, ResultProjectionFailure> {
    let mut matching_inputs = core_intent.body.locals.iter().filter(|local| {
        local.id == CORE_APPLICATION_INPUT_LOCAL_ID && local.ty == core_intent.input
    });
    let input = matching_inputs.next().cloned().ok_or_else(|| {
        failure(
            ResultProjectionFailureKind::InvalidApplicationInput,
            intent_name,
        )
    })?;
    if matching_inputs.next().is_some() {
        return Err(failure(
            ResultProjectionFailureKind::InvalidApplicationInput,
            intent_name,
        ));
    }
    Ok(input)
}

type ProjectionByLocal = BTreeMap<String, (String, LocalRef)>;
type ProjectionById = BTreeMap<String, LocalRef>;

fn resolve_projection_sources(
    core_intent: &CoreIntent,
    target_intent: &TargetIrIntent,
    intent_name: &str,
    input: &LocalRef,
) -> Result<
    (
        ProjectionByLocal,
        ProjectionById,
        ProjectionByLocal,
        ProjectionById,
    ),
    ResultProjectionFailure,
> {
    let core_pure_bindings = core_intent
        .body
        .nodes
        .iter()
        .filter_map(|node| match node {
            CoreNode::Let { binding, value } => Some((binding, value)),
            _ => None,
        })
        .collect::<Vec<_>>();
    let mut core_effects = Vec::new();
    for node in &core_intent.body.nodes {
        match node {
            CoreNode::Effect {
                binding,
                effect,
                input,
                ..
            } => core_effects.push((binding, effect, input)),
            CoreNode::Let { .. }
            | CoreNode::Require { .. }
            | CoreNode::ExternalActionRequest { .. } => {}
            CoreNode::For { .. } | CoreNode::Branch { .. } => {
                return Err(failure(
                    ResultProjectionFailureKind::CoreTargetMismatch,
                    format!("{intent_name}.structuredControl"),
                ));
            }
        }
    }
    if core_pure_bindings.len() != target_intent.pure_bindings.len() {
        return Err(failure(
            ResultProjectionFailureKind::CoreTargetMismatch,
            format!("{intent_name}.pureBindings"),
        ));
    }
    if core_effects.len() != target_intent.steps.len() {
        return Err(failure(
            ResultProjectionFailureKind::CoreTargetMismatch,
            format!("{intent_name}.steps"),
        ));
    }

    let mut claimed_local_ids = BTreeSet::from([input.id.clone()]);
    let mut pure_by_local = BTreeMap::new();
    let mut pure_by_id = BTreeMap::new();
    for (binding_index, ((core_binding, core_value), binding)) in core_pure_bindings
        .iter()
        .zip(&target_intent.pure_bindings)
        .enumerate()
    {
        validate_pure_binding_matches_core(
            binding,
            core_binding,
            core_value,
            intent_name,
            binding_index,
        )?;
        if !claimed_local_ids.insert(binding.binding.id.clone())
            || pure_by_local
                .insert(
                    binding.binding.id.clone(),
                    (binding.id.clone(), binding.binding.clone()),
                )
                .is_some()
            || pure_by_id
                .insert(binding.id.clone(), binding.binding.clone())
                .is_some()
        {
            return Err(failure(
                ResultProjectionFailureKind::CoreTargetMismatch,
                format!("{intent_name}.pureBindings"),
            ));
        }
    }
    let mut capability_by_local = BTreeMap::new();
    let mut capability_by_step = BTreeMap::new();
    for step in &target_intent.steps {
        validate_step_matches_core(step, &core_effects, intent_name)?;
        if !claimed_local_ids.insert(step.binding.id.clone())
            || capability_by_local
                .insert(
                    step.binding.id.clone(),
                    (step.id.clone(), step.binding.clone()),
                )
                .is_some()
            || capability_by_step
                .insert(step.id.clone(), step.binding.clone())
                .is_some()
        {
            return Err(failure(
                ResultProjectionFailureKind::CoreTargetMismatch,
                format!("{intent_name}.steps"),
            ));
        }
    }
    Ok((
        pure_by_local,
        pure_by_id,
        capability_by_local,
        capability_by_step,
    ))
}

fn validate_pure_binding_matches_core(
    binding: &TargetIrPureBinding,
    core_binding: &LocalRef,
    core_value: &CoreExpr,
    intent_name: &str,
    binding_index: usize,
) -> Result<(), ResultProjectionFailure> {
    let expected_id = format!("{intent_name}.binding.{binding_index}");
    if binding.id == expected_id && core_binding == &binding.binding && core_value == &binding.value
    {
        Ok(())
    } else {
        Err(failure(
            ResultProjectionFailureKind::CoreTargetMismatch,
            format!("{intent_name}.{}", binding.id),
        ))
    }
}

fn validate_step_matches_core(
    step: &TargetIrStep,
    core_effects: &[(&LocalRef, &String, &CoreExpr)],
    intent_name: &str,
) -> Result<(), ResultProjectionFailure> {
    let matches = core_effects
        .iter()
        .filter(|(binding, effect, input)| {
            **binding == step.binding && **effect == step.effect && **input == step.input
        })
        .count();
    if matches == 1 {
        Ok(())
    } else {
        Err(failure(
            ResultProjectionFailureKind::CoreTargetMismatch,
            format!("{intent_name}.{}", step.id),
        ))
    }
}

fn project_core_expression(
    semantics: &ProjectionSemantics<'_>,
    expression: &CoreExpr,
) -> Result<ResultProjectionExpr, ResultProjectionFailure> {
    match expression {
        CoreExpr::Record { fields } => Ok(ResultProjectionExpr::Record {
            fields: fields
                .iter()
                .map(|(name, field)| Ok((name.clone(), project_core_expression(semantics, field)?)))
                .collect::<Result<BTreeMap<_, _>, ResultProjectionFailure>>()?,
        }),
        CoreExpr::Local { .. } | CoreExpr::Field { .. } => {
            let (reference, path) = source_path(expression)?;
            let source = if reference == &semantics.input {
                ResultProjectionSource::ApplicationInput
            } else if let Some((binding_id, binding)) = semantics.pure_by_local.get(&reference.id) {
                if binding != reference {
                    return Err(failure(
                        ResultProjectionFailureKind::UndeclaredProjectionSource,
                        &reference.id,
                    ));
                }
                ResultProjectionSource::PureBinding {
                    binding_id: binding_id.clone(),
                }
            } else if let Some((step_id, binding)) =
                semantics.capability_by_local.get(&reference.id)
            {
                if binding != reference {
                    return Err(failure(
                        ResultProjectionFailureKind::UndeclaredProjectionSource,
                        &reference.id,
                    ));
                }
                ResultProjectionSource::CapabilityResult {
                    step_id: step_id.clone(),
                }
            } else {
                return Err(failure(
                    ResultProjectionFailureKind::UndeclaredProjectionSource,
                    &reference.id,
                ));
            };
            Ok(ResultProjectionExpr::Source { source, path })
        }
        CoreExpr::Fold { .. }
        | CoreExpr::Match { .. }
        | CoreExpr::Variant { .. }
        | CoreExpr::Const(_)
        | CoreExpr::List { .. }
        | CoreExpr::Call { .. }
        | CoreExpr::If { .. } => Err(failure(
            ResultProjectionFailureKind::UnsupportedExpression,
            "result expression",
        )),
    }
}

fn source_path(expression: &CoreExpr) -> Result<(&LocalRef, Vec<String>), ResultProjectionFailure> {
    let mut current = expression;
    let mut path = Vec::new();
    while let CoreExpr::Field { base, field } = current {
        path.push(field.clone());
        current = base;
    }
    path.reverse();
    match current {
        CoreExpr::Local { reference } => Ok((reference, path)),
        CoreExpr::Fold { .. }
        | CoreExpr::Match { .. }
        | CoreExpr::Variant { .. }
        | CoreExpr::Const(_)
        | CoreExpr::Record { .. }
        | CoreExpr::List { .. }
        | CoreExpr::Call { .. }
        | CoreExpr::If { .. } => Err(failure(
            ResultProjectionFailureKind::UnsupportedExpression,
            "projection source",
        )),
        CoreExpr::Field { .. } => unreachable!("field roots are consumed by the loop"),
    }
}

fn reconstruct_core_expression(
    semantics: &ProjectionSemantics<'_>,
    expression: &ResultProjectionExpr,
) -> Result<CoreExpr, ResultProjectionFailure> {
    match expression {
        ResultProjectionExpr::Record { fields } => Ok(CoreExpr::Record {
            fields: fields
                .iter()
                .map(|(name, field)| {
                    Ok((name.clone(), reconstruct_core_expression(semantics, field)?))
                })
                .collect::<Result<BTreeMap<_, _>, ResultProjectionFailure>>()?,
        }),
        ResultProjectionExpr::Source { source, path } => {
            let reference = match source {
                ResultProjectionSource::ApplicationInput => semantics.input.clone(),
                ResultProjectionSource::PureBinding { binding_id } => semantics
                    .pure_by_id
                    .get(binding_id)
                    .cloned()
                    .ok_or_else(|| {
                        failure(
                            ResultProjectionFailureKind::UndeclaredProjectionSource,
                            binding_id,
                        )
                    })?,
                ResultProjectionSource::CapabilityResult { step_id } => semantics
                    .capability_by_step
                    .get(step_id)
                    .cloned()
                    .ok_or_else(|| {
                        failure(ResultProjectionFailureKind::UnknownCapabilityStep, step_id)
                    })?,
            };
            let mut expression = CoreExpr::Local { reference };
            for field in path {
                expression = CoreExpr::Field {
                    base: Box::new(expression),
                    field: field.clone(),
                };
            }
            Ok(expression)
        }
    }
}

// Variant values currently have no application-result encoding contract.
// Walk the reachable output graph once, including structural references, so
// projecting a whole aggregate cannot hide a nested variant from either gate.
fn validate_projection_output_type(
    core: &CoreModule,
    output: &str,
) -> Result<(), ResultProjectionFailure> {
    let mut pending = vec![output.to_owned()];
    let mut seen = BTreeSet::new();
    while let Some(coordinate) = pending.pop() {
        if !seen.insert(coordinate.clone()) {
            continue;
        }
        let ty = resolved_core_type(core, &coordinate).ok_or_else(|| {
            failure(
                ResultProjectionFailureKind::OutputShapeMismatch,
                &coordinate,
            )
        })?;
        match ty {
            CoreType::Variant { .. } => {
                return Err(failure(
                    ResultProjectionFailureKind::OutputShapeMismatch,
                    output,
                ));
            }
            CoreType::Record { fields } => pending.extend(fields.into_values()),
            CoreType::Nominal { representation, .. } => pending.push(representation),
            CoreType::Option { item }
            | CoreType::List { item, .. }
            | CoreType::CapabilityRef { item } => pending.push(item),
            CoreType::Map { key, value, .. } => pending.extend([key, value]),
            CoreType::ExternalActionRequest { settlement } => pending.push(settlement),
            CoreType::Bool
            | CoreType::Unit
            | CoreType::Int { .. }
            | CoreType::String { .. }
            | CoreType::Bytes { .. } => {}
        }
    }
    Ok(())
}

fn validate_output_shape(
    semantics: &ProjectionSemantics<'_>,
    expression: &ResultProjectionExpr,
    expected_type: &str,
) -> Result<(), ResultProjectionFailure> {
    match expression {
        ResultProjectionExpr::Record { fields } => {
            let CoreType::Record {
                fields: expected_fields,
            } = resolve_type(semantics.core, expected_type).ok_or_else(|| {
                failure(
                    ResultProjectionFailureKind::OutputShapeMismatch,
                    expected_type,
                )
            })?
            else {
                return Err(failure(
                    ResultProjectionFailureKind::OutputShapeMismatch,
                    expected_type,
                ));
            };
            if fields.keys().ne(expected_fields.keys()) {
                return Err(failure(
                    ResultProjectionFailureKind::OutputShapeMismatch,
                    expected_type,
                ));
            }
            for (field, expected_field_type) in expected_fields {
                validate_output_shape(
                    semantics,
                    fields.get(field).expect("equal field keys"),
                    expected_field_type,
                )?;
            }
            Ok(())
        }
        ResultProjectionExpr::Source { source, path } => {
            let source_type = source_type(semantics, source, path)?;
            if core_type_fits(semantics.core, source_type, expected_type) {
                Ok(())
            } else {
                Err(failure(
                    ResultProjectionFailureKind::OutputShapeMismatch,
                    expected_type,
                ))
            }
        }
    }
}

fn source_type<'a>(
    semantics: &'a ProjectionSemantics<'_>,
    source: &ResultProjectionSource,
    path: &[String],
) -> Result<&'a str, ResultProjectionFailure> {
    let mut coordinate = match source {
        ResultProjectionSource::ApplicationInput => semantics.input.ty.as_str(),
        ResultProjectionSource::PureBinding { binding_id } => semantics
            .pure_by_id
            .get(binding_id)
            .map(|binding| binding.ty.as_str())
            .ok_or_else(|| {
                failure(
                    ResultProjectionFailureKind::UndeclaredProjectionSource,
                    binding_id,
                )
            })?,
        ResultProjectionSource::CapabilityResult { step_id } => semantics
            .capability_by_step
            .get(step_id)
            .map(|binding| binding.ty.as_str())
            .ok_or_else(|| failure(ResultProjectionFailureKind::UnknownCapabilityStep, step_id))?,
    };
    for field in path {
        let CoreType::Record { fields } = resolve_type(semantics.core, coordinate)
            .ok_or_else(|| failure(ResultProjectionFailureKind::OutputShapeMismatch, coordinate))?
        else {
            return Err(failure(
                ResultProjectionFailureKind::OutputShapeMismatch,
                coordinate,
            ));
        };
        coordinate = fields.get(field).map(String::as_str).ok_or_else(|| {
            failure(
                ResultProjectionFailureKind::OutputShapeMismatch,
                format!("{coordinate}.{field}"),
            )
        })?;
    }
    Ok(coordinate)
}

fn resolve_type<'a>(core: &'a CoreModule, coordinate: &str) -> Option<&'a CoreType> {
    core.types.get(coordinate).or_else(|| {
        coordinate
            .strip_prefix(core.coordinate.as_str())
            .and_then(|relative| relative.strip_prefix('.'))
            .and_then(|relative| core.types.get(relative))
    })
}

fn validate_projection_bounds(
    projection: &ResultProjection,
) -> Result<(), ResultProjectionFailure> {
    if projection.api_version != RESULT_PROJECTION_API_VERSION {
        return Err(failure(
            ResultProjectionFailureKind::InvalidCanonicalArtifact,
            "projection schema",
        ));
    }
    validate_text(&projection.operation_coordinate, "operation coordinate")?;
    validate_text(&projection.output_type, "output type")?;
    if projection.max_output_bytes == 0 {
        return Err(failure(
            ResultProjectionFailureKind::InvalidOutputBound,
            "max output bytes",
        ));
    }
    let nodes = validate_expression_bounds(&projection.expression)?;
    if nodes > MAX_RESULT_PROJECTION_NODES {
        return Err(limit_failure("expression nodes"));
    }
    Ok(())
}

fn validate_expression_bounds(
    expression: &ResultProjectionExpr,
) -> Result<usize, ResultProjectionFailure> {
    match expression {
        ResultProjectionExpr::Record { fields } => {
            let mut nodes = 1usize;
            for (field, value) in fields {
                validate_text(field, "record field")?;
                nodes = nodes
                    .checked_add(validate_expression_bounds(value)?)
                    .ok_or_else(|| limit_failure("expression nodes"))?;
                if nodes > MAX_RESULT_PROJECTION_NODES {
                    return Err(limit_failure("expression nodes"));
                }
            }
            Ok(nodes)
        }
        ResultProjectionExpr::Source { source, path } => {
            match source {
                ResultProjectionSource::ApplicationInput => {}
                ResultProjectionSource::PureBinding { binding_id } => {
                    validate_text(binding_id, "pure binding")?;
                }
                ResultProjectionSource::CapabilityResult { step_id } => {
                    validate_text(step_id, "capability step")?;
                }
            }
            if path.len() > MAX_RESULT_PROJECTION_PATH_SEGMENTS {
                return Err(limit_failure("source path"));
            }
            for field in path {
                validate_text(field, "source path field")?;
            }
            Ok(1)
        }
    }
}

fn validate_text(value: &str, subject: &str) -> Result<(), ResultProjectionFailure> {
    if value.is_empty() {
        return Err(failure(
            ResultProjectionFailureKind::InvalidCanonicalArtifact,
            subject,
        ));
    }
    if value.len() > MAX_RESULT_PROJECTION_TEXT_BYTES {
        return Err(limit_failure(subject));
    }
    Ok(())
}

fn projection_value(projection: &ResultProjection) -> CanonicalValue {
    map([
        ("schema", text(&projection.api_version)),
        (
            "operationCoordinate",
            text(&projection.operation_coordinate),
        ),
        ("outputType", text(&projection.output_type)),
        (
            "maxOutputBytes",
            CanonicalValue::Integer(i128::from(projection.max_output_bytes)),
        ),
        ("expression", expression_value(&projection.expression)),
    ])
}

fn expression_value(expression: &ResultProjectionExpr) -> CanonicalValue {
    match expression {
        ResultProjectionExpr::Record { fields } => map([
            ("kind", text("record")),
            (
                "fields",
                CanonicalValue::Map(
                    fields
                        .iter()
                        .map(|(field, value)| (text(field), expression_value(value)))
                        .collect(),
                ),
            ),
        ]),
        ResultProjectionExpr::Source { source, path } => map([
            ("kind", text("source")),
            ("source", source_value(source)),
            (
                "path",
                CanonicalValue::Array(path.iter().map(|field| text(field)).collect()),
            ),
        ]),
    }
}

fn source_value(source: &ResultProjectionSource) -> CanonicalValue {
    match source {
        ResultProjectionSource::ApplicationInput => map([("kind", text("applicationInput"))]),
        ResultProjectionSource::PureBinding { binding_id } => map([
            ("kind", text("pureBinding")),
            ("bindingId", text(binding_id)),
        ]),
        ResultProjectionSource::CapabilityResult { step_id } => map([
            ("kind", text("capabilityResult")),
            ("stepId", text(step_id)),
        ]),
    }
}

fn projection_from_value(
    value: CanonicalValue,
) -> Result<ResultProjection, ResultProjectionFailure> {
    let mut fields = exact_map(
        value,
        &[
            "schema",
            "operationCoordinate",
            "outputType",
            "maxOutputBytes",
            "expression",
        ],
        "result projection",
    )?;
    let api_version = take_text(&mut fields, "schema", "result projection")?;
    let operation_coordinate = take_text(&mut fields, "operationCoordinate", "result projection")?;
    let output_type = take_text(&mut fields, "outputType", "result projection")?;
    let max_output_bytes = take_u64(&mut fields, "maxOutputBytes", "result projection")?;
    let expression = expression_from_value(
        fields.remove("expression").expect("exact projection field"),
        MAX_RESULT_PROJECTION_NODES,
    )?;
    Ok(ResultProjection {
        api_version,
        operation_coordinate,
        output_type,
        max_output_bytes,
        expression,
    })
}

fn expression_from_value(
    value: CanonicalValue,
    remaining_depth: usize,
) -> Result<ResultProjectionExpr, ResultProjectionFailure> {
    if remaining_depth == 0 {
        return Err(limit_failure("expression depth"));
    }
    let kind = map_text_field(&value, "kind", "projection expression")?;
    match kind.as_str() {
        "record" => {
            let mut fields = exact_map(value, &["kind", "fields"], "record expression")?;
            require_text(&mut fields, "kind", "record", "record expression")?;
            let CanonicalValue::Map(entries) = fields.remove("fields").expect("exact record field")
            else {
                return Err(invalid_artifact("record expression fields"));
            };
            let mut projected = BTreeMap::new();
            for (field, value) in entries {
                let CanonicalValue::Text(field) = field else {
                    return Err(invalid_artifact("record expression field name"));
                };
                if projected
                    .insert(field, expression_from_value(value, remaining_depth - 1)?)
                    .is_some()
                {
                    return Err(invalid_artifact("duplicate record expression field"));
                }
            }
            Ok(ResultProjectionExpr::Record { fields: projected })
        }
        "source" => {
            let mut fields = exact_map(value, &["kind", "source", "path"], "source expression")?;
            require_text(&mut fields, "kind", "source", "source expression")?;
            let source = source_from_value(fields.remove("source").expect("exact source field"))?;
            let CanonicalValue::Array(path) =
                fields.remove("path").expect("exact source path field")
            else {
                return Err(invalid_artifact("source expression path"));
            };
            let path = path
                .into_iter()
                .map(|field| match field {
                    CanonicalValue::Text(field) => Ok(field),
                    _ => Err(invalid_artifact("source expression path field")),
                })
                .collect::<Result<Vec<_>, _>>()?;
            Ok(ResultProjectionExpr::Source { source, path })
        }
        _ => Err(invalid_artifact("projection expression kind")),
    }
}

fn source_from_value(
    value: CanonicalValue,
) -> Result<ResultProjectionSource, ResultProjectionFailure> {
    let kind = map_text_field(&value, "kind", "projection source")?;
    match kind.as_str() {
        "applicationInput" => {
            let mut fields = exact_map(value, &["kind"], "application input source")?;
            require_text(
                &mut fields,
                "kind",
                "applicationInput",
                "application input source",
            )?;
            Ok(ResultProjectionSource::ApplicationInput)
        }
        "capabilityResult" => {
            let mut fields = exact_map(value, &["kind", "stepId"], "capability result source")?;
            require_text(
                &mut fields,
                "kind",
                "capabilityResult",
                "capability result source",
            )?;
            Ok(ResultProjectionSource::CapabilityResult {
                step_id: take_text(&mut fields, "stepId", "capability result source")?,
            })
        }
        "pureBinding" => {
            let mut fields = exact_map(value, &["kind", "bindingId"], "pure binding source")?;
            require_text(&mut fields, "kind", "pureBinding", "pure binding source")?;
            Ok(ResultProjectionSource::PureBinding {
                binding_id: take_text(&mut fields, "bindingId", "pure binding source")?,
            })
        }
        _ => Err(invalid_artifact("projection source kind")),
    }
}

fn exact_map(
    value: CanonicalValue,
    expected: &[&str],
    subject: &str,
) -> Result<BTreeMap<String, CanonicalValue>, ResultProjectionFailure> {
    let CanonicalValue::Map(entries) = value else {
        return Err(invalid_artifact(subject));
    };
    let mut fields = BTreeMap::new();
    for (key, value) in entries {
        let CanonicalValue::Text(key) = key else {
            return Err(invalid_artifact(subject));
        };
        if fields.insert(key, value).is_some() {
            return Err(invalid_artifact(subject));
        }
    }
    if fields.len() != expected.len() || expected.iter().any(|field| !fields.contains_key(*field)) {
        return Err(invalid_artifact(subject));
    }
    Ok(fields)
}

fn map_text_field(
    value: &CanonicalValue,
    field: &str,
    subject: &str,
) -> Result<String, ResultProjectionFailure> {
    let CanonicalValue::Map(entries) = value else {
        return Err(invalid_artifact(subject));
    };
    entries
        .iter()
        .find_map(|(key, value)| match (key, value) {
            (CanonicalValue::Text(key), CanonicalValue::Text(value)) if key == field => {
                Some(value.clone())
            }
            _ => None,
        })
        .ok_or_else(|| invalid_artifact(subject))
}

fn take_text(
    fields: &mut BTreeMap<String, CanonicalValue>,
    field: &str,
    subject: &str,
) -> Result<String, ResultProjectionFailure> {
    match fields.remove(field) {
        Some(CanonicalValue::Text(value)) => Ok(value),
        _ => Err(invalid_artifact(subject)),
    }
}

fn take_u64(
    fields: &mut BTreeMap<String, CanonicalValue>,
    field: &str,
    subject: &str,
) -> Result<u64, ResultProjectionFailure> {
    match fields.remove(field) {
        Some(CanonicalValue::Integer(value)) => {
            u64::try_from(value).map_err(|_| invalid_artifact(subject))
        }
        _ => Err(invalid_artifact(subject)),
    }
}

fn require_text(
    fields: &mut BTreeMap<String, CanonicalValue>,
    field: &str,
    expected: &str,
    subject: &str,
) -> Result<(), ResultProjectionFailure> {
    if take_text(fields, field, subject)? == expected {
        Ok(())
    } else {
        Err(invalid_artifact(subject))
    }
}

fn map<const N: usize>(entries: [(&str, CanonicalValue); N]) -> CanonicalValue {
    CanonicalValue::Map(
        entries
            .into_iter()
            .map(|(key, value)| (text(key), value))
            .collect(),
    )
}

fn text(value: &str) -> CanonicalValue {
    CanonicalValue::Text(value.to_owned())
}

fn invalid_artifact(subject: impl Into<String>) -> ResultProjectionFailure {
    failure(
        ResultProjectionFailureKind::InvalidCanonicalArtifact,
        subject,
    )
}

fn limit_failure(subject: impl Into<String>) -> ResultProjectionFailure {
    failure(
        ResultProjectionFailureKind::ProjectionLimitExceeded,
        subject,
    )
}

fn failure(
    kind: ResultProjectionFailureKind,
    subject: impl Into<String>,
) -> ResultProjectionFailure {
    ResultProjectionFailure::new(kind, subject)
}

#[cfg(test)]
mod tests {
    use super::{
        expression_from_value, map, text, CanonicalValue, ResultProjectionFailureKind,
        MAX_RESULT_PROJECTION_NODES,
    };

    fn nested_record_expression(record_count: usize) -> CanonicalValue {
        let mut expression = map([
            ("kind", text("source")),
            ("source", map([("kind", text("applicationInput"))])),
            ("path", CanonicalValue::Array(Vec::new())),
        ]);
        for index in 0..record_count {
            expression = map([
                ("kind", text("record")),
                (
                    "fields",
                    CanonicalValue::Map(vec![(text(&format!("field{index:03}")), expression)]),
                ),
            ]);
        }
        expression
    }

    #[test]
    fn expression_parser_refuses_recursion_beyond_the_node_budget() {
        expression_from_value(
            nested_record_expression(MAX_RESULT_PROJECTION_NODES - 1),
            MAX_RESULT_PROJECTION_NODES,
        )
        .expect("the exact expression-node boundary parses");

        let failure = expression_from_value(
            nested_record_expression(MAX_RESULT_PROJECTION_NODES),
            MAX_RESULT_PROJECTION_NODES,
        )
        .expect_err("one recursive expression node beyond the budget must reject");
        assert_eq!(
            failure.kind(),
            ResultProjectionFailureKind::ProjectionLimitExceeded
        );
    }
}

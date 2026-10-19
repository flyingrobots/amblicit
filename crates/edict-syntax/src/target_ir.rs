//! First target-owned IR generation surface.
//!
//! This module contains the narrow v0.9 target slices. It lowers supported Core
//! effect nodes into in-memory Echo or git-warp review artifacts. It does not
//! execute a runtime, run a verifier, assemble bundles, or perform admission.

mod byte_concat;
mod byte_length;
mod byte_slice;
mod byte_utf8;
pub(crate) mod execution_order;
mod totality;
mod unsigned_subtraction;

use std::collections::{BTreeMap, BTreeSet};

use crate::core_ir::{
    core_type_fits, is_lowercase_sha256_review_digest, named_core_type_closure, resolved_core_type,
    validate_core_module_type_integrity, CoreBudget, CoreExpr, CoreExternalActionBudget,
    CoreImportKind, CoreIntent, CoreModule, CoreNode, CoreObstructionArm, CoreObstructionReason,
    CorePredicate, CoreRequireFailureArm, CoreType, CoreValue, InputConstraint, LocalRef,
    ResourceRef, CORE_API_VERSION, CORE_APPLICATION_INPUT_LOCAL_ID, MAX_CORE_TYPE_DEPTH,
};
use crate::digest_core_module;
use crate::lowerability::{LowerabilityEffectStatus, LowerabilityReport, LowerabilityStatus};
use crate::result_projection::emit_result_projection_with_closure;
use crate::{ResultProjectionArtifact, ResultProjectionFailure};

pub const ECHO_DPO_TARGET_PROFILE: &str = "echo.dpo@1";
pub const ECHO_SPAN_IR_DOMAIN: &str = "echo.span-ir/v1";
pub const ECHO_ORDERED_SPAN_IR_DOMAIN: &str = "echo.span-ir/v2";
pub const GITWARP_REF_CRDT_TARGET_PROFILE: &str = "gitwarp.ref_crdt@1";
pub const GITWARP_COMMIT_REDUCER_IR_DOMAIN: &str = "gitwarp.commit-reducer-ir/v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ExecutionModel {
    PreStepGuards,
    Ordered,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct TargetSelection {
    target_ir_domain: &'static str,
    target_intrinsic_prefix: &'static str,
    supports_requirements: bool,
    execution_model: ExecutionModel,
}

impl TargetSelection {
    fn supports_intrinsic(self, target_intrinsic: &str) -> bool {
        target_intrinsic
            .strip_prefix(self.target_intrinsic_prefix)
            .is_some_and(|suffix| suffix.starts_with('.'))
    }
}

fn target_selection_for_profile(target_profile: &str) -> Option<TargetSelection> {
    match target_profile {
        ECHO_DPO_TARGET_PROFILE => Some(TargetSelection {
            target_ir_domain: ECHO_SPAN_IR_DOMAIN,
            target_intrinsic_prefix: ECHO_DPO_TARGET_PROFILE,
            supports_requirements: true,
            execution_model: ExecutionModel::PreStepGuards,
        }),
        GITWARP_REF_CRDT_TARGET_PROFILE => Some(TargetSelection {
            target_ir_domain: GITWARP_COMMIT_REDUCER_IR_DOMAIN,
            target_intrinsic_prefix: GITWARP_REF_CRDT_TARGET_PROFILE,
            supports_requirements: false,
            execution_model: ExecutionModel::PreStepGuards,
        }),
        _ => None,
    }
}

/// Canonical non-generic helper signature owned by one exact imported lawpack.
///
/// Validated lawpack preparation projects this fact alongside the compiler
/// fact. The lawpack digest binds the exported helper implementation and its
/// resolved named-type closure without embedding application vocabulary in a
/// target runtime.
///
/// Callers cannot fabricate helper authority from an arbitrary coordinate and
/// signature:
///
/// ```compile_fail
/// use edict_syntax::{ResourceRef, TargetPureFunctionFact};
///
/// let _forged = TargetPureFunctionFact {
///     lawpack: ResourceRef {
///         coordinate: "hello.echo@1".to_owned(),
///         digest: Some(format!("sha256:{}", "1".repeat(64))),
///     },
///     coordinate: "hello.echo@1.notExported".to_owned(),
///     type_parameters: Vec::new(),
///     parameter_types: vec!["U64".to_owned()],
///     return_type: "U64".to_owned(),
/// };
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TargetPureFunctionFact {
    lawpack: ResourceRef,
    coordinate: String,
    type_parameters: Vec<String>,
    parameter_types: Vec<String>,
    return_type: String,
    type_closure: BTreeMap<String, CoreType>,
}

impl TargetPureFunctionFact {
    pub(crate) fn from_validated_lawpack_export(
        lawpack: ResourceRef,
        coordinate: String,
        type_parameters: Vec<String>,
        parameter_types: Vec<String>,
        return_type: String,
        type_closure: BTreeMap<String, CoreType>,
    ) -> Self {
        Self {
            lawpack,
            coordinate,
            type_parameters,
            parameter_types,
            return_type,
            type_closure,
        }
    }

    /// Exact digest-locked lawpack that exported this helper.
    #[must_use]
    pub fn lawpack(&self) -> &ResourceRef {
        &self.lawpack
    }

    /// Canonical exported helper coordinate.
    #[must_use]
    pub fn coordinate(&self) -> &str {
        &self.coordinate
    }

    /// Declared generic parameters; v1 lowering requires this to be empty.
    #[must_use]
    pub fn type_parameters(&self) -> &[String] {
        &self.type_parameters
    }

    /// Complete ordered parameter type coordinates.
    #[must_use]
    pub fn parameter_types(&self) -> &[String] {
        &self.parameter_types
    }

    /// Declared return type coordinate.
    #[must_use]
    pub fn return_type(&self) -> &str {
        &self.return_type
    }
}

/// Canonical non-generic effect signature owned by one exact imported lawpack.
///
/// The private fields and constructor prevent caller-authored Target facts from
/// manufacturing lawpack effect authority. Validated lawpack preparation binds
/// the source alias, canonical export, signature, and exact resolved type
/// closure together.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TargetEffectSignatureFact {
    lawpack: ResourceRef,
    source_coordinate: String,
    coordinate: String,
    type_parameters: Vec<String>,
    input_type: String,
    output_type: String,
    failure_payload_types: BTreeMap<String, String>,
    type_closure: BTreeMap<String, CoreType>,
}

pub(crate) struct TargetEffectSignatureShape {
    pub(crate) coordinate: String,
    pub(crate) type_parameters: Vec<String>,
    pub(crate) input_type: String,
    pub(crate) output_type: String,
    pub(crate) failure_payload_types: BTreeMap<String, String>,
}

impl TargetEffectSignatureFact {
    pub(crate) fn from_validated_lawpack_export(
        lawpack: ResourceRef,
        source_coordinate: String,
        shape: TargetEffectSignatureShape,
        type_closure: BTreeMap<String, CoreType>,
    ) -> Self {
        Self {
            lawpack,
            source_coordinate,
            coordinate: shape.coordinate,
            type_parameters: shape.type_parameters,
            input_type: shape.input_type,
            output_type: shape.output_type,
            failure_payload_types: shape.failure_payload_types,
            type_closure,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TargetIrLoweringFacts {
    pub target_profile: ResourceRef,
    pub target_ir_domain: String,
    pub operation_profiles: Vec<String>,
    pub obstruction_coordinates: Vec<String>,
    pub effect_lowerings: Vec<TargetEffectLowering>,
    pub effect_signatures: Vec<TargetEffectSignatureFact>,
    pub pure_functions: Vec<TargetPureFunctionFact>,
}

impl TargetIrLoweringFacts {
    /// Build Target IR lowering facts from an accepted native lowerability report.
    ///
    /// # Errors
    ///
    /// Returns `UnsupportedLowerabilityReport` when the lowerability report did
    /// not select native support. The v0.9 Target IR bridge does not
    /// derive target facts from unsupported or adapter-backed reports. It also
    /// cannot derive lawpack helper authority; helper-bearing Core must use
    /// validated lawpack preparation.
    pub fn from_lowerability_report(
        target_profile: ResourceRef,
        target_ir_domain: impl Into<String>,
        report: &LowerabilityReport,
    ) -> Result<Self, TargetLoweringFailure> {
        if report.status != LowerabilityStatus::Native {
            return Err(TargetLoweringFailure {
                kind: TargetLoweringFailureKind::UnsupportedLowerabilityReport,
                intent: None,
                node_index: None,
                detail: format!("{:?}", report.status),
            });
        }
        if target_profile.coordinate != report.target_profile {
            return Err(TargetLoweringFailure {
                kind: TargetLoweringFailureKind::UnsupportedTargetProfile,
                intent: None,
                node_index: None,
                detail: target_profile.coordinate,
            });
        }
        if !target_profile.is_digest_locked() {
            return Err(TargetLoweringFailure {
                kind: TargetLoweringFailureKind::UndigestedTargetProfile,
                intent: None,
                node_index: None,
                detail: target_profile
                    .digest
                    .unwrap_or_else(|| "<missing>".to_owned()),
            });
        }

        Ok(Self {
            target_profile,
            target_ir_domain: target_ir_domain.into(),
            operation_profiles: vec![report.operation_profile.clone()],
            obstruction_coordinates: report.obstruction_coordinates.clone(),
            effect_lowerings: selected_native_effect_lowerings(report),
            effect_signatures: Vec::new(),
            pure_functions: Vec::new(),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TargetEffectLowering {
    pub effect: String,
    pub target_intrinsic: String,
    pub failure_mappings: BTreeMap<String, String>,
}

fn selected_native_effect_lowerings(report: &LowerabilityReport) -> Vec<TargetEffectLowering> {
    let mut seen = BTreeSet::new();
    let mut lowerings = Vec::new();
    for effect in &report.effect_results {
        let LowerabilityEffectStatus::Native { target_intrinsic } = &effect.status else {
            continue;
        };
        if seen.insert((effect.semantic_effect.as_str(), target_intrinsic.as_str())) {
            lowerings.push(TargetEffectLowering {
                effect: effect.semantic_effect.clone(),
                target_intrinsic: target_intrinsic.clone(),
                failure_mappings: BTreeMap::new(),
            });
        }
    }
    lowerings
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TargetLoweringStatus {
    Lowered,
    Unsupported,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TargetLoweringFailureKind {
    UnsupportedTargetProfile,
    UnsupportedTargetIrDomain,
    UndigestedTargetProfile,
    UnsupportedTargetFeature,
    UnsupportedCoreNode,
    MissingOperationProfile,
    MissingObstruction,
    AmbiguousObstructionMapping,
    MissingEffectLowering,
    AmbiguousEffectLowering,
    UnsupportedLowerabilityReport,
    UnsupportedTargetIntrinsic,
    UnsupportedCoreAbi,
    UnsupportedCoreCapability,
    UndigestedCoreImport,
    InvalidCoreIdentity,
    NoTargetSteps,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TargetLoweringFailure {
    pub kind: TargetLoweringFailureKind,
    pub intent: Option<String>,
    pub node_index: Option<usize>,
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TargetLoweringReport {
    /// Overall Target IR lowering status.
    pub status: TargetLoweringStatus,
    /// Target IR artifact when general target lowering succeeds.
    pub artifact: Option<TargetIrArtifact>,
    /// Compiler-owned result projection per supported intent. This map may be
    /// empty even when `status` is `Lowered`; callers own admission cardinality.
    pub result_projections: BTreeMap<String, ResultProjectionArtifact>,
    /// Per-intent result-projection refusals. This map may be non-empty while
    /// `status` is `Lowered` and general `failures` remains empty.
    pub result_projection_failures: BTreeMap<String, ResultProjectionFailure>,
    /// General target-lowering failures.
    pub failures: Vec<TargetLoweringFailure>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TargetIrArtifact {
    pub domain: String,
    pub target_profile: ResourceRef,
    pub source_core_coordinate: String,
    /// Exact semantic inputs for operation artifacts. Legacy artifacts without
    /// an explicit basis or lawpack remain byte-identical by omitting it.
    pub semantic_closure: Option<TargetIrSemanticClosure>,
    pub intents: BTreeMap<String, TargetIrIntent>,
}

/// Digest-locked Edict inputs whose meaning the Target IR artifact closes over.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TargetIrSemanticClosure {
    pub source_core: ResourceRef,
    pub lawpacks: Vec<ResourceRef>,
    pub capabilities: Vec<ResourceRef>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TargetIrIntent {
    pub operation_profile: String,
    /// Exact instruction-id permutation for explicitly selected ordered Target IR.
    /// Legacy artifacts omit this field.
    pub execution_order: Option<Vec<String>>,
    pub basis: Option<CoreExpr>,
    pub input_constraints: Vec<InputConstraint>,
    pub core_evaluation_budget: CoreBudget,
    pub pure_bindings: Vec<TargetIrPureBinding>,
    pub requirements: Vec<TargetIrRequirement>,
    pub steps: Vec<TargetIrStep>,
    pub external_action_requests: Vec<TargetIrExternalActionRequest>,
    pub result: CoreExpr,
}

/// One source-ordered pure Core binding retained for target evaluation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TargetIrPureBinding {
    pub id: String,
    pub binding: LocalRef,
    pub value: CoreExpr,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TargetIrRequirement {
    pub id: String,
    pub predicate: CorePredicate,
    pub on_failure: TargetIrRequireFailure,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TargetIrRequireFailure {
    Terminal { reason: CoreObstructionReason },
    ContinueObstructed { reason: CoreObstructionReason },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TargetIrStep {
    pub id: String,
    pub binding: LocalRef,
    pub effect: String,
    pub target_intrinsic: String,
    pub input: CoreExpr,
    pub obstruction_failures: Vec<String>,
    pub obstruction_arms: BTreeMap<String, CoreObstructionArm>,
}

/// A deterministic external-action request preserved as data for Echo admission.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TargetIrExternalActionRequest {
    pub id: String,
    pub binding: LocalRef,
    pub operation: ResourceRef,
    pub input_type: String,
    pub settlement_type: String,
    pub input_schema: ResourceRef,
    pub settlement_schema: ResourceRef,
    pub input: CoreExpr,
    pub authority_scope: CoreExpr,
    pub basis: CoreExpr,
    pub budget: CoreExternalActionBudget,
    pub reconciliation_law: ResourceRef,
}

#[must_use]
pub fn lower_to_target_ir(
    core: &CoreModule,
    facts: &TargetIrLoweringFacts,
) -> TargetLoweringReport {
    let target_selection = match validate_target_selection(facts) {
        Ok(target_selection) => target_selection,
        Err(failures) => return unsupported(failures),
    };
    let validated_core = match validate_core_module_type_integrity(core) {
        Ok(validated) => validated,
        Err(failure) => {
            return unsupported(vec![TargetLoweringFailure {
                kind: TargetLoweringFailureKind::InvalidCoreIdentity,
                intent: None,
                node_index: None,
                detail: failure.path().to_owned(),
            }]);
        }
    };
    let core = validated_core.module();
    let core_failures = validate_core_module(core, &facts.pure_functions, &facts.effect_signatures);
    if !core_failures.is_empty() {
        return unsupported(core_failures);
    }
    let semantic_closure = match semantic_closure_for_core(core) {
        Ok(semantic_closure) => semantic_closure,
        Err(failure) => return unsupported(vec![failure]),
    };

    let effect_lowerings = effect_lowerings_by_coordinate(facts);
    let operation_profiles = facts
        .operation_profiles
        .iter()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    let obstruction_coordinates = facts
        .obstruction_coordinates
        .iter()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    let context = TargetLoweringContext {
        target_selection,
        obstruction_coordinates: &obstruction_coordinates,
        effect_lowerings: &effect_lowerings,
    };
    let mut failures = Vec::new();
    let mut intents = BTreeMap::new();

    for (intent_name, intent) in &core.intents {
        let lowered = lower_intent(
            intent_name,
            intent,
            &operation_profiles,
            &context,
            &mut failures,
        );
        intents.insert(intent_name.clone(), lowered);
    }

    if failures.is_empty() {
        let artifact = TargetIrArtifact {
            domain: facts.target_ir_domain.clone(),
            target_profile: facts.target_profile.clone(),
            source_core_coordinate: core.coordinate.clone(),
            semantic_closure,
            intents,
        };
        let (result_projections, result_projection_failures) =
            if let Some(expected_semantic_closure) = artifact.semantic_closure.as_ref() {
                lower_result_projections(core, &artifact, expected_semantic_closure)
            } else {
                (BTreeMap::new(), BTreeMap::new())
            };
        TargetLoweringReport {
            status: TargetLoweringStatus::Lowered,
            artifact: Some(artifact),
            result_projections,
            result_projection_failures,
            failures,
        }
    } else {
        unsupported(failures)
    }
}

fn lower_result_projections(
    core: &CoreModule,
    target_ir: &TargetIrArtifact,
    expected_semantic_closure: &TargetIrSemanticClosure,
) -> (
    BTreeMap<String, ResultProjectionArtifact>,
    BTreeMap<String, ResultProjectionFailure>,
) {
    let mut projections = BTreeMap::new();
    let mut failures = BTreeMap::new();
    for intent_name in target_ir.intents.keys() {
        match emit_result_projection_with_closure(
            core,
            target_ir,
            intent_name,
            expected_semantic_closure,
        ) {
            Ok(projection) => {
                projections.insert(intent_name.clone(), projection);
            }
            Err(failure) => {
                failures.insert(intent_name.clone(), failure);
            }
        }
    }
    (projections, failures)
}

pub(crate) fn semantic_closure_for_core(
    core: &CoreModule,
) -> Result<Option<TargetIrSemanticClosure>, TargetLoweringFailure> {
    let lawpacks = digest_locked_import_set(core, CoreImportKind::Lawpack, "lawpack")?;
    let capabilities = digest_locked_import_set(core, CoreImportKind::Capability, "capability")?;
    let has_explicit_basis = core.intents.values().any(|intent| intent.basis.is_some());
    let has_pure_bindings = core.intents.values().any(|intent| {
        intent
            .body
            .nodes
            .iter()
            .any(|node| matches!(node, CoreNode::Let { .. }))
    });
    if core.functions.is_empty()
        && !has_explicit_basis
        && !has_pure_bindings
        && lawpacks.is_empty()
        && capabilities.is_empty()
    {
        return Ok(None);
    }

    let digest = digest_core_module(core).map_err(|error| TargetLoweringFailure {
        kind: TargetLoweringFailureKind::InvalidCoreIdentity,
        intent: None,
        node_index: None,
        detail: error.to_string(),
    })?;
    Ok(Some(TargetIrSemanticClosure {
        source_core: ResourceRef {
            coordinate: core.coordinate.clone(),
            digest: Some(digest.to_review_string()),
        },
        lawpacks: lawpacks.into_values().collect(),
        capabilities: capabilities.into_values().collect(),
    }))
}

fn digest_locked_import_set(
    core: &CoreModule,
    kind: CoreImportKind,
    field_name: &str,
) -> Result<BTreeMap<String, ResourceRef>, TargetLoweringFailure> {
    let mut resources = BTreeMap::new();
    for resource in core
        .imports
        .iter()
        .filter(|import| import.kind == kind)
        .map(|import| &import.resource)
    {
        if !resource
            .digest
            .as_deref()
            .is_some_and(is_lowercase_sha256_review_digest)
        {
            return Err(TargetLoweringFailure {
                kind: TargetLoweringFailureKind::UndigestedCoreImport,
                intent: None,
                node_index: None,
                detail: resource.coordinate.clone(),
            });
        }
        if let Some(prior) = resources.get(&resource.coordinate) {
            if prior != resource {
                return Err(TargetLoweringFailure {
                    kind: TargetLoweringFailureKind::InvalidCoreIdentity,
                    intent: None,
                    node_index: None,
                    detail: format!(
                        "{field_name} coordinate `{}` is bound to conflicting resources",
                        resource.coordinate
                    ),
                });
            }
        } else {
            resources.insert(resource.coordinate.clone(), resource.clone());
        }
    }
    Ok(resources)
}

fn validate_target_selection(
    facts: &TargetIrLoweringFacts,
) -> Result<TargetSelection, Vec<TargetLoweringFailure>> {
    let Some(mut target_selection) = target_selection_for_profile(&facts.target_profile.coordinate)
    else {
        return Err(vec![TargetLoweringFailure {
            kind: TargetLoweringFailureKind::UnsupportedTargetProfile,
            intent: None,
            node_index: None,
            detail: facts.target_profile.coordinate.clone(),
        }]);
    };
    if !facts.target_profile.is_digest_locked() {
        return Err(vec![TargetLoweringFailure {
            kind: TargetLoweringFailureKind::UndigestedTargetProfile,
            intent: None,
            node_index: None,
            detail: facts
                .target_profile
                .digest
                .clone()
                .unwrap_or_else(|| "<missing>".to_owned()),
        }]);
    }
    if facts.target_profile.coordinate == ECHO_DPO_TARGET_PROFILE
        && facts.target_ir_domain == ECHO_ORDERED_SPAN_IR_DOMAIN
    {
        target_selection.target_ir_domain = ECHO_ORDERED_SPAN_IR_DOMAIN;
        target_selection.execution_model = ExecutionModel::Ordered;
    }
    if facts.target_ir_domain != target_selection.target_ir_domain {
        return Err(vec![TargetLoweringFailure {
            kind: TargetLoweringFailureKind::UnsupportedTargetIrDomain,
            intent: None,
            node_index: None,
            detail: facts.target_ir_domain.clone(),
        }]);
    }
    Ok(target_selection)
}

fn validate_core_module(
    core: &CoreModule,
    pure_functions: &[TargetPureFunctionFact],
    effect_signatures: &[TargetEffectSignatureFact],
) -> Vec<TargetLoweringFailure> {
    if core.api_version != CORE_API_VERSION {
        return vec![TargetLoweringFailure {
            kind: TargetLoweringFailureKind::UnsupportedCoreAbi,
            intent: None,
            node_index: None,
            detail: core.api_version.clone(),
        }];
    }
    if core.coordinate.is_empty() {
        return vec![TargetLoweringFailure {
            kind: TargetLoweringFailureKind::InvalidCoreIdentity,
            intent: None,
            node_index: None,
            detail: "source Core coordinate is empty".to_owned(),
        }];
    }
    if core.intents.is_empty() {
        return vec![TargetLoweringFailure {
            kind: TargetLoweringFailureKind::NoTargetSteps,
            intent: None,
            node_index: None,
            detail: "core module has no target-owned intents".to_owned(),
        }];
    }
    let floating_imports = core
        .imports
        .iter()
        .filter(|import| !import.resource.is_digest_locked())
        .map(|import| TargetLoweringFailure {
            kind: TargetLoweringFailureKind::UndigestedCoreImport,
            intent: None,
            node_index: None,
            detail: import.resource.coordinate.clone(),
        })
        .collect::<Vec<_>>();
    if !floating_imports.is_empty() {
        return floating_imports;
    }
    let capability_failures = core
        .required_core_capabilities
        .iter()
        .map(|capability| TargetLoweringFailure {
            kind: TargetLoweringFailureKind::UnsupportedCoreCapability,
            intent: None,
            node_index: None,
            detail: capability.clone(),
        })
        .collect::<Vec<_>>();
    if !capability_failures.is_empty() {
        return capability_failures;
    }
    if let Some(failure) = validate_source_function_bodies(core, pure_functions, effect_signatures)
    {
        return vec![failure];
    }
    validate_pure_binding_graphs(core, pure_functions, effect_signatures)
}

fn validate_pure_binding_graphs(
    core: &CoreModule,
    pure_functions: &[TargetPureFunctionFact],
    effect_signatures: &[TargetEffectSignatureFact],
) -> Vec<TargetLoweringFailure> {
    core.intents
        .iter()
        .filter_map(|(intent_name, intent)| {
            if !block_local_identities_are_valid(&intent.body, 0) {
                return Some(invalid_pure_binding(
                    intent_name,
                    None,
                    "Core local table or producer contains an empty or duplicate identity",
                ));
            }
            if !totality::intent_is_total(intent) {
                return Some(invalid_pure_binding(
                    intent_name,
                    None,
                    "Core operation is not proven total in its evaluation scope",
                ));
            }
            if intent
                .body
                .nodes
                .iter()
                .any(|node| matches!(node, CoreNode::For { .. }))
            {
                return None;
            }
            validate_pure_binding_graph(
                core,
                pure_functions,
                effect_signatures,
                intent_name,
                intent,
            )
        })
        .collect()
}

fn validate_pure_binding_graph(
    core: &CoreModule,
    pure_functions: &[TargetPureFunctionFact],
    effect_signatures: &[TargetEffectSignatureFact],
    intent_name: &str,
    intent: &CoreIntent,
) -> Option<TargetLoweringFailure> {
    let locals = intent
        .body
        .locals
        .iter()
        .map(|local| (local.id.as_str(), local))
        .collect::<BTreeMap<_, _>>();
    if locals.len() != intent.body.locals.len() {
        return Some(invalid_pure_binding(
            intent_name,
            None,
            "Core local table contains a duplicate identity",
        ));
    }
    let Some(input) = locals.get(CORE_APPLICATION_INPUT_LOCAL_ID).copied() else {
        return Some(invalid_pure_binding(
            intent_name,
            None,
            "missing application input local",
        ));
    };
    if input.ty != intent.input {
        return Some(invalid_pure_binding(
            intent_name,
            None,
            "application input local type does not match the intent input",
        ));
    }

    let mut available = BTreeMap::from([(input.id.as_str(), input)]);
    if !intent.basis.as_ref().is_none_or(|basis| {
        expression_has_closed_authority(core, pure_functions, basis, &available)
    }) || !intent.input_constraints.iter().all(|constraint| {
        predicate_fits_core_types(core, pure_functions, &constraint.predicate, &available)
    }) {
        return Some(invalid_pure_binding(
            intent_name,
            None,
            "intent metadata contains an invalid or unauthorized expression",
        ));
    }
    for (node_index, node) in intent.body.nodes.iter().enumerate() {
        if !node_expressions_have_closed_authority(
            core,
            pure_functions,
            effect_signatures,
            node,
            &locals,
            &available,
        ) {
            return Some(invalid_pure_binding(
                intent_name,
                Some(node_index),
                "Core node contains an invalid or unauthorized expression",
            ));
        }
        let binding = match node {
            CoreNode::Let { binding, .. } => Some(binding),
            CoreNode::Effect { binding, .. } | CoreNode::ExternalActionRequest { binding, .. } => {
                Some(binding)
            }
            CoreNode::Branch { binding, .. } => binding.as_ref(),
            CoreNode::Require { .. } | CoreNode::For { .. } => None,
        };
        let Some(binding) = binding else {
            continue;
        };
        if locals.get(binding.id.as_str()).copied() != Some(binding)
            || available.insert(binding.id.as_str(), binding).is_some()
        {
            return Some(invalid_pure_binding(
                intent_name,
                Some(node_index),
                "binding identity is missing, duplicated, or conflicts with the Core local table",
            ));
        }
    }
    if !expression_references_are_available(&intent.body.result, &available) {
        return Some(invalid_pure_binding(
            intent_name,
            None,
            "result references an undeclared or conflicting local",
        ));
    }
    (!expression_fits_declared_type(
        core,
        pure_functions,
        &intent.body.result,
        &intent.output,
        &available,
    ))
    .then(|| {
        invalid_pure_binding(
            intent_name,
            None,
            &format!(
                "result expression does not match declared output type `{}`",
                intent.output
            ),
        )
    })
}

fn block_local_identities_are_valid(block: &crate::core_ir::CoreBlock, depth: usize) -> bool {
    if depth > MAX_CORE_TYPE_DEPTH {
        return false;
    }
    let mut local_ids = BTreeSet::new();
    if !block
        .locals
        .iter()
        .all(|local| !local.id.is_empty() && local_ids.insert(local.id.as_str()))
    {
        return false;
    }
    block.nodes.iter().all(|node| match node {
        CoreNode::Let { binding, .. } | CoreNode::ExternalActionRequest { binding, .. } => {
            !binding.id.is_empty()
        }
        CoreNode::Effect {
            binding,
            obstruction_map,
            ..
        } => {
            !binding.id.is_empty()
                && obstruction_map
                    .values()
                    .all(|arm| !arm.binder.id.is_empty())
        }
        CoreNode::For { binder, body, .. } => {
            !binder.id.is_empty() && block_local_identities_are_valid(body, depth + 1)
        }
        CoreNode::Branch {
            binding,
            then_block,
            else_block,
            ..
        } => {
            binding
                .as_ref()
                .is_none_or(|binding| !binding.id.is_empty())
                && block_local_identities_are_valid(then_block, depth + 1)
                && block_local_identities_are_valid(else_block, depth + 1)
        }
        CoreNode::Require { .. } => true,
    })
}

fn node_expressions_have_closed_authority(
    core: &CoreModule,
    pure_functions: &[TargetPureFunctionFact],
    effect_signatures: &[TargetEffectSignatureFact],
    node: &CoreNode,
    locals: &BTreeMap<&str, &LocalRef>,
    available: &BTreeMap<&str, &LocalRef>,
) -> bool {
    match node {
        CoreNode::Let { binding, value } => {
            expression_fits_declared_type(core, pure_functions, value, &binding.ty, available)
        }
        CoreNode::Require { predicate, arm } => {
            predicate_fits_core_types(core, pure_functions, predicate, available)
                && require_arm_expressions_have_closed_authority(
                    core,
                    pure_functions,
                    arm,
                    available,
                )
        }
        CoreNode::Effect {
            effect,
            binding,
            input,
            obstruction_map,
            ..
        } => {
            effect_signature_matches_core(
                core,
                pure_functions,
                effect_signatures,
                effect,
                binding,
                input,
                available,
            ) && obstruction_map.iter().all(|(failure, arm)| {
                locals.get(arm.binder.id.as_str()).copied() == Some(&arm.binder)
                    && obstruction_binder_matches_effect_signature(
                        core,
                        effect_signatures,
                        effect,
                        failure,
                        &arm.binder,
                    )
                    && obstruction_value_has_closed_authority(
                        core,
                        pure_functions,
                        &arm.value,
                        available,
                    )
            })
        }
        CoreNode::ExternalActionRequest {
            binding,
            input_type,
            input,
            settlement_type,
            authority_scope,
            basis,
            budget,
            ..
        } => {
            let settlement_matches_binding =
                resolved_core_type(core, &binding.ty).is_some_and(|binding_type| {
                    let CoreType::ExternalActionRequest { settlement } = binding_type else {
                        return false;
                    };
                    resolved_core_type(core, settlement_type).is_some()
                        && core_type_fits(core, settlement_type, &settlement)
                        && core_type_fits(core, &settlement, settlement_type)
                });
            settlement_matches_binding
                && expression_fits_declared_type(core, pure_functions, input, input_type, available)
                && expression_has_closed_authority(core, pure_functions, authority_scope, available)
                && expression_has_closed_authority(core, pure_functions, basis, available)
                && expression_fits_declared_type(
                    core,
                    pure_functions,
                    &budget.max_settlement_bytes,
                    "U64",
                    available,
                )
                && expression_fits_declared_type(
                    core,
                    pure_functions,
                    &budget.max_attempts,
                    "U32",
                    available,
                )
        }
        CoreNode::For { .. } | CoreNode::Branch { .. } => true,
    }
}

fn obstruction_binder_matches_effect_signature(
    core: &CoreModule,
    effect_signatures: &[TargetEffectSignatureFact],
    effect: &str,
    failure: &str,
    binder: &LocalRef,
) -> bool {
    if lawpack_import_for_source_coordinate(core, effect).is_none() {
        return binder.ty == "Unit";
    }
    validated_effect_signature_fact(core, effect_signatures, effect).is_some_and(|signature| {
        signature
            .failure_payload_types
            .get(failure)
            .is_some_and(|payload_type| payload_type == &binder.ty)
    })
}

fn effect_signature_matches_core(
    core: &CoreModule,
    pure_functions: &[TargetPureFunctionFact],
    effect_signatures: &[TargetEffectSignatureFact],
    effect: &str,
    binding: &LocalRef,
    input: &CoreExpr,
    available: &BTreeMap<&str, &LocalRef>,
) -> bool {
    if lawpack_import_for_source_coordinate(core, effect).is_none() {
        return expression_has_closed_authority(core, pure_functions, input, available);
    }
    let Some(signature) = validated_effect_signature_fact(core, effect_signatures, effect) else {
        return false;
    };
    expression_fits_declared_type(
        core,
        pure_functions,
        input,
        &signature.input_type,
        available,
    ) && core_type_fits(core, &signature.output_type, &binding.ty)
}

fn validated_effect_signature_fact<'a>(
    core: &CoreModule,
    effect_signatures: &'a [TargetEffectSignatureFact],
    effect: &str,
) -> Option<&'a TargetEffectSignatureFact> {
    let mut matches = effect_signatures
        .iter()
        .filter(|signature| signature.source_coordinate == effect);
    let signature = matches.next()?;
    if matches.next().is_some()
        || !signature.lawpack.is_digest_locked()
        || !coordinate_is_below_lawpack(&signature.coordinate, &signature.lawpack)
        || !signature.type_parameters.is_empty()
        || signature.input_type.is_empty()
        || signature.output_type.is_empty()
        || signature
            .failure_payload_types
            .values()
            .any(String::is_empty)
    {
        return None;
    }
    let import = lawpack_import_for_source_coordinate(core, effect)?;
    let suffix = signature
        .coordinate
        .strip_prefix(&signature.lawpack.coordinate)
        .and_then(|suffix| suffix.strip_prefix('.'))?;
    if import.resource != signature.lawpack
        || import
            .alias
            .as_deref()
            .is_none_or(|alias| format!("{alias}.{suffix}") != signature.source_coordinate)
        || !authenticated_named_type_closure_matches(
            core,
            &signature.lawpack,
            std::iter::once(signature.input_type.as_str())
                .chain(std::iter::once(signature.output_type.as_str()))
                .chain(signature.failure_payload_types.values().map(String::as_str)),
            &signature.type_closure,
        )
    {
        return None;
    }
    Some(signature)
}

fn lawpack_import_for_source_coordinate<'a>(
    core: &'a CoreModule,
    source_coordinate: &str,
) -> Option<&'a crate::core_ir::CoreImport> {
    let (alias, suffix) = source_coordinate.split_once('.')?;
    if alias.is_empty() || suffix.is_empty() {
        return None;
    }
    core.imports.iter().find(|import| {
        import.kind == CoreImportKind::Lawpack && import.alias.as_deref() == Some(alias)
    })
}

fn obstruction_value_has_closed_authority(
    core: &CoreModule,
    pure_functions: &[TargetPureFunctionFact],
    value: &CoreExpr,
    available: &BTreeMap<&str, &LocalRef>,
) -> bool {
    match value {
        CoreExpr::Call {
            callee,
            type_args,
            args,
        } if !callee.is_empty() && type_args.is_empty() && args.is_empty() => true,
        _ => expression_has_closed_authority(core, pure_functions, value, available),
    }
}

fn require_arm_expressions_have_closed_authority(
    core: &CoreModule,
    pure_functions: &[TargetPureFunctionFact],
    arm: &CoreRequireFailureArm,
    available: &BTreeMap<&str, &LocalRef>,
) -> bool {
    match arm {
        CoreRequireFailureArm::Terminal { reason }
        | CoreRequireFailureArm::ContinueObstructed { reason } => reason
            .payload
            .values()
            .all(|value| expression_has_closed_authority(core, pure_functions, value, available)),
    }
}

fn expression_has_closed_authority(
    core: &CoreModule,
    pure_functions: &[TargetPureFunctionFact],
    expression: &CoreExpr,
    available: &BTreeMap<&str, &LocalRef>,
) -> bool {
    if !expression_references_are_available(expression, available) {
        return false;
    }
    match expression {
        CoreExpr::Local { reference } => resolved_core_type(core, &reference.ty).is_some(),
        CoreExpr::Const(CoreValue::Int { width, value }) => {
            crate::core_ir::parse_core_integer(width, value).is_some()
        }
        CoreExpr::Const(
            CoreValue::Null | CoreValue::Bool(_) | CoreValue::String(_) | CoreValue::Bytes(_),
        ) => true,
        CoreExpr::Record { fields } => fields
            .values()
            .all(|value| expression_has_closed_authority(core, pure_functions, value, available)),
        CoreExpr::Field { .. } => {
            expression_type_coordinate(core, pure_functions, expression, available).is_some()
        }
        CoreExpr::Call {
            callee,
            type_args,
            args,
        } if callee == byte_concat::OPERATION => {
            byte_concat::type_coordinate(core, pure_functions, type_args, args, available).is_some()
        }
        CoreExpr::Call {
            callee,
            type_args,
            args,
        } if callee == byte_length::OPERATION => {
            byte_length::type_coordinate(core, pure_functions, type_args, args, available).is_some()
        }
        CoreExpr::Call {
            callee,
            type_args,
            args,
        } if callee == byte_utf8::OPERATION => {
            byte_utf8::type_coordinate(core, pure_functions, type_args, args, available).is_some()
        }
        CoreExpr::Call {
            callee,
            type_args,
            args,
        } if callee == byte_slice::OPERATION => {
            byte_slice::type_coordinate(core, pure_functions, type_args, args, available).is_some()
        }
        CoreExpr::Call {
            callee,
            type_args,
            args,
        } if callee == unsigned_subtraction::OPERATION => {
            unsigned_subtraction::type_coordinate(core, pure_functions, type_args, args, available)
                .is_some()
        }
        CoreExpr::Call { callee, .. } if callee == "core.string.concat" => {
            expression_string_shape(core, pure_functions, expression, available).is_some()
        }
        CoreExpr::Call {
            callee,
            type_args,
            args,
        } => {
            validated_function_return_type(core, pure_functions, callee, type_args, args, available)
                .is_some()
        }
        CoreExpr::If {
            predicate,
            then_value,
            else_value,
        } => {
            predicate_fits_core_types(core, pure_functions, predicate, available)
                && expression_has_closed_authority(core, pure_functions, then_value, available)
                && expression_has_closed_authority(core, pure_functions, else_value, available)
        }
    }
}

fn expression_fits_declared_type(
    core: &CoreModule,
    pure_functions: &[TargetPureFunctionFact],
    expression: &CoreExpr,
    expected: &str,
    available: &BTreeMap<&str, &LocalRef>,
) -> bool {
    match expression {
        CoreExpr::Local { reference } => {
            available.get(reference.id.as_str()).copied() == Some(reference)
                && core_type_fits(core, &reference.ty, expected)
        }
        CoreExpr::Const(value) => core_value_fits_declared_type(core, value, expected),
        CoreExpr::Record { fields } => {
            let Some(CoreType::Record {
                fields: expected_fields,
            }) = resolved_core_type(core, expected)
            else {
                return false;
            };
            fields.keys().eq(expected_fields.keys())
                && fields.iter().all(|(field, value)| {
                    expression_fits_declared_type(
                        core,
                        pure_functions,
                        value,
                        expected_fields.get(field).expect("equal record field keys"),
                        available,
                    )
                })
        }
        CoreExpr::Field { base, field } => {
            let Some(field_shape) =
                field_expression_shape(core, pure_functions, base, field, available)
            else {
                return false;
            };
            comparison_shape_fits(
                core,
                &field_shape,
                &ComparisonOperandShape::Coordinate(expected.to_owned()),
                0,
            )
        }
        CoreExpr::Call { callee, .. }
            if callee == byte_concat::OPERATION
                || callee == byte_length::OPERATION
                || callee == byte_utf8::OPERATION
                || callee == byte_slice::OPERATION
                || callee == unsigned_subtraction::OPERATION =>
        {
            expression_type_coordinate(core, pure_functions, expression, available)
                .is_some_and(|actual| core_type_fits(core, &actual, expected))
        }
        CoreExpr::Call {
            callee,
            type_args,
            args,
        } if callee == "core.string.concat" => {
            let Some(CoreType::String {
                max: expected_max,
                canonical: expected_canonical,
            }) = resolved_core_type(core, expected)
            else {
                return false;
            };
            if expected_canonical != "raw-utf8" || !type_args.is_empty() {
                return false;
            }
            args.iter()
                .try_fold(0_u64, |actual_max, argument| {
                    let (max, _) =
                        expression_string_shape(core, pure_functions, argument, available)?;
                    actual_max.checked_add(max)
                })
                .is_some_and(|actual_max| actual_max <= expected_max)
        }
        CoreExpr::Call {
            callee,
            type_args,
            args,
        } => {
            validated_function_return_type(core, pure_functions, callee, type_args, args, available)
                .is_some_and(|return_type| core_type_fits(core, return_type, expected))
        }
        CoreExpr::If {
            predicate,
            then_value,
            else_value,
        } => {
            predicate_fits_core_types(core, pure_functions, predicate, available)
                && expression_fits_declared_type(
                    core,
                    pure_functions,
                    then_value,
                    expected,
                    available,
                )
                && expression_fits_declared_type(
                    core,
                    pure_functions,
                    else_value,
                    expected,
                    available,
                )
        }
    }
}

fn validated_function_return_type<'a>(
    core: &'a CoreModule,
    imported: &'a [TargetPureFunctionFact],
    callee: &str,
    type_args: &[String],
    args: &[CoreExpr],
    available: &BTreeMap<&str, &LocalRef>,
) -> Option<&'a str> {
    if let Some(name) = callee.strip_prefix(&format!("{}.", core.coordinate)) {
        if let Some(function) = core.functions.get(name) {
            if !type_args.is_empty()
                || args.len() != function.params.len()
                || imported.iter().any(|fact| fact.coordinate == callee)
            {
                return None;
            }
            return args
                .iter()
                .zip(&function.params)
                .all(|(argument, parameter)| {
                    expression_fits_declared_type(
                        core,
                        imported,
                        argument,
                        &parameter.ty,
                        available,
                    )
                })
                .then_some(function.return_type.as_str());
        }
    }
    validated_pure_function_fact(core, imported, callee, type_args, args, available)
        .map(|fact| fact.return_type.as_str())
}

fn validate_source_function_bodies(
    core: &CoreModule,
    imported: &[TargetPureFunctionFact],
    effects: &[TargetEffectSignatureFact],
) -> Option<TargetLoweringFailure> {
    for (name, function) in &core.functions {
        let coordinate = format!("{}.{name}", core.coordinate);
        let invalid = |detail: &str| TargetLoweringFailure {
            kind: TargetLoweringFailureKind::InvalidCoreIdentity,
            intent: None,
            node_index: None,
            detail: format!("source function {coordinate}: {detail}"),
        };
        if imported.iter().any(|fact| fact.coordinate == coordinate)
            || effects.iter().any(|fact| fact.coordinate == coordinate)
        {
            return Some(invalid("coordinate collides with imported authority"));
        }
        let mut available = function
            .params
            .iter()
            .map(|local| (local.id.as_str(), local))
            .collect::<BTreeMap<_, _>>();
        for binding in &function.body.bindings {
            if !expression_fits_declared_type(
                core,
                imported,
                &binding.value,
                &binding.binding.ty,
                &available,
            ) || !totality::expression_is_total(&binding.value, &[])
            {
                return Some(invalid("binding has invalid type, authority, or totality"));
            }
            available.insert(binding.binding.id.as_str(), &binding.binding);
        }
        if !expression_fits_declared_type(
            core,
            imported,
            &function.body.result,
            &function.return_type,
            &available,
        ) || !totality::expression_is_total(&function.body.result, &[])
        {
            return Some(invalid("result has invalid type, authority, or totality"));
        }
    }
    None
}

fn validated_pure_function_fact<'a>(
    core: &CoreModule,
    pure_functions: &'a [TargetPureFunctionFact],
    callee: &str,
    type_args: &[String],
    args: &[CoreExpr],
    available: &BTreeMap<&str, &LocalRef>,
) -> Option<&'a TargetPureFunctionFact> {
    let mut matches = pure_functions
        .iter()
        .filter(|fact| fact.coordinate == callee);
    let fact = matches.next()?;
    if matches.next().is_some()
        || !fact.lawpack.is_digest_locked()
        || !coordinate_is_below_lawpack(&fact.coordinate, &fact.lawpack)
        || !fact.type_parameters.is_empty()
        || !type_args.is_empty()
        || args.len() != fact.parameter_types.len()
        || !core
            .imports
            .iter()
            .any(|import| import.kind == CoreImportKind::Lawpack && import.resource == fact.lawpack)
        || !authenticated_named_type_closure_matches(
            core,
            &fact.lawpack,
            fact.parameter_types
                .iter()
                .map(String::as_str)
                .chain(std::iter::once(fact.return_type.as_str())),
            &fact.type_closure,
        )
    {
        return None;
    }
    args.iter()
        .zip(&fact.parameter_types)
        .all(|(argument, parameter_type)| {
            expression_fits_declared_type(core, pure_functions, argument, parameter_type, available)
        })
        .then_some(fact)
}

fn coordinate_is_below_lawpack(coordinate: &str, lawpack: &ResourceRef) -> bool {
    coordinate
        .strip_prefix(&lawpack.coordinate)
        .and_then(|suffix| suffix.strip_prefix('.'))
        .is_some_and(|suffix| !suffix.is_empty())
}

fn authenticated_named_type_closure_matches<'a>(
    core: &CoreModule,
    lawpack: &ResourceRef,
    roots: impl IntoIterator<Item = &'a str>,
    expected: &BTreeMap<String, CoreType>,
) -> bool {
    named_core_type_closure(core, roots, |coordinate| {
        coordinate_is_below_lawpack(coordinate, lawpack)
    })
    .is_some_and(|actual| actual.eq(expected))
}

fn expression_string_shape(
    core: &CoreModule,
    pure_functions: &[TargetPureFunctionFact],
    expression: &CoreExpr,
    available: &BTreeMap<&str, &LocalRef>,
) -> Option<(u64, String)> {
    match expression {
        CoreExpr::Const(CoreValue::String(value)) => Some((
            u64::try_from(value.chars().count()).ok()?,
            "raw-utf8".to_owned(),
        )),
        CoreExpr::Local { .. } | CoreExpr::Field { .. } => {
            let coordinate =
                expression_type_coordinate(core, pure_functions, expression, available)?;
            let CoreType::String { max, canonical } = resolved_core_type(core, &coordinate)? else {
                return None;
            };
            Some((max, canonical))
        }
        CoreExpr::Call {
            callee,
            type_args,
            args,
        } if callee == "core.string.concat" => {
            if !type_args.is_empty() {
                return None;
            }
            let mut max = 0_u64;
            for argument in args {
                let (argument_max, _) =
                    expression_string_shape(core, pure_functions, argument, available)?;
                max = max.checked_add(argument_max)?;
            }
            Some((max, "raw-utf8".to_owned()))
        }
        CoreExpr::Call {
            callee,
            type_args,
            args,
        } => {
            let fact = validated_function_return_type(
                core,
                pure_functions,
                callee,
                type_args,
                args,
                available,
            )?;
            let CoreType::String { max, canonical } = resolved_core_type(core, fact)? else {
                return None;
            };
            Some((max, canonical))
        }
        CoreExpr::If {
            predicate,
            then_value,
            else_value,
        } => {
            if !predicate_fits_core_types(core, pure_functions, predicate, available) {
                return None;
            }
            let (then_max, then_canonical) =
                expression_string_shape(core, pure_functions, then_value, available)?;
            let (else_max, else_canonical) =
                expression_string_shape(core, pure_functions, else_value, available)?;
            (then_canonical == else_canonical).then_some((then_max.max(else_max), then_canonical))
        }
        CoreExpr::Const(
            CoreValue::Null | CoreValue::Bool(_) | CoreValue::Int { .. } | CoreValue::Bytes(_),
        )
        | CoreExpr::Record { .. } => None,
    }
}

// A returned coordinate also establishes operand validity. Callers must not
// recursively validate the same expression again just to recover its type.
fn expression_type_coordinate(
    core: &CoreModule,
    pure_functions: &[TargetPureFunctionFact],
    expression: &CoreExpr,
    available: &BTreeMap<&str, &LocalRef>,
) -> Option<String> {
    match expression {
        CoreExpr::Local { reference }
            if available.get(reference.id.as_str()).copied() == Some(reference)
                && resolved_core_type(core, &reference.ty).is_some() =>
        {
            Some(reference.ty.clone())
        }
        CoreExpr::Const(CoreValue::Bool(_)) => Some("Bool".to_owned()),
        CoreExpr::Const(CoreValue::Int { width, value }) => {
            crate::core_ir::parse_core_integer(width, value).map(|_| width.clone())
        }
        CoreExpr::Const(CoreValue::String(value)) => Some(format!(
            "String<max={},canonical=raw-utf8>",
            u64::try_from(value.chars().count()).ok()?
        )),
        CoreExpr::Const(CoreValue::Bytes(value)) => {
            Some(format!("Bytes<exact={}>", u64::try_from(value.len()).ok()?))
        }
        CoreExpr::Field { base, field } => {
            let ComparisonOperandShape::Coordinate(coordinate) =
                field_expression_shape(core, pure_functions, base, field, available)?
            else {
                return None;
            };
            Some(coordinate)
        }
        CoreExpr::Call {
            callee,
            type_args,
            args,
        } if callee == byte_concat::OPERATION => {
            byte_concat::type_coordinate(core, pure_functions, type_args, args, available)
        }
        CoreExpr::Call {
            callee,
            type_args,
            args,
        } if callee == byte_length::OPERATION => {
            byte_length::type_coordinate(core, pure_functions, type_args, args, available)
        }
        CoreExpr::Call {
            callee,
            type_args,
            args,
        } if callee == byte_utf8::OPERATION => {
            byte_utf8::type_coordinate(core, pure_functions, type_args, args, available)
        }
        CoreExpr::Call {
            callee,
            type_args,
            args,
        } if callee == byte_slice::OPERATION => {
            byte_slice::type_coordinate(core, pure_functions, type_args, args, available)
        }
        CoreExpr::Call {
            callee,
            type_args,
            args,
        } if callee == unsigned_subtraction::OPERATION => {
            unsigned_subtraction::type_coordinate(core, pure_functions, type_args, args, available)
        }
        CoreExpr::Call { callee, .. } if callee == "core.string.concat" => {
            expression_string_shape(core, pure_functions, expression, available)
                .map(|(max, canonical)| format!("String<max={max},canonical={canonical}>"))
        }
        CoreExpr::Call {
            callee,
            type_args,
            args,
        } if callee != "core.string.concat" => {
            validated_function_return_type(core, pure_functions, callee, type_args, args, available)
                .map(str::to_owned)
        }
        CoreExpr::If {
            predicate,
            then_value,
            else_value,
        } => {
            if !predicate_fits_core_types(core, pure_functions, predicate, available) {
                return None;
            }
            let then_shape = comparison_operand_shape(core, pure_functions, then_value, available)?;
            let else_shape = comparison_operand_shape(core, pure_functions, else_value, available)?;
            join_conditional_shapes(core, &then_shape, &else_shape, 0)?.type_coordinate()
        }
        CoreExpr::Local { .. }
        | CoreExpr::Const(CoreValue::Null)
        | CoreExpr::Record { .. }
        | CoreExpr::Call { .. } => None,
    }
}

fn join_conditional_coordinates(
    core: &CoreModule,
    left_coordinate: &str,
    right_coordinate: &str,
) -> Option<String> {
    if core_type_fits(core, left_coordinate, right_coordinate) {
        return Some(right_coordinate.to_owned());
    }
    if core_type_fits(core, right_coordinate, left_coordinate) {
        return Some(left_coordinate.to_owned());
    }

    let (
        Some(CoreType::Bytes {
            min: left_min,
            max: left_max,
        }),
        Some(CoreType::Bytes {
            min: right_min,
            max: right_max,
        }),
    ) = (
        resolved_core_type(core, left_coordinate),
        resolved_core_type(core, right_coordinate),
    )
    else {
        return None;
    };
    let min = match (left_min, right_min) {
        (Some(left), Some(right)) => Some(left.min(right)),
        (None, _) | (_, None) => None,
    };
    let max = left_max.max(right_max);
    Some(match min {
        Some(min) if min == max => format!("Bytes<exact={max}>"),
        Some(min) => format!("Bytes<min={min},max={max}>"),
        None => format!("Bytes<max={max}>"),
    })
}

fn core_value_fits_declared_type(core: &CoreModule, value: &CoreValue, expected: &str) -> bool {
    let Some(expected) = resolved_core_type(core, expected) else {
        return false;
    };
    match (value, expected) {
        (CoreValue::Null, CoreType::Unit | CoreType::Option { .. })
        | (CoreValue::Bool(_), CoreType::Bool) => true,
        (
            CoreValue::Int {
                width: actual_width,
                value,
            },
            CoreType::Int { width },
        ) => actual_width == &width && crate::core_ir::parse_core_integer(&width, value).is_some(),
        (CoreValue::String(value), CoreType::String { max, canonical }) => {
            canonical == "raw-utf8"
                && u64::try_from(value.chars().count()).is_ok_and(|length| length <= max)
        }
        (CoreValue::Bytes(value), CoreType::Bytes { min, max }) => u64::try_from(value.len())
            .is_ok_and(|length| length >= min.unwrap_or(0) && length <= max),
        (
            _,
            CoreType::Nominal { .. }
            | CoreType::Record { .. }
            | CoreType::Variant { .. }
            | CoreType::List { .. }
            | CoreType::Map { .. }
            | CoreType::CapabilityRef { .. }
            | CoreType::ExternalActionRequest { .. }
            | CoreType::Option { .. }
            | CoreType::Bool
            | CoreType::Unit
            | CoreType::Int { .. }
            | CoreType::String { .. }
            | CoreType::Bytes { .. },
        ) => false,
    }
}

fn invalid_pure_binding(
    intent_name: &str,
    node_index: Option<usize>,
    detail: &str,
) -> TargetLoweringFailure {
    TargetLoweringFailure {
        kind: TargetLoweringFailureKind::InvalidCoreIdentity,
        intent: Some(intent_name.to_owned()),
        node_index,
        detail: detail.to_owned(),
    }
}

fn expression_references_are_available(
    expression: &CoreExpr,
    available: &BTreeMap<&str, &LocalRef>,
) -> bool {
    match expression {
        CoreExpr::Local { reference } => {
            available.get(reference.id.as_str()).copied() == Some(reference)
        }
        CoreExpr::Const(_) => true,
        CoreExpr::Record { fields } => fields
            .values()
            .all(|value| expression_references_are_available(value, available)),
        CoreExpr::Field { base, .. } => expression_references_are_available(base, available),
        CoreExpr::Call { args, .. } => args
            .iter()
            .all(|argument| expression_references_are_available(argument, available)),
        CoreExpr::If {
            then_value,
            else_value,
            ..
        } => {
            expression_references_are_available(then_value, available)
                && expression_references_are_available(else_value, available)
        }
    }
}

fn predicate_fits_core_types(
    core: &CoreModule,
    pure_functions: &[TargetPureFunctionFact],
    predicate: &CorePredicate,
    available: &BTreeMap<&str, &LocalRef>,
) -> bool {
    match predicate {
        CorePredicate::True | CorePredicate::False => true,
        CorePredicate::Not(value) => {
            predicate_fits_core_types(core, pure_functions, value, available)
        }
        CorePredicate::All(values) | CorePredicate::Any(values) => {
            !values.is_empty()
                && values
                    .iter()
                    .all(|value| predicate_fits_core_types(core, pure_functions, value, available))
        }
        CorePredicate::Compare { left, right, .. } => {
            predicate_operands_are_comparable(core, pure_functions, left, right, available)
        }
    }
}

fn predicate_operands_are_comparable(
    core: &CoreModule,
    pure_functions: &[TargetPureFunctionFact],
    left: &CoreExpr,
    right: &CoreExpr,
    available: &BTreeMap<&str, &LocalRef>,
) -> bool {
    let Some(left) = comparison_operand_shape(core, pure_functions, left, available) else {
        return false;
    };
    let Some(right) = comparison_operand_shape(core, pure_functions, right, available) else {
        return false;
    };
    comparison_shape_fits(core, &left, &right, 0) || comparison_shape_fits(core, &right, &left, 0)
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum ComparisonOperandShape {
    Coordinate(String),
    Record(BTreeMap<String, ComparisonOperandShape>),
    List {
        item: Box<ComparisonOperandShape>,
        max: u64,
    },
}

impl ComparisonOperandShape {
    fn type_coordinate(&self) -> Option<String> {
        match self {
            Self::Coordinate(coordinate) => Some(coordinate.clone()),
            Self::List { item, max } => {
                Some(format!("List<{},max={max}>", item.type_coordinate()?))
            }
            Self::Record(_) => None,
        }
    }
}

fn field_expression_shape(
    core: &CoreModule,
    pure_functions: &[TargetPureFunctionFact],
    base: &CoreExpr,
    field: &str,
    available: &BTreeMap<&str, &LocalRef>,
) -> Option<ComparisonOperandShape> {
    let base_shape = comparison_operand_shape(core, pure_functions, base, available)?;
    comparison_record_fields(core, &base_shape)?.remove(field)
}

fn comparison_operand_shape(
    core: &CoreModule,
    pure_functions: &[TargetPureFunctionFact],
    expression: &CoreExpr,
    available: &BTreeMap<&str, &LocalRef>,
) -> Option<ComparisonOperandShape> {
    match expression {
        CoreExpr::Record { fields } => fields
            .iter()
            .map(|(field, value)| {
                Some((
                    field.clone(),
                    comparison_operand_shape(core, pure_functions, value, available)?,
                ))
            })
            .collect::<Option<BTreeMap<_, _>>>()
            .map(ComparisonOperandShape::Record),
        CoreExpr::Field { base, field } => {
            field_expression_shape(core, pure_functions, base, field, available)
        }
        CoreExpr::If {
            predicate,
            then_value,
            else_value,
        } => {
            if !predicate_fits_core_types(core, pure_functions, predicate, available) {
                return None;
            }
            let then_shape = comparison_operand_shape(core, pure_functions, then_value, available)?;
            let else_shape = comparison_operand_shape(core, pure_functions, else_value, available)?;
            join_conditional_shapes(core, &then_shape, &else_shape, 0)
        }
        _ => expression_type_coordinate(core, pure_functions, expression, available)
            .map(ComparisonOperandShape::Coordinate),
    }
}

fn join_conditional_shapes(
    core: &CoreModule,
    left: &ComparisonOperandShape,
    right: &ComparisonOperandShape,
    depth: usize,
) -> Option<ComparisonOperandShape> {
    if depth > MAX_CORE_TYPE_DEPTH {
        return None;
    }
    match (
        comparison_record_fields(core, left),
        comparison_record_fields(core, right),
    ) {
        (Some(left), Some(right)) => {
            if !left.keys().eq(right.keys()) {
                return None;
            }
            return left
                .iter()
                .map(|(field, left)| {
                    Some((
                        field.clone(),
                        join_conditional_shapes(
                            core,
                            left,
                            right.get(field).expect("equal record field keys"),
                            depth + 1,
                        )?,
                    ))
                })
                .collect::<Option<BTreeMap<_, _>>>()
                .map(ComparisonOperandShape::Record);
        }
        (Some(_), None) | (None, Some(_)) => return None,
        (None, None) => {}
    }

    match (
        comparison_list_shape(core, left),
        comparison_list_shape(core, right),
    ) {
        (Some((left_item, left_max)), Some((right_item, right_max))) => {
            return Some(ComparisonOperandShape::List {
                item: Box::new(join_conditional_shapes(
                    core,
                    &left_item,
                    &right_item,
                    depth + 1,
                )?),
                max: left_max.max(right_max),
            });
        }
        (Some(_), None) | (None, Some(_)) => return None,
        (None, None) => {}
    }

    let (ComparisonOperandShape::Coordinate(left), ComparisonOperandShape::Coordinate(right)) =
        (left, right)
    else {
        return None;
    };
    join_conditional_coordinates(core, left, right).map(ComparisonOperandShape::Coordinate)
}

fn comparison_shape_fits(
    core: &CoreModule,
    source: &ComparisonOperandShape,
    target: &ComparisonOperandShape,
    depth: usize,
) -> bool {
    if depth > MAX_CORE_TYPE_DEPTH {
        return false;
    }
    match (
        comparison_record_fields(core, source),
        comparison_record_fields(core, target),
    ) {
        (Some(source), Some(target)) => {
            return source.keys().eq(target.keys())
                && source.iter().all(|(field, source)| {
                    comparison_shape_fits(
                        core,
                        source,
                        target.get(field).expect("equal record field keys"),
                        depth + 1,
                    )
                });
        }
        (Some(_), None) | (None, Some(_)) => return false,
        (None, None) => {}
    }

    match (
        comparison_list_shape(core, source),
        comparison_list_shape(core, target),
    ) {
        (Some((source_item, source_max)), Some((target_item, target_max))) => {
            return source_max <= target_max
                && comparison_shape_fits(core, &source_item, &target_item, depth + 1);
        }
        (Some(_), None) | (None, Some(_)) => return false,
        (None, None) => {}
    }

    let (ComparisonOperandShape::Coordinate(source), ComparisonOperandShape::Coordinate(target)) =
        (source, target)
    else {
        return false;
    };
    core_type_fits(core, source, target)
}

fn comparison_record_fields(
    core: &CoreModule,
    shape: &ComparisonOperandShape,
) -> Option<BTreeMap<String, ComparisonOperandShape>> {
    match shape {
        ComparisonOperandShape::Record(fields) => Some(fields.clone()),
        ComparisonOperandShape::List { .. } => None,
        ComparisonOperandShape::Coordinate(coordinate) => {
            let CoreType::Record { fields } = resolved_core_type(core, coordinate)? else {
                return None;
            };
            Some(
                fields
                    .into_iter()
                    .map(|(field, coordinate)| {
                        (field, ComparisonOperandShape::Coordinate(coordinate))
                    })
                    .collect(),
            )
        }
    }
}

fn comparison_list_shape(
    core: &CoreModule,
    shape: &ComparisonOperandShape,
) -> Option<(ComparisonOperandShape, u64)> {
    match shape {
        ComparisonOperandShape::List { item, max } => Some((item.as_ref().clone(), *max)),
        ComparisonOperandShape::Record(_) => None,
        ComparisonOperandShape::Coordinate(coordinate) => {
            let CoreType::List { item, max } = resolved_core_type(core, coordinate)? else {
                return None;
            };
            Some((ComparisonOperandShape::Coordinate(item), max))
        }
    }
}

fn lower_intent(
    intent_name: &str,
    intent: &CoreIntent,
    operation_profiles: &BTreeSet<&str>,
    context: &TargetLoweringContext<'_>,
    failures: &mut Vec<TargetLoweringFailure>,
) -> TargetIrIntent {
    if !operation_profiles.contains(intent.required_operation_profile.as_str()) {
        failures.push(TargetLoweringFailure {
            kind: TargetLoweringFailureKind::MissingOperationProfile,
            intent: Some(intent_name.to_owned()),
            node_index: None,
            detail: intent.required_operation_profile.clone(),
        });
    }

    let mut state = IntentLoweringState::default();
    for (node_index, node) in intent.body.nodes.iter().enumerate() {
        if context.target_selection.execution_model == ExecutionModel::Ordered
            && matches!(node, CoreNode::ExternalActionRequest { .. })
        {
            failures.push(TargetLoweringFailure {
                kind: TargetLoweringFailureKind::UnsupportedCoreNode,
                intent: Some(intent_name.to_owned()),
                node_index: Some(node_index),
                detail: "ordered_external_action_request".to_owned(),
            });
            continue;
        }
        lower_node(intent_name, node_index, node, context, &mut state, failures);
        if context.target_selection.execution_model == ExecutionModel::Ordered {
            let id = match node {
                CoreNode::Let { .. } => state.pure_bindings.last().map(|node| &node.id),
                CoreNode::Effect { .. } => state.steps.last().map(|node| &node.id),
                CoreNode::Require { .. } => state.requirements.last().map(|node| &node.id),
                _ => None,
            };
            if let Some(id) = id {
                state.execution_order.push(id.clone());
            }
        }
    }
    if state.pure_bindings.is_empty()
        && state.requirements.is_empty()
        && state.steps.is_empty()
        && state.external_action_requests.is_empty()
        && intent.body.nodes.is_empty()
    {
        failures.push(TargetLoweringFailure {
            kind: TargetLoweringFailureKind::NoTargetSteps,
            intent: Some(intent_name.to_owned()),
            node_index: None,
            detail: "intent has no target-owned steps".to_owned(),
        });
    }

    TargetIrIntent {
        operation_profile: intent.required_operation_profile.clone(),
        execution_order: (context.target_selection.execution_model == ExecutionModel::Ordered)
            .then_some(state.execution_order),
        basis: intent.basis.clone(),
        input_constraints: intent.input_constraints.clone(),
        core_evaluation_budget: intent.core_evaluation_budget.clone(),
        pure_bindings: state.pure_bindings,
        requirements: state.requirements,
        steps: state.steps,
        external_action_requests: state.external_action_requests,
        result: intent.body.result.clone(),
    }
}

struct TargetLoweringContext<'a> {
    target_selection: TargetSelection,
    obstruction_coordinates: &'a BTreeSet<&'a str>,
    effect_lowerings: &'a BTreeMap<&'a str, Vec<&'a TargetEffectLowering>>,
}

#[derive(Default)]
struct IntentLoweringState {
    execution_order: Vec<String>,
    pure_bindings: Vec<TargetIrPureBinding>,
    requirements: Vec<TargetIrRequirement>,
    steps: Vec<TargetIrStep>,
    external_action_requests: Vec<TargetIrExternalActionRequest>,
    step_outputs: BTreeSet<String>,
}

fn lower_node(
    intent_name: &str,
    node_index: usize,
    node: &CoreNode,
    context: &TargetLoweringContext<'_>,
    state: &mut IntentLoweringState,
    failures: &mut Vec<TargetLoweringFailure>,
) {
    match node {
        CoreNode::Effect {
            binding,
            effect,
            input,
            obstruction_map,
        } => {
            let steps_before = state.steps.len();
            lower_effect_node(
                intent_name,
                node_index,
                EffectNodeParts {
                    binding,
                    effect,
                    input,
                    obstruction_map,
                },
                context,
                &mut state.steps,
                failures,
            );
            if state.steps.len() > steps_before {
                state.step_outputs.insert(binding.id.clone());
            }
        }
        CoreNode::ExternalActionRequest {
            binding,
            operation,
            input_type,
            settlement_type,
            input_schema,
            settlement_schema,
            input,
            authority_scope,
            basis,
            budget,
            reconciliation_law,
        } => state
            .external_action_requests
            .push(TargetIrExternalActionRequest {
                id: format!(
                    "{}.request.{}",
                    intent_name,
                    state.external_action_requests.len()
                ),
                binding: binding.clone(),
                operation: operation.clone(),
                input_type: input_type.clone(),
                settlement_type: settlement_type.clone(),
                input_schema: input_schema.clone(),
                settlement_schema: settlement_schema.clone(),
                input: input.clone(),
                authority_scope: authority_scope.as_ref().clone(),
                basis: basis.as_ref().clone(),
                budget: budget.as_ref().clone(),
                reconciliation_law: reconciliation_law.clone(),
            }),
        CoreNode::Let { binding, value } => {
            state.pure_bindings.push(TargetIrPureBinding {
                id: format!("{}.binding.{}", intent_name, state.pure_bindings.len()),
                binding: binding.clone(),
                value: value.clone(),
            });
        }
        CoreNode::Require { predicate, arm } => lower_require_node(
            intent_name,
            node_index,
            predicate,
            arm,
            context,
            state,
            failures,
        ),
        CoreNode::For { .. } => failures.push(TargetLoweringFailure {
            kind: TargetLoweringFailureKind::UnsupportedCoreNode,
            intent: Some(intent_name.to_owned()),
            node_index: Some(node_index),
            detail: "for".to_owned(),
        }),
        CoreNode::Branch { .. } => failures.push(TargetLoweringFailure {
            kind: TargetLoweringFailureKind::UnsupportedCoreNode,
            intent: Some(intent_name.to_owned()),
            node_index: Some(node_index),
            detail: "branch".to_owned(),
        }),
    }
}

fn lower_require_node(
    intent_name: &str,
    node_index: usize,
    predicate: &CorePredicate,
    arm: &CoreRequireFailureArm,
    context: &TargetLoweringContext<'_>,
    state: &mut IntentLoweringState,
    failures: &mut Vec<TargetLoweringFailure>,
) {
    if !context.target_selection.supports_requirements {
        failures.push(TargetLoweringFailure {
            kind: TargetLoweringFailureKind::UnsupportedTargetFeature,
            intent: Some(intent_name.to_owned()),
            node_index: Some(node_index),
            detail: "obstruction_requirement".to_owned(),
        });
        return;
    }
    if context.target_selection.execution_model == ExecutionModel::PreStepGuards
        && require_references_step_output(predicate, arm, &state.step_outputs)
    {
        failures.push(TargetLoweringFailure {
            kind: TargetLoweringFailureKind::UnsupportedTargetFeature,
            intent: Some(intent_name.to_owned()),
            node_index: Some(node_index),
            detail: "obstruction_requirement_step_output_dependency".to_owned(),
        });
        return;
    }
    if context.target_selection.execution_model == ExecutionModel::PreStepGuards
        && !state.steps.is_empty()
    {
        failures.push(TargetLoweringFailure {
            kind: TargetLoweringFailureKind::UnsupportedTargetFeature,
            intent: Some(intent_name.to_owned()),
            node_index: Some(node_index),
            detail: "obstruction_requirement_after_target_step".to_owned(),
        });
        return;
    }
    state.requirements.push(TargetIrRequirement {
        id: format!("{}.require.{}", intent_name, state.requirements.len()),
        predicate: predicate.clone(),
        on_failure: target_ir_require_failure(arm),
    });
}

fn target_ir_require_failure(arm: &CoreRequireFailureArm) -> TargetIrRequireFailure {
    match arm {
        CoreRequireFailureArm::Terminal { reason } => TargetIrRequireFailure::Terminal {
            reason: reason.clone(),
        },
        CoreRequireFailureArm::ContinueObstructed { reason } => {
            TargetIrRequireFailure::ContinueObstructed {
                reason: reason.clone(),
            }
        }
    }
}

fn require_references_step_output(
    predicate: &CorePredicate,
    arm: &CoreRequireFailureArm,
    step_outputs: &BTreeSet<String>,
) -> bool {
    predicate_references_step_output(predicate, step_outputs)
        || require_arm_references_step_output(arm, step_outputs)
}

fn require_arm_references_step_output(
    arm: &CoreRequireFailureArm,
    step_outputs: &BTreeSet<String>,
) -> bool {
    match arm {
        CoreRequireFailureArm::Terminal { reason }
        | CoreRequireFailureArm::ContinueObstructed { reason } => reason
            .payload
            .values()
            .any(|expr| expr_references_step_output(expr, step_outputs)),
    }
}

fn predicate_references_step_output(
    predicate: &CorePredicate,
    step_outputs: &BTreeSet<String>,
) -> bool {
    match predicate {
        CorePredicate::True | CorePredicate::False => false,
        CorePredicate::Not(value) => predicate_references_step_output(value, step_outputs),
        CorePredicate::All(values) | CorePredicate::Any(values) => values
            .iter()
            .any(|value| predicate_references_step_output(value, step_outputs)),
        CorePredicate::Compare { left, right, .. } => {
            expr_references_step_output(left, step_outputs)
                || expr_references_step_output(right, step_outputs)
        }
    }
}

fn expr_references_step_output(expr: &CoreExpr, step_outputs: &BTreeSet<String>) -> bool {
    match expr {
        CoreExpr::Local { reference } => step_outputs.contains(&reference.id),
        CoreExpr::Const(_) => false,
        CoreExpr::Record { fields } => fields
            .values()
            .any(|field| expr_references_step_output(field, step_outputs)),
        CoreExpr::Field { base, .. } => expr_references_step_output(base, step_outputs),
        CoreExpr::Call { args, .. } => args
            .iter()
            .any(|arg| expr_references_step_output(arg, step_outputs)),
        CoreExpr::If {
            predicate,
            then_value,
            else_value,
        } => {
            predicate_references_step_output(predicate, step_outputs)
                || expr_references_step_output(then_value, step_outputs)
                || expr_references_step_output(else_value, step_outputs)
        }
    }
}

#[derive(Clone, Copy)]
struct EffectNodeParts<'a> {
    binding: &'a LocalRef,
    effect: &'a str,
    input: &'a CoreExpr,
    obstruction_map: &'a BTreeMap<String, CoreObstructionArm>,
}

fn lower_effect_node(
    intent_name: &str,
    node_index: usize,
    node: EffectNodeParts<'_>,
    context: &TargetLoweringContext<'_>,
    steps: &mut Vec<TargetIrStep>,
    failures: &mut Vec<TargetLoweringFailure>,
) {
    let lowerings = context
        .effect_lowerings
        .get(node.effect)
        .map_or([].as_slice(), Vec::as_slice);
    match lowerings {
        [lowering]
            if !context
                .target_selection
                .supports_intrinsic(&lowering.target_intrinsic) =>
        {
            failures.push(TargetLoweringFailure {
                kind: TargetLoweringFailureKind::UnsupportedTargetIntrinsic,
                intent: Some(intent_name.to_owned()),
                node_index: Some(node_index),
                detail: lowering.target_intrinsic.clone(),
            });
        }
        [lowering] => {
            let mut mapped_obstructions = BTreeMap::new();
            for (failure, arm) in node.obstruction_map {
                let mapped_failure = lowering
                    .failure_mappings
                    .get(failure)
                    .unwrap_or(failure)
                    .clone();
                if mapped_obstructions
                    .insert(mapped_failure.clone(), arm.clone())
                    .is_some()
                {
                    failures.push(TargetLoweringFailure {
                        kind: TargetLoweringFailureKind::AmbiguousObstructionMapping,
                        intent: Some(intent_name.to_owned()),
                        node_index: Some(node_index),
                        detail: mapped_failure,
                    });
                    return;
                }
            }
            let unsupported_obstructions = mapped_obstructions
                .keys()
                .filter(|failure| !context.obstruction_coordinates.contains(failure.as_str()))
                .cloned()
                .collect::<Vec<_>>();
            if !unsupported_obstructions.is_empty() {
                failures.extend(unsupported_obstructions.into_iter().map(|failure| {
                    TargetLoweringFailure {
                        kind: TargetLoweringFailureKind::MissingObstruction,
                        intent: Some(intent_name.to_owned()),
                        node_index: Some(node_index),
                        detail: failure,
                    }
                }));
                return;
            }
            steps.push(TargetIrStep {
                id: format!("{}.step.{}", intent_name, steps.len()),
                binding: node.binding.clone(),
                effect: node.effect.to_owned(),
                target_intrinsic: lowering.target_intrinsic.clone(),
                input: node.input.clone(),
                obstruction_failures: mapped_obstructions.keys().cloned().collect(),
                obstruction_arms: mapped_obstructions,
            });
        }
        [] => failures.push(TargetLoweringFailure {
            kind: TargetLoweringFailureKind::MissingEffectLowering,
            intent: Some(intent_name.to_owned()),
            node_index: Some(node_index),
            detail: node.effect.to_owned(),
        }),
        _ => failures.push(TargetLoweringFailure {
            kind: TargetLoweringFailureKind::AmbiguousEffectLowering,
            intent: Some(intent_name.to_owned()),
            node_index: Some(node_index),
            detail: node.effect.to_owned(),
        }),
    }
}

fn effect_lowerings_by_coordinate(
    facts: &TargetIrLoweringFacts,
) -> BTreeMap<&str, Vec<&TargetEffectLowering>> {
    let mut out: BTreeMap<&str, Vec<&TargetEffectLowering>> = BTreeMap::new();
    for lowering in &facts.effect_lowerings {
        out.entry(&lowering.effect).or_default().push(lowering);
    }
    out
}

fn unsupported(failures: Vec<TargetLoweringFailure>) -> TargetLoweringReport {
    TargetLoweringReport {
        status: TargetLoweringStatus::Unsupported,
        artifact: None,
        result_projections: BTreeMap::new(),
        result_projection_failures: BTreeMap::new(),
        failures,
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::authenticated_named_type_closure_matches;
    use crate::core_ir::{CoreModule, CoreType, ResourceRef, CORE_API_VERSION};

    #[test]
    fn authenticated_named_type_closure_rejects_unrelated_extra_entry() {
        let lawpack = ResourceRef {
            coordinate: "hello.echo@1".to_owned(),
            digest: Some(format!("sha256:{}", "1".repeat(64))),
        };
        let parent = "hello.echo@1.Envelope".to_owned();
        let leaf = "hello.echo@1.Leaf".to_owned();
        let unrelated = "hello.echo@1.Unrelated".to_owned();
        let parent_definition = CoreType::Record {
            fields: BTreeMap::from([("payload".to_owned(), format!("Record<value:{leaf}>"))]),
        };
        let leaf_definition = CoreType::Int {
            width: "U64".to_owned(),
        };
        let unrelated_definition = CoreType::Bool;
        let core = CoreModule {
            functions: BTreeMap::new(),
            api_version: CORE_API_VERSION.to_owned(),
            coordinate: "examples.closure@1".to_owned(),
            imports: Vec::new(),
            types: BTreeMap::from([
                (parent.clone(), parent_definition.clone()),
                (leaf.clone(), leaf_definition.clone()),
                (unrelated.clone(), unrelated_definition.clone()),
            ]),
            intents: BTreeMap::new(),
            required_core_capabilities: Vec::new(),
        };
        let exact = BTreeMap::from([(parent.clone(), parent_definition), (leaf, leaf_definition)]);

        assert!(authenticated_named_type_closure_matches(
            &core,
            &lawpack,
            [parent.as_str()],
            &exact,
        ));

        let mut with_unrelated = exact;
        with_unrelated.insert(unrelated, unrelated_definition);
        assert!(!authenticated_named_type_closure_matches(
            &core,
            &lawpack,
            [parent.as_str()],
            &with_unrelated,
        ));
    }
}

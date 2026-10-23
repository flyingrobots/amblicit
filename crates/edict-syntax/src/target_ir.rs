//! First target-owned IR generation surface.
//!
//! This module contains the narrow v0.9 target slices. It lowers supported Core
//! effect nodes into in-memory Echo or git-warp review artifacts. It does not
//! execute a runtime, run a verifier, assemble bundles, or perform admission.

mod byte_concat;
mod byte_count;
mod byte_length;
mod byte_slice;
mod byte_utf8;
pub(crate) mod execution_order;
mod pure_intrinsics;
mod totality;
mod unsigned_addition;
mod unsigned_subtraction;

mod unsigned_arithmetic {
    pub(super) use super::unsigned_subtraction::type_coordinate;
}

use std::collections::{BTreeMap, BTreeSet};

use crate::core_ir::{
    core_type_fits, is_lowercase_sha256_review_digest, named_core_type_closure, resolved_core_type,
    validate_core_module_type_integrity, CompareOp, CoreBlock, CoreBound, CoreBudget, CoreExpr,
    CoreExternalActionBudget, CoreImportKind, CoreIntent, CoreModule, CoreNode, CoreObstructionArm,
    CoreObstructionReason, CorePredicate, CoreRequireFailureArm, CoreType, CoreValue,
    InputConstraint, LocalRef, ResourceRef, CORE_API_VERSION, CORE_APPLICATION_INPUT_LOCAL_ID,
    MAX_CORE_GRAPH_DEPTH, MAX_CORE_TYPE_DEPTH,
};
use crate::digest_core_module;
use crate::lowerability::{LowerabilityEffectStatus, LowerabilityReport, LowerabilityStatus};
use crate::result_projection::emit_result_projection_with_closure;
use crate::target_intrinsics::TargetPureIntrinsicFact;
use crate::MAX_CANONICAL_NESTING_DEPTH;
use crate::{ResultProjectionArtifact, ResultProjectionFailure};

pub const ECHO_DPO_TARGET_PROFILE: &str = "echo.dpo@1";
pub const ECHO_SPAN_IR_DOMAIN: &str = "echo.span-ir/v1";
pub const ECHO_ORDERED_SPAN_IR_DOMAIN: &str = "echo.span-ir/v2";
pub const ECHO_STRUCTURED_SPAN_IR_DOMAIN: &str = "echo.span-ir/v3";
pub const GITWARP_REF_CRDT_TARGET_PROFILE: &str = "gitwarp.ref_crdt@1";
pub const GITWARP_COMMIT_REDUCER_IR_DOMAIN: &str = "gitwarp.commit-reducer-ir/v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ExecutionModel {
    PreStepGuards,
    Ordered,
    /// `echo.span-ir/v3`: ordered instructions plus bounded loops and
    /// statement branches.
    Structured,
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
    /// Present for a pinned target-catalog intrinsic, whose `lawpack` is then
    /// the target profile import rather than a lawpack.
    intrinsic_cost: Option<IntrinsicCost>,
}

/// Linear cost rule of a pinned target pure intrinsic.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct IntrinsicCost {
    pub(crate) base: u64,
    pub(crate) per_argument_max: Vec<u64>,
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
            intrinsic_cost: None,
        }
    }

    pub(crate) fn from_target_pure_intrinsic(fact: &TargetPureIntrinsicFact) -> Self {
        Self {
            lawpack: fact.target().clone(),
            coordinate: fact.coordinate().to_owned(),
            type_parameters: Vec::new(),
            parameter_types: fact.parameter_types().to_vec(),
            return_type: fact.return_type().to_owned(),
            type_closure: fact.provider_types().clone(),
            intrinsic_cost: Some(IntrinsicCost {
                base: fact.cost_base(),
                per_argument_max: fact.cost_per_argument_max().to_vec(),
            }),
        }
    }

    /// Whether this is a pinned target-catalog intrinsic rather than a lawpack
    /// helper.
    #[must_use]
    pub fn is_target_intrinsic(&self) -> bool {
        self.intrinsic_cost.is_some()
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
    /// Add admitted target-catalog pure intrinsics. Lowering refuses any whose
    /// target is not exactly [`Self::target_profile`].
    #[must_use]
    pub fn with_target_pure_intrinsics(mut self, intrinsics: &[TargetPureIntrinsicFact]) -> Self {
        self.pure_functions.extend(
            intrinsics
                .iter()
                .map(TargetPureFunctionFact::from_target_pure_intrinsic),
        );
        self
    }

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
    /// Exact execution order over the instruction tables for explicitly
    /// selected ordered (`echo.span-ir/v2`) or structured (`echo.span-ir/v3`)
    /// Target IR. v2 permits only instruction entries; v3 adds bounded loops
    /// and statement branches. Legacy artifacts omit this field.
    pub execution_order: Option<Vec<TargetIrOrderEntry>>,
    pub basis: Option<CoreExpr>,
    pub input_constraints: Vec<InputConstraint>,
    pub core_evaluation_budget: CoreBudget,
    pub pure_bindings: Vec<TargetIrPureBinding>,
    pub requirements: Vec<TargetIrRequirement>,
    pub steps: Vec<TargetIrStep>,
    pub external_action_requests: Vec<TargetIrExternalActionRequest>,
    pub result: CoreExpr,
}

/// One entry of an ordered or structured execution order.
///
/// Instruction payloads stay in the intent's `pure_bindings`, `steps` and
/// `requirements` tables; the order references them by id exactly once.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TargetIrOrderEntry {
    /// Id of a pure binding, target step or requirement.
    Instruction(String),
    /// Bounded loop over a Core `List` (`echo.span-ir/v3` only).
    For(TargetIrForEntry),
    /// Statement-only branch (`echo.span-ir/v3` only).
    Branch(TargetIrBranchEntry),
}

/// Bounded loop copied from a Core `for` node.
///
/// The iterable is evaluated once at loop entry in the enclosing scope. The
/// binder and locals introduced by the body are visible only to later entries
/// of the same body, are fresh for each iteration, and are not visible after
/// the loop. The bound is the Core bound; no effective cardinality is derived.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TargetIrForEntry {
    pub id: String,
    pub binder: LocalRef,
    pub iterable: CoreExpr,
    pub bound: CoreBound,
    pub body: Vec<TargetIrOrderEntry>,
}

/// Statement-only branch copied from a Core `branch` node without a binding.
///
/// The predicate is evaluated in the enclosing scope. Each arm has its own
/// scope; locals introduced by an arm are not visible after the branch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TargetIrBranchEntry {
    pub id: String,
    pub predicate: CorePredicate,
    pub then_entries: Vec<TargetIrOrderEntry>,
    pub else_entries: Vec<TargetIrOrderEntry>,
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
    if let Some(failure) = pure_intrinsics::foreign_target(facts) {
        return unsupported(vec![failure]);
    }
    let core_failures = validate_core_module(
        core,
        &facts.pure_functions,
        &facts.effect_signatures,
        target_selection.execution_model,
    );
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
        if let Some(failure) = structured_encoding_failure(&artifact) {
            return unsupported(vec![failure]);
        }
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

/// Lowering never returns a v3 artifact its own encoder refuses. Each control
/// level adds a map and an array of canonical nesting on top of its Core
/// values, so an artifact whose Core blocks validated can still exceed
/// `MAX_CANONICAL_NESTING_DEPTH` (`canonical_nesting_depth`); any other encoder
/// refusal is a validation gap and refuses as `structured_artifact_unencodable`.
/// The first refused intent is named.
fn structured_encoding_failure(artifact: &TargetIrArtifact) -> Option<TargetLoweringFailure> {
    if artifact.domain != ECHO_STRUCTURED_SPAN_IR_DOMAIN {
        return None;
    }
    let error = crate::encode_target_ir_artifact(artifact).err()?;
    // Name the refused intent without rebuilding any envelope: the shared
    // header is checked once, then each borrowed intent with the encoder's own
    // per-intent checks at its canonical depth. A header refusal names none.
    let intent = if crate::canonical::check_target_ir_header(artifact).is_err() {
        None
    } else {
        artifact.intents.iter().find_map(|(name, intent)| {
            crate::canonical::check_target_ir_intent(artifact, name, intent)
                .is_err_and(|intent_error| intent_error.kind() == error.kind())
                .then(|| name.clone())
        })
    };
    let (kind, detail) = if error.kind() == crate::CanonicalErrorKind::NestingLimitExceeded {
        (
            TargetLoweringFailureKind::UnsupportedCoreNode,
            "canonical_nesting_depth",
        )
    } else {
        (
            TargetLoweringFailureKind::InvalidCoreIdentity,
            "structured_artifact_unencodable",
        )
    };
    Some(TargetLoweringFailure {
        kind,
        intent,
        node_index: None,
        detail: detail.to_owned(),
    })
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
    let has_pure_bindings = core
        .intents
        .values()
        .any(|intent| block_has_pure_binding(&intent.body, 0));
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

/// Whether a block or any nested control block contains a `let`. Nested lets
/// become pure bindings only in structured Target IR; legacy domains refuse
/// the enclosing control node before an artifact exists.
fn block_has_pure_binding(block: &CoreBlock, depth: usize) -> bool {
    depth <= MAX_CORE_GRAPH_DEPTH
        && block.nodes.iter().any(|node| match node {
            CoreNode::Let { .. } => true,
            CoreNode::For { body, .. } => block_has_pure_binding(body, depth + 1),
            CoreNode::Branch {
                then_block,
                else_block,
                ..
            } => {
                block_has_pure_binding(then_block, depth + 1)
                    || block_has_pure_binding(else_block, depth + 1)
            }
            CoreNode::Require { .. }
            | CoreNode::Effect { .. }
            | CoreNode::ExternalActionRequest { .. } => false,
        })
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
    if facts.target_profile.coordinate == ECHO_DPO_TARGET_PROFILE
        && facts.target_ir_domain == ECHO_STRUCTURED_SPAN_IR_DOMAIN
    {
        target_selection.target_ir_domain = ECHO_STRUCTURED_SPAN_IR_DOMAIN;
        target_selection.execution_model = ExecutionModel::Structured;
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
    execution_model: ExecutionModel,
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
    validate_pure_binding_graphs(core, pure_functions, effect_signatures, execution_model)
}

fn validate_pure_binding_graphs(
    core: &CoreModule,
    pure_functions: &[TargetPureFunctionFact],
    effect_signatures: &[TargetEffectSignatureFact],
    execution_model: ExecutionModel,
) -> Vec<TargetLoweringFailure> {
    let structured = execution_model == ExecutionModel::Structured;
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
            if !totality::intent_is_total(core, intent) {
                return Some(invalid_pure_binding(
                    intent_name,
                    None,
                    "Core operation is not proven total in its evaluation scope",
                ));
            }
            if !pure_intrinsics::intent_cost_fits(core, pure_functions, intent) {
                return Some(invalid_pure_binding(
                    intent_name,
                    None,
                    "target pure-intrinsic cost is unestablished or exceeds the step budget",
                ));
            }
            if structured {
                // Unsupported structured shapes refuse with their stable
                // detail before the nested-block graph judgment runs.
                if let Some(failure) =
                    structured_shape_failure(intent_name, &intent.body.nodes, None, 0)
                {
                    return Some(failure);
                }
                // The Core schema requires one or more `all`/`any` values; one
                // check covers every predicate the v3 intent carries.
                if let Some(site) = intent_empty_aggregate(intent) {
                    let node_index = match site {
                        EmptyAggregateSite::Intent => None,
                        EmptyAggregateSite::Node(index) => Some(index),
                    };
                    return Some(invalid_pure_binding(
                        intent_name,
                        node_index,
                        "empty_predicate_aggregate",
                    ));
                }
            } else if intent
                .body
                .nodes
                .iter()
                .any(|node| matches!(node, CoreNode::For { .. }))
            {
                return None;
            }
            validate_pure_binding_graph(
                GraphContext {
                    core,
                    pure_functions,
                    effect_signatures,
                    intent_name,
                },
                intent,
                structured,
            )
        })
        .collect()
}

#[derive(Clone, Copy)]
struct GraphContext<'a> {
    core: &'a CoreModule,
    pure_functions: &'a [TargetPureFunctionFact],
    effect_signatures: &'a [TargetEffectSignatureFact],
    intent_name: &'a str,
}

fn validate_pure_binding_graph(
    context: GraphContext<'_>,
    intent: &CoreIntent,
    structured: bool,
) -> Option<TargetLoweringFailure> {
    let GraphContext {
        core,
        pure_functions,
        intent_name,
        ..
    } = context;
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
    let failure = if structured {
        let mut claimed = BTreeSet::from([input.id.as_str()]);
        validate_structured_block(context, &intent.body, None, &mut available, &mut claimed, 0)
    } else {
        validate_flat_nodes(context, &intent.body.nodes, &locals, &mut available)
    };
    if failure.is_some() {
        return failure;
    }
    if !expression_references_are_available(&intent.body.result, &available) {
        return Some(invalid_pure_binding(
            intent_name,
            None,
            "result references an undeclared or conflicting local",
        ));
    }
    if structured && !intent_comparisons_admissible(core, pure_functions, intent, &available) {
        return Some(invalid_pure_binding(
            intent_name,
            None,
            INADMISSIBLE_COMPARISON,
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

/// v1/v2 graph judgment over a flat intent body. Control nodes are refused at
/// lowering; their nested blocks are not part of this judgment.
fn validate_flat_nodes<'a>(
    context: GraphContext<'_>,
    nodes: &'a [CoreNode],
    locals: &BTreeMap<&str, &LocalRef>,
    available: &mut BTreeMap<&'a str, &'a LocalRef>,
) -> Option<TargetLoweringFailure> {
    let GraphContext {
        core,
        pure_functions,
        effect_signatures,
        intent_name,
    } = context;
    for (node_index, node) in nodes.iter().enumerate() {
        if !node_expressions_have_closed_authority(
            core,
            pure_functions,
            effect_signatures,
            node,
            locals,
            available,
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
    None
}

/// Stable refusal for Core shapes outside the structured v3 contract, checked
/// over every nested block before the structured graph judgment.
fn structured_shape_failure(
    intent_name: &str,
    nodes: &[CoreNode],
    top_index: Option<usize>,
    depth: usize,
) -> Option<TargetLoweringFailure> {
    if depth > MAX_CORE_GRAPH_DEPTH {
        return Some(invalid_pure_binding(
            intent_name,
            top_index,
            "structured control nesting exceeds the Core graph depth",
        ));
    }
    for (index, node) in nodes.iter().enumerate() {
        let node_index = top_index.unwrap_or(index);
        if let Some(detail) = structured_node_refusal(node) {
            return Some(TargetLoweringFailure {
                kind: TargetLoweringFailureKind::UnsupportedCoreNode,
                intent: Some(intent_name.to_owned()),
                node_index: Some(node_index),
                detail: detail.to_owned(),
            });
        }
        let blocks: Vec<&CoreBlock> = match node {
            CoreNode::Branch {
                then_block,
                else_block,
                ..
            } => vec![then_block, else_block],
            CoreNode::For { body, .. } => vec![body],
            CoreNode::Let { .. }
            | CoreNode::Effect { .. }
            | CoreNode::Require { .. }
            | CoreNode::ExternalActionRequest { .. } => continue,
        };
        for block in blocks {
            let failure =
                structured_shape_failure(intent_name, &block.nodes, Some(node_index), depth + 1);
            if failure.is_some() {
                return failure;
            }
        }
    }
    None
}

/// Validate one structured block in its own scope.
///
/// `available` holds the locals visible to the block's first node; locals the
/// block introduces are added for later nodes of the same block only, because
/// nested blocks validate against a clone of their enclosing scope. `claimed`
/// spans the whole intent, so no identity is introduced twice in any scope.
fn validate_structured_block<'a>(
    context: GraphContext<'a>,
    block: &'a CoreBlock,
    top_index: Option<usize>,
    available: &mut BTreeMap<&'a str, &'a LocalRef>,
    claimed: &mut BTreeSet<&'a str>,
    depth: usize,
) -> Option<TargetLoweringFailure> {
    let locals = block
        .locals
        .iter()
        .map(|local| (local.id.as_str(), local))
        .collect::<BTreeMap<_, _>>();
    if depth > MAX_CORE_GRAPH_DEPTH || locals.len() != block.locals.len() {
        return Some(invalid_pure_binding(
            context.intent_name,
            top_index,
            "structured block local table is invalid",
        ));
    }
    for (index, node) in block.nodes.iter().enumerate() {
        let node_index = top_index.unwrap_or(index);
        let failure = match node {
            CoreNode::For { .. } | CoreNode::Branch { .. } => {
                validate_structured_control(context, node, node_index, available, claimed, depth)
            }
            CoreNode::Let { .. }
            | CoreNode::Effect { .. }
            | CoreNode::Require { .. }
            | CoreNode::ExternalActionRequest { .. } => validate_structured_instruction(
                context, node, node_index, &locals, available, claimed,
            ),
        };
        if failure.is_some() {
            return failure;
        }
    }
    None
}

fn validate_structured_instruction<'a>(
    context: GraphContext<'_>,
    node: &'a CoreNode,
    node_index: usize,
    locals: &BTreeMap<&str, &LocalRef>,
    available: &mut BTreeMap<&'a str, &'a LocalRef>,
    claimed: &mut BTreeSet<&'a str>,
) -> Option<TargetLoweringFailure> {
    let invalid = |detail: &str| {
        Some(invalid_pure_binding(
            context.intent_name,
            Some(node_index),
            detail,
        ))
    };
    let binding = match node {
        CoreNode::Let { binding, .. } | CoreNode::Effect { binding, .. } => Some(binding),
        CoreNode::Require { .. } => None,
        CoreNode::ExternalActionRequest { .. } | CoreNode::For { .. } | CoreNode::Branch { .. } => {
            return invalid("structured Target IR does not support this Core node");
        }
    };
    if !node_expressions_have_closed_authority(
        context.core,
        context.pure_functions,
        context.effect_signatures,
        node,
        locals,
        available,
    ) {
        return invalid("Core node contains an invalid or unauthorized expression");
    }
    if !node_comparisons_admissible(context.core, context.pure_functions, node, available) {
        return invalid(INADMISSIBLE_COMPARISON);
    }
    // Obstruction binders stay scoped to their arm for references, but are
    // recorded intent-wide like every other introduced identity, so no visible,
    // claimed or later identity (sibling arm, loop binder, producer) reuses one.
    if let CoreNode::Effect {
        obstruction_map, ..
    } = node
    {
        for arm in obstruction_map.values() {
            if available.contains_key(arm.binder.id.as_str())
                || !claimed.insert(arm.binder.id.as_str())
            {
                return invalid("obstruction_binder_not_fresh");
            }
        }
    }
    if let Some(binding) = binding {
        if locals.get(binding.id.as_str()).copied() != Some(binding)
            || !claimed.insert(binding.id.as_str())
            || available.insert(binding.id.as_str(), binding).is_some()
        {
            return invalid(
                "binding identity is missing, duplicated, or conflicts with its block table",
            );
        }
    }
    None
}

/// Validate a loop or statement branch against its enclosing scope, then each
/// nested block in a child scope that is discarded afterwards.
fn validate_structured_control<'a>(
    context: GraphContext<'a>,
    node: &'a CoreNode,
    node_index: usize,
    available: &BTreeMap<&'a str, &'a LocalRef>,
    claimed: &mut BTreeSet<&'a str>,
    depth: usize,
) -> Option<TargetLoweringFailure> {
    let invalid = |detail: &str| {
        Some(invalid_pure_binding(
            context.intent_name,
            Some(node_index),
            detail,
        ))
    };
    let mut scope = available.clone();
    let blocks: Vec<&'a CoreBlock> = match node {
        CoreNode::For {
            binder,
            iter,
            bound,
            body,
        } => {
            if !loop_iterable_fits_binder(
                context.core,
                context.pure_functions,
                iter,
                bound,
                binder,
                available,
            ) {
                return invalid("loop iterable is not a bounded list whose items fit the binder");
            }
            if !node_comparisons_admissible(context.core, context.pure_functions, node, available) {
                return invalid(INADMISSIBLE_COMPARISON);
            }
            if body.locals.iter().find(|local| local.id == binder.id) != Some(binder)
                || !claimed.insert(binder.id.as_str())
            {
                return invalid(
                    "loop binder is missing from its body table or reuses an existing identity",
                );
            }
            scope.insert(binder.id.as_str(), binder);
            vec![body]
        }
        CoreNode::Branch {
            binding: None,
            predicate,
            then_block,
            else_block,
        } => {
            if !predicate_fits_core_types(
                context.core,
                context.pure_functions,
                predicate,
                available,
            ) {
                return invalid("branch predicate contains an invalid or unauthorized expression");
            }
            if !node_comparisons_admissible(context.core, context.pure_functions, node, available) {
                return invalid(INADMISSIBLE_COMPARISON);
            }
            vec![then_block, else_block]
        }
        CoreNode::Branch { .. }
        | CoreNode::Let { .. }
        | CoreNode::Effect { .. }
        | CoreNode::Require { .. }
        | CoreNode::ExternalActionRequest { .. } => {
            return invalid("structured Target IR does not support this Core node");
        }
    };
    for block in blocks {
        let mut block_scope = scope.clone();
        let failure = validate_structured_block(
            context,
            block,
            Some(node_index),
            &mut block_scope,
            claimed,
            depth + 1,
        );
        if failure.is_some() {
            return failure;
        }
    }
    None
}

/// A loop iterable must be a bounded list whose items fit the binder, and its
/// literal bound must cover the list maximum. Target has no authenticated
/// constant values, so a coordinate bound never fits (it is refused earlier
/// with `coordinate_loop_bound`).
fn loop_iterable_fits_binder(
    core: &CoreModule,
    pure_functions: &[TargetPureFunctionFact],
    iterable: &CoreExpr,
    bound: &CoreBound,
    binder: &LocalRef,
    available: &BTreeMap<&str, &LocalRef>,
) -> bool {
    let CoreBound::Literal(bound) = *bound else {
        return false;
    };
    if resolved_core_type(core, &binder.ty).is_none() {
        return false;
    }
    if let CoreExpr::List { values } = iterable {
        return u64::try_from(values.len()).is_ok_and(|count| count <= bound)
            && values.iter().all(|value| {
                expression_fits_declared_type(core, pure_functions, value, &binder.ty, available)
            });
    }
    let Some(coordinate) = expression_type_coordinate(core, pure_functions, iterable, available)
    else {
        return false;
    };
    let Some(CoreType::List { item, max }) = resolved_core_type(core, &coordinate) else {
        return false;
    };
    core_type_fits(core, &item, &binder.ty) && max <= bound
}

fn block_local_identities_are_valid(block: &CoreBlock, depth: usize) -> bool {
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
        CoreExpr::List { values } => values
            .iter()
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
        } if callee == byte_count::OPERATION => {
            byte_count::type_coordinate(core, pure_functions, type_args, args, available).is_some()
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
        } if is_unsigned_arithmetic(callee) => {
            unsigned_arithmetic::type_coordinate(core, pure_functions, type_args, args, available)
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

fn is_unsigned_arithmetic(callee: &str) -> bool {
    callee == unsigned_subtraction::OPERATION || callee == unsigned_addition::OPERATION
}

/// A record fits when it has exactly the declared fields and each fits its type.
fn record_fits_declared_type(
    core: &CoreModule,
    pure_functions: &[TargetPureFunctionFact],
    fields: &BTreeMap<String, CoreExpr>,
    expected: &str,
    available: &BTreeMap<&str, &LocalRef>,
) -> bool {
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

/// A list fits when it has at most the declared maximum of elements and every
/// element fits the declared item type.
fn list_fits_declared_type(
    core: &CoreModule,
    pure_functions: &[TargetPureFunctionFact],
    values: &[CoreExpr],
    expected: &str,
    available: &BTreeMap<&str, &LocalRef>,
) -> bool {
    let Some(CoreType::List { item, max }) = resolved_core_type(core, expected) else {
        return false;
    };
    u64::try_from(values.len()).is_ok_and(|count| count <= max)
        && values.iter().all(|value| {
            expression_fits_declared_type(core, pure_functions, value, &item, available)
        })
}

/// Prelude operations whose result coordinate Target derives itself.
fn is_typed_prelude_operation(callee: &str) -> bool {
    [
        byte_concat::OPERATION,
        byte_length::OPERATION,
        byte_utf8::OPERATION,
        byte_count::OPERATION,
        byte_slice::OPERATION,
    ]
    .contains(&callee)
        || is_unsigned_arithmetic(callee)
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
        CoreExpr::List { values } => {
            list_fits_declared_type(core, pure_functions, values, expected, available)
        }
        CoreExpr::Const(value) => core_value_fits_declared_type(core, value, expected),
        CoreExpr::Record { fields } => {
            record_fits_declared_type(core, pure_functions, fields, expected, available)
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
        CoreExpr::Call { callee, .. } if is_typed_prelude_operation(callee) => {
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
        if !totality::bindings_are_total(
            core,
            function
                .body
                .bindings
                .iter()
                .map(|binding| (&binding.binding, &binding.value)),
            &function.body.result,
        ) {
            return Some(invalid(
                "a binding or the result failed totality validation",
            ));
        }
        for binding in &function.body.bindings {
            if !expression_fits_declared_type(
                core,
                imported,
                &binding.value,
                &binding.binding.ty,
                &available,
            ) {
                return Some(invalid(
                    "binding does not fit its declared type or available scope",
                ));
            }
            available.insert(binding.binding.id.as_str(), &binding.binding);
        }
        if !expression_fits_declared_type(
            core,
            imported,
            &function.body.result,
            &function.return_type,
            &available,
        ) {
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
        || !core.imports.iter().any(|import| {
            let kind = if fact.intrinsic_cost.is_some() {
                CoreImportKind::Target
            } else {
                CoreImportKind::Lawpack
            };
            import.kind == kind && import.resource == fact.lawpack
        })
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
        | CoreExpr::Record { .. }
        | CoreExpr::List { .. } => None,
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
        } if callee == byte_count::OPERATION => {
            byte_count::type_coordinate(core, pure_functions, type_args, args, available)
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
        } if is_unsigned_arithmetic(callee) => {
            unsigned_arithmetic::type_coordinate(core, pure_functions, type_args, args, available)
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
        | CoreExpr::List { .. }
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
        CoreExpr::List { values } => values
            .iter()
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

/// Whether a predicate, or a predicate inside one of its expressions, contains
/// an `all` or `any` with no values, which `docs/abi/edict-core.cddl` forbids.
/// Nesting beyond the canonical depth is left to the callers' depth checks.
fn predicate_has_empty_aggregate(predicate: &CorePredicate, depth: usize) -> bool {
    if depth > MAX_CANONICAL_NESTING_DEPTH {
        return false;
    }
    match predicate {
        CorePredicate::True | CorePredicate::False => false,
        CorePredicate::Not(value) => predicate_has_empty_aggregate(value, depth + 1),
        CorePredicate::All(values) | CorePredicate::Any(values) => {
            values.is_empty()
                || values
                    .iter()
                    .any(|value| predicate_has_empty_aggregate(value, depth + 1))
        }
        CorePredicate::Compare { left, right, .. } => {
            expression_has_empty_aggregate(left, depth + 1)
                || expression_has_empty_aggregate(right, depth + 1)
        }
    }
}

/// Where an empty `all`/`any` sits in a v3 intent.
enum EmptyAggregateSite {
    /// Intent metadata (basis, input constraints) or the result.
    Intent,
    /// A top-level node, including its nested blocks.
    Node(usize),
}

/// Locate an empty `all`/`any` anywhere in a v3 intent, or `None` when every
/// aggregate is nonempty.
fn intent_empty_aggregate(intent: &CoreIntent) -> Option<EmptyAggregateSite> {
    if intent
        .basis
        .as_ref()
        .is_some_and(|basis| expression_has_empty_aggregate(basis, 0))
        || intent
            .input_constraints
            .iter()
            .any(|constraint| predicate_has_empty_aggregate(&constraint.predicate, 0))
    {
        return Some(EmptyAggregateSite::Intent);
    }
    if let Some(index) = intent
        .body
        .nodes
        .iter()
        .position(|node| node_has_empty_aggregate(node, 0))
    {
        return Some(EmptyAggregateSite::Node(index));
    }
    expression_has_empty_aggregate(&intent.body.result, 0).then_some(EmptyAggregateSite::Intent)
}

fn block_has_empty_aggregate(block: &CoreBlock, depth: usize) -> bool {
    depth <= MAX_CANONICAL_NESTING_DEPTH
        && (block
            .nodes
            .iter()
            .any(|node| node_has_empty_aggregate(node, depth + 1))
            || expression_has_empty_aggregate(&block.result, depth + 1))
}

fn node_has_empty_aggregate(node: &CoreNode, depth: usize) -> bool {
    if depth > MAX_CANONICAL_NESTING_DEPTH {
        return false;
    }
    let reason_has = |reason: &CoreObstructionReason| {
        reason
            .payload
            .values()
            .any(|value| expression_has_empty_aggregate(value, depth + 1))
    };
    match node {
        CoreNode::Let { value, .. } => expression_has_empty_aggregate(value, depth + 1),
        CoreNode::Require { predicate, arm } => {
            let (CoreRequireFailureArm::Terminal { reason }
            | CoreRequireFailureArm::ContinueObstructed { reason }) = arm;
            predicate_has_empty_aggregate(predicate, depth + 1) || reason_has(reason)
        }
        CoreNode::Effect {
            input,
            obstruction_map,
            ..
        } => {
            expression_has_empty_aggregate(input, depth + 1)
                || obstruction_map
                    .values()
                    .any(|arm| expression_has_empty_aggregate(&arm.value, depth + 1))
        }
        CoreNode::ExternalActionRequest {
            input,
            authority_scope,
            basis,
            ..
        } => [input, authority_scope.as_ref(), basis.as_ref()]
            .into_iter()
            .any(|expression| expression_has_empty_aggregate(expression, depth + 1)),
        CoreNode::For { iter, body, .. } => {
            expression_has_empty_aggregate(iter, depth + 1)
                || block_has_empty_aggregate(body, depth + 1)
        }
        CoreNode::Branch {
            predicate,
            then_block,
            else_block,
            ..
        } => {
            predicate_has_empty_aggregate(predicate, depth + 1)
                || block_has_empty_aggregate(then_block, depth + 1)
                || block_has_empty_aggregate(else_block, depth + 1)
        }
    }
}

fn expression_has_empty_aggregate(expression: &CoreExpr, depth: usize) -> bool {
    if depth > MAX_CANONICAL_NESTING_DEPTH {
        return false;
    }
    match expression {
        CoreExpr::Local { .. } | CoreExpr::Const(_) => false,
        CoreExpr::Record { fields } => fields
            .values()
            .any(|value| expression_has_empty_aggregate(value, depth + 1)),
        CoreExpr::List { values } | CoreExpr::Call { args: values, .. } => values
            .iter()
            .any(|value| expression_has_empty_aggregate(value, depth + 1)),
        CoreExpr::Field { base, .. } => expression_has_empty_aggregate(base, depth + 1),
        CoreExpr::If {
            predicate,
            then_value,
            else_value,
        } => {
            predicate_has_empty_aggregate(predicate, depth + 1)
                || expression_has_empty_aggregate(then_value, depth + 1)
                || expression_has_empty_aggregate(else_value, depth + 1)
        }
    }
}

/// Stable v3 detail for an ordered comparison outside the closed operator set.
const INADMISSIBLE_COMPARISON: &str = "inadmissible_comparison_operator";

/// The closed operator set admits `<`, `<=`, `>` and `>=` only between
/// integers; bytes, strings, booleans and records admit only `==` and `!=`.
fn ordered_operands_are_integers(
    core: &CoreModule,
    pure_functions: &[TargetPureFunctionFact],
    operands: [&CoreExpr; 2],
    available: &BTreeMap<&str, &LocalRef>,
) -> bool {
    operands.into_iter().all(|operand| {
        matches!(
            comparison_operand_shape(core, pure_functions, operand, available),
            Some(ComparisonOperandShape::Coordinate(coordinate))
                if matches!(resolved_core_type(core, &coordinate), Some(CoreType::Int { .. }))
        )
    })
}

/// Whether every comparison in a predicate, including predicates inside its
/// `if` operands, uses an operator its operand types admit.
fn predicate_comparisons_admissible(
    core: &CoreModule,
    pure_functions: &[TargetPureFunctionFact],
    predicate: &CorePredicate,
    available: &BTreeMap<&str, &LocalRef>,
    depth: usize,
) -> bool {
    if depth > MAX_CANONICAL_NESTING_DEPTH {
        return false;
    }
    match predicate {
        CorePredicate::True | CorePredicate::False => true,
        CorePredicate::Not(value) => {
            predicate_comparisons_admissible(core, pure_functions, value, available, depth + 1)
        }
        CorePredicate::All(values) | CorePredicate::Any(values) => values.iter().all(|value| {
            predicate_comparisons_admissible(core, pure_functions, value, available, depth + 1)
        }),
        CorePredicate::Compare { op, left, right } => {
            (!matches!(
                op,
                CompareOp::Lt | CompareOp::Le | CompareOp::Gt | CompareOp::Ge
            ) || ordered_operands_are_integers(core, pure_functions, [left, right], available))
                && expression_comparisons_admissible(
                    core,
                    pure_functions,
                    left,
                    available,
                    depth + 1,
                )
                && expression_comparisons_admissible(
                    core,
                    pure_functions,
                    right,
                    available,
                    depth + 1,
                )
        }
    }
}

fn expression_comparisons_admissible(
    core: &CoreModule,
    pure_functions: &[TargetPureFunctionFact],
    expression: &CoreExpr,
    available: &BTreeMap<&str, &LocalRef>,
    depth: usize,
) -> bool {
    if depth > MAX_CANONICAL_NESTING_DEPTH {
        return false;
    }
    let child = |value: &CoreExpr| {
        expression_comparisons_admissible(core, pure_functions, value, available, depth + 1)
    };
    match expression {
        CoreExpr::Local { .. } | CoreExpr::Const(_) => true,
        CoreExpr::Record { fields } => fields.values().all(child),
        CoreExpr::List { values } | CoreExpr::Call { args: values, .. } => values.iter().all(child),
        CoreExpr::Field { base, .. } => child(base),
        CoreExpr::If {
            predicate,
            then_value,
            else_value,
        } => {
            predicate_comparisons_admissible(core, pure_functions, predicate, available, depth + 1)
                && child(then_value)
                && child(else_value)
        }
    }
}

/// Operator admissibility for a v3 intent's basis, input constraints and
/// result, judged in the end-of-body scope (metadata references only the input).
fn intent_comparisons_admissible(
    core: &CoreModule,
    pure_functions: &[TargetPureFunctionFact],
    intent: &CoreIntent,
    available: &BTreeMap<&str, &LocalRef>,
) -> bool {
    intent.basis.as_ref().is_none_or(|basis| {
        expression_comparisons_admissible(core, pure_functions, basis, available, 0)
    }) && intent.input_constraints.iter().all(|constraint| {
        predicate_comparisons_admissible(core, pure_functions, &constraint.predicate, available, 0)
    }) && expression_comparisons_admissible(core, pure_functions, &intent.body.result, available, 0)
}

/// Operator admissibility for one v3 node's own predicates and expressions,
/// in the scope its expressions see; nested blocks are judged on their own.
fn node_comparisons_admissible<'a>(
    core: &CoreModule,
    pure_functions: &[TargetPureFunctionFact],
    node: &'a CoreNode,
    available: &BTreeMap<&'a str, &'a LocalRef>,
) -> bool {
    let expression = |value: &CoreExpr, scope: &BTreeMap<&str, &LocalRef>| {
        expression_comparisons_admissible(core, pure_functions, value, scope, 0)
    };
    match node {
        CoreNode::Let { value, .. } => expression(value, available),
        CoreNode::Require { predicate, arm } => {
            let (CoreRequireFailureArm::Terminal { reason }
            | CoreRequireFailureArm::ContinueObstructed { reason }) = arm;
            predicate_comparisons_admissible(core, pure_functions, predicate, available, 0)
                && reason
                    .payload
                    .values()
                    .all(|value| expression(value, available))
        }
        CoreNode::Effect {
            input,
            obstruction_map,
            ..
        } => {
            expression(input, available)
                && obstruction_map.values().all(|arm| {
                    let mut scope = available.clone();
                    scope.insert(arm.binder.id.as_str(), &arm.binder);
                    expression(&arm.value, &scope)
                })
        }
        CoreNode::ExternalActionRequest { .. } => true,
        CoreNode::For { iter, .. } => expression(iter, available),
        CoreNode::Branch { predicate, .. } => {
            predicate_comparisons_admissible(core, pure_functions, predicate, available, 0)
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
        // Lists are not comparison operands in v1.
        CoreExpr::List { .. } => None,
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
    if context.target_selection.execution_model == ExecutionModel::Structured {
        let mut counters = ControlCounters::default();
        state.execution_order = lower_structured_nodes(
            intent_name,
            &intent.body.nodes,
            None,
            context,
            &mut state,
            &mut counters,
            failures,
        );
    }
    for (node_index, node) in intent
        .body
        .nodes
        .iter()
        .enumerate()
        .filter(|_| context.target_selection.execution_model != ExecutionModel::Structured)
    {
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
                state
                    .execution_order
                    .push(TargetIrOrderEntry::Instruction(id.clone()));
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
        execution_order: (context.target_selection.execution_model
            != ExecutionModel::PreStepGuards)
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

#[derive(Default)]
struct ControlCounters {
    loops: usize,
    branches: usize,
}

/// Lower Core nodes into structured order entries. Control ids are numbered
/// per kind in source pre-order; instruction ids continue the intent tables.
fn lower_structured_nodes(
    intent_name: &str,
    nodes: &[CoreNode],
    top_index: Option<usize>,
    context: &TargetLoweringContext<'_>,
    state: &mut IntentLoweringState,
    counters: &mut ControlCounters,
    failures: &mut Vec<TargetLoweringFailure>,
) -> Vec<TargetIrOrderEntry> {
    let mut entries = Vec::new();
    for (index, node) in nodes.iter().enumerate() {
        let node_index = top_index.unwrap_or(index);
        if let Some(detail) = structured_node_refusal(node) {
            failures.push(TargetLoweringFailure {
                kind: TargetLoweringFailureKind::UnsupportedCoreNode,
                intent: Some(intent_name.to_owned()),
                node_index: Some(node_index),
                detail: detail.to_owned(),
            });
            continue;
        }
        let mut lower_block =
            |state: &mut IntentLoweringState, counters: &mut ControlCounters, block: &CoreBlock| {
                lower_structured_nodes(
                    intent_name,
                    &block.nodes,
                    Some(node_index),
                    context,
                    state,
                    counters,
                    failures,
                )
            };
        let entry = match node {
            CoreNode::For {
                binder,
                iter,
                bound,
                body,
            } => {
                let id = format!("{intent_name}.for.{}", counters.loops);
                counters.loops += 1;
                Some(TargetIrOrderEntry::For(TargetIrForEntry {
                    id,
                    binder: binder.clone(),
                    iterable: iter.clone(),
                    bound: bound.clone(),
                    body: lower_block(state, counters, body),
                }))
            }
            CoreNode::Branch {
                predicate,
                then_block,
                else_block,
                ..
            } => {
                let id = format!("{intent_name}.branch.{}", counters.branches);
                counters.branches += 1;
                let then_entries = lower_block(state, counters, then_block);
                let else_entries = lower_block(state, counters, else_block);
                Some(TargetIrOrderEntry::Branch(TargetIrBranchEntry {
                    id,
                    predicate: predicate.clone(),
                    then_entries,
                    else_entries,
                }))
            }
            CoreNode::Let { .. }
            | CoreNode::Effect { .. }
            | CoreNode::Require { .. }
            | CoreNode::ExternalActionRequest { .. } => lower_structured_instruction(
                intent_name,
                node_index,
                node,
                context,
                state,
                failures,
            ),
        };
        entries.extend(entry);
    }
    entries
}

/// The stable `UnsupportedCoreNode` detail for a Core node outside the
/// structured v3 contract, judged on the node itself (not its nested blocks).
fn structured_node_refusal(node: &CoreNode) -> Option<&'static str> {
    let null = CoreExpr::Const(CoreValue::Null);
    match node {
        CoreNode::ExternalActionRequest { .. } => Some("ordered_external_action_request"),
        CoreNode::Branch {
            binding: Some(_), ..
        } => Some("branch_binding"),
        // Target has no authenticated constant values, so it cannot prove that
        // a coordinate bound covers the iterable maximum; it fails closed.
        CoreNode::For {
            bound: CoreBound::Coordinate(_),
            ..
        } => Some("coordinate_loop_bound"),
        CoreNode::For { body, .. } => (body.result != null).then_some("control_block_result"),
        CoreNode::Branch {
            then_block,
            else_block,
            ..
        } => (then_block.result != null || else_block.result != null)
            .then_some("control_block_result"),
        CoreNode::Let { .. } | CoreNode::Effect { .. } | CoreNode::Require { .. } => None,
    }
}

fn lower_structured_instruction(
    intent_name: &str,
    node_index: usize,
    node: &CoreNode,
    context: &TargetLoweringContext<'_>,
    state: &mut IntentLoweringState,
    failures: &mut Vec<TargetLoweringFailure>,
) -> Option<TargetIrOrderEntry> {
    let before = (
        state.pure_bindings.len(),
        state.steps.len(),
        state.requirements.len(),
    );
    lower_node(intent_name, node_index, node, context, state, failures);
    let id = if state.pure_bindings.len() > before.0 {
        state.pure_bindings.last().map(|node| &node.id)
    } else if state.steps.len() > before.1 {
        state.steps.last().map(|node| &node.id)
    } else if state.requirements.len() > before.2 {
        state.requirements.last().map(|node| &node.id)
    } else {
        None
    };
    id.map(|id| TargetIrOrderEntry::Instruction(id.clone()))
}

struct TargetLoweringContext<'a> {
    target_selection: TargetSelection,
    obstruction_coordinates: &'a BTreeSet<&'a str>,
    effect_lowerings: &'a BTreeMap<&'a str, Vec<&'a TargetEffectLowering>>,
}

#[derive(Default)]
struct IntentLoweringState {
    execution_order: Vec<TargetIrOrderEntry>,
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
        CoreExpr::List { values } => values
            .iter()
            .any(|value| expr_references_step_output(value, step_outputs)),
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

    use super::{
        authenticated_named_type_closure_matches, structured_encoding_failure, TargetIrArtifact,
        TargetIrIntent, TargetIrOrderEntry, TargetLoweringFailureKind, ECHO_DPO_TARGET_PROFILE,
        ECHO_STRUCTURED_SPAN_IR_DOMAIN,
    };
    use crate::core_ir::{
        CoreBudget, CoreExpr, CoreModule, CoreType, CoreValue, ResourceRef, CORE_API_VERSION,
    };

    fn structured_artifact(order: Vec<TargetIrOrderEntry>) -> TargetIrArtifact {
        TargetIrArtifact {
            domain: ECHO_STRUCTURED_SPAN_IR_DOMAIN.to_owned(),
            target_profile: ResourceRef {
                coordinate: ECHO_DPO_TARGET_PROFILE.to_owned(),
                digest: Some(format!("sha256:{}", "1".repeat(64))),
            },
            source_core_coordinate: "examples.guard@1".to_owned(),
            semantic_closure: None,
            intents: BTreeMap::from([(
                "t".to_owned(),
                TargetIrIntent {
                    operation_profile: "continuum.profile.write/v1".to_owned(),
                    execution_order: Some(order),
                    basis: None,
                    input_constraints: Vec::new(),
                    core_evaluation_budget: CoreBudget {
                        max_steps: 8,
                        max_allocated_bytes: 1024,
                        max_output_bytes: 256,
                    },
                    pure_bindings: Vec::new(),
                    requirements: Vec::new(),
                    steps: Vec::new(),
                    external_action_requests: Vec::new(),
                    result: CoreExpr::Const(CoreValue::Null),
                },
            )]),
        }
    }

    /// Lowering must never return `Lowered` for a v3 artifact its own encoder
    /// refuses, whatever the encoder's reason.
    #[test]
    fn structured_encoding_guard_refuses_every_encoder_refusal() {
        let encodable = structured_artifact(Vec::new());
        crate::encode_target_ir_artifact(&encodable).expect("control artifact encodes");
        assert_eq!(structured_encoding_failure(&encodable), None);

        // An order entry that names no table instruction.
        let unencodable =
            structured_artifact(vec![TargetIrOrderEntry::Instruction("t.step.9".to_owned())]);
        assert_eq!(
            crate::encode_target_ir_artifact(&unencodable)
                .expect_err("dangling id")
                .kind(),
            crate::CanonicalErrorKind::UnsupportedValue
        );
        let failure =
            structured_encoding_failure(&unencodable).expect("guard refuses the artifact");
        assert_eq!(failure.kind, TargetLoweringFailureKind::InvalidCoreIdentity);
        assert_eq!(failure.detail, "structured_artifact_unencodable");
        assert_eq!(failure.intent.as_deref(), Some("t"));
    }

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

    /// Isolating the refused intent copies the shared header a constant number
    /// of times and builds each intent value a constant number of times, so the
    /// work is linear in the artifact size however many intents share a large
    /// header. Counted at the encoder's real copy and build sites.
    #[test]
    fn encoder_refusal_isolation_does_shared_work_once() {
        const INTENTS: usize = 64;
        let coordinate = "c".repeat(64 * 1024);
        let mut template = structured_artifact(Vec::new());
        template.source_core_coordinate.clone_from(&coordinate);
        let valid = template.intents["t"].clone();
        let mut dangling = valid.clone();
        dangling.execution_order =
            Some(vec![TargetIrOrderEntry::Instruction("t.step.9".to_owned())]);
        for failing in [0, INTENTS - 1] {
            let mut artifact = template.clone();
            artifact.intents = (0..INTENTS)
                .map(|index| {
                    let intent = if index == failing {
                        dangling.clone()
                    } else {
                        valid.clone()
                    };
                    (format!("intent{index:03}"), intent)
                })
                .collect();
            crate::canonical::target_ir_work::take();
            let failure = structured_encoding_failure(&artifact).expect("guard refuses");
            let expected = format!("intent{failing:03}");
            assert_eq!(failure.intent.as_deref(), Some(expected.as_str()));
            let (header_bytes, intent_values) = crate::canonical::target_ir_work::take();
            assert!(
                header_bytes <= 2 * coordinate.len(),
                "failing intent {failing}: the shared coordinate was copied {header_bytes} bytes"
            );
            assert!(
                intent_values <= 2 * INTENTS,
                "failing intent {failing}: {intent_values} intent values built"
            );
        }
    }
}

//! Deterministic application-owned lawpack authoring.
//!
//! This module accepts a typed review model, constructs canonical lawpack
//! artifacts, derives every local identity, and sends the exact emitted bytes
//! back through the public lawpack and adapter validators. It performs no I/O,
//! provider invocation, target execution, or application compilation.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Component, Path};

use serde::Deserialize;
use serde_json::Value;

use crate::canonical::{digest_canonical_artifact, encode_canonical_cbor, CanonicalValue};
use crate::lawpack::{
    decode_lawpack_bundle, validate_lawpack_dependency_graph, LawpackValidationFailure,
    ValidatedLawpackBundle, LAWPACK_API_VERSION,
};
use crate::lawpack_adapter::{decode_lawpack_adapter, LawpackAdapterFailure};

/// Versioned review schema accepted by the public authoring boundary.
pub const LAWPACK_AUTHORING_API_VERSION: &str = "edict.lawpack-authoring/v1";

/// Maximum JSON container depth accepted by lawpack authoring.
///
/// The lower authoring-specific boundary leaves stack headroom for the typed
/// lawpack validators that consume the resulting canonical value. The general
/// canonical-CBOR encoder and decoder retain their independent 128-level limit.
pub const MAX_LAWPACK_AUTHORING_VALUE_NESTING_DEPTH: usize = 48;

const LAWPACK_OUTPUT_INDEX_PATH: &str = "edict.lawpack-output.json";
const EXPORT_VALUE_CANONICAL_ENCLOSING_DEPTH: usize = 3;
const MAX_PORTABLE_ARTIFACT_COMPONENT_BYTES: usize = 255;
const MAX_PORTABLE_RELATIVE_ARTIFACT_BYTES: usize = 1024;
const DIGEST_SIDECAR_EXTENSION_GROWTH: usize = ".sha256".len() - ".cbor".len();
const MAX_PORTABLE_OUTPUT_COMPONENT_BYTES: usize =
    MAX_PORTABLE_ARTIFACT_COMPONENT_BYTES - DIGEST_SIDECAR_EXTENSION_GROWTH;
const MAX_PORTABLE_RELATIVE_OUTPUT_BYTES: usize =
    MAX_PORTABLE_RELATIVE_ARTIFACT_BYTES - DIGEST_SIDECAR_EXTENSION_GROWTH;

/// Stable lawpack-authoring failure categories.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum LawpackAuthoringFailureKind {
    /// The versioned authoring document is malformed or internally inconsistent.
    InvalidDefinition,
    /// A supplied digest is not lowercase reviewed SHA-256.
    InvalidDigest,
    /// A canonical value cannot be represented by Edict's canonical profile.
    InvalidCanonicalValue,
    /// A local-resource reference does not resolve exactly once.
    MissingLocalResource,
    /// Two semantic inputs claim the same identity or output path.
    DuplicateIdentity,
    /// An authored output path is absolute, empty, or escapes its output root.
    InvalidOutputPath,
    /// Canonical artifact construction failed.
    EncodingFailed,
    /// The emitted manifest or export surface failed its owning public decoder.
    InvalidLawpack,
    /// An emitted adapter failed its owning public decoder.
    InvalidAdapter,
    /// A declared root dependency was not supplied in the exact closure.
    MissingDependency,
    /// A supplied dependency does not match the declared digest pin.
    DependencyDigestMismatch,
    /// The supplied dependency set is invalid or contains disconnected bundles.
    InvalidDependencyClosure,
}

/// One structured authoring failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LawpackAuthoringFailure {
    /// Stable machine-facing category.
    pub kind: LawpackAuthoringFailureKind,
    /// Authoring-document path or artifact role that failed.
    pub path: String,
    /// Obligation that was not met.
    pub obligation: String,
    /// Exact lower-level validator failure, when authored bytes were rejected.
    pub cause: Option<LawpackAuthoringFailureCause>,
}

/// Typed lower-level cause retained across the authoring boundary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LawpackAuthoringFailureCause {
    /// Failure returned by the canonical lawpack or dependency validator.
    Lawpack(LawpackValidationFailure),
    /// Failure returned by the canonical target-adapter validator.
    Adapter(LawpackAdapterFailure),
}

/// Exact external resource identity supplied by the application.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LawpackAuthoringPinnedResource {
    /// Resource coordinate or ABI identity.
    pub id: String,
    /// Lowercase reviewed SHA-256 digest.
    pub digest: String,
}

/// Reference to either an exact external resource or one locally authored resource.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(untagged)]
pub enum LawpackAuthoringResourceRef {
    /// Exact resource whose bytes are owned outside this authoring invocation.
    External(LawpackAuthoringPinnedResource),
    /// Alias of one entry in `localResources`.
    Local(LawpackAuthoringLocalReference),
}

/// Alias of one locally authored resource.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LawpackAuthoringLocalReference {
    /// Local resource name.
    pub local: String,
}

/// One canonical local resource whose identity is derived by Edict.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LawpackAuthoringLocalResource {
    /// Definition-local alias used by resource references.
    pub name: String,
    /// Digest domain and public resource coordinate.
    pub coordinate: String,
    /// Relative canonical-CBOR output path.
    pub output: String,
    /// Reviewable JSON value converted to canonical CBOR by Edict.
    pub value: Value,
}

/// One exact root dependency edge.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LawpackAuthoringDependency {
    /// Dependency lawpack id.
    pub id: String,
    /// Dependency lawpack version.
    pub version: String,
    /// Expected manifest identity corroborated against the supplied bundle.
    pub digest: String,
}

/// Bounded executable component reference.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LawpackAuthoringExecutableComponent {
    /// Exact component bytes.
    pub component: LawpackAuthoringResourceRef,
    /// Exact sandbox contract.
    pub sandbox: LawpackAuthoringResourceRef,
    /// Exact fuel model.
    pub fuel_model: LawpackAuthoringResourceRef,
}

/// Declarative or bounded executable verifier metadata.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LawpackAuthoringVerifier {
    /// Authority-free declarative verifier rules.
    Declarative {
        /// Exact or locally authored ruleset.
        ruleset: LawpackAuthoringResourceRef,
    },
    /// Bounded executable verifier.
    Executable {
        /// Exact executable component closure.
        executable: LawpackAuthoringExecutableComponent,
    },
}

#[derive(Deserialize)]
#[serde(
    tag = "class",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
enum LawpackAuthoringVerifierWire {
    Declarative {
        ruleset: LawpackAuthoringResourceRef,
    },
    Executable {
        component: LawpackAuthoringResourceRef,
        sandbox: LawpackAuthoringResourceRef,
        fuel_model: LawpackAuthoringResourceRef,
    },
}

impl<'de> Deserialize<'de> for LawpackAuthoringVerifier {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        Ok(
            match LawpackAuthoringVerifierWire::deserialize(deserializer)? {
                LawpackAuthoringVerifierWire::Declarative { ruleset } => {
                    Self::Declarative { ruleset }
                }
                LawpackAuthoringVerifierWire::Executable {
                    component,
                    sandbox,
                    fuel_model,
                } => Self::Executable {
                    executable: LawpackAuthoringExecutableComponent {
                        component,
                        sandbox,
                        fuel_model,
                    },
                },
            },
        )
    }
}

/// One exported bounded type.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LawpackAuthoringType {
    /// Canonical export coordinate.
    pub coordinate: String,
    /// Bounded Core type definition.
    pub definition: String,
}

/// One typed exported constant.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LawpackAuthoringConstant {
    /// Canonical export coordinate.
    pub coordinate: String,
    /// Bounded Core type reference.
    #[serde(rename = "type")]
    pub ty: String,
    /// Hash-significant review value.
    pub value: Value,
}

/// Pure helper determinism classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum LawpackAuthoringDeterminismClass {
    /// Total helper without a diagnostic result.
    Total,
    /// Total helper whose result may carry a typed diagnostic.
    TotalWithTypedDiagnostic,
}

/// One exported pure helper and its exact implementation.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(
    tag = "source",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum LawpackAuthoringPureFunction {
    /// Inline Edict pure-Core body.
    Edict {
        /// Canonical helper coordinate.
        coordinate: String,
        /// Bounded type parameters.
        #[serde(default)]
        type_parameters: Vec<String>,
        /// Bounded parameter types.
        #[serde(default)]
        parameter_types: Vec<String>,
        /// Bounded return type.
        return_type: String,
        /// Exported cost obligation.
        cost_template: String,
        /// Totality contract.
        determinism_class: LawpackAuthoringDeterminismClass,
        /// Review rendering of the closed pure `core-fn-body` shape.
        body: Value,
    },
    /// Digest-locked bounded component implementation.
    Component {
        /// Canonical helper coordinate.
        coordinate: String,
        /// Bounded type parameters.
        #[serde(default)]
        type_parameters: Vec<String>,
        /// Bounded parameter types.
        #[serde(default)]
        parameter_types: Vec<String>,
        /// Bounded return type.
        return_type: String,
        /// Exported cost obligation.
        cost_template: String,
        /// Totality contract.
        determinism_class: LawpackAuthoringDeterminismClass,
        /// Exact component closure.
        implementation: LawpackAuthoringExecutableComponent,
    },
}

/// Semantic-effect execution classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum LawpackAuthoringExecutionClass {
    /// Effect participates only in proof.
    ProofOnly,
    /// Effect requires a runtime adapter.
    Runtime,
}

/// Advisory semantic-effect classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum LawpackAuthoringEffectKind {
    /// Read effect.
    Read,
    /// Create effect.
    Create,
    /// Ensure effect.
    Ensure,
    /// Replace effect.
    Replace,
    /// Delete effect.
    Delete,
    /// Append effect.
    Append,
    /// Reduce effect.
    Reduce,
    /// Semantic emission.
    SemanticEmit,
    /// Application-defined classification.
    Custom,
}

/// Authority owner for a failure or obstruction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum LawpackAuthoringAuthorityClass {
    /// Adapter may map the failure into a domain obstruction.
    DomainMappable,
    /// Participant owns the obstruction.
    ParticipantOwned,
    /// Integrity layer owns the fault.
    IntegrityFault,
    /// Resource boundary owns the fault.
    ResourceFault,
    /// Internal implementation owns the fault.
    InternalFault,
}

/// One named semantic-effect failure.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LawpackAuthoringEffectFailure {
    /// Failure authority owner.
    pub authority_class: LawpackAuthoringAuthorityClass,
    /// Bounded payload type.
    pub payload_type: String,
}

/// One exported semantic effect.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LawpackAuthoringEffect {
    /// Canonical effect coordinate.
    pub coordinate: String,
    /// Bounded type parameters.
    #[serde(default)]
    pub type_parameters: Vec<String>,
    /// Bounded input type.
    pub input_type: String,
    /// Bounded output type.
    pub output_type: String,
    /// Proof-only or runtime execution class.
    pub execution_class: LawpackAuthoringExecutionClass,
    /// Advisory effect kind.
    pub effect_kind_hint: LawpackAuthoringEffectKind,
    /// Exact footprint obligation.
    pub footprint_obligation: String,
    /// Exact cost obligation.
    pub cost_obligation: String,
    /// Named low-level failures.
    #[serde(default)]
    pub effect_failures: BTreeMap<String, LawpackAuthoringEffectFailure>,
    /// Whether guarded execution is supported.
    pub guard_support: bool,
}

/// One exported typed obstruction.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LawpackAuthoringObstruction {
    /// Canonical obstruction coordinate.
    pub coordinate: String,
    /// Authority owner.
    pub authority_class: LawpackAuthoringAuthorityClass,
    /// Bounded payload schema.
    pub payload_schema: String,
}

/// Optional bounded aperture requirement.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
pub enum LawpackAuthoringApertureRequirement {
    /// Concrete footprint ceiling.
    FootprintCeiling {
        /// Exported ceiling reference.
        reference: String,
    },
    /// Abstract footprint proof obligation.
    AbstractFootprintObligation {
        /// Exported obligation reference.
        reference: String,
    },
}

/// Runtime-neutral optic template.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LawpackAuthoringOpticTemplate {
    /// Optic kind.
    pub optic_kind: String,
    /// Boundary kind.
    pub boundary_kind: String,
    /// Support policy coordinate.
    pub support_policy: String,
    /// Loss disposition coordinate.
    pub loss_disposition: String,
    /// Optional basis template.
    pub basis_template: Option<String>,
    /// Optional aperture requirement.
    pub aperture_requirement: Option<LawpackAuthoringApertureRequirement>,
}

/// One exported operation profile.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LawpackAuthoringOperationProfile {
    /// Runtime-neutral optic template.
    pub optic_template: LawpackAuthoringOpticTemplate,
    /// Effect predicate coordinate.
    pub effect_predicate: String,
}

/// Complete authored export surface.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LawpackAuthoringExports {
    /// Bounded exported types.
    #[serde(default)]
    pub types: Vec<LawpackAuthoringType>,
    /// Typed exported constants.
    #[serde(default)]
    pub constants: Vec<LawpackAuthoringConstant>,
    /// Pure helpers.
    #[serde(default)]
    pub pure_functions: Vec<LawpackAuthoringPureFunction>,
    /// Semantic effects.
    #[serde(default)]
    pub effects: Vec<LawpackAuthoringEffect>,
    /// Typed obstructions.
    #[serde(default)]
    pub obstructions: Vec<LawpackAuthoringObstruction>,
    /// Operation profiles keyed by canonical coordinate.
    #[serde(default)]
    pub operation_profiles: BTreeMap<String, LawpackAuthoringOperationProfile>,
}

/// One operation-profile discharge in a direct adapter.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LawpackAuthoringAdapterOperationProfile {
    /// Selected Core profile.
    pub core: String,
    /// Semantic effects discharged by this profile.
    #[serde(default)]
    pub semantic_effects: Vec<String>,
    /// Exact request-only budget obligation.
    pub budget_obligation: Option<String>,
    /// Exact request-only target configuration.
    pub target_configuration: Option<LawpackAuthoringResourceRef>,
}

/// One runtime-effect discharge in a direct adapter.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LawpackAuthoringAdapterEffect {
    /// Generic target intrinsic coordinate.
    pub target_intrinsic: String,
    /// Exact target configuration.
    pub target_configuration: LawpackAuthoringResourceRef,
    /// Runtime-neutral write class.
    pub write_class: String,
    /// Footprint obligation matching the exported effect.
    pub footprint_obligation: String,
    /// Cost obligation matching the exported effect.
    pub cost_obligation: String,
    /// Complete named failure mapping.
    #[serde(default)]
    pub failure_mappings: BTreeMap<String, String>,
}

/// One exact adapter budget.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LawpackAuthoringAdapterBudget {
    /// Maximum Core steps.
    pub max_steps: u64,
    /// Maximum allocated bytes.
    pub max_allocated_bytes: u64,
    /// Maximum output bytes.
    pub max_output_bytes: u64,
}

/// One authored direct adapter and its selected target identities.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LawpackAuthoringAdapter {
    /// Adapter resource coordinate.
    pub coordinate: String,
    /// Relative canonical-CBOR output path.
    pub output: String,
    /// Exact accepted target profile.
    pub accepted_target_profile: LawpackAuthoringPinnedResource,
    /// Exact accepted Target IR ABI.
    pub accepted_target_ir: LawpackAuthoringPinnedResource,
    /// Operation-profile discharges.
    #[serde(default)]
    pub operation_profiles: BTreeMap<String, LawpackAuthoringAdapterOperationProfile>,
    /// Runtime-effect implementations.
    #[serde(default)]
    pub effect_implementations: BTreeMap<String, LawpackAuthoringAdapterEffect>,
    /// Exact cost obligations.
    #[serde(default)]
    pub budgets: BTreeMap<String, LawpackAuthoringAdapterBudget>,
}

/// Complete semantic input to one deterministic authoring invocation.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LawpackAuthoringDefinition {
    /// Authoring schema identifier.
    pub schema: String,
    /// Lawpack id.
    pub id: String,
    /// Lawpack version.
    pub version: String,
    /// Accepted Core ABI identifiers.
    pub accepted_core_abi: Vec<String>,
    /// Exact direct dependency pins.
    #[serde(default)]
    pub dependencies: Vec<LawpackAuthoringDependency>,
    /// Coordinate used to identify the generated export surface.
    pub exports_coordinate: String,
    /// Application-authored export semantics.
    pub exports: LawpackAuthoringExports,
    /// Direct target adapters.
    #[serde(default)]
    pub target_adapters: Vec<LawpackAuthoringAdapter>,
    /// Optional bounded helper component.
    pub helper_component: Option<LawpackAuthoringExecutableComponent>,
    /// Verifier metadata.
    pub verifier: LawpackAuthoringVerifier,
    /// Compatibility resource.
    pub compatibility: LawpackAuthoringResourceRef,
    /// Conformance fixture corpus resource.
    pub conformance_fixture_corpus: LawpackAuthoringResourceRef,
    /// Locally authored canonical resources.
    #[serde(default)]
    pub local_resources: Vec<LawpackAuthoringLocalResource>,
}

/// Semantic role of one emitted file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum LawpackArtifactKind {
    /// Canonical lawpack manifest.
    Manifest,
    /// Manifest digest sidecar.
    ManifestDigest,
    /// Canonical export surface.
    Exports,
    /// Export-surface digest sidecar.
    ExportsDigest,
    /// Canonical direct target adapter.
    Adapter,
    /// Adapter digest sidecar.
    AdapterDigest,
    /// Canonical locally authored resource.
    LocalResource,
    /// Local-resource digest sidecar.
    LocalResourceDigest,
}

/// One exact emitted file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LawpackAuthoredArtifact {
    kind: LawpackArtifactKind,
    path: String,
    coordinate: String,
    bytes: Vec<u8>,
    digest: String,
}

impl LawpackAuthoredArtifact {
    /// Artifact role.
    #[must_use]
    pub const fn kind(&self) -> LawpackArtifactKind {
        self.kind
    }

    /// Relative publication path.
    #[must_use]
    pub fn path(&self) -> &str {
        &self.path
    }

    /// Identity domain or resource coordinate.
    #[must_use]
    pub fn coordinate(&self) -> &str {
        &self.coordinate
    }

    /// Exact file bytes.
    #[must_use]
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// Identity of the owning canonical artifact.
    #[must_use]
    pub fn digest(&self) -> &str {
        &self.digest
    }
}

/// Complete deterministic artifact set returned by one authoring invocation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LawpackAuthoredArtifactSet {
    artifacts: Vec<LawpackAuthoredArtifact>,
}

impl LawpackAuthoredArtifactSet {
    /// Ordered emitted files.
    #[must_use]
    pub fn artifacts(&self) -> &[LawpackAuthoredArtifact] {
        &self.artifacts
    }

    /// First artifact with the requested semantic role.
    #[must_use]
    pub fn artifact(&self, kind: LawpackArtifactKind) -> Option<&LawpackAuthoredArtifact> {
        self.artifacts.iter().find(|artifact| artifact.kind == kind)
    }
}

#[derive(Debug, Clone)]
struct BuiltResource {
    coordinate: String,
    output: String,
    bytes: Vec<u8>,
    digest: [u8; 32],
    digest_review: String,
}

#[derive(Debug, Clone)]
struct BuiltAdapter {
    coordinate: String,
    output: String,
    target_profile: String,
    bytes: Vec<u8>,
    digest: [u8; 32],
    digest_review: String,
}

/// Construct, validate, and return one complete canonical lawpack artifact set.
///
/// The function performs no I/O. `dependencies` must contain the complete exact
/// closure referenced by the definition, including transitive bundles.
///
/// # Errors
///
/// Returns stable structured failures for invalid definitions, resource or
/// dependency substitution, canonical construction failures, invalid emitted
/// lawpack/adapter bytes, and incomplete or disconnected dependency closures.
pub fn author_lawpack(
    definition: &LawpackAuthoringDefinition,
    dependencies: &[ValidatedLawpackBundle],
) -> Result<LawpackAuthoredArtifactSet, Vec<LawpackAuthoringFailure>> {
    validate_definition_header(definition)?;
    preflight_lawpack_authoring_paths(definition)?;
    validate_dependencies(definition, dependencies)?;
    let resources = build_local_resources(&definition.local_resources)?;
    let exports_value = exports_value(&definition.exports, &resources)?;
    let exports = build_resource(
        &definition.exports_coordinate,
        "exports.cbor",
        &exports_value,
        "exports",
    )?;
    let adapters = build_adapters(&definition.target_adapters, &resources)?;
    let manifest_value = manifest_value(definition, &exports, &adapters, &resources)?;
    let manifest = build_resource(
        LAWPACK_API_VERSION,
        "manifest.cbor",
        &manifest_value,
        "manifest",
    )?;

    let bundle = decode_lawpack_bundle(&manifest.bytes, &exports.bytes).map_err(|failures| {
        wrap_lawpack_failures(LawpackAuthoringFailureKind::InvalidLawpack, failures)
    })?;
    for adapter in &adapters {
        decode_lawpack_adapter(&bundle, &adapter.target_profile, &adapter.bytes)
            .map_err(|failures| wrap_adapter_failures(&adapter.coordinate, failures))?;
    }
    validate_complete_closure(&bundle, dependencies)?;

    let mut artifacts = Vec::new();
    push_artifact_pair(
        &mut artifacts,
        LawpackArtifactKind::Manifest,
        LawpackArtifactKind::ManifestDigest,
        &manifest,
    )?;
    push_artifact_pair(
        &mut artifacts,
        LawpackArtifactKind::Exports,
        LawpackArtifactKind::ExportsDigest,
        &exports,
    )?;
    for resource in resources.values() {
        push_artifact_pair(
            &mut artifacts,
            LawpackArtifactKind::LocalResource,
            LawpackArtifactKind::LocalResourceDigest,
            resource,
        )?;
    }
    for adapter in &adapters {
        push_artifact_pair(
            &mut artifacts,
            LawpackArtifactKind::Adapter,
            LawpackArtifactKind::AdapterDigest,
            &BuiltResource {
                coordinate: adapter.coordinate.clone(),
                output: adapter.output.clone(),
                bytes: adapter.bytes.clone(),
                digest: adapter.digest,
                digest_review: adapter.digest_review.clone(),
            },
        )?;
    }
    validate_artifact_paths(&artifacts)?;
    Ok(LawpackAuthoredArtifactSet { artifacts })
}

/// Validate derivable application-authored artifact paths and coordinates without
/// dependency or filesystem I/O.
///
/// # Errors
///
/// Returns the same structured output-path failures used by [`author_lawpack`].
pub fn preflight_lawpack_authoring_paths(
    definition: &LawpackAuthoringDefinition,
) -> Result<(), Vec<LawpackAuthoringFailure>> {
    let mut artifacts = Vec::new();
    push_preflight_artifact_pair(
        &mut artifacts,
        "manifest.cbor",
        LawpackArtifactKind::Manifest,
        LAWPACK_API_VERSION,
        0,
    )?;
    push_preflight_artifact_pair(
        &mut artifacts,
        "exports.cbor",
        LawpackArtifactKind::Exports,
        &definition.exports_coordinate,
        1,
    )?;
    for (index, resource) in definition.local_resources.iter().enumerate() {
        push_preflight_artifact_pair(
            &mut artifacts,
            &resource.output,
            LawpackArtifactKind::LocalResource,
            &resource.coordinate,
            index + 2,
        )?;
    }
    for (index, adapter) in definition.target_adapters.iter().enumerate() {
        push_preflight_artifact_pair(
            &mut artifacts,
            &adapter.output,
            LawpackArtifactKind::Adapter,
            &adapter.coordinate,
            definition.local_resources.len() + index + 2,
        )?;
    }
    validate_artifact_paths(&artifacts)
}

fn push_preflight_artifact_pair(
    artifacts: &mut Vec<LawpackAuthoredArtifact>,
    output: &str,
    kind: LawpackArtifactKind,
    coordinate: &str,
    index: usize,
) -> Result<(), Vec<LawpackAuthoringFailure>> {
    validate_output_path(output, &format!("artifactPaths.{index}"))?;
    let sidecar = digest_sidecar_path(output)?;
    artifacts.push(LawpackAuthoredArtifact {
        kind,
        path: output.to_owned(),
        coordinate: coordinate.to_owned(),
        bytes: Vec::new(),
        digest: String::new(),
    });
    artifacts.push(LawpackAuthoredArtifact {
        kind: match kind {
            LawpackArtifactKind::Manifest => LawpackArtifactKind::ManifestDigest,
            LawpackArtifactKind::Exports => LawpackArtifactKind::ExportsDigest,
            LawpackArtifactKind::Adapter => LawpackArtifactKind::AdapterDigest,
            LawpackArtifactKind::LocalResource => LawpackArtifactKind::LocalResourceDigest,
            LawpackArtifactKind::ManifestDigest
            | LawpackArtifactKind::ExportsDigest
            | LawpackArtifactKind::AdapterDigest
            | LawpackArtifactKind::LocalResourceDigest => {
                return Err(one(failure(
                    LawpackAuthoringFailureKind::InvalidDefinition,
                    output,
                    "a canonical artifact kind with one digest sidecar",
                )));
            }
        },
        path: sidecar,
        coordinate: coordinate.to_owned(),
        bytes: Vec::new(),
        digest: String::new(),
    });
    Ok(())
}

fn validate_definition_header(
    definition: &LawpackAuthoringDefinition,
) -> Result<(), Vec<LawpackAuthoringFailure>> {
    if definition.schema != LAWPACK_AUTHORING_API_VERSION {
        return Err(one(failure(
            LawpackAuthoringFailureKind::InvalidDefinition,
            "schema",
            format!("exact schema `{LAWPACK_AUTHORING_API_VERSION}`"),
        )));
    }
    if definition.id.is_empty()
        || definition.version.is_empty()
        || definition.exports_coordinate.is_empty()
        || definition.accepted_core_abi.is_empty()
        || definition.accepted_core_abi.iter().any(String::is_empty)
    {
        return Err(one(failure(
            LawpackAuthoringFailureKind::InvalidDefinition,
            "lawpack",
            "non-empty id, version, exports coordinate, and accepted Core ABI",
        )));
    }
    Ok(())
}

fn validate_dependencies(
    definition: &LawpackAuthoringDefinition,
    supplied: &[ValidatedLawpackBundle],
) -> Result<(), Vec<LawpackAuthoringFailure>> {
    let mut declared = BTreeSet::new();
    for dependency in &definition.dependencies {
        let key = (dependency.id.as_str(), dependency.version.as_str());
        if !declared.insert(key) {
            return Err(one(failure(
                LawpackAuthoringFailureKind::DuplicateIdentity,
                "dependencies",
                format!(
                    "one dependency edge for {}@{}",
                    dependency.id, dependency.version
                ),
            )));
        }
        parse_digest(&dependency.digest, "dependencies.digest")?;
        let resolved = supplied.iter().find(|bundle| {
            bundle.manifest().id == dependency.id && bundle.manifest().version == dependency.version
        });
        let Some(resolved) = resolved else {
            return Err(one(failure(
                LawpackAuthoringFailureKind::MissingDependency,
                format!("dependencies.{}@{}", dependency.id, dependency.version),
                "the exact declared dependency bundle in the supplied closure",
            )));
        };
        if resolved.manifest_digest_review_string() != dependency.digest {
            return Err(one(failure(
                LawpackAuthoringFailureKind::DependencyDigestMismatch,
                format!(
                    "dependencies.{}@{}.digest",
                    dependency.id, dependency.version
                ),
                "the digest of the exact supplied dependency manifest",
            )));
        }
    }
    Ok(())
}

fn validate_complete_closure(
    root: &ValidatedLawpackBundle,
    supplied: &[ValidatedLawpackBundle],
) -> Result<(), Vec<LawpackAuthoringFailure>> {
    let mut closure = Vec::with_capacity(supplied.len() + 1);
    closure.push(root.clone());
    closure.extend_from_slice(supplied);
    validate_lawpack_dependency_graph(&closure).map_err(|failures| {
        wrap_lawpack_failures(
            LawpackAuthoringFailureKind::InvalidDependencyClosure,
            failures,
        )
    })?;

    let mut by_identity = BTreeMap::new();
    for bundle in &closure {
        let identity = (
            bundle.manifest().id.as_str(),
            bundle.manifest().version.as_str(),
        );
        if by_identity.insert(identity, bundle).is_some() {
            return Err(one(failure(
                LawpackAuthoringFailureKind::InvalidDependencyClosure,
                "dependencies",
                "each supplied dependency identity exactly once",
            )));
        }
    }
    let mut reachable = BTreeSet::new();
    let mut pending = vec![(
        root.manifest().id.as_str(),
        root.manifest().version.as_str(),
    )];
    while let Some(identity) = pending.pop() {
        if !reachable.insert(identity) {
            continue;
        }
        let Some(bundle) = by_identity.get(&identity) else {
            return Err(one(failure(
                LawpackAuthoringFailureKind::MissingDependency,
                format!("dependencies.{}@{}", identity.0, identity.1),
                "every transitively reachable dependency in the supplied closure",
            )));
        };
        for dependency in &bundle.manifest().dependencies {
            pending.push((dependency.id.as_str(), dependency.version.as_str()));
        }
    }
    if by_identity
        .keys()
        .any(|identity| !reachable.contains(identity))
    {
        return Err(one(failure(
            LawpackAuthoringFailureKind::InvalidDependencyClosure,
            "dependencies",
            "every supplied bundle reachable from the authored root",
        )));
    }
    Ok(())
}

fn build_local_resources(
    definitions: &[LawpackAuthoringLocalResource],
) -> Result<BTreeMap<String, BuiltResource>, Vec<LawpackAuthoringFailure>> {
    let mut resources = BTreeMap::new();
    let mut coordinates = BTreeSet::new();
    for (index, definition) in definitions.iter().enumerate() {
        if definition.name.is_empty() || definition.coordinate.is_empty() {
            return Err(one(failure(
                LawpackAuthoringFailureKind::InvalidDefinition,
                format!("localResources.{index}"),
                "non-empty local resource name and coordinate",
            )));
        }
        if resources.contains_key(&definition.name)
            || !coordinates.insert(definition.coordinate.as_str())
        {
            return Err(one(failure(
                LawpackAuthoringFailureKind::DuplicateIdentity,
                format!("localResources.{index}"),
                "unique local resource name and coordinate",
            )));
        }
        let value =
            canonical_json_value(&definition.value, &format!("localResources.{index}.value"))?;
        let built = build_resource(
            &definition.coordinate,
            &definition.output,
            &value,
            &format!("localResources.{index}"),
        )?;
        resources.insert(definition.name.clone(), built);
    }
    Ok(resources)
}

fn build_adapters(
    definitions: &[LawpackAuthoringAdapter],
    resources: &BTreeMap<String, BuiltResource>,
) -> Result<Vec<BuiltAdapter>, Vec<LawpackAuthoringFailure>> {
    let mut adapters = Vec::with_capacity(definitions.len());
    let mut coordinates = BTreeSet::new();
    for (index, definition) in definitions.iter().enumerate() {
        if !coordinates.insert(definition.coordinate.as_str()) {
            return Err(one(failure(
                LawpackAuthoringFailureKind::DuplicateIdentity,
                format!("targetAdapters.{index}.coordinate"),
                "unique adapter coordinate",
            )));
        }
        parse_digest(
            &definition.accepted_target_profile.digest,
            &format!("targetAdapters.{index}.acceptedTargetProfile.digest"),
        )?;
        parse_digest(
            &definition.accepted_target_ir.digest,
            &format!("targetAdapters.{index}.acceptedTargetIr.digest"),
        )?;
        let value = adapter_value(definition, resources, index)?;
        let built = build_resource(
            &definition.coordinate,
            &definition.output,
            &value,
            &format!("targetAdapters.{index}"),
        )?;
        adapters.push(BuiltAdapter {
            coordinate: built.coordinate,
            output: built.output,
            target_profile: definition.accepted_target_profile.id.clone(),
            bytes: built.bytes,
            digest: built.digest,
            digest_review: built.digest_review,
        });
    }
    Ok(adapters)
}

fn build_resource(
    coordinate: &str,
    output: &str,
    value: &CanonicalValue,
    path: &str,
) -> Result<BuiltResource, Vec<LawpackAuthoringFailure>> {
    if coordinate.is_empty() {
        return Err(one(failure(
            LawpackAuthoringFailureKind::InvalidDefinition,
            path,
            "non-empty artifact coordinate",
        )));
    }
    validate_output_path(output, path)?;
    let bytes = encode_canonical_cbor(value).map_err(|error| {
        one(failure(
            LawpackAuthoringFailureKind::EncodingFailed,
            path,
            format!("canonical CBOR encoding: {error}"),
        ))
    })?;
    let digest = digest_canonical_artifact(coordinate, &bytes).map_err(|error| {
        one(failure(
            LawpackAuthoringFailureKind::EncodingFailed,
            path,
            format!("domain-framed canonical identity: {error}"),
        ))
    })?;
    Ok(BuiltResource {
        coordinate: coordinate.to_owned(),
        output: output.to_owned(),
        bytes,
        digest: *digest.bytes(),
        digest_review: digest.to_review_string(),
    })
}

fn manifest_value(
    definition: &LawpackAuthoringDefinition,
    exports: &BuiltResource,
    adapters: &[BuiltAdapter],
    resources: &BTreeMap<String, BuiltResource>,
) -> Result<CanonicalValue, Vec<LawpackAuthoringFailure>> {
    let dependencies = definition
        .dependencies
        .iter()
        .map(|dependency| {
            Ok(map([
                ("id", text(&dependency.id)),
                ("version", text(&dependency.version)),
                (
                    "digest",
                    digest_value(parse_digest(&dependency.digest, "dependencies.digest")?),
                ),
            ]))
        })
        .collect::<Result<Vec<_>, Vec<LawpackAuthoringFailure>>>()?;
    let mut fields = vec![
        ("apiVersion", text(LAWPACK_API_VERSION)),
        ("id", text(&definition.id)),
        ("version", text(&definition.version)),
        (
            "acceptedCoreAbi",
            array(definition.accepted_core_abi.iter().map(|abi| text(abi))),
        ),
        ("dependencies", CanonicalValue::Array(dependencies)),
        (
            "exports",
            resource_value(&exports.coordinate, exports.digest),
        ),
    ];
    if !adapters.is_empty() {
        let target_adapters = definition
            .target_adapters
            .iter()
            .zip(adapters)
            .enumerate()
            .map(|(index, (definition, built))| {
                Ok(map([
                    (
                        "acceptedTargetProfile",
                        pinned_resource_value_at(
                            &definition.accepted_target_profile,
                            &format!("targetAdapters.{index}.acceptedTargetProfile"),
                        )?,
                    ),
                    (
                        "acceptedTargetIr",
                        pinned_resource_value_at(
                            &definition.accepted_target_ir,
                            &format!("targetAdapters.{index}.acceptedTargetIr"),
                        )?,
                    ),
                    ("adapter", resource_value(&built.coordinate, built.digest)),
                ]))
            })
            .collect::<Result<Vec<_>, Vec<LawpackAuthoringFailure>>>()?;
        fields.push(("targetAdapters", CanonicalValue::Array(target_adapters)));
    }
    if let Some(component) = &definition.helper_component {
        fields.push((
            "helperComponent",
            executable_component_value(component, resources, "helperComponent")?,
        ));
    }
    fields.extend([
        ("verifier", verifier_value(&definition.verifier, resources)?),
        (
            "compatibility",
            resolve_resource_value(&definition.compatibility, resources, "compatibility")?,
        ),
        (
            "conformanceFixtureCorpus",
            resolve_resource_value(
                &definition.conformance_fixture_corpus,
                resources,
                "conformanceFixtureCorpus",
            )?,
        ),
    ]);
    Ok(map(fields))
}

fn exports_value(
    exports: &LawpackAuthoringExports,
    resources: &BTreeMap<String, BuiltResource>,
) -> Result<CanonicalValue, Vec<LawpackAuthoringFailure>> {
    let types = exports.types.iter().map(|export| {
        map([
            ("coordinate", text(&export.coordinate)),
            ("definition", text(&export.definition)),
        ])
    });
    let constants = exports
        .constants
        .iter()
        .enumerate()
        .map(|(index, export)| {
            Ok(map([
                ("coordinate", text(&export.coordinate)),
                ("type", text(&export.ty)),
                (
                    "value",
                    canonical_export_json_value(
                        &export.value,
                        &format!("exports.constants.{index}.value"),
                    )?,
                ),
            ]))
        })
        .collect::<Result<Vec<_>, Vec<LawpackAuthoringFailure>>>()?;
    let pure_functions = exports
        .pure_functions
        .iter()
        .enumerate()
        .map(|(index, function)| pure_function_value(function, resources, index))
        .collect::<Result<Vec<_>, Vec<LawpackAuthoringFailure>>>()?;
    let effects = exports.effects.iter().map(effect_value);
    let obstructions = exports.obstructions.iter().map(|obstruction| {
        map([
            ("coordinate", text(&obstruction.coordinate)),
            (
                "authorityClass",
                text(authority_class_name(obstruction.authority_class)),
            ),
            ("payloadSchema", text(&obstruction.payload_schema)),
        ])
    });
    let operation_profiles = CanonicalValue::Map(
        exports
            .operation_profiles
            .iter()
            .map(|(coordinate, profile)| (text(coordinate), operation_profile_value(profile)))
            .collect(),
    );
    Ok(map([
        ("types", array(types)),
        ("constants", CanonicalValue::Array(constants)),
        ("pureFunctions", CanonicalValue::Array(pure_functions)),
        ("effects", array(effects)),
        ("obstructions", array(obstructions)),
        ("operationProfiles", operation_profiles),
    ]))
}

fn pure_function_value(
    function: &LawpackAuthoringPureFunction,
    resources: &BTreeMap<String, BuiltResource>,
    index: usize,
) -> Result<CanonicalValue, Vec<LawpackAuthoringFailure>> {
    let (coordinate, type_parameters, parameter_types, return_type, cost_template, class) =
        match function {
            LawpackAuthoringPureFunction::Edict {
                coordinate,
                type_parameters,
                parameter_types,
                return_type,
                cost_template,
                determinism_class,
                ..
            }
            | LawpackAuthoringPureFunction::Component {
                coordinate,
                type_parameters,
                parameter_types,
                return_type,
                cost_template,
                determinism_class,
                ..
            } => (
                coordinate,
                type_parameters,
                parameter_types,
                return_type,
                cost_template,
                determinism_class,
            ),
        };
    let mut fields = vec![
        ("coordinate", text(coordinate)),
        (
            "typeParameters",
            array(type_parameters.iter().map(|value| text(value))),
        ),
        (
            "parameterTypes",
            array(parameter_types.iter().map(|value| text(value))),
        ),
        ("returnType", text(return_type)),
        ("costTemplate", text(cost_template)),
        (
            "determinismClass",
            text(match class {
                LawpackAuthoringDeterminismClass::Total => "total",
                LawpackAuthoringDeterminismClass::TotalWithTypedDiagnostic => {
                    "total-with-typed-diagnostic"
                }
            }),
        ),
    ];
    match function {
        LawpackAuthoringPureFunction::Edict { body, .. } => {
            fields.push(("source", text("edict")));
            fields.push((
                "body",
                canonical_export_json_value(body, &format!("exports.pureFunctions.{index}.body"))?,
            ));
        }
        LawpackAuthoringPureFunction::Component { implementation, .. } => {
            fields.push(("source", text("component")));
            fields.push((
                "implementation",
                executable_component_value(
                    implementation,
                    resources,
                    &format!("exports.pureFunctions.{index}.implementation"),
                )?,
            ));
        }
    }
    Ok(map(fields))
}

fn effect_value(effect: &LawpackAuthoringEffect) -> CanonicalValue {
    let failures = CanonicalValue::Map(
        effect
            .effect_failures
            .iter()
            .map(|(name, failure)| {
                (
                    text(name),
                    map([
                        (
                            "authorityClass",
                            text(authority_class_name(failure.authority_class)),
                        ),
                        ("payloadType", text(&failure.payload_type)),
                    ]),
                )
            })
            .collect(),
    );
    map([
        ("coordinate", text(&effect.coordinate)),
        (
            "typeParameters",
            array(effect.type_parameters.iter().map(|value| text(value))),
        ),
        ("inputType", text(&effect.input_type)),
        ("outputType", text(&effect.output_type)),
        (
            "executionClass",
            text(match effect.execution_class {
                LawpackAuthoringExecutionClass::ProofOnly => "proofOnly",
                LawpackAuthoringExecutionClass::Runtime => "runtime",
            }),
        ),
        (
            "effectKindHint",
            text(effect_kind_name(effect.effect_kind_hint)),
        ),
        ("footprintObligation", text(&effect.footprint_obligation)),
        ("costObligation", text(&effect.cost_obligation)),
        ("effectFailures", failures),
        ("guardSupport", CanonicalValue::Bool(effect.guard_support)),
    ])
}

fn operation_profile_value(profile: &LawpackAuthoringOperationProfile) -> CanonicalValue {
    let mut optic = vec![
        ("opticKind", text(&profile.optic_template.optic_kind)),
        ("boundaryKind", text(&profile.optic_template.boundary_kind)),
        (
            "supportPolicy",
            text(&profile.optic_template.support_policy),
        ),
        (
            "lossDisposition",
            text(&profile.optic_template.loss_disposition),
        ),
    ];
    if let Some(basis) = &profile.optic_template.basis_template {
        optic.push(("basisTemplate", text(basis)));
    }
    if let Some(aperture) = &profile.optic_template.aperture_requirement {
        let (kind, reference) = match aperture {
            LawpackAuthoringApertureRequirement::FootprintCeiling { reference } => {
                ("footprintCeiling", reference)
            }
            LawpackAuthoringApertureRequirement::AbstractFootprintObligation { reference } => {
                ("abstractFootprintObligation", reference)
            }
        };
        optic.push((
            "apertureRequirement",
            map([("kind", text(kind)), ("ref", text(reference))]),
        ));
    }
    map([
        ("opticTemplate", map(optic)),
        ("effectPredicate", text(&profile.effect_predicate)),
    ])
}

fn adapter_value(
    adapter: &LawpackAuthoringAdapter,
    resources: &BTreeMap<String, BuiltResource>,
    index: usize,
) -> Result<CanonicalValue, Vec<LawpackAuthoringFailure>> {
    let operation_profiles = adapter
        .operation_profiles
        .iter()
        .map(|(coordinate, profile)| {
            let mut fields = vec![
                ("core", text(&profile.core)),
                (
                    "semanticEffects",
                    array(profile.semantic_effects.iter().map(|effect| text(effect))),
                ),
            ];
            if let Some(budget) = &profile.budget_obligation {
                fields.push(("budgetObligation", text(budget)));
            }
            if let Some(configuration) = &profile.target_configuration {
                fields.push((
                    "targetConfiguration",
                    resolve_resource_value(
                        configuration,
                        resources,
                        &format!(
                            "targetAdapters.{index}.operationProfiles.{coordinate}.targetConfiguration"
                        ),
                    )?,
                ));
            }
            Ok((text(coordinate), map(fields)))
        })
        .collect::<Result<Vec<_>, Vec<LawpackAuthoringFailure>>>()?;
    let effects = adapter
        .effect_implementations
        .iter()
        .map(|(coordinate, effect)| {
            Ok((
                text(coordinate),
                map([
                    ("targetIntrinsic", text(&effect.target_intrinsic)),
                    (
                        "targetConfiguration",
                        resolve_resource_value(
                            &effect.target_configuration,
                            resources,
                            &format!(
                                "targetAdapters.{index}.effectImplementations.{coordinate}.targetConfiguration"
                            ),
                        )?,
                    ),
                    ("writeClass", text(&effect.write_class)),
                    (
                        "footprintObligation",
                        text(&effect.footprint_obligation),
                    ),
                    ("costObligation", text(&effect.cost_obligation)),
                    (
                        "failureMappings",
                        CanonicalValue::Map(
                            effect
                                .failure_mappings
                                .iter()
                                .map(|(name, mapped)| (text(name), text(mapped)))
                                .collect(),
                        ),
                    ),
                ]),
            ))
        })
        .collect::<Result<Vec<_>, Vec<LawpackAuthoringFailure>>>()?;
    let budgets = CanonicalValue::Map(
        adapter
            .budgets
            .iter()
            .map(|(coordinate, budget)| {
                (
                    text(coordinate),
                    map([
                        (
                            "maxSteps",
                            CanonicalValue::Integer(i128::from(budget.max_steps)),
                        ),
                        (
                            "maxAllocatedBytes",
                            CanonicalValue::Integer(i128::from(budget.max_allocated_bytes)),
                        ),
                        (
                            "maxOutputBytes",
                            CanonicalValue::Integer(i128::from(budget.max_output_bytes)),
                        ),
                    ]),
                )
            })
            .collect(),
    );
    Ok(map([
        ("apiVersion", text("edict.lawpack-adapter/v1")),
        ("class", text("declarative")),
        ("operationProfiles", CanonicalValue::Map(operation_profiles)),
        ("effectImplementations", CanonicalValue::Map(effects)),
        ("budgets", budgets),
    ]))
}

fn verifier_value(
    verifier: &LawpackAuthoringVerifier,
    resources: &BTreeMap<String, BuiltResource>,
) -> Result<CanonicalValue, Vec<LawpackAuthoringFailure>> {
    match verifier {
        LawpackAuthoringVerifier::Declarative { ruleset } => Ok(map([
            ("class", text("declarative")),
            (
                "ruleset",
                resolve_resource_value(ruleset, resources, "verifier.ruleset")?,
            ),
        ])),
        LawpackAuthoringVerifier::Executable { executable } => {
            let CanonicalValue::Map(mut fields) =
                executable_component_value(executable, resources, "verifier")?
            else {
                return Err(one(failure(
                    LawpackAuthoringFailureKind::EncodingFailed,
                    "verifier",
                    "an executable verifier encoded as a canonical map",
                )));
            };
            fields.push((text("class"), text("executable")));
            Ok(CanonicalValue::Map(fields))
        }
    }
}

fn executable_component_value(
    component: &LawpackAuthoringExecutableComponent,
    resources: &BTreeMap<String, BuiltResource>,
    path: &str,
) -> Result<CanonicalValue, Vec<LawpackAuthoringFailure>> {
    Ok(map([
        (
            "component",
            resolve_resource_value(
                &component.component,
                resources,
                &format!("{path}.component"),
            )?,
        ),
        (
            "sandbox",
            resolve_resource_value(&component.sandbox, resources, &format!("{path}.sandbox"))?,
        ),
        (
            "fuelModel",
            resolve_resource_value(
                &component.fuel_model,
                resources,
                &format!("{path}.fuelModel"),
            )?,
        ),
    ]))
}

fn resolve_resource_value(
    reference: &LawpackAuthoringResourceRef,
    resources: &BTreeMap<String, BuiltResource>,
    path: &str,
) -> Result<CanonicalValue, Vec<LawpackAuthoringFailure>> {
    match reference {
        LawpackAuthoringResourceRef::External(reference) => pinned_resource_value(reference),
        LawpackAuthoringResourceRef::Local(reference) => resources
            .get(&reference.local)
            .map(|resource| resource_value(&resource.coordinate, resource.digest))
            .ok_or_else(|| {
                one(failure(
                    LawpackAuthoringFailureKind::MissingLocalResource,
                    path,
                    format!("declared local resource `{}`", reference.local),
                ))
            }),
    }
}

fn pinned_resource_value(
    reference: &LawpackAuthoringPinnedResource,
) -> Result<CanonicalValue, Vec<LawpackAuthoringFailure>> {
    pinned_resource_value_at(reference, "resource")
}

fn pinned_resource_value_at(
    reference: &LawpackAuthoringPinnedResource,
    path: &str,
) -> Result<CanonicalValue, Vec<LawpackAuthoringFailure>> {
    let digest = parse_digest(&reference.digest, &format!("{path}.digest"))?;
    if reference.id.is_empty() {
        return Err(one(failure(
            LawpackAuthoringFailureKind::InvalidDefinition,
            format!("{path}.id"),
            "non-empty resource id",
        )));
    }
    Ok(resource_value(&reference.id, digest))
}

fn resource_value(id: &str, digest: [u8; 32]) -> CanonicalValue {
    map([("id", text(id)), ("digest", digest_value(digest))])
}

fn digest_value(digest: [u8; 32]) -> CanonicalValue {
    CanonicalValue::Array(vec![text("sha256"), CanonicalValue::Bytes(digest.to_vec())])
}

fn canonical_json_value(
    value: &Value,
    path: &str,
) -> Result<CanonicalValue, Vec<LawpackAuthoringFailure>> {
    canonical_json_value_with_enclosing_depth(value, path, 0)
}

fn canonical_export_json_value(
    value: &Value,
    path: &str,
) -> Result<CanonicalValue, Vec<LawpackAuthoringFailure>> {
    canonical_json_value_with_enclosing_depth(value, path, EXPORT_VALUE_CANONICAL_ENCLOSING_DEPTH)
}

fn canonical_json_value_with_enclosing_depth(
    value: &Value,
    path: &str,
    enclosing_depth: usize,
) -> Result<CanonicalValue, Vec<LawpackAuthoringFailure>> {
    let remaining_depth = MAX_LAWPACK_AUTHORING_VALUE_NESTING_DEPTH
        .checked_sub(enclosing_depth)
        .ok_or_else(|| {
            one(failure(
                LawpackAuthoringFailureKind::InvalidCanonicalValue,
                path,
                "canonical JSON nesting within the enclosing artifact depth limit",
            ))
        })?;
    canonical_json_value_at_depth(value, path, remaining_depth)
}

fn canonical_json_value_at_depth(
    value: &Value,
    path: &str,
    remaining_depth: usize,
) -> Result<CanonicalValue, Vec<LawpackAuthoringFailure>> {
    match value {
        Value::Null => Ok(CanonicalValue::Null),
        Value::Bool(value) => Ok(CanonicalValue::Bool(*value)),
        Value::Number(value) => {
            if let Some(value) = value.as_i64() {
                Ok(CanonicalValue::Integer(i128::from(value)))
            } else if let Some(value) = value.as_u64() {
                Ok(CanonicalValue::Integer(i128::from(value)))
            } else {
                Err(one(failure(
                    LawpackAuthoringFailureKind::InvalidCanonicalValue,
                    path,
                    "an integral JSON number representable by canonical CBOR",
                )))
            }
        }
        Value::String(value) => Ok(text(value)),
        Value::Array(values) => {
            let next_depth = descend_canonical_json(path, remaining_depth)?;
            values
                .iter()
                .enumerate()
                .map(|(index, value)| {
                    canonical_json_value_at_depth(value, &format!("{path}.{index}"), next_depth)
                })
                .collect::<Result<Vec<_>, Vec<LawpackAuthoringFailure>>>()
                .map(CanonicalValue::Array)
        }
        Value::Object(values) if values.len() == 1 && values.contains_key("$edictBytes") => {
            let encoded = values
                .get("$edictBytes")
                .and_then(Value::as_str)
                .ok_or_else(|| {
                    one(failure(
                        LawpackAuthoringFailureKind::InvalidCanonicalValue,
                        path,
                        "`$edictBytes` containing lowercase even-length hexadecimal text",
                    ))
                })?;
            parse_hex_bytes(encoded, path).map(CanonicalValue::Bytes)
        }
        Value::Object(values) => {
            let next_depth = descend_canonical_json(path, remaining_depth)?;
            values
                .iter()
                .map(|(key, value)| {
                    Ok((
                        text(key),
                        canonical_json_value_at_depth(value, &format!("{path}.{key}"), next_depth)?,
                    ))
                })
                .collect::<Result<Vec<_>, Vec<LawpackAuthoringFailure>>>()
                .map(CanonicalValue::Map)
        }
    }
}

fn descend_canonical_json(
    path: &str,
    remaining_depth: usize,
) -> Result<usize, Vec<LawpackAuthoringFailure>> {
    remaining_depth.checked_sub(1).ok_or_else(|| {
        one(failure(
            LawpackAuthoringFailureKind::InvalidCanonicalValue,
            path,
            "canonical JSON nesting within the authoring depth limit",
        ))
    })
}

fn parse_digest(digest: &str, path: &str) -> Result<[u8; 32], Vec<LawpackAuthoringFailure>> {
    let Some(encoded) = digest.strip_prefix("sha256:") else {
        return Err(one(failure(
            LawpackAuthoringFailureKind::InvalidDigest,
            path,
            "lowercase `sha256:` followed by 64 lowercase hexadecimal characters",
        )));
    };
    let bytes = parse_hex_bytes(encoded, path).map_err(|_| {
        one(failure(
            LawpackAuthoringFailureKind::InvalidDigest,
            path,
            "lowercase `sha256:` followed by 64 lowercase hexadecimal characters",
        ))
    })?;
    if bytes.len() != 32 {
        return Err(one(failure(
            LawpackAuthoringFailureKind::InvalidDigest,
            path,
            "exactly 32 SHA-256 digest bytes",
        )));
    }
    let mut digest_bytes = [0_u8; 32];
    digest_bytes.copy_from_slice(&bytes);
    Ok(digest_bytes)
}

fn parse_hex_bytes(encoded: &str, path: &str) -> Result<Vec<u8>, Vec<LawpackAuthoringFailure>> {
    if !encoded.len().is_multiple_of(2)
        || encoded
            .bytes()
            .any(|byte| !byte.is_ascii_digit() && !(b'a'..=b'f').contains(&byte))
    {
        return Err(one(failure(
            LawpackAuthoringFailureKind::InvalidCanonicalValue,
            path,
            "lowercase even-length hexadecimal text",
        )));
    }
    encoded
        .as_bytes()
        .as_chunks::<2>()
        .0
        .iter()
        .map(|pair| {
            let high = hex_nibble(pair[0]);
            let low = hex_nibble(pair[1]);
            high.zip(low).map(|(high, low)| (high << 4) | low)
        })
        .collect::<Option<Vec<_>>>()
        .ok_or_else(|| {
            one(failure(
                LawpackAuthoringFailureKind::InvalidCanonicalValue,
                path,
                "lowercase hexadecimal text",
            ))
        })
}

const fn hex_nibble(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        _ => None,
    }
}

fn push_artifact_pair(
    artifacts: &mut Vec<LawpackAuthoredArtifact>,
    artifact_kind: LawpackArtifactKind,
    digest_kind: LawpackArtifactKind,
    built: &BuiltResource,
) -> Result<(), Vec<LawpackAuthoringFailure>> {
    let sidecar = digest_sidecar_path(&built.output)?;
    artifacts.push(LawpackAuthoredArtifact {
        kind: artifact_kind,
        path: built.output.clone(),
        coordinate: built.coordinate.clone(),
        bytes: built.bytes.clone(),
        digest: built.digest_review.clone(),
    });
    artifacts.push(LawpackAuthoredArtifact {
        kind: digest_kind,
        path: sidecar,
        coordinate: built.coordinate.clone(),
        bytes: format!("{}\n", built.digest_review).into_bytes(),
        digest: built.digest_review.clone(),
    });
    Ok(())
}

fn digest_sidecar_path(output: &str) -> Result<String, Vec<LawpackAuthoringFailure>> {
    let path = Path::new(output);
    if path.extension().and_then(|extension| extension.to_str()) != Some("cbor") {
        return Err(one(failure(
            LawpackAuthoringFailureKind::InvalidOutputPath,
            output,
            "a relative output path ending in `.cbor`",
        )));
    }
    let mut sidecar = path.to_path_buf();
    sidecar.set_extension("sha256");
    let sidecar = sidecar.into_os_string().into_string().map_err(|_| {
        one(failure(
            LawpackAuthoringFailureKind::InvalidOutputPath,
            output,
            "a UTF-8 relative output path",
        ))
    })?;
    if !portable_output_path(
        &sidecar,
        "sha256",
        MAX_PORTABLE_RELATIVE_ARTIFACT_BYTES,
        MAX_PORTABLE_ARTIFACT_COMPONENT_BYTES,
    ) {
        return Err(one(failure(
            LawpackAuthoringFailureKind::InvalidOutputPath,
            output,
            "a derived digest sidecar within portable relative-path and component limits",
        )));
    }
    Ok(sidecar)
}

fn validate_output_path(output: &str, path: &str) -> Result<(), Vec<LawpackAuthoringFailure>> {
    if output.is_empty()
        || output.as_bytes().contains(&0)
        || !portable_output_path(
            output,
            "cbor",
            MAX_PORTABLE_RELATIVE_OUTPUT_BYTES,
            MAX_PORTABLE_OUTPUT_COMPONENT_BYTES,
        )
    {
        return Err(one(failure(
            LawpackAuthoringFailureKind::InvalidOutputPath,
            path,
            "a bounded lowercase portable relative UTF-8 `.cbor` path without parent traversal or reserved filesystem names",
        )));
    }
    Ok(())
}

fn portable_output_path(
    output: &str,
    extension: &str,
    max_path_bytes: usize,
    max_component_bytes: usize,
) -> bool {
    let output_path = Path::new(output);
    portable_output_path_grammar(output)
        && !output_path.is_absolute()
        && output_path
            .extension()
            .and_then(|candidate| candidate.to_str())
            == Some(extension)
        && output.len() <= max_path_bytes
        && output_path.components().all(|component| match component {
            Component::Normal(component) => {
                portable_output_component(component.to_str(), max_component_bytes)
            }
            Component::ParentDir
            | Component::CurDir
            | Component::RootDir
            | Component::Prefix(_) => false,
        })
}

fn portable_output_path_grammar(output: &str) -> bool {
    !output.as_bytes().contains(&b'\\') && output.split('/').all(|component| !component.is_empty())
}

fn portable_output_component(component: Option<&str>, max_bytes: usize) -> bool {
    let Some(component) = component else {
        return false;
    };
    if component.is_empty()
        || component.len() > max_bytes
        || component.ends_with('.')
        || !component.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || b"._-".contains(&byte)
        })
    {
        return false;
    }
    let stem = component.split('.').next().unwrap_or_default();
    !matches!(stem, "con" | "prn" | "aux" | "nul")
        && !(stem.len() == 4
            && (stem.starts_with("com") || stem.starts_with("lpt"))
            && matches!(stem.as_bytes()[3], b'1'..=b'9'))
}

fn validate_artifact_paths(
    artifacts: &[LawpackAuthoredArtifact],
) -> Result<(), Vec<LawpackAuthoringFailure>> {
    let mut paths = BTreeSet::new();
    let mut coordinates = BTreeSet::new();
    for artifact in artifacts {
        let artifact_path = Path::new(&artifact.path);
        if artifact_path.starts_with(Path::new(LAWPACK_OUTPUT_INDEX_PATH)) {
            return Err(one(failure(
                LawpackAuthoringFailureKind::InvalidOutputPath,
                &artifact.path,
                "a path outside the generated ownership-index namespace",
            )));
        }
        if !paths.insert(artifact_path) {
            return Err(one(failure(
                LawpackAuthoringFailureKind::DuplicateIdentity,
                &artifact.path,
                "one emitted artifact per relative path",
            )));
        }
        if matches!(
            artifact.kind,
            LawpackArtifactKind::Manifest
                | LawpackArtifactKind::Exports
                | LawpackArtifactKind::Adapter
                | LawpackArtifactKind::LocalResource
        ) && !coordinates.insert(artifact.coordinate.as_str())
        {
            return Err(one(failure(
                LawpackAuthoringFailureKind::DuplicateIdentity,
                &artifact.coordinate,
                "one canonical artifact per resource coordinate",
            )));
        }
    }
    for path in &paths {
        let mut ancestor = path.parent();
        while let Some(parent) = ancestor {
            if parent.as_os_str().is_empty() {
                break;
            }
            if paths.contains(parent) {
                return Err(one(failure(
                    LawpackAuthoringFailureKind::DuplicateIdentity,
                    path.to_string_lossy(),
                    "emitted artifact paths without file/descendant collisions",
                )));
            }
            ancestor = parent.parent();
        }
    }
    Ok(())
}

fn map<K: Into<String>>(fields: impl IntoIterator<Item = (K, CanonicalValue)>) -> CanonicalValue {
    CanonicalValue::Map(
        fields
            .into_iter()
            .map(|(key, value)| {
                let key = key.into();
                (text(&key), value)
            })
            .collect(),
    )
}

fn array(values: impl IntoIterator<Item = CanonicalValue>) -> CanonicalValue {
    CanonicalValue::Array(values.into_iter().collect())
}

fn text(value: &str) -> CanonicalValue {
    CanonicalValue::Text(value.to_owned())
}

const fn authority_class_name(class: LawpackAuthoringAuthorityClass) -> &'static str {
    match class {
        LawpackAuthoringAuthorityClass::DomainMappable => "domainMappable",
        LawpackAuthoringAuthorityClass::ParticipantOwned => "participantOwned",
        LawpackAuthoringAuthorityClass::IntegrityFault => "integrityFault",
        LawpackAuthoringAuthorityClass::ResourceFault => "resourceFault",
        LawpackAuthoringAuthorityClass::InternalFault => "internalFault",
    }
}

const fn effect_kind_name(kind: LawpackAuthoringEffectKind) -> &'static str {
    match kind {
        LawpackAuthoringEffectKind::Read => "read",
        LawpackAuthoringEffectKind::Create => "create",
        LawpackAuthoringEffectKind::Ensure => "ensure",
        LawpackAuthoringEffectKind::Replace => "replace",
        LawpackAuthoringEffectKind::Delete => "delete",
        LawpackAuthoringEffectKind::Append => "append",
        LawpackAuthoringEffectKind::Reduce => "reduce",
        LawpackAuthoringEffectKind::SemanticEmit => "semanticEmit",
        LawpackAuthoringEffectKind::Custom => "custom",
    }
}

fn one(failure: LawpackAuthoringFailure) -> Vec<LawpackAuthoringFailure> {
    vec![failure]
}

fn failure(
    kind: LawpackAuthoringFailureKind,
    path: impl Into<String>,
    obligation: impl Into<String>,
) -> LawpackAuthoringFailure {
    LawpackAuthoringFailure {
        kind,
        path: path.into(),
        obligation: obligation.into(),
        cause: None,
    }
}

fn wrap_lawpack_failures(
    kind: LawpackAuthoringFailureKind,
    failures: Vec<LawpackValidationFailure>,
) -> Vec<LawpackAuthoringFailure> {
    failures
        .into_iter()
        .map(|cause| LawpackAuthoringFailure {
            kind,
            path: cause.path.clone(),
            obligation: cause.obligation.clone(),
            cause: Some(LawpackAuthoringFailureCause::Lawpack(cause)),
        })
        .collect()
}

fn wrap_adapter_failures(
    coordinate: &str,
    failures: Vec<LawpackAdapterFailure>,
) -> Vec<LawpackAuthoringFailure> {
    failures
        .into_iter()
        .map(|cause| LawpackAuthoringFailure {
            kind: LawpackAuthoringFailureKind::InvalidAdapter,
            path: format!("targetAdapters.{coordinate}.{}", cause.path),
            obligation: cause.obligation.clone(),
            cause: Some(LawpackAuthoringFailureCause::Adapter(cause)),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::portable_output_path_grammar;

    #[test]
    fn portable_output_path_grammar_rejects_windows_separators() {
        assert!(portable_output_path_grammar("resources/item.cbor"));
        assert!(!portable_output_path_grammar("resources\\item.cbor"));
    }
}

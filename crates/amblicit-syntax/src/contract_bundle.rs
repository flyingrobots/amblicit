//! Typed v1 contract bundle assembly, validation, and assurance manifest checks.
//!
//! This module models the participant-neutral bundle boundary after Core and
//! target lowering have produced hash-addressed artifacts. It can assemble a
//! manifest from digest-locked references, a real Core module, and optionally a
//! real Target IR artifact. It does not load files, run target verifiers, or
//! perform admission.

use std::fmt;

use crate::{
    canonical::{
        digest_bundle_layer, digest_core_module, digest_target_ir_artifact,
        is_logical_package_relative_path, BundleDigestDomain, BundlePreimageComponent,
        BundleSourceDescriptor, CanonicalError,
    },
    core_ir::{CoreModule, ResourceRef},
    target_ir::{semantic_closure_for_core, TargetIrArtifact},
    target_profile::CANONICAL_CBOR_ABI,
};

/// Contract bundle manifest ABI supported by this crate.
pub const CONTRACT_BUNDLE_API_VERSION: &str = "edict.contract-bundle/v1";

/// Stable error categories returned while assembling a contract bundle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContractBundleAssemblyErrorKind {
    InvalidDigest,
    EmptyCoordinate,
    InvalidSourcePath,
    TargetIrSourceMismatch,
    CanonicalDigest,
    InvalidManifest,
}

/// Assembly failure with stable kind plus field context.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContractBundleAssemblyError {
    kind: ContractBundleAssemblyErrorKind,
    field: String,
    message: String,
}

impl ContractBundleAssemblyError {
    fn new(
        kind: ContractBundleAssemblyErrorKind,
        field: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            kind,
            field: field.into(),
            message: message.into(),
        }
    }

    fn invalid_digest(field: impl Into<String>) -> Self {
        Self::new(
            ContractBundleAssemblyErrorKind::InvalidDigest,
            field,
            "sha256:<64 lowercase hex> digest",
        )
    }

    fn canonical(field: impl Into<String>, err: &CanonicalError) -> Self {
        Self::new(
            ContractBundleAssemblyErrorKind::CanonicalDigest,
            field,
            err.to_string(),
        )
    }

    fn invalid_manifest(report: &ContractBundleValidationReport) -> Self {
        let first = report
            .failures
            .first()
            .expect("invalid validation report has at least one failure");
        Self::new(
            ContractBundleAssemblyErrorKind::InvalidManifest,
            first.field.clone(),
            format!("{:?}: {}", first.kind, first.obligation),
        )
    }

    /// Return the stable assembly error category.
    #[must_use]
    pub const fn kind(&self) -> ContractBundleAssemblyErrorKind {
        self.kind
    }

    /// Return the input or output field associated with the failure.
    #[must_use]
    pub fn field(&self) -> &str {
        &self.field
    }
}

impl fmt::Display for ContractBundleAssemblyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?} on {}: {}", self.kind, self.field, self.message)
    }
}

impl std::error::Error for ContractBundleAssemblyError {}

/// A supplied bundle-layer digest reference.
///
/// Unlike Core canonical imports, bundle artifact references are strict review
/// strings: only `sha256:<64 lowercase hex>` is accepted.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SuppliedDigest {
    review: String,
}

impl SuppliedDigest {
    /// Build a supplied digest from a review string.
    ///
    /// # Errors
    ///
    /// Returns [`ContractBundleAssemblyErrorKind::InvalidDigest`] unless the
    /// review string is exactly `sha256:<64 lowercase hex>`.
    pub fn new(review: impl Into<String>) -> Result<Self, ContractBundleAssemblyError> {
        Self::new_for_field("digest", review)
    }

    fn new_for_field(
        field: &'static str,
        review: impl Into<String>,
    ) -> Result<Self, ContractBundleAssemblyError> {
        let review = review.into();
        if is_bundle_digest(&review) {
            Ok(Self { review })
        } else {
            Err(ContractBundleAssemblyError::invalid_digest(field))
        }
    }

    /// Borrow the strict review rendering.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.review
    }
}

impl fmt::Display for SuppliedDigest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.review)
    }
}

/// A supplied artifact reference where coordinate and digest are both bound.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DigestLockedResource {
    coordinate: String,
    digest: SuppliedDigest,
}

impl DigestLockedResource {
    /// Build a digest-locked resource reference.
    ///
    /// # Errors
    ///
    /// Returns [`ContractBundleAssemblyErrorKind::EmptyCoordinate`] for an empty
    /// coordinate and [`ContractBundleAssemblyErrorKind::InvalidDigest`] for any
    /// non-lowercase or malformed digest.
    pub fn new(
        coordinate: impl Into<String>,
        digest: impl Into<String>,
    ) -> Result<Self, ContractBundleAssemblyError> {
        Self::new_for_field("resource", coordinate, digest)
    }

    fn new_for_field(
        field: &'static str,
        coordinate: impl Into<String>,
        digest: impl Into<String>,
    ) -> Result<Self, ContractBundleAssemblyError> {
        let coordinate = coordinate.into();
        if coordinate.is_empty() {
            return Err(ContractBundleAssemblyError::new(
                ContractBundleAssemblyErrorKind::EmptyCoordinate,
                field,
                "non-empty resource coordinate",
            ));
        }
        let digest = SuppliedDigest::new_for_field(field, digest)?;
        Ok(Self { coordinate, digest })
    }

    /// Borrow the resource coordinate.
    #[must_use]
    pub fn coordinate(&self) -> &str {
        &self.coordinate
    }

    /// Borrow the resource digest.
    #[must_use]
    pub const fn digest(&self) -> &SuppliedDigest {
        &self.digest
    }

    /// Borrow the strict digest review string.
    #[must_use]
    pub fn digest_str(&self) -> &str {
        self.digest.as_str()
    }

    /// Convert to the public manifest resource shape.
    #[must_use]
    pub fn to_resource_ref(&self) -> ResourceRef {
        ResourceRef {
            coordinate: self.coordinate.clone(),
            digest: Some(self.digest.to_string()),
        }
    }
}

/// A supplied Target IR reference.
///
/// This path remains available for already-digested external artifact graphs.
/// The supplied target-IR digest enters exactly once through this type and is
/// reused for both the manifest field and semantic digest preimage.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SuppliedTargetIrResource {
    resource: DigestLockedResource,
}

impl SuppliedTargetIrResource {
    /// Build a supplied digest-locked Target IR reference.
    ///
    /// # Errors
    ///
    /// Returns an assembly error if the coordinate is empty or the digest is not
    /// strict lowercase `sha256:<64 hex>`.
    pub fn new(
        coordinate: impl Into<String>,
        digest: impl Into<String>,
    ) -> Result<Self, ContractBundleAssemblyError> {
        Ok(Self {
            resource: DigestLockedResource::new_for_field("target_ir", coordinate, digest)?,
        })
    }

    /// Borrow the Target IR digest.
    #[must_use]
    pub fn digest_str(&self) -> &str {
        self.resource.digest_str()
    }

    fn to_resource_ref(&self) -> ResourceRef {
        self.resource.to_resource_ref()
    }
}

/// Source artifact provenance supplied to bundle assembly.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContractBundleSourceArtifact {
    logical_path: String,
    artifact: DigestLockedResource,
}

impl ContractBundleSourceArtifact {
    /// Build a source artifact descriptor.
    ///
    /// # Errors
    ///
    /// Returns an assembly error for non-logical paths, empty coordinates, or
    /// malformed/non-lowercase digest renderings.
    pub fn new(
        logical_path: impl Into<String>,
        coordinate: impl Into<String>,
        digest: impl Into<String>,
    ) -> Result<Self, ContractBundleAssemblyError> {
        let logical_path = logical_path.into();
        if !is_logical_package_relative_path(&logical_path) {
            return Err(ContractBundleAssemblyError::new(
                ContractBundleAssemblyErrorKind::InvalidSourcePath,
                "source_artifacts.logical_path",
                "logical package-relative path",
            ));
        }
        Ok(Self {
            logical_path,
            artifact: DigestLockedResource::new_for_field(
                "source_artifacts.artifact",
                coordinate,
                digest,
            )?,
        })
    }

    fn to_source_artifact_ref(&self) -> SourceArtifactRef {
        SourceArtifactRef {
            logical_path: self.logical_path.clone(),
            artifact: self.artifact.to_resource_ref(),
        }
    }
}

/// Optional assurance evidence supplied to bundle assembly.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContractBundleAssuranceEvidenceInput {
    role: AssuranceRole,
    subject_kind: BundleSubjectKind,
    artifact: DigestLockedResource,
}

impl ContractBundleAssuranceEvidenceInput {
    /// Build optional assurance evidence input.
    ///
    /// The assembler fills the evidence subject digest plus target-profile and
    /// target-IR bindings from the computed manifest, so optional evidence does
    /// not introduce a second top-level digest preimage.
    ///
    /// # Errors
    ///
    /// Returns an assembly error for empty coordinates or malformed/non-
    /// lowercase digest renderings.
    pub fn new(
        role: AssuranceRole,
        subject_kind: BundleSubjectKind,
        coordinate: impl Into<String>,
        digest: impl Into<String>,
    ) -> Result<Self, ContractBundleAssemblyError> {
        Ok(Self {
            role,
            subject_kind,
            artifact: DigestLockedResource::new_for_field(
                "assurance_evidence.artifact",
                coordinate,
                digest,
            )?,
        })
    }
}

/// Inputs needed to assemble a participant-neutral contract bundle manifest.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContractBundleAssemblyInput {
    pub core_module: CoreModule,
    pub core_ir_coordinate: String,
    pub source_artifacts: Vec<ContractBundleSourceArtifact>,
    pub source_profile_semantic_facts: DigestLockedResource,
    pub target_profile: DigestLockedResource,
    pub target_ir: SuppliedTargetIrResource,
    pub lawpacks: Vec<DigestLockedResource>,
    pub generated_artifacts: Vec<DigestLockedResource>,
    pub compiler: DigestLockedResource,
    pub lowerer: DigestLockedResource,
    pub verifier: DigestLockedResource,
    pub semantic_compile_options: DigestLockedResource,
    pub non_semantic_compile_options: DigestLockedResource,
    pub build_provenance: DigestLockedResource,
    pub canonicalization_profile: DigestLockedResource,
    pub conformance_fixture_corpora: Vec<DigestLockedResource>,
    pub verifier_report: DigestLockedResource,
    pub compile_explanation: DigestLockedResource,
    pub assurance_evidence: Vec<ContractBundleAssuranceEvidenceInput>,
}

/// Inputs for assembling a bundle from an actual Target IR artifact.
///
/// This path has no supplied target-IR digest field. The assembler computes the
/// digest from `target_ir_artifact`, uses `target_ir_artifact.domain` as the
/// manifest Target IR coordinate, and derives the manifest target-profile
/// reference from `target_ir_artifact.target_profile`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContractBundleAssemblyFromTargetIrInput {
    pub core_module: CoreModule,
    pub core_ir_coordinate: String,
    pub source_artifacts: Vec<ContractBundleSourceArtifact>,
    pub source_profile_semantic_facts: DigestLockedResource,
    pub target_ir_artifact: TargetIrArtifact,
    pub lawpacks: Vec<DigestLockedResource>,
    pub generated_artifacts: Vec<DigestLockedResource>,
    pub compiler: DigestLockedResource,
    pub lowerer: DigestLockedResource,
    pub verifier: DigestLockedResource,
    pub semantic_compile_options: DigestLockedResource,
    pub non_semantic_compile_options: DigestLockedResource,
    pub build_provenance: DigestLockedResource,
    pub canonicalization_profile: DigestLockedResource,
    pub conformance_fixture_corpora: Vec<DigestLockedResource>,
    pub verifier_report: DigestLockedResource,
    pub compile_explanation: DigestLockedResource,
    pub assurance_evidence: Vec<ContractBundleAssuranceEvidenceInput>,
}

/// Which pre-admission bundle digest an artifact references.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum BundleSubjectKind {
    Semantic,
    Release,
}

/// Required assurance roles bound to a participant-neutral bundle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum AssuranceRole {
    Holmes,
    Watson,
    Moriarty,
}

/// Explicit subject reference used by assurance and admission-adjacent artifacts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BundleSubject {
    pub kind: BundleSubjectKind,
    pub digest: String,
}

/// Source artifact provenance recorded in the release bundle layer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceArtifactRef {
    pub logical_path: String,
    pub artifact: ResourceRef,
}

/// Participant-neutral assurance evidence included with a bundle manifest.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssuranceEvidenceRef {
    pub role: AssuranceRole,
    pub artifact: ResourceRef,
    pub subject: BundleSubject,
    pub target_profile_digest: String,
    pub target_ir_digest: String,
}

/// Typed contract checked before admission.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContractBundleManifest {
    pub api_version: String,
    pub semantic_bundle_digest: String,
    pub release_bundle_digest: String,
    pub source_artifacts: Vec<SourceArtifactRef>,
    pub source_profile_semantic_facts: ResourceRef,
    pub core_ir: ResourceRef,
    pub target_profile: ResourceRef,
    pub target_ir: ResourceRef,
    pub lawpacks: Vec<ResourceRef>,
    pub generated_artifacts: Vec<ResourceRef>,
    pub compiler: ResourceRef,
    pub lowerer: ResourceRef,
    pub verifier: ResourceRef,
    pub semantic_compile_options: ResourceRef,
    pub non_semantic_compile_options: ResourceRef,
    pub build_provenance: ResourceRef,
    pub canonicalization_profile: ResourceRef,
    pub conformance_fixture_corpora: Vec<ResourceRef>,
    pub verifier_report: ResourceRef,
    pub compile_explanation: ResourceRef,
    pub assurance_evidence: Vec<AssuranceEvidenceRef>,
    pub admission_artifacts: Vec<ResourceRef>,
}

/// Assemble a v1 participant-neutral contract bundle manifest.
///
/// The Core digest is always computed from `input.core_module`. The Target IR
/// digest remains a supplied digest-locked reference for v0.11 and is reused by
/// construction in both `manifest.target_ir.digest` and the semantic bundle
/// preimage.
///
/// # Errors
///
/// Returns an assembly error if the Core module cannot be canonically digested
/// or if any digest-layer preimage component cannot be represented.
pub fn assemble_contract_bundle(
    input: ContractBundleAssemblyInput,
) -> Result<ContractBundleManifest, ContractBundleAssemblyError> {
    let parts = assembly_parts(input)?;
    let semantic_bundle_digest = semantic_bundle_digest(&parts)?;
    let release_bundle_digest = release_bundle_digest(&parts, &semantic_bundle_digest)?;
    let manifest = manifest_from_parts(parts, semantic_bundle_digest, release_bundle_digest);
    let report = validate_contract_bundle_manifest(&manifest);
    if report.status == ContractBundleValidationStatus::Valid {
        Ok(manifest)
    } else {
        Err(ContractBundleAssemblyError::invalid_manifest(&report))
    }
}

/// Assemble a v1 bundle manifest from an actual Target IR artifact.
///
/// The Core digest is computed from `input.core_module`, and the Target IR
/// digest is computed from `input.target_ir_artifact`. The caller cannot supply
/// an alternate Target IR digest on this path.
///
/// # Errors
///
/// Returns an assembly error if the Core or Target IR artifact cannot be
/// canonically digested, if the Target IR artifact is not digest-locked to a
/// strict target-profile reference, or if the assembled manifest fails
/// validation.
pub fn assemble_contract_bundle_from_target_ir(
    input: ContractBundleAssemblyFromTargetIrInput,
) -> Result<ContractBundleManifest, ContractBundleAssemblyError> {
    let ContractBundleAssemblyFromTargetIrInput {
        core_module,
        core_ir_coordinate,
        source_artifacts,
        source_profile_semantic_facts,
        target_ir_artifact,
        mut lawpacks,
        generated_artifacts,
        compiler,
        lowerer,
        verifier,
        semantic_compile_options,
        non_semantic_compile_options,
        build_provenance,
        canonicalization_profile,
        conformance_fixture_corpora,
        verifier_report,
        compile_explanation,
        assurance_evidence,
    } = input;

    digest_core_module(&core_module)
        .map_err(|error| ContractBundleAssemblyError::canonical("core_module", &error))?;
    if target_ir_artifact.source_core_coordinate.as_str() != core_module.coordinate.as_str() {
        return Err(ContractBundleAssemblyError::new(
            ContractBundleAssemblyErrorKind::TargetIrSourceMismatch,
            "target_ir_artifact.source_core_coordinate",
            format!(
                "expected Target IR source Core coordinate `{}`, got `{}`",
                core_module.coordinate, target_ir_artifact.source_core_coordinate
            ),
        ));
    }
    corroborate_target_ir_semantic_closure(&core_module, &target_ir_artifact, &lawpacks)?;
    canonicalize_digest_locked_resource_set(&mut lawpacks);

    let target_profile =
        target_profile_from_target_ir_artifact(&target_ir_artifact.target_profile)?;
    let target_ir_digest = digest_target_ir_artifact(&target_ir_artifact)
        .map_err(|err| ContractBundleAssemblyError::canonical("target_ir_artifact", &err))?
        .to_review_string();
    let target_ir = SuppliedTargetIrResource::new(target_ir_artifact.domain, target_ir_digest)?;

    assemble_contract_bundle(ContractBundleAssemblyInput {
        core_module,
        core_ir_coordinate,
        source_artifacts,
        source_profile_semantic_facts,
        target_profile,
        target_ir,
        lawpacks,
        generated_artifacts,
        compiler,
        lowerer,
        verifier,
        semantic_compile_options,
        non_semantic_compile_options,
        build_provenance,
        canonicalization_profile,
        conformance_fixture_corpora,
        verifier_report,
        compile_explanation,
        assurance_evidence,
    })
}

fn corroborate_target_ir_semantic_closure(
    core_module: &CoreModule,
    target_ir_artifact: &TargetIrArtifact,
    lawpacks: &[DigestLockedResource],
) -> Result<(), ContractBundleAssemblyError> {
    let expected = semantic_closure_for_core(core_module).map_err(|failure| {
        ContractBundleAssemblyError::new(
            ContractBundleAssemblyErrorKind::TargetIrSourceMismatch,
            "core_module",
            failure.detail,
        )
    })?;

    let expected_lawpacks = match (&expected, &target_ir_artifact.semantic_closure) {
        (None, None) => Vec::new(),
        (Some(_), None) => {
            return Err(ContractBundleAssemblyError::new(
                ContractBundleAssemblyErrorKind::TargetIrSourceMismatch,
                "target_ir_artifact.semantic_closure",
                "Target IR semantic closure does not match the supplied Core module",
            ));
        }
        (None, Some(actual)) => {
            // This branch may use the Target IR closure only as the expected set
            // because the independently supplied lawpack set is corroborated
            // below. Callers must derive `lawpacks` independently of a
            // provider-asserted Target IR closure.
            let expected_source_core = ResourceRef {
                coordinate: core_module.coordinate.clone(),
                digest: Some(
                    digest_core_module(core_module)
                        .map_err(|error| {
                            ContractBundleAssemblyError::canonical("core_module", &error)
                        })?
                        .to_review_string(),
                ),
            };
            if actual.source_core != expected_source_core {
                return Err(ContractBundleAssemblyError::new(
                    ContractBundleAssemblyErrorKind::TargetIrSourceMismatch,
                    "target_ir_artifact.semantic_closure.source_core",
                    "Target IR source Core identity does not match the supplied Core module",
                ));
            }
            if !actual.capabilities.is_empty() {
                return Err(ContractBundleAssemblyError::new(
                    ContractBundleAssemblyErrorKind::TargetIrSourceMismatch,
                    "target_ir_artifact.semantic_closure.capabilities",
                    "Target IR capability closure does not match the supplied Core module",
                ));
            }
            canonical_resource_set(&actual.lawpacks)
        }
        (Some(expected), Some(actual)) => {
            if actual.source_core != expected.source_core {
                return Err(ContractBundleAssemblyError::new(
                    ContractBundleAssemblyErrorKind::TargetIrSourceMismatch,
                    "target_ir_artifact.semantic_closure.source_core",
                    "Target IR source Core identity does not match the supplied Core module",
                ));
            }
            require_matching_resource_set(
                &actual.lawpacks,
                &expected.lawpacks,
                "target_ir_artifact.semantic_closure.lawpacks",
                "Target IR lawpack closure does not match the supplied Core module",
            )?;
            require_matching_resource_set(
                &actual.capabilities,
                &expected.capabilities,
                "target_ir_artifact.semantic_closure.capabilities",
                "Target IR capability closure does not match the supplied Core module",
            )?;
            canonical_resource_set(&expected.lawpacks)
        }
    };
    let supplied_lawpacks = lawpacks
        .iter()
        .map(DigestLockedResource::to_resource_ref)
        .collect::<Vec<_>>();
    if canonical_resource_set(&supplied_lawpacks) != expected_lawpacks {
        return Err(ContractBundleAssemblyError::new(
            ContractBundleAssemblyErrorKind::TargetIrSourceMismatch,
            "lawpacks",
            "bundle lawpacks do not match the supplied Core module",
        ));
    }

    Ok(())
}

fn canonicalize_digest_locked_resource_set(resources: &mut Vec<DigestLockedResource>) {
    resources.sort_by(|left, right| {
        left.coordinate()
            .cmp(right.coordinate())
            .then_with(|| left.digest_str().cmp(right.digest_str()))
    });
    resources.dedup();
}

fn canonical_resource_set(resources: &[ResourceRef]) -> Vec<(String, Option<String>)> {
    let mut resources = resources
        .iter()
        .map(|resource| (resource.coordinate.clone(), resource.digest.clone()))
        .collect::<Vec<_>>();
    resources.sort();
    resources.dedup();
    resources
}

fn require_matching_resource_set(
    actual: &[ResourceRef],
    expected: &[ResourceRef],
    field: &'static str,
    message: &'static str,
) -> Result<(), ContractBundleAssemblyError> {
    if canonical_resource_set(actual) != canonical_resource_set(expected) {
        return Err(ContractBundleAssemblyError::new(
            ContractBundleAssemblyErrorKind::TargetIrSourceMismatch,
            field,
            message,
        ));
    }
    Ok(())
}

fn target_profile_from_target_ir_artifact(
    target_profile: &ResourceRef,
) -> Result<DigestLockedResource, ContractBundleAssemblyError> {
    let Some(digest) = target_profile.digest.as_deref() else {
        return Err(ContractBundleAssemblyError::new(
            ContractBundleAssemblyErrorKind::InvalidDigest,
            "target_ir_artifact.target_profile",
            "digest-locked target profile",
        ));
    };
    DigestLockedResource::new_for_field(
        "target_ir_artifact.target_profile",
        target_profile.coordinate.clone(),
        digest,
    )
}

struct ContractBundleAssemblyParts {
    source_artifacts: Vec<SourceArtifactRef>,
    source_profile_semantic_facts: ResourceRef,
    core_ir: ResourceRef,
    core_digest: String,
    target_profile: ResourceRef,
    target_profile_digest: String,
    target_ir: ResourceRef,
    target_ir_digest: String,
    lawpacks: Vec<ResourceRef>,
    generated_artifacts: Vec<ResourceRef>,
    compiler: ResourceRef,
    lowerer: ResourceRef,
    verifier: ResourceRef,
    semantic_compile_options: ResourceRef,
    non_semantic_compile_options: ResourceRef,
    build_provenance: ResourceRef,
    canonicalization_profile: ResourceRef,
    conformance_fixture_corpora: Vec<ResourceRef>,
    verifier_report: ResourceRef,
    compile_explanation: ResourceRef,
    assurance_evidence: Vec<ContractBundleAssuranceEvidenceInput>,
}

fn assembly_parts(
    input: ContractBundleAssemblyInput,
) -> Result<ContractBundleAssemblyParts, ContractBundleAssemblyError> {
    let ContractBundleAssemblyInput {
        core_module,
        core_ir_coordinate,
        source_artifacts,
        source_profile_semantic_facts,
        target_profile,
        target_ir,
        lawpacks,
        generated_artifacts,
        compiler,
        lowerer,
        verifier,
        semantic_compile_options,
        non_semantic_compile_options,
        build_provenance,
        canonicalization_profile,
        conformance_fixture_corpora,
        verifier_report,
        compile_explanation,
        assurance_evidence,
    } = input;

    if core_ir_coordinate.is_empty() {
        return Err(ContractBundleAssemblyError::new(
            ContractBundleAssemblyErrorKind::EmptyCoordinate,
            "core_ir",
            "non-empty Core IR coordinate",
        ));
    }

    let core_digest = digest_core_module(&core_module)
        .map_err(|err| ContractBundleAssemblyError::canonical("core_module", &err))?;
    let core_digest = core_digest.to_review_string();
    let core_ir = ResourceRef {
        coordinate: core_ir_coordinate,
        digest: Some(core_digest.clone()),
    };
    let source_artifacts = source_artifacts
        .iter()
        .map(ContractBundleSourceArtifact::to_source_artifact_ref)
        .collect::<Vec<_>>();

    let source_profile_semantic_facts = source_profile_semantic_facts.to_resource_ref();
    let target_profile = target_profile.to_resource_ref();
    let target_profile_digest = required_digest(&target_profile).to_owned();
    let target_ir_digest = target_ir.digest_str().to_owned();
    let target_ir = target_ir.to_resource_ref();

    Ok(ContractBundleAssemblyParts {
        source_artifacts,
        source_profile_semantic_facts,
        core_ir,
        core_digest,
        target_profile,
        target_profile_digest,
        target_ir,
        target_ir_digest,
        lawpacks: resource_refs(&lawpacks),
        generated_artifacts: resource_refs(&generated_artifacts),
        compiler: compiler.to_resource_ref(),
        lowerer: lowerer.to_resource_ref(),
        verifier: verifier.to_resource_ref(),
        semantic_compile_options: semantic_compile_options.to_resource_ref(),
        non_semantic_compile_options: non_semantic_compile_options.to_resource_ref(),
        build_provenance: build_provenance.to_resource_ref(),
        canonicalization_profile: canonicalization_profile.to_resource_ref(),
        conformance_fixture_corpora: resource_refs(&conformance_fixture_corpora),
        verifier_report: verifier_report.to_resource_ref(),
        compile_explanation: compile_explanation.to_resource_ref(),
        assurance_evidence,
    })
}

fn semantic_bundle_digest(
    parts: &ContractBundleAssemblyParts,
) -> Result<String, ContractBundleAssemblyError> {
    let lawpack_digests = digest_strings(&parts.lawpacks);
    let generated_artifact_digests = digest_strings(&parts.generated_artifacts);
    let conformance_fixture_corpus_digests = digest_strings(&parts.conformance_fixture_corpora);
    let semantic_components = [
        BundlePreimageComponent::Digest(&parts.core_digest),
        BundlePreimageComponent::Digest(&parts.target_profile_digest),
        BundlePreimageComponent::Digest(&parts.target_ir_digest),
        BundlePreimageComponent::DigestList(&lawpack_digests),
        BundlePreimageComponent::Digest(required_digest(&parts.source_profile_semantic_facts)),
        BundlePreimageComponent::DigestList(&generated_artifact_digests),
        BundlePreimageComponent::Digest(required_digest(&parts.canonicalization_profile)),
        BundlePreimageComponent::Digest(required_digest(&parts.semantic_compile_options)),
        BundlePreimageComponent::DigestList(&conformance_fixture_corpus_digests),
        BundlePreimageComponent::Digest(required_digest(&parts.verifier_report)),
    ];
    digest_bundle_layer(BundleDigestDomain::Semantic, &semantic_components)
        .map_err(|err| ContractBundleAssemblyError::canonical("semantic_bundle_digest", &err))
        .map(|digest| digest.to_review_string())
}

fn release_bundle_digest(
    parts: &ContractBundleAssemblyParts,
    semantic_bundle_digest: &str,
) -> Result<String, ContractBundleAssemblyError> {
    let source_descriptors = parts
        .source_artifacts
        .iter()
        .map(|source| BundleSourceDescriptor {
            logical_path: source.logical_path.as_str(),
            artifact: &source.artifact,
        })
        .collect::<Vec<_>>();
    let release_components = [
        BundlePreimageComponent::Digest(semantic_bundle_digest),
        BundlePreimageComponent::SourceArtifacts(&source_descriptors),
        BundlePreimageComponent::Resource(&parts.compiler),
        BundlePreimageComponent::Resource(&parts.lowerer),
        BundlePreimageComponent::Resource(&parts.verifier),
        BundlePreimageComponent::Digest(required_digest(&parts.non_semantic_compile_options)),
        BundlePreimageComponent::Digest(required_digest(&parts.build_provenance)),
        BundlePreimageComponent::Digest(required_digest(&parts.compile_explanation)),
    ];
    digest_bundle_layer(BundleDigestDomain::Release, &release_components)
        .map_err(|err| ContractBundleAssemblyError::canonical("release_bundle_digest", &err))
        .map(|digest| digest.to_review_string())
}

fn manifest_from_parts(
    parts: ContractBundleAssemblyParts,
    semantic_bundle_digest: String,
    release_bundle_digest: String,
) -> ContractBundleManifest {
    let assurance_evidence = parts
        .assurance_evidence
        .iter()
        .map(|evidence| {
            let subject_digest = match evidence.subject_kind {
                BundleSubjectKind::Semantic => &semantic_bundle_digest,
                BundleSubjectKind::Release => &release_bundle_digest,
            };
            AssuranceEvidenceRef {
                role: evidence.role,
                artifact: evidence.artifact.to_resource_ref(),
                subject: BundleSubject {
                    kind: evidence.subject_kind,
                    digest: subject_digest.clone(),
                },
                target_profile_digest: parts.target_profile_digest.clone(),
                target_ir_digest: parts.target_ir_digest.clone(),
            }
        })
        .collect();

    ContractBundleManifest {
        api_version: CONTRACT_BUNDLE_API_VERSION.to_owned(),
        semantic_bundle_digest,
        release_bundle_digest,
        source_artifacts: parts.source_artifacts,
        source_profile_semantic_facts: parts.source_profile_semantic_facts,
        core_ir: parts.core_ir,
        target_profile: parts.target_profile,
        target_ir: parts.target_ir,
        lawpacks: parts.lawpacks,
        generated_artifacts: parts.generated_artifacts,
        compiler: parts.compiler,
        lowerer: parts.lowerer,
        verifier: parts.verifier,
        semantic_compile_options: parts.semantic_compile_options,
        non_semantic_compile_options: parts.non_semantic_compile_options,
        build_provenance: parts.build_provenance,
        canonicalization_profile: parts.canonicalization_profile,
        conformance_fixture_corpora: parts.conformance_fixture_corpora,
        verifier_report: parts.verifier_report,
        compile_explanation: parts.compile_explanation,
        assurance_evidence,
        admission_artifacts: Vec::new(),
    }
}

fn resource_refs(resources: &[DigestLockedResource]) -> Vec<ResourceRef> {
    resources
        .iter()
        .map(DigestLockedResource::to_resource_ref)
        .collect()
}

fn digest_strings(resources: &[ResourceRef]) -> Vec<String> {
    resources
        .iter()
        .map(|resource| required_digest(resource).to_owned())
        .collect()
}

fn required_digest(resource: &ResourceRef) -> &str {
    resource
        .digest
        .as_deref()
        .expect("digest-locked assembly resource has a digest")
}

/// Overall contract bundle validation classification for v1.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContractBundleValidationStatus {
    Valid,
    Invalid,
}

/// Stable failure categories returned by contract bundle checks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContractBundleValidationFailureKind {
    InvalidApiVersion,
    InvalidBundleDigest,
    EmptyArtifactSet,
    InvalidArtifactReference,
    InvalidSourcePath,
    UnsupportedCanonicalizationProfile,
    AssuranceSubjectMismatch,
    AssuranceTargetProfileMismatch,
    AssuranceTargetIrMismatch,
    AdmissionArtifactUnsupported,
}

/// One failed contract bundle validation obligation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContractBundleValidationFailure {
    pub kind: ContractBundleValidationFailureKind,
    pub field: String,
    pub obligation: String,
}

/// Complete v1 contract bundle validation report.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContractBundleValidationReport {
    pub status: ContractBundleValidationStatus,
    pub semantic_bundle_digest: String,
    pub release_bundle_digest: String,
    pub failures: Vec<ContractBundleValidationFailure>,
}

/// Validate a typed contract bundle manifest against the v1 boundary.
#[must_use]
pub fn validate_contract_bundle_manifest(
    manifest: &ContractBundleManifest,
) -> ContractBundleValidationReport {
    let mut failures = Vec::new();

    check_manifest_identity(manifest, &mut failures);
    check_required_artifact_sets(manifest, &mut failures);
    check_source_artifacts(manifest, &mut failures);
    check_artifact_refs(manifest, &mut failures);
    check_assurance_evidence(manifest, &mut failures);
    check_admission_exclusion(manifest, &mut failures);

    let status = if failures.is_empty() {
        ContractBundleValidationStatus::Valid
    } else {
        ContractBundleValidationStatus::Invalid
    };

    ContractBundleValidationReport {
        status,
        semantic_bundle_digest: manifest.semantic_bundle_digest.clone(),
        release_bundle_digest: manifest.release_bundle_digest.clone(),
        failures,
    }
}

fn check_manifest_identity(
    manifest: &ContractBundleManifest,
    failures: &mut Vec<ContractBundleValidationFailure>,
) {
    if manifest.api_version != CONTRACT_BUNDLE_API_VERSION {
        push_failure(
            failures,
            ContractBundleValidationFailureKind::InvalidApiVersion,
            "api_version",
            CONTRACT_BUNDLE_API_VERSION,
        );
    }
    check_digest(
        "semantic_bundle_digest",
        &manifest.semantic_bundle_digest,
        failures,
    );
    check_digest(
        "release_bundle_digest",
        &manifest.release_bundle_digest,
        failures,
    );
}

fn check_required_artifact_sets(
    manifest: &ContractBundleManifest,
    failures: &mut Vec<ContractBundleValidationFailure>,
) {
    if manifest.source_artifacts.is_empty() {
        push_failure(
            failures,
            ContractBundleValidationFailureKind::EmptyArtifactSet,
            "source_artifacts",
            "at least one digest-locked source artifact reference",
        );
    }
}

fn check_source_artifacts(
    manifest: &ContractBundleManifest,
    failures: &mut Vec<ContractBundleValidationFailure>,
) {
    for source in &manifest.source_artifacts {
        if !is_logical_package_relative_path(&source.logical_path) {
            push_failure(
                failures,
                ContractBundleValidationFailureKind::InvalidSourcePath,
                "source_artifacts.logical_path",
                "logical package-relative path",
            );
        }
        check_digest_locked_resource("source_artifacts.artifact", &source.artifact, failures);
    }
}

fn check_artifact_refs(
    manifest: &ContractBundleManifest,
    failures: &mut Vec<ContractBundleValidationFailure>,
) {
    for (field, resource) in [
        (
            "source_profile_semantic_facts",
            &manifest.source_profile_semantic_facts,
        ),
        ("core_ir", &manifest.core_ir),
        ("target_profile", &manifest.target_profile),
        ("target_ir", &manifest.target_ir),
        ("compiler", &manifest.compiler),
        ("lowerer", &manifest.lowerer),
        ("verifier", &manifest.verifier),
        (
            "semantic_compile_options",
            &manifest.semantic_compile_options,
        ),
        (
            "non_semantic_compile_options",
            &manifest.non_semantic_compile_options,
        ),
        ("build_provenance", &manifest.build_provenance),
        (
            "canonicalization_profile",
            &manifest.canonicalization_profile,
        ),
        ("verifier_report", &manifest.verifier_report),
        ("compile_explanation", &manifest.compile_explanation),
    ] {
        check_digest_locked_resource(field, resource, failures);
    }

    if manifest.canonicalization_profile.coordinate != CANONICAL_CBOR_ABI {
        push_failure(
            failures,
            ContractBundleValidationFailureKind::UnsupportedCanonicalizationProfile,
            "canonicalization_profile",
            CANONICAL_CBOR_ABI,
        );
    }

    check_resource_list("lawpacks", &manifest.lawpacks, failures);
    check_resource_list(
        "generated_artifacts",
        &manifest.generated_artifacts,
        failures,
    );
    check_resource_list(
        "conformance_fixture_corpora",
        &manifest.conformance_fixture_corpora,
        failures,
    );
}

fn check_assurance_evidence(
    manifest: &ContractBundleManifest,
    failures: &mut Vec<ContractBundleValidationFailure>,
) {
    for evidence in &manifest.assurance_evidence {
        check_one_assurance_evidence(manifest, evidence, failures);
    }
}

fn check_one_assurance_evidence(
    manifest: &ContractBundleManifest,
    evidence: &AssuranceEvidenceRef,
    failures: &mut Vec<ContractBundleValidationFailure>,
) {
    check_digest_locked_resource("assurance_evidence.artifact", &evidence.artifact, failures);
    let expected_subject_digest = bundle_subject_digest(manifest, evidence.subject.kind);
    if is_bundle_digest(expected_subject_digest)
        && evidence.subject.digest != expected_subject_digest
    {
        push_failure(
            failures,
            ContractBundleValidationFailureKind::AssuranceSubjectMismatch,
            assurance_role_field(evidence.role),
            "evidence subject digest matches selected bundle digest",
        );
    }
    if digest_locked_value(&manifest.target_profile)
        .is_some_and(|digest| digest != evidence.target_profile_digest)
    {
        push_failure(
            failures,
            ContractBundleValidationFailureKind::AssuranceTargetProfileMismatch,
            assurance_role_field(evidence.role),
            "evidence target profile digest matches bundle target profile digest",
        );
    }
    if digest_locked_value(&manifest.target_ir)
        .is_some_and(|digest| digest != evidence.target_ir_digest)
    {
        push_failure(
            failures,
            ContractBundleValidationFailureKind::AssuranceTargetIrMismatch,
            assurance_role_field(evidence.role),
            "evidence target IR digest matches bundle target IR digest",
        );
    }
}

fn check_admission_exclusion(
    manifest: &ContractBundleManifest,
    failures: &mut Vec<ContractBundleValidationFailure>,
) {
    if !manifest.admission_artifacts.is_empty() {
        push_failure(
            failures,
            ContractBundleValidationFailureKind::AdmissionArtifactUnsupported,
            "admission_artifacts",
            "admission requests, receipts, policies, and signatures live outside the bundle",
        );
    }
}

fn check_resource_list(
    field: &str,
    resources: &[ResourceRef],
    failures: &mut Vec<ContractBundleValidationFailure>,
) {
    for resource in resources {
        check_digest_locked_resource(field, resource, failures);
    }
}

fn check_digest_locked_resource(
    field: &str,
    resource: &ResourceRef,
    failures: &mut Vec<ContractBundleValidationFailure>,
) {
    if !is_bundle_digest_locked_resource(resource) {
        push_failure(
            failures,
            ContractBundleValidationFailureKind::InvalidArtifactReference,
            field,
            "non-empty coordinate and lowercase sha256 digest",
        );
    }
}

fn check_digest(field: &str, digest: &str, failures: &mut Vec<ContractBundleValidationFailure>) {
    if !is_bundle_digest(digest) {
        push_failure(
            failures,
            ContractBundleValidationFailureKind::InvalidBundleDigest,
            field,
            "sha256:<64 lowercase hex> digest",
        );
    }
}

fn digest_locked_value(resource: &ResourceRef) -> Option<&str> {
    resource
        .digest
        .as_deref()
        .filter(|digest| !resource.coordinate.is_empty() && is_bundle_digest(digest))
}

fn is_bundle_digest_locked_resource(resource: &ResourceRef) -> bool {
    !resource.coordinate.is_empty() && resource.digest.as_deref().is_some_and(is_bundle_digest)
}

fn is_bundle_digest(digest: &str) -> bool {
    let Some(hex) = digest.strip_prefix("sha256:") else {
        return false;
    };
    hex.len() == 64
        && hex
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

fn bundle_subject_digest(manifest: &ContractBundleManifest, kind: BundleSubjectKind) -> &str {
    match kind {
        BundleSubjectKind::Semantic => &manifest.semantic_bundle_digest,
        BundleSubjectKind::Release => &manifest.release_bundle_digest,
    }
}

fn assurance_role_field(role: AssuranceRole) -> &'static str {
    match role {
        AssuranceRole::Holmes => "assurance_evidence.holmes",
        AssuranceRole::Watson => "assurance_evidence.watson",
        AssuranceRole::Moriarty => "assurance_evidence.moriarty",
    }
}

fn push_failure(
    failures: &mut Vec<ContractBundleValidationFailure>,
    kind: ContractBundleValidationFailureKind,
    field: impl Into<String>,
    obligation: impl Into<String>,
) {
    let failure = ContractBundleValidationFailure {
        kind,
        field: field.into(),
        obligation: obligation.into(),
    };
    if !failures.contains(&failure) {
        failures.push(failure);
    }
}

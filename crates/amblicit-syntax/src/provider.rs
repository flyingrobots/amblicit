//! Runtime-neutral target-provider manifest validation.
//!
//! A provider package is an assembled boundary: generated lawpacks, target
//! profiles, authority facts, and review artifacts plus provider-owned lowerer
//! and verifier components. This module validates the generic manifest and
//! provenance envelope. It does not load provider files, execute WIT
//! components, run target verifiers, or interpret runtime-specific semantics.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use serde::{Deserialize, Serialize};

use crate::core_ir::ResourceRef;
use crate::provider_invocation::ProviderInvocationKind;

/// Target-provider manifest ABI supported by this crate.
pub const TARGET_PROVIDER_MANIFEST_API_VERSION: &str = "edict.provider-manifest/v1";

/// Exact WIT package ABI authorized by a provider manifest.
pub const TARGET_PROVIDER_ABI: &str = "edict:target-provider@1.0.0";

/// Digest-covered identity attestation required from lowerer components.
pub const TARGET_PROVIDER_LOWERER_CONTRACT: &str = "edict:target-provider/lowerer@1.0.0";

/// Digest-covered identity attestation required from verifier components.
pub const TARGET_PROVIDER_VERIFIER_CONTRACT: &str = "edict:target-provider/verifier@1.0.0";

/// Typed provider manifest value for `edict.provider-manifest/v1`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TargetProviderManifest {
    pub api_version: String,
    pub provider_abi: String,
    pub provider: ResourceRef,
    pub artifacts: Vec<ProviderArtifactRef>,
    pub schema_bindings: Vec<ProviderSchemaBinding>,
}

/// Immutable binding from one artifact domain to a generated schema role.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderSchemaBinding {
    pub domain: String,
    pub schema_role: String,
    pub format: ProviderSchemaFormat,
    pub root_rule: String,
}

/// Schema formats accepted by the provider host alpha.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ProviderSchemaFormat {
    /// A complete CDDL v1 document requiring no external rule resolution.
    SelfContainedCddlV1,
}

/// One artifact or component exposed by a target-provider package.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderArtifactRef {
    /// Unique package role slot, such as `lawpack.echo-dpo` or `lowerer.echo`.
    pub role: String,
    pub artifact_kind: ProviderArtifactKind,
    pub resource: ResourceRef,
    pub source: ProviderArtifactSource,
}

/// Runtime-neutral artifact categories Edict can route without interpreting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ProviderArtifactKind {
    Lawpack,
    TargetProfile,
    AuthorityFacts,
    ProviderManifest,
    ReviewArtifact,
    GeneratedArtifactProfile,
    GenerationProvenance,
    ArtifactSchema,
    Lowerer,
    Verifier,
}

impl ProviderArtifactKind {
    const fn requires_generated_source(self) -> bool {
        matches!(
            self,
            Self::Lawpack
                | Self::TargetProfile
                | Self::AuthorityFacts
                | Self::ProviderManifest
                | Self::ReviewArtifact
                | Self::GeneratedArtifactProfile
                | Self::GenerationProvenance
                | Self::ArtifactSchema
        )
    }

    const fn requires_component_source(self) -> bool {
        matches!(self, Self::Lowerer | Self::Verifier)
    }
}

/// Provenance for a provider artifact.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum ProviderArtifactSource {
    /// Generated from runtime-owned semantic source by a digest-locked generator.
    Generated {
        #[serde(rename = "semanticSource")]
        semantic_source: ResourceRef,
        generator: ResourceRef,
    },
    /// Provider-owned executable component, such as a lowerer or verifier.
    Component { component: ResourceRef },
}

/// Overall provider-manifest validation classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderManifestValidationStatus {
    Valid,
    Invalid,
}

/// Stable failure categories returned by provider-manifest validation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderManifestValidationFailureKind {
    InvalidApiVersion,
    InvalidProviderAbi,
    NonDigestLockedProvider,
    MissingArtifact,
    MissingRole,
    UnsafeArtifactRole,
    DuplicateArtifactRole,
    NonDigestLockedArtifact,
    NonDigestLockedGeneratedSource,
    NonDigestLockedGenerator,
    NonDigestLockedComponent,
    GeneratedRoleRequiresGeneratedSource,
    ComponentRoleRequiresComponentSource,
    ComponentResourceMismatch,
    MissingSchemaBinding,
    MissingSchemaDomain,
    DuplicateSchemaDomain,
    OutOfOrderSchemaDomain,
    MissingSchemaRole,
    UnknownSchemaRole,
    SchemaRoleKindMismatch,
    MissingSchemaRootRule,
}

/// One failed provider-manifest validation obligation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderManifestValidationFailure {
    pub kind: ProviderManifestValidationFailureKind,
    pub field: String,
    pub obligation: String,
}

/// Complete provider-manifest validation report.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderManifestValidationReport {
    pub status: ProviderManifestValidationStatus,
    pub provider: String,
    pub failures: Vec<ProviderManifestValidationFailure>,
}

/// Validate a typed target-provider manifest value against the v1 contract.
///
/// This check is deliberately runtime-neutral. It validates provider package
/// envelope, provenance, and digest-locking rules; it does not decide whether a
/// lawpack, target profile, lowerer, verifier, or generated artifact is
/// semantically correct for a runtime.
#[must_use]
pub fn validate_target_provider_manifest(
    manifest: &TargetProviderManifest,
) -> ProviderManifestValidationReport {
    let mut failures = Vec::new();

    if manifest.api_version != TARGET_PROVIDER_MANIFEST_API_VERSION {
        push_failure(
            &mut failures,
            ProviderManifestValidationFailureKind::InvalidApiVersion,
            "api_version",
            TARGET_PROVIDER_MANIFEST_API_VERSION,
        );
    }

    if manifest.provider_abi != TARGET_PROVIDER_ABI {
        push_failure(
            &mut failures,
            ProviderManifestValidationFailureKind::InvalidProviderAbi,
            "provider_abi",
            TARGET_PROVIDER_ABI,
        );
    }

    if !is_digest_locked_resource(&manifest.provider) {
        push_failure(
            &mut failures,
            ProviderManifestValidationFailureKind::NonDigestLockedProvider,
            "provider",
            "non-empty coordinate and lowercase sha256 digest",
        );
    }

    if manifest.artifacts.is_empty() {
        push_failure(
            &mut failures,
            ProviderManifestValidationFailureKind::MissingArtifact,
            "artifacts",
            "at least one provider artifact or component",
        );
    }

    check_artifacts(&manifest.artifacts, &mut failures);
    if !manifest.artifacts.is_empty() {
        check_schema_bindings(
            &manifest.artifacts,
            &manifest.schema_bindings,
            &mut failures,
        );
    }

    let status = if failures.is_empty() {
        ProviderManifestValidationStatus::Valid
    } else {
        ProviderManifestValidationStatus::Invalid
    };

    ProviderManifestValidationReport {
        status,
        provider: manifest.provider.coordinate.clone(),
        failures,
    }
}

fn check_artifacts(
    artifacts: &[ProviderArtifactRef],
    failures: &mut Vec<ProviderManifestValidationFailure>,
) {
    let mut roles = BTreeSet::new();

    for artifact in artifacts {
        if artifact.role.is_empty() {
            push_failure(
                failures,
                ProviderManifestValidationFailureKind::MissingRole,
                "artifacts.role",
                "non-empty unique artifact role",
            );
        } else if !is_safe_artifact_role(&artifact.role) {
            push_failure(
                failures,
                ProviderManifestValidationFailureKind::UnsafeArtifactRole,
                "artifacts.role",
                "path-neutral artifact role without separators, traversal, drive syntax, or control characters",
            );
        } else if !roles.insert(artifact.role.as_str()) {
            push_failure(
                failures,
                ProviderManifestValidationFailureKind::DuplicateArtifactRole,
                "artifacts.role",
                "unique artifact role",
            );
        }

        if !is_digest_locked_resource(&artifact.resource) {
            push_failure(
                failures,
                ProviderManifestValidationFailureKind::NonDigestLockedArtifact,
                "artifacts.resource",
                "non-empty coordinate and lowercase sha256 digest",
            );
        }

        match &artifact.source {
            ProviderArtifactSource::Generated {
                semantic_source,
                generator,
            } => {
                if artifact.artifact_kind.requires_component_source() {
                    push_failure(
                        failures,
                        ProviderManifestValidationFailureKind::ComponentRoleRequiresComponentSource,
                        "artifacts.source",
                        "lowerer and verifier artifacts must use component provenance",
                    );
                }
                if !is_digest_locked_resource(semantic_source) {
                    push_failure(
                        failures,
                        ProviderManifestValidationFailureKind::NonDigestLockedGeneratedSource,
                        "artifacts.source.semantic_source",
                        "non-empty coordinate and lowercase sha256 digest",
                    );
                }
                if !is_digest_locked_resource(generator) {
                    push_failure(
                        failures,
                        ProviderManifestValidationFailureKind::NonDigestLockedGenerator,
                        "artifacts.source.generator",
                        "non-empty coordinate and lowercase sha256 digest",
                    );
                }
            }
            ProviderArtifactSource::Component { component } => {
                if artifact.artifact_kind.requires_generated_source() {
                    push_failure(
                        failures,
                        ProviderManifestValidationFailureKind::GeneratedRoleRequiresGeneratedSource,
                        "artifacts.source",
                        "metadata artifacts must use generated provenance",
                    );
                }
                if !is_digest_locked_resource(component) {
                    push_failure(
                        failures,
                        ProviderManifestValidationFailureKind::NonDigestLockedComponent,
                        "artifacts.source.component",
                        "non-empty coordinate and lowercase sha256 digest",
                    );
                }
                if artifact.artifact_kind.requires_component_source()
                    && is_digest_locked_resource(component)
                    && is_digest_locked_resource(&artifact.resource)
                    && component != &artifact.resource
                {
                    push_failure(
                        failures,
                        ProviderManifestValidationFailureKind::ComponentResourceMismatch,
                        "artifacts.source.component",
                        "component provenance must equal the artifact resource identity",
                    );
                }
            }
        }
    }
}

fn is_safe_artifact_role(role: &str) -> bool {
    !role.contains("..")
        && !role
            .chars()
            .any(|character| matches!(character, '/' | '\\' | ':') || character.is_control())
}

fn check_schema_bindings(
    artifacts: &[ProviderArtifactRef],
    bindings: &[ProviderSchemaBinding],
    failures: &mut Vec<ProviderManifestValidationFailure>,
) {
    if bindings.is_empty() {
        push_failure(
            failures,
            ProviderManifestValidationFailureKind::MissingSchemaBinding,
            "schema_bindings",
            "at least one explicit artifact-domain schema binding",
        );
        return;
    }

    let artifacts_by_role: BTreeMap<&str, &ProviderArtifactRef> = artifacts
        .iter()
        .filter(|artifact| !artifact.role.is_empty())
        .map(|artifact| (artifact.role.as_str(), artifact))
        .collect();
    let mut domains = BTreeSet::new();
    let mut previous_domain: Option<&str> = None;

    for binding in bindings {
        if binding.domain.is_empty() {
            push_failure(
                failures,
                ProviderManifestValidationFailureKind::MissingSchemaDomain,
                "schema_bindings.domain",
                "non-empty unique artifact domain",
            );
        } else {
            if !domains.insert(binding.domain.as_str()) {
                push_failure(
                    failures,
                    ProviderManifestValidationFailureKind::DuplicateSchemaDomain,
                    "schema_bindings.domain",
                    "unique artifact domain",
                );
            }
            if previous_domain.is_some_and(|previous| previous >= binding.domain.as_str()) {
                push_failure(
                    failures,
                    ProviderManifestValidationFailureKind::OutOfOrderSchemaDomain,
                    "schema_bindings.domain",
                    "strict ascending order by exact UTF-8 domain bytes",
                );
            }
            previous_domain = Some(binding.domain.as_str());
        }

        if binding.schema_role.is_empty() {
            push_failure(
                failures,
                ProviderManifestValidationFailureKind::MissingSchemaRole,
                "schema_bindings.schema_role",
                "non-empty artifact-schema role",
            );
        } else if let Some(artifact) = artifacts_by_role.get(binding.schema_role.as_str()) {
            if artifact.artifact_kind != ProviderArtifactKind::ArtifactSchema {
                push_failure(
                    failures,
                    ProviderManifestValidationFailureKind::SchemaRoleKindMismatch,
                    "schema_bindings.schema_role",
                    "role naming an artifactSchema entry",
                );
            }
        } else {
            push_failure(
                failures,
                ProviderManifestValidationFailureKind::UnknownSchemaRole,
                "schema_bindings.schema_role",
                "role naming an artifactSchema entry",
            );
        }

        if binding.root_rule.is_empty() {
            push_failure(
                failures,
                ProviderManifestValidationFailureKind::MissingSchemaRootRule,
                "schema_bindings.root_rule",
                "non-empty CDDL root rule",
            );
        }
    }
}

/// Opaque proof that a target-provider manifest passed all envelope checks.
#[derive(Debug, Clone, Copy)]
pub struct ValidatedTargetProviderManifest<'a> {
    manifest: &'a TargetProviderManifest,
}

impl<'a> ValidatedTargetProviderManifest<'a> {
    /// Return the validated manifest value.
    #[must_use]
    pub const fn manifest(&self) -> &'a TargetProviderManifest {
        self.manifest
    }
}

/// Validate a manifest and return an opaque proof on complete success.
///
/// # Errors
///
/// Returns the complete structured manifest report when any obligation fails.
pub fn bind_target_provider_manifest(
    manifest: &TargetProviderManifest,
) -> Result<ValidatedTargetProviderManifest<'_>, ProviderManifestValidationReport> {
    let report = validate_target_provider_manifest(manifest);
    if report.status == ProviderManifestValidationStatus::Valid {
        Ok(ValidatedTargetProviderManifest { manifest })
    } else {
        Err(report)
    }
}

/// Stable explicit component-selection failure kinds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderComponentSelectionFailureKind {
    ComponentRoleNotFound,
    ComponentKindMismatch,
}

/// Failure to select one manifest-authorized provider component role.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderComponentSelectionFailure {
    kind: ProviderComponentSelectionFailureKind,
    role: String,
}

impl ProviderComponentSelectionFailure {
    #[must_use]
    pub const fn kind(&self) -> ProviderComponentSelectionFailureKind {
        self.kind
    }

    #[must_use]
    pub fn role(&self) -> &str {
        &self.role
    }
}

impl fmt::Display for ProviderComponentSelectionFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{:?}: {}", self.kind, self.role)
    }
}

impl std::error::Error for ProviderComponentSelectionFailure {}

/// One explicitly selected component identity from a validated manifest.
#[derive(Debug, Clone, Copy)]
pub struct SelectedProviderComponent<'a> {
    manifest: &'a TargetProviderManifest,
    artifact: &'a ProviderArtifactRef,
    invocation: ProviderInvocationKind,
}

impl<'a> SelectedProviderComponent<'a> {
    #[must_use]
    pub const fn manifest(&self) -> &'a TargetProviderManifest {
        self.manifest
    }

    #[must_use]
    pub fn role(&self) -> &'a str {
        &self.artifact.role
    }

    #[must_use]
    pub const fn resource(&self) -> &'a ResourceRef {
        &self.artifact.resource
    }

    #[must_use]
    pub const fn invocation(&self) -> ProviderInvocationKind {
        self.invocation
    }

    #[must_use]
    pub const fn contract_identity(&self) -> &'static str {
        match self.invocation {
            ProviderInvocationKind::Lowering => TARGET_PROVIDER_LOWERER_CONTRACT,
            ProviderInvocationKind::Verification => TARGET_PROVIDER_VERIFIER_CONTRACT,
        }
    }
}

/// Select one lowerer or verifier role from a validated manifest.
///
/// # Errors
///
/// Returns a stable failure when the role is absent or has the wrong world.
pub fn select_provider_component<'a>(
    validated: &ValidatedTargetProviderManifest<'a>,
    role: &str,
    invocation: ProviderInvocationKind,
) -> Result<SelectedProviderComponent<'a>, ProviderComponentSelectionFailure> {
    let artifact = validated
        .manifest
        .artifacts
        .iter()
        .find(|artifact| artifact.role == role)
        .ok_or_else(|| ProviderComponentSelectionFailure {
            kind: ProviderComponentSelectionFailureKind::ComponentRoleNotFound,
            role: role.to_owned(),
        })?;
    let expected_kind = match invocation {
        ProviderInvocationKind::Lowering => ProviderArtifactKind::Lowerer,
        ProviderInvocationKind::Verification => ProviderArtifactKind::Verifier,
    };
    if artifact.artifact_kind != expected_kind {
        return Err(ProviderComponentSelectionFailure {
            kind: ProviderComponentSelectionFailureKind::ComponentKindMismatch,
            role: role.to_owned(),
        });
    }
    Ok(SelectedProviderComponent {
        manifest: validated.manifest,
        artifact,
        invocation,
    })
}

fn is_digest_locked_resource(resource: &ResourceRef) -> bool {
    !resource.coordinate.is_empty()
        && resource
            .digest
            .as_deref()
            .is_some_and(is_lowercase_sha256_review_digest)
}

fn is_lowercase_sha256_review_digest(digest: &str) -> bool {
    let Some(hex) = digest.strip_prefix("sha256:") else {
        return false;
    };
    hex.len() == 64
        && hex
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn push_failure(
    failures: &mut Vec<ProviderManifestValidationFailure>,
    kind: ProviderManifestValidationFailureKind,
    field: &str,
    obligation: &str,
) {
    failures.push(ProviderManifestValidationFailure {
        kind,
        field: field.to_owned(),
        obligation: obligation.to_owned(),
    });
}

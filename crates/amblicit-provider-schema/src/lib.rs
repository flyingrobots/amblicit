//! Immutable artifact-schema authority for external Edict providers.
//!
//! Registry construction consumes only a validated provider manifest and
//! explicit in-memory schema bytes. It verifies each raw schema digest and
//! compiles every CDDL document before an external component can run. Selected
//! roots must belong to the host's total, non-generic validation subset.
//! Recursive variable occurrences use finite specialization, while competing
//! recursive alternatives additionally require an Edict-owned discriminator.
//! Construction proves the specializer total for the admitted shape, and
//! validation selects only the finite arm graph before invoking the exact
//! native validator.
//! Validation performs no discovery, file access, network access, clock,
//! randomness, environment inspection, or mutable-global lookup.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::sync::Arc;

use amblicit_syntax::{
    encode_canonical_cbor, CanonicalValue, ProviderArtifactSchemaValidationErrorKind,
    ProviderArtifactSchemaValidator, ProviderArtifactSource, ProviderSchemaBinding,
    ProviderSchemaFormat, ResourceRef, ValidatedTargetProviderManifest,
};
use cddl_cat::cbor::validate_cbor;
use cddl_cat::context::BasicContext;
use cddl_cat::flatten::flatten_from_str;
use sha2::{Digest, Sha256};

mod contract_pack;
mod recursive_dispatch;
mod schema_safety;

pub use contract_pack::{
    assemble_provider_contract_pack, validate_provider_contract_pack_manifest,
    ProviderContractDomainBinding, ProviderContractInstanceValidationErrorKind,
    ProviderContractPack, ProviderContractPackFailure, ProviderContractPackFailureKind,
    ProviderContractPackInput, ProviderContractPackManifest, ProviderContractPackResource,
    ProviderContractRootBinding, PROVIDER_CONTRACT_PACK_API_VERSION,
    PROVIDER_CONTRACT_PACK_COORDINATE, PROVIDER_CONTRACT_PACK_LICENSE, SOURCE_CLOSURE_CDDL_ROOT,
    TARGET_IR_ARTIFACT_CDDL_ROOT,
};

/// Maximum canonical container nesting admitted before native CDDL validation.
pub const PROVIDER_SCHEMA_VALIDATION_MAX_NESTING_DEPTH: usize = 50;

/// Explicit bytes alleged to implement one manifest schema role.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedProviderSchemaArtifact {
    pub role: String,
    pub bytes: Arc<[u8]>,
}

/// Stable construction failure categories for the immutable registry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderSchemaRegistryFailureKind {
    SchemaArtifactMissing,
    SchemaArtifactAmbiguous,
    SchemaArtifactUnsupported,
    SchemaArtifactDigestMismatch,
    SchemaCompileFailed,
    SchemaRootRuleMissing,
    RequiredDomainMissing,
}

/// One deterministic registry-construction failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderSchemaRegistryFailure {
    kind: ProviderSchemaRegistryFailureKind,
    domain: Option<String>,
    schema_role: Option<String>,
}

impl ProviderSchemaRegistryFailure {
    #[must_use]
    pub const fn kind(&self) -> ProviderSchemaRegistryFailureKind {
        self.kind
    }

    #[must_use]
    pub fn domain(&self) -> Option<&str> {
        self.domain.as_deref()
    }

    #[must_use]
    pub fn schema_role(&self) -> Option<&str> {
        self.schema_role.as_deref()
    }

    fn for_role(kind: ProviderSchemaRegistryFailureKind, schema_role: &str) -> Self {
        Self {
            kind,
            domain: None,
            schema_role: Some(schema_role.to_owned()),
        }
    }

    fn for_binding(
        kind: ProviderSchemaRegistryFailureKind,
        domain: &str,
        schema_role: &str,
    ) -> Self {
        Self {
            kind,
            domain: Some(domain.to_owned()),
            schema_role: Some(schema_role.to_owned()),
        }
    }

    fn for_domain(kind: ProviderSchemaRegistryFailureKind, domain: &str) -> Self {
        Self {
            kind,
            domain: Some(domain.to_owned()),
            schema_role: None,
        }
    }
}

impl fmt::Display for ProviderSchemaRegistryFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{:?}", self.kind)?;
        if let Some(domain) = &self.domain {
            write!(formatter, ": domain {domain}")?;
        }
        if let Some(role) = &self.schema_role {
            write!(formatter, ": schema role {role}")?;
        }
        Ok(())
    }
}

impl std::error::Error for ProviderSchemaRegistryFailure {}

/// Immutable evidence for one compiled domain binding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderSchemaRegistryBinding {
    pub domain: String,
    pub schema_role: String,
    pub schema: ResourceRef,
    pub source: ProviderArtifactSource,
    pub format: ProviderSchemaFormat,
    pub root_rule: String,
}

struct CompiledSchema {
    context: Arc<BasicContext>,
    root_rule: String,
    recursive_dispatch: recursive_dispatch::RecursiveDispatchRequirement,
}

/// Complete immutable mapping from artifact domains to compiled schemas.
pub struct ProviderArtifactSchemaRegistry {
    manifest: amblicit_syntax::TargetProviderManifest,
    schemas: BTreeMap<String, CompiledSchema>,
    bindings: Vec<ProviderSchemaRegistryBinding>,
}

impl fmt::Debug for ProviderArtifactSchemaRegistry {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ProviderArtifactSchemaRegistry")
            .field("bindings", &self.bindings)
            .field("provider", &self.manifest.provider)
            .finish_non_exhaustive()
    }
}

impl ProviderArtifactSchemaRegistry {
    /// Construct and compile a complete registry from explicit schema bytes.
    ///
    /// `required_domains` is the host-authored closure needed by the prepared
    /// invocation. Every manifest binding is still compiled so no latent,
    /// digest-locked schema failure can be deferred until after guest execution.
    ///
    /// # Errors
    ///
    /// Returns a stable failure for missing, ambiguous, or unsupported bytes,
    /// digest disagreement, invalid, incomplete, or non-total CDDL, a missing
    /// root, or an unbound required domain.
    pub fn from_manifest<'a>(
        validated: &ValidatedTargetProviderManifest<'_>,
        resolved: impl IntoIterator<Item = ResolvedProviderSchemaArtifact>,
        required_domains: impl IntoIterator<Item = &'a str>,
    ) -> Result<Self, ProviderSchemaRegistryFailure> {
        let resolved_by_role = collect_resolved_by_role(resolved)?;
        let manifest = validated.manifest();
        reject_unsupported_roles(manifest, &resolved_by_role)?;
        let artifacts_by_role: BTreeMap<&str, _> = manifest
            .artifacts
            .iter()
            .map(|artifact| (artifact.role.as_str(), artifact))
            .collect();
        let mut compiled_by_role: BTreeMap<String, Arc<BasicContext>> = BTreeMap::new();
        let mut schemas = BTreeMap::new();
        let mut bindings = Vec::with_capacity(manifest.schema_bindings.len());

        for binding in &manifest.schema_bindings {
            let Some(artifact) = artifacts_by_role.get(binding.schema_role.as_str()) else {
                return Err(ProviderSchemaRegistryFailure::for_binding(
                    ProviderSchemaRegistryFailureKind::SchemaCompileFailed,
                    &binding.domain,
                    &binding.schema_role,
                ));
            };
            let resolved = resolved_by_role.get(&binding.schema_role).ok_or_else(|| {
                ProviderSchemaRegistryFailure::for_binding(
                    ProviderSchemaRegistryFailureKind::SchemaArtifactMissing,
                    &binding.domain,
                    &binding.schema_role,
                )
            })?;
            let resolved_digest = digest_review_string(&resolved.bytes);
            if Some(resolved_digest.as_str()) != artifact.resource.digest.as_deref() {
                return Err(ProviderSchemaRegistryFailure::for_binding(
                    ProviderSchemaRegistryFailureKind::SchemaArtifactDigestMismatch,
                    &binding.domain,
                    &binding.schema_role,
                ));
            }

            let context = compiled_context_for(binding, resolved, &mut compiled_by_role)?;
            if !context.rules.contains_key(&binding.root_rule) {
                return Err(ProviderSchemaRegistryFailure::for_binding(
                    ProviderSchemaRegistryFailureKind::SchemaRootRuleMissing,
                    &binding.domain,
                    &binding.schema_role,
                ));
            }
            if !schema_safety::root_supports_total_validation(&context.rules, &binding.root_rule) {
                return Err(ProviderSchemaRegistryFailure::for_binding(
                    ProviderSchemaRegistryFailureKind::SchemaCompileFailed,
                    &binding.domain,
                    &binding.schema_role,
                ));
            }
            let recursive_dispatch =
                recursive_dispatch::root_dispatch_requirement(&context.rules, &binding.root_rule)
                    .ok_or_else(|| {
                    ProviderSchemaRegistryFailure::for_binding(
                        ProviderSchemaRegistryFailureKind::SchemaCompileFailed,
                        &binding.domain,
                        &binding.schema_role,
                    )
                })?;

            schemas.insert(
                binding.domain.clone(),
                CompiledSchema {
                    context,
                    root_rule: binding.root_rule.clone(),
                    recursive_dispatch,
                },
            );
            bindings.push(ProviderSchemaRegistryBinding {
                domain: binding.domain.clone(),
                schema_role: binding.schema_role.clone(),
                schema: artifact.resource.clone(),
                source: artifact.source.clone(),
                format: binding.format,
                root_rule: binding.root_rule.clone(),
            });
        }

        let required: BTreeSet<&str> = required_domains.into_iter().collect();
        for domain in required {
            if !schemas.contains_key(domain) {
                return Err(ProviderSchemaRegistryFailure::for_domain(
                    ProviderSchemaRegistryFailureKind::RequiredDomainMissing,
                    domain,
                ));
            }
        }

        Ok(Self {
            manifest: manifest.clone(),
            schemas,
            bindings,
        })
    }

    /// Return the exact validated manifest that authorized this registry.
    #[must_use]
    pub const fn manifest(&self) -> &amblicit_syntax::TargetProviderManifest {
        &self.manifest
    }

    /// Return the sorted immutable registry receipt.
    #[must_use]
    pub fn bindings(&self) -> &[ProviderSchemaRegistryBinding] {
        &self.bindings
    }
}

fn collect_resolved_by_role(
    resolved: impl IntoIterator<Item = ResolvedProviderSchemaArtifact>,
) -> Result<BTreeMap<String, ResolvedProviderSchemaArtifact>, ProviderSchemaRegistryFailure> {
    let mut resolved_by_role = BTreeMap::new();
    for artifact in resolved {
        let role = artifact.role.clone();
        if resolved_by_role.insert(role.clone(), artifact).is_some() {
            return Err(ProviderSchemaRegistryFailure::for_role(
                ProviderSchemaRegistryFailureKind::SchemaArtifactAmbiguous,
                &role,
            ));
        }
    }
    Ok(resolved_by_role)
}

fn reject_unsupported_roles(
    manifest: &amblicit_syntax::TargetProviderManifest,
    resolved_by_role: &BTreeMap<String, ResolvedProviderSchemaArtifact>,
) -> Result<(), ProviderSchemaRegistryFailure> {
    let bound_schema_roles: BTreeSet<&str> = manifest
        .schema_bindings
        .iter()
        .map(|binding| binding.schema_role.as_str())
        .collect();
    if let Some(role) = resolved_by_role
        .keys()
        .find(|role| !bound_schema_roles.contains(role.as_str()))
    {
        return Err(ProviderSchemaRegistryFailure::for_role(
            ProviderSchemaRegistryFailureKind::SchemaArtifactUnsupported,
            role,
        ));
    }
    Ok(())
}

impl ProviderArtifactSchemaValidator for ProviderArtifactSchemaRegistry {
    fn supports_domain(&self, domain: &str) -> bool {
        self.schemas.contains_key(domain)
    }

    fn validate_canonical_value(
        &self,
        domain: &str,
        value: &CanonicalValue,
    ) -> Result<(), ProviderArtifactSchemaValidationErrorKind> {
        let schema = self
            .schemas
            .get(domain)
            .ok_or(ProviderArtifactSchemaValidationErrorKind::UnsupportedDomain)?;
        let native_rule = schema
            .context
            .rules
            .get(&schema.root_rule)
            .ok_or(ProviderArtifactSchemaValidationErrorKind::SchemaMismatch)?;
        let specialized;
        let rule = match schema.recursive_dispatch {
            recursive_dispatch::RecursiveDispatchRequirement::Native => native_rule,
            recursive_dispatch::RecursiveDispatchRequirement::Specialized => {
                specialized = recursive_dispatch::specialize_root_for_value(
                    schema.context.as_ref(),
                    &schema.root_rule,
                    value,
                )
                .ok_or(ProviderArtifactSchemaValidationErrorKind::SchemaMismatch)?;
                &specialized
            }
        };
        if !value_within_provider_schema_nesting_limit(value, 0) {
            return Err(ProviderArtifactSchemaValidationErrorKind::SchemaMismatch);
        }
        let bytes = encode_canonical_cbor(value)
            .map_err(|_| ProviderArtifactSchemaValidationErrorKind::SchemaMismatch)?;
        let cbor_value: ciborium::Value = ciborium::from_reader(bytes.as_slice())
            .map_err(|_| ProviderArtifactSchemaValidationErrorKind::SchemaMismatch)?;
        validate_cbor(rule, &cbor_value, schema.context.as_ref())
            .map_err(|_| ProviderArtifactSchemaValidationErrorKind::SchemaMismatch)
    }
}

fn value_within_provider_schema_nesting_limit(value: &CanonicalValue, depth: usize) -> bool {
    match value {
        CanonicalValue::Array(values) => {
            if depth >= PROVIDER_SCHEMA_VALIDATION_MAX_NESTING_DEPTH {
                return false;
            }
            values.iter().all(|value| {
                value_within_provider_schema_nesting_limit(value, depth.saturating_add(1))
            })
        }
        CanonicalValue::Map(entries) => {
            if depth >= PROVIDER_SCHEMA_VALIDATION_MAX_NESTING_DEPTH {
                return false;
            }
            entries.iter().all(|(key, value)| {
                let child_depth = depth.saturating_add(1);
                value_within_provider_schema_nesting_limit(key, child_depth)
                    && value_within_provider_schema_nesting_limit(value, child_depth)
            })
        }
        CanonicalValue::Null
        | CanonicalValue::Bool(_)
        | CanonicalValue::Integer(_)
        | CanonicalValue::Bytes(_)
        | CanonicalValue::Text(_) => depth <= PROVIDER_SCHEMA_VALIDATION_MAX_NESTING_DEPTH,
    }
}

fn digest_review_string(bytes: &[u8]) -> String {
    format!("sha256:{:x}", Sha256::digest(bytes))
}

fn compiled_context_for(
    binding: &ProviderSchemaBinding,
    resolved: &ResolvedProviderSchemaArtifact,
    compiled_by_role: &mut BTreeMap<String, Arc<BasicContext>>,
) -> Result<Arc<BasicContext>, ProviderSchemaRegistryFailure> {
    if let Some(context) = compiled_by_role.get(&binding.schema_role) {
        return Ok(Arc::clone(context));
    }
    let failure = || {
        ProviderSchemaRegistryFailure::for_binding(
            ProviderSchemaRegistryFailureKind::SchemaCompileFailed,
            &binding.domain,
            &binding.schema_role,
        )
    };
    let source = std::str::from_utf8(&resolved.bytes).map_err(|_| failure())?;
    let rules = flatten_from_str(source).map_err(|_| failure())?;
    let context = Arc::new(BasicContext::new(rules));
    compiled_by_role.insert(binding.schema_role.clone(), Arc::clone(&context));
    Ok(context)
}

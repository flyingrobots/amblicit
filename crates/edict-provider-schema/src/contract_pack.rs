//! Deterministic assembly of the Rust-neutral provider contract pack.

use std::fmt;
use std::sync::Arc;

use cddl_cat::cbor::validate_cbor;
use cddl_cat::context::BasicContext;
use cddl_cat::flatten::flatten_from_str;
use cddl_cat::ivt::{Control, Node, Rule, RulesByName};
use edict_syntax::source_closure::SOURCE_CLOSURE_DIGEST_DOMAIN;
use edict_syntax::{
    encode_canonical_cbor, validate_target_profile_contract_resources, CanonicalValue,
    ProviderArtifactSchemaValidationErrorKind, TargetProfileContractResource,
    TargetProfileContractResourceFailureKind, TargetProfileContractResourceProvenance,
    AUTHORITY_FACTS_API_VERSION, CORE_MODULE_DIGEST_DOMAIN, PROVIDER_LAWPACK_ARTIFACT_DOMAIN,
    RESULT_PROJECTION_CDDL_ROOT, RESULT_PROJECTION_DIGEST_DOMAIN, TARGET_IR_ARTIFACT_DIGEST_DOMAIN,
    TARGET_PROFILE_API_VERSION,
};
use sha2::{Digest, Sha256};

/// Semantic API version of the generated provider contract-pack manifest.
pub const PROVIDER_CONTRACT_PACK_API_VERSION: &str = "edict.provider-contract-pack/v1";

/// Coordinate of the self-contained CDDL artifact carried by the pack.
pub const PROVIDER_CONTRACT_PACK_COORDINATE: &str = "edict.provider-contract-pack.cddl@1";

/// SPDX license expression carried by both generated pack artifacts.
pub const PROVIDER_CONTRACT_PACK_LICENSE: &str = "Apache-2.0";

/// CDDL root matching Edict's canonical Target IR artifact value.
pub const TARGET_IR_ARTIFACT_CDDL_ROOT: &str = "target-ir-artifact";

const PACK_HEADER: &str = "; SPDX-License-Identifier: Apache-2.0\n\
; edict-provider-contracts.cddl\n\
; Generated from Edict-owned ABI fragments. DO NOT EDIT.\n";

const CONTRACT_BINDINGS: [(&str, &str); 11] = [
    ("authority-facts", "authority-facts"),
    ("core-module", "core-module"),
    ("lawpack-adapter", "lawpack-adapter"),
    ("lawpack-exports", "lawpack-exports"),
    ("lawpack-manifest", "lawpack-manifest"),
    ("lowering-requirements", "lowering-requirements"),
    ("result-projection", RESULT_PROJECTION_CDDL_ROOT),
    ("target-ir-artifact", TARGET_IR_ARTIFACT_CDDL_ROOT),
    ("target-profile-intrinsics", "intrinsics-document"),
    ("target-profile-manifest", "target-profile-manifest"),
    (
        "target-profile-operation-profiles",
        "operation-profiles-document",
    ),
];

const DOMAIN_BINDINGS: [(&str, &str); 7] = [
    (AUTHORITY_FACTS_API_VERSION, "authority-facts"),
    (CORE_MODULE_DIGEST_DOMAIN, "core-module"),
    (PROVIDER_LAWPACK_ARTIFACT_DOMAIN, "lawpack-manifest"),
    ("edict.lowering-requirements/v1", "lowering-requirements"),
    (RESULT_PROJECTION_DIGEST_DOMAIN, RESULT_PROJECTION_CDDL_ROOT),
    (
        TARGET_IR_ARTIFACT_DIGEST_DOMAIN,
        TARGET_IR_ARTIFACT_CDDL_ROOT,
    ),
    (TARGET_PROFILE_API_VERSION, "target-profile-manifest"),
];

/// Explicit Edict-owned schema fragments and resource bytes used for assembly.
///
/// The assembler performs no file, registry, network, environment, or mutable
/// global lookup. Callers choose and supply every byte in this value.
#[derive(Debug)]
pub struct ProviderContractPackInput<'a> {
    pub common_cddl: &'a [u8],
    pub core_cddl: &'a [u8],
    pub lawpack_cddl: &'a [u8],
    pub lawpack_adapter_cddl: &'a [u8],
    pub target_profile_cddl: &'a [u8],
    pub authority_facts_cddl: &'a [u8],
    pub result_projection_cddl: &'a [u8],
    pub target_ir_cddl: &'a [u8],
    /// Explicitly select source-closure publication; None retains earlier bytes.
    pub source_closure_cddl: Option<&'a [u8]>,
    pub contract_resources: Vec<TargetProfileContractResource>,
}

/// One stable logical contract-to-CDDL-root binding.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct ProviderContractRootBinding {
    pub contract: String,
    pub root_rule: String,
}

/// One provider artifact-domain-to-CDDL-root binding.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct ProviderContractDomainBinding {
    pub domain: String,
    pub root_rule: String,
}

/// One exact Edict-owned target-profile contract resource in the pack.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderContractPackResource {
    pub coordinate: String,
    pub provenance: TargetProfileContractResourceProvenance,
    pub canonical_bytes: Vec<u8>,
    pub raw_sha256: [u8; 32],
    pub domain_framed_digest: String,
}

/// Deterministic transport manifest for the complete contract pack.
///
/// Public fields intentionally make this an untrusted transport value. Use
/// [`validate_provider_contract_pack_manifest`] before treating a supplied
/// manifest as equal to an assembled pack.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderContractPackManifest {
    pub api_version: String,
    pub coordinate: String,
    pub license: String,
    pub schema_bytes: Vec<u8>,
    pub schema_sha256: [u8; 32],
    pub contracts: Vec<ProviderContractRootBinding>,
    pub domains: Vec<ProviderContractDomainBinding>,
    pub resources: Vec<ProviderContractPackResource>,
}

/// Stable contract-pack assembly and manifest rejection categories.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ProviderContractPackFailureKind {
    SchemaFragmentMissing,
    SchemaFragmentInvalidUtf8,
    SchemaCompileFailed,
    SchemaExternalRuleUnresolved,
    SchemaControlUnsupported,
    SchemaRootMissing,
    ContractResourceMissing,
    ContractResourceUnknown,
    ContractResourceDuplicate,
    ContractResourceInvalidCanonicalArtifact,
    ContractResourceBytesMismatch,
    ContractResourceRawDigestMismatch,
    ContractResourceDomainFramedDigestMismatch,
    ContractResourceProvenanceMismatch,
    ManifestApiVersionMismatch,
    ManifestCoordinateMismatch,
    ManifestLicenseMismatch,
    ManifestSchemaBytesMismatch,
    ManifestSchemaDigestMismatch,
    ManifestContractBindingsMismatch,
    ManifestDomainBindingsMismatch,
    ManifestResourceOrderMismatch,
}

/// One deterministic contract-pack failure.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct ProviderContractPackFailure {
    kind: ProviderContractPackFailureKind,
    subject: String,
}

impl ProviderContractPackFailure {
    /// Return the stable machine-readable failure category.
    #[must_use]
    pub const fn kind(&self) -> ProviderContractPackFailureKind {
        self.kind
    }

    /// Return the fragment, root, resource, or manifest field that failed.
    #[must_use]
    pub fn subject(&self) -> &str {
        &self.subject
    }
}

impl fmt::Display for ProviderContractPackFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{:?}: {}", self.kind, self.subject)
    }
}

impl std::error::Error for ProviderContractPackFailure {}

/// Stable instance-validation categories for named non-domain contracts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderContractInstanceValidationErrorKind {
    UnknownContract,
    SchemaMismatch,
}

/// Complete compiled provider contract pack.
pub struct ProviderContractPack {
    manifest: ProviderContractPackManifest,
    context: Arc<BasicContext>,
}

impl fmt::Debug for ProviderContractPack {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ProviderContractPack")
            .field("manifest", &self.manifest)
            .finish_non_exhaustive()
    }
}

impl ProviderContractPack {
    /// Borrow the exact generated CDDL bytes.
    #[must_use]
    pub fn cddl_bytes(&self) -> &[u8] {
        &self.manifest.schema_bytes
    }

    /// Borrow the raw SHA-256 digest of the generated CDDL bytes.
    #[must_use]
    pub const fn raw_sha256(&self) -> &[u8; 32] {
        &self.manifest.schema_sha256
    }

    /// Borrow the deterministic typed manifest.
    #[must_use]
    pub const fn manifest(&self) -> &ProviderContractPackManifest {
        &self.manifest
    }

    /// Render deterministic review JSON with exactly one trailing newline.
    #[must_use]
    pub fn manifest_bytes(&self) -> Vec<u8> {
        render_manifest_json(&self.manifest).into_bytes()
    }

    /// Validate a canonical value against one named logical contract.
    ///
    /// # Errors
    ///
    /// Returns `UnknownContract` when the contract is not published by this
    /// pack and `SchemaMismatch` when the value does not satisfy its CDDL root.
    pub fn validate_contract(
        &self,
        contract: &str,
        value: &CanonicalValue,
    ) -> Result<(), ProviderContractInstanceValidationErrorKind> {
        let root = self
            .manifest
            .contracts
            .binary_search_by_key(&contract, |binding| binding.contract.as_str())
            .ok()
            .map(|index| self.manifest.contracts[index].root_rule.as_str())
            .ok_or(ProviderContractInstanceValidationErrorKind::UnknownContract)?;
        validate_root(&self.context, root, value)
            .map_err(|()| ProviderContractInstanceValidationErrorKind::SchemaMismatch)
    }

    /// Return whether this publication pack declares an owning domain root.
    #[must_use]
    pub fn supports_domain(&self, domain: &str) -> bool {
        self.manifest
            .domains
            .binary_search_by_key(&domain, |binding| binding.domain.as_str())
            .is_ok()
    }

    /// Validate generation-time evidence through one declared domain root.
    ///
    /// This checks the trusted Edict publication pack. It is deliberately not
    /// an implementation of the provider host's schema-validator capability:
    /// untrusted provider schemas must still cross the production registry's
    /// stricter structural-safety and manifest-authority boundary.
    ///
    /// # Errors
    ///
    /// Returns `UnsupportedDomain` for an undeclared domain and
    /// `SchemaMismatch` for a value outside the declared CDDL root.
    pub fn validate_domain(
        &self,
        domain: &str,
        value: &CanonicalValue,
    ) -> Result<(), ProviderArtifactSchemaValidationErrorKind> {
        let root = self
            .manifest
            .domains
            .binary_search_by_key(&domain, |binding| binding.domain.as_str())
            .ok()
            .map(|index| self.manifest.domains[index].root_rule.as_str())
            .ok_or(ProviderArtifactSchemaValidationErrorKind::UnsupportedDomain)?;
        validate_root(&self.context, root, value)
            .map_err(|()| ProviderArtifactSchemaValidationErrorKind::SchemaMismatch)
    }
}

/// Assemble and compile the complete provider contract pack from explicit bytes.
///
/// # Errors
///
/// Returns stable, sorted failures for missing or non-UTF-8 schema fragments,
/// CDDL compilation or root closure failures, or any incomplete or inauthentic
/// target-profile contract resource.
pub fn assemble_provider_contract_pack(
    input: ProviderContractPackInput<'_>,
) -> Result<ProviderContractPack, Vec<ProviderContractPackFailure>> {
    let fragments = schema_fragments(&input);
    let with_source_closure = input.source_closure_cddl.is_some();
    let mut failures = validate_fragments(&fragments);
    let resources = match validate_target_profile_contract_resources(input.contract_resources) {
        Ok(resources) => Some(resources.resources().to_vec()),
        Err(resource_failures) => {
            failures.extend(resource_failures.into_iter().map(map_resource_failure));
            None
        }
    };

    if failures.iter().any(|failure| {
        matches!(
            failure.kind,
            ProviderContractPackFailureKind::SchemaFragmentMissing
                | ProviderContractPackFailureKind::SchemaFragmentInvalidUtf8
        )
    }) {
        sort_failures(&mut failures);
        return Err(failures);
    }

    let schema_bytes = assemble_schema_bytes(&fragments);
    let schema = match std::str::from_utf8(&schema_bytes) {
        Ok(schema) => schema,
        Err(_err) => {
            failures.push(failure(
                ProviderContractPackFailureKind::SchemaCompileFailed,
                PROVIDER_CONTRACT_PACK_COORDINATE,
            ));
            sort_failures(&mut failures);
            return Err(failures);
        }
    };
    let context = match flatten_from_str(schema) {
        Ok(rules) => Some(Arc::new(BasicContext::new(rules))),
        Err(_err) => {
            failures.push(failure(
                ProviderContractPackFailureKind::SchemaCompileFailed,
                PROVIDER_CONTRACT_PACK_COORDINATE,
            ));
            None
        }
    };
    if let Some(context) = &context {
        if let Some(closure_failure) = unresolved_schema_reference(&context.rules) {
            let (kind, subject) = match closure_failure {
                SchemaClosureFailure::ExternalRule(reference) => (
                    ProviderContractPackFailureKind::SchemaExternalRuleUnresolved,
                    reference,
                ),
                SchemaClosureFailure::UnsupportedControl(control) => (
                    ProviderContractPackFailureKind::SchemaControlUnsupported,
                    control.to_owned(),
                ),
            };
            failures.push(failure(kind, subject));
        }
        for root in all_roots(with_source_closure) {
            if !context.rules.contains_key(root) {
                failures.push(failure(
                    ProviderContractPackFailureKind::SchemaRootMissing,
                    root,
                ));
            }
        }
    }

    if !failures.is_empty() {
        sort_failures(&mut failures);
        return Err(failures);
    }

    let (Some(context), Some(resources)) = (context, resources) else {
        failures.push(failure(
            ProviderContractPackFailureKind::SchemaCompileFailed,
            PROVIDER_CONTRACT_PACK_COORDINATE,
        ));
        sort_failures(&mut failures);
        return Err(failures);
    };
    let manifest = build_manifest(schema_bytes, resources, with_source_closure);
    Ok(ProviderContractPack { manifest, context })
}

fn schema_fragments<'a>(input: &ProviderContractPackInput<'a>) -> Vec<(&'static str, &'a [u8])> {
    let mut fragments = vec![
        ("edict-common.cddl", input.common_cddl),
        ("edict-core.cddl", input.core_cddl),
        ("edict-lawpack.cddl", input.lawpack_cddl),
        ("edict-lawpack-adapter.cddl", input.lawpack_adapter_cddl),
        ("edict-target-profile.cddl", input.target_profile_cddl),
        ("edict-authority-facts.cddl", input.authority_facts_cddl),
        ("edict-result-projection.cddl", input.result_projection_cddl),
        ("edict-target-ir.cddl", input.target_ir_cddl),
    ];
    if let Some(source_closure) = input.source_closure_cddl {
        fragments.push(("edict-source-closure.cddl", source_closure));
    }
    fragments
}

fn build_manifest(
    schema_bytes: Vec<u8>,
    resources: Vec<TargetProfileContractResource>,
    with_source_closure: bool,
) -> ProviderContractPackManifest {
    assert!(
        bindings_are_strictly_sorted_by_key(&CONTRACT_BINDINGS),
        "CONTRACT_BINDINGS must stay sorted and unique by contract name",
    );
    assert!(
        bindings_are_strictly_sorted_by_key(&DOMAIN_BINDINGS),
        "DOMAIN_BINDINGS must stay sorted and unique by domain",
    );
    let mut contracts = CONTRACT_BINDINGS
        .into_iter()
        .map(|(contract, root_rule)| ProviderContractRootBinding {
            contract: contract.to_owned(),
            root_rule: root_rule.to_owned(),
        })
        .collect::<Vec<_>>();
    let mut domains = DOMAIN_BINDINGS
        .into_iter()
        .map(|(domain, root_rule)| ProviderContractDomainBinding {
            domain: domain.to_owned(),
            root_rule: root_rule.to_owned(),
        })
        .collect::<Vec<_>>();
    if with_source_closure {
        contracts.push(ProviderContractRootBinding {
            contract: "source-closure".into(),
            root_rule: "source-closure".into(),
        });
        domains.push(ProviderContractDomainBinding {
            domain: SOURCE_CLOSURE_DIGEST_DOMAIN.into(),
            root_rule: "source-closure".into(),
        });
        contracts.sort_unstable();
        domains.sort_unstable();
    }
    ProviderContractPackManifest {
        api_version: PROVIDER_CONTRACT_PACK_API_VERSION.to_owned(),
        coordinate: PROVIDER_CONTRACT_PACK_COORDINATE.to_owned(),
        license: PROVIDER_CONTRACT_PACK_LICENSE.to_owned(),
        schema_sha256: Sha256::digest(&schema_bytes).into(),
        schema_bytes,
        contracts,
        domains,
        resources: resources.into_iter().map(pack_resource).collect(),
    }
}

fn bindings_are_strictly_sorted_by_key(bindings: &[(&str, &str)]) -> bool {
    bindings.windows(2).all(|pair| pair[0].0 < pair[1].0)
}

/// Validate an untrusted manifest against one already assembled authority pack.
///
/// # Errors
///
/// Returns stable, sorted failures for any metadata, schema byte, raw digest,
/// root binding, resource ordering, resource byte, resource digest, or resource
/// provenance disagreement. No partial authority is returned.
pub fn validate_provider_contract_pack_manifest(
    expected: &ProviderContractPack,
    supplied: &ProviderContractPackManifest,
) -> Result<(), Vec<ProviderContractPackFailure>> {
    let expected = expected.manifest();
    let mut failures = Vec::new();
    compare_field(
        supplied.api_version == expected.api_version,
        ProviderContractPackFailureKind::ManifestApiVersionMismatch,
        "apiVersion",
        &mut failures,
    );
    compare_field(
        supplied.coordinate == expected.coordinate,
        ProviderContractPackFailureKind::ManifestCoordinateMismatch,
        "coordinate",
        &mut failures,
    );
    compare_field(
        supplied.license == expected.license,
        ProviderContractPackFailureKind::ManifestLicenseMismatch,
        "license",
        &mut failures,
    );
    compare_field(
        supplied.schema_bytes == expected.schema_bytes,
        ProviderContractPackFailureKind::ManifestSchemaBytesMismatch,
        "schema.bytes",
        &mut failures,
    );
    let supplied_digest: [u8; 32] = Sha256::digest(&supplied.schema_bytes).into();
    compare_field(
        supplied.schema_sha256 == expected.schema_sha256
            && supplied.schema_sha256 == supplied_digest,
        ProviderContractPackFailureKind::ManifestSchemaDigestMismatch,
        "schema.rawSha256",
        &mut failures,
    );
    compare_field(
        supplied.contracts == expected.contracts,
        ProviderContractPackFailureKind::ManifestContractBindingsMismatch,
        "contracts",
        &mut failures,
    );
    compare_field(
        supplied.domains == expected.domains,
        ProviderContractPackFailureKind::ManifestDomainBindingsMismatch,
        "domains",
        &mut failures,
    );

    for resource in &supplied.resources {
        let raw_sha256: [u8; 32] = Sha256::digest(&resource.canonical_bytes).into();
        if raw_sha256 != resource.raw_sha256 {
            failures.push(failure(
                ProviderContractPackFailureKind::ContractResourceRawDigestMismatch,
                &resource.coordinate,
            ));
        }
    }
    let supplied_resources = supplied
        .resources
        .iter()
        .map(unpack_resource)
        .collect::<Vec<_>>();
    match validate_target_profile_contract_resources(supplied_resources) {
        Ok(validated) => {
            let canonical_order = validated
                .resources()
                .iter()
                .cloned()
                .map(pack_resource)
                .collect::<Vec<_>>();
            compare_field(
                same_resource_order(&canonical_order, &supplied.resources)
                    && same_resource_order(&expected.resources, &supplied.resources),
                ProviderContractPackFailureKind::ManifestResourceOrderMismatch,
                "resources",
                &mut failures,
            );
        }
        Err(resource_failures) => {
            failures.extend(resource_failures.into_iter().map(map_resource_failure));
        }
    }

    if failures.is_empty() {
        Ok(())
    } else {
        sort_failures(&mut failures);
        Err(failures)
    }
}

fn same_resource_order(
    left: &[ProviderContractPackResource],
    right: &[ProviderContractPackResource],
) -> bool {
    left.len() == right.len()
        && left
            .iter()
            .zip(right)
            .all(|(left, right)| left.coordinate == right.coordinate)
}

fn validate_fragments(fragments: &[(&str, &[u8])]) -> Vec<ProviderContractPackFailure> {
    let mut failures = Vec::new();
    for (name, bytes) in fragments {
        if bytes.is_empty() {
            failures.push(failure(
                ProviderContractPackFailureKind::SchemaFragmentMissing,
                *name,
            ));
        } else if std::str::from_utf8(bytes).is_err() {
            failures.push(failure(
                ProviderContractPackFailureKind::SchemaFragmentInvalidUtf8,
                *name,
            ));
        }
    }
    failures
}

fn assemble_schema_bytes(fragments: &[(&str, &[u8])]) -> Vec<u8> {
    let capacity = PACK_HEADER.len()
        + fragments
            .iter()
            .map(|(name, bytes)| name.len() + bytes.len() + 16)
            .sum::<usize>();
    let mut output = Vec::with_capacity(capacity);
    output.extend_from_slice(PACK_HEADER.as_bytes());
    for (name, bytes) in fragments {
        output.extend_from_slice(b"\n; --- ");
        output.extend_from_slice(name.as_bytes());
        output.extend_from_slice(b" ---\n");
        output.extend_from_slice(bytes);
        if !bytes.ends_with(b"\n") {
            output.push(b'\n');
        }
    }
    output
}

fn all_roots(with_source_closure: bool) -> impl Iterator<Item = &'static str> {
    CONTRACT_BINDINGS
        .into_iter()
        .map(|(_contract, root)| root)
        .chain(with_source_closure.then_some("source-closure"))
}

enum SchemaClosureFailure {
    ExternalRule(String),
    UnsupportedControl(&'static str),
}

fn unresolved_schema_reference(rules: &RulesByName) -> Option<SchemaClosureFailure> {
    rules.values().find_map(|definition| {
        unresolved_node_reference(&definition.node, rules, &definition.generic_parms)
    })
}

fn unresolved_node_reference(
    node: &Node,
    rules: &RulesByName,
    generic_parameters: &[String],
) -> Option<SchemaClosureFailure> {
    match node {
        Node::Literal(_) | Node::PreludeType(_) => None,
        Node::Rule(rule) | Node::Unwrap(rule) | Node::Choiceify(rule) => {
            unresolved_rule_reference(rule, rules, generic_parameters)
        }
        Node::Choice(choice) => choice
            .options
            .iter()
            .find_map(|node| unresolved_node_reference(node, rules, generic_parameters)),
        Node::Map(map) => map
            .members
            .iter()
            .find_map(|node| unresolved_node_reference(node, rules, generic_parameters)),
        Node::Array(array) | Node::ChoiceifyInline(array) => array
            .members
            .iter()
            .find_map(|node| unresolved_node_reference(node, rules, generic_parameters)),
        Node::Group(group) => group
            .members
            .iter()
            .find_map(|node| unresolved_node_reference(node, rules, generic_parameters)),
        Node::KeyValue(pair) => unresolved_node_reference(&pair.key, rules, generic_parameters)
            .or_else(|| unresolved_node_reference(&pair.value, rules, generic_parameters)),
        Node::Occur(occur) => unresolved_node_reference(&occur.node, rules, generic_parameters),
        Node::Range(range) => unresolved_node_reference(&range.start, rules, generic_parameters)
            .or_else(|| unresolved_node_reference(&range.end, rules, generic_parameters)),
        Node::Control(control) => unresolved_control_reference(control, rules, generic_parameters),
    }
}

fn unresolved_rule_reference(
    rule: &Rule,
    rules: &RulesByName,
    generic_parameters: &[String],
) -> Option<SchemaClosureFailure> {
    rule.generic_args
        .iter()
        .find_map(|argument| unresolved_node_reference(argument, rules, generic_parameters))
        .or_else(|| {
            (!rules.contains_key(&rule.name) && !generic_parameters.contains(&rule.name))
                .then(|| SchemaClosureFailure::ExternalRule(rule.name.clone()))
        })
}

fn unresolved_control_reference(
    control: &Control,
    rules: &RulesByName,
    generic_parameters: &[String],
) -> Option<SchemaClosureFailure> {
    let pair = match control {
        Control::Size(value) => Some((&*value.target, &*value.size)),
        Control::Lt(value) => Some((&*value.target, &*value.lt)),
        Control::Le(value) => Some((&*value.target, &*value.le)),
        Control::Gt(value) => Some((&*value.target, &*value.gt)),
        Control::Ge(value) => Some((&*value.target, &*value.ge)),
        Control::Regexp(_) => None,
        Control::Cbor(_) => return Some(SchemaClosureFailure::UnsupportedControl(".cbor")),
        _ => return Some(SchemaClosureFailure::UnsupportedControl("unknown")),
    };
    pair.and_then(|(left, right)| {
        unresolved_node_reference(left, rules, generic_parameters)
            .or_else(|| unresolved_node_reference(right, rules, generic_parameters))
    })
}

fn validate_root(context: &BasicContext, root: &str, value: &CanonicalValue) -> Result<(), ()> {
    let bytes = encode_canonical_cbor(value).map_err(|_err| ())?;
    let cbor_value: ciborium::Value = ciborium::from_reader(bytes.as_slice()).map_err(|_err| ())?;
    let rule = context.rules.get(root).ok_or(())?;
    validate_cbor(rule, &cbor_value, context).map_err(|_err| ())
}

fn map_resource_failure(
    resource: edict_syntax::TargetProfileContractResourceFailure,
) -> ProviderContractPackFailure {
    let kind = match resource.kind {
        TargetProfileContractResourceFailureKind::MissingResource => {
            ProviderContractPackFailureKind::ContractResourceMissing
        }
        TargetProfileContractResourceFailureKind::UnknownResource => {
            ProviderContractPackFailureKind::ContractResourceUnknown
        }
        TargetProfileContractResourceFailureKind::DuplicateResource => {
            ProviderContractPackFailureKind::ContractResourceDuplicate
        }
        TargetProfileContractResourceFailureKind::InvalidCanonicalArtifact => {
            ProviderContractPackFailureKind::ContractResourceInvalidCanonicalArtifact
        }
        TargetProfileContractResourceFailureKind::ArtifactBytesMismatch => {
            ProviderContractPackFailureKind::ContractResourceBytesMismatch
        }
        TargetProfileContractResourceFailureKind::ArtifactDigestMismatch => {
            ProviderContractPackFailureKind::ContractResourceDomainFramedDigestMismatch
        }
        TargetProfileContractResourceFailureKind::ProvenanceMismatch => {
            ProviderContractPackFailureKind::ContractResourceProvenanceMismatch
        }
    };
    failure(kind, resource.coordinate)
}

fn pack_resource(resource: TargetProfileContractResource) -> ProviderContractPackResource {
    ProviderContractPackResource {
        raw_sha256: Sha256::digest(&resource.canonical_bytes).into(),
        coordinate: resource.coordinate,
        provenance: resource.provenance,
        canonical_bytes: resource.canonical_bytes,
        domain_framed_digest: resource.digest,
    }
}

fn unpack_resource(resource: &ProviderContractPackResource) -> TargetProfileContractResource {
    TargetProfileContractResource {
        coordinate: resource.coordinate.clone(),
        provenance: resource.provenance.clone(),
        canonical_bytes: resource.canonical_bytes.clone(),
        digest: resource.domain_framed_digest.clone(),
    }
}

fn compare_field(
    matches: bool,
    kind: ProviderContractPackFailureKind,
    subject: &str,
    failures: &mut Vec<ProviderContractPackFailure>,
) {
    if !matches {
        failures.push(failure(kind, subject));
    }
}

fn failure(
    kind: ProviderContractPackFailureKind,
    subject: impl Into<String>,
) -> ProviderContractPackFailure {
    ProviderContractPackFailure {
        kind,
        subject: subject.into(),
    }
}

fn sort_failures(failures: &mut [ProviderContractPackFailure]) {
    failures.sort();
}

fn render_manifest_json(manifest: &ProviderContractPackManifest) -> String {
    let mut output = String::new();
    output.push_str("{\n  \"apiVersion\": ");
    push_json_string(&mut output, &manifest.api_version);
    output.push_str(",\n  \"coordinate\": ");
    push_json_string(&mut output, &manifest.coordinate);
    output.push_str(",\n  \"license\": ");
    push_json_string(&mut output, &manifest.license);
    output.push_str(",\n  \"schema\": {\n    \"bytesHex\": ");
    push_json_string(&mut output, &hex_bytes(&manifest.schema_bytes));
    output.push_str(",\n    \"rawSha256\": ");
    push_json_string(&mut output, &hex_bytes(&manifest.schema_sha256));
    output.push_str("\n  },\n  \"contracts\": [");
    for (index, binding) in manifest.contracts.iter().enumerate() {
        push_array_separator(&mut output, index);
        output.push_str("    {\"contract\": ");
        push_json_string(&mut output, &binding.contract);
        output.push_str(", \"rootRule\": ");
        push_json_string(&mut output, &binding.root_rule);
        output.push('}');
    }
    close_array(&mut output, &manifest.contracts);
    output.push_str(",\n  \"domains\": [");
    for (index, binding) in manifest.domains.iter().enumerate() {
        push_array_separator(&mut output, index);
        output.push_str("    {\"domain\": ");
        push_json_string(&mut output, &binding.domain);
        output.push_str(", \"rootRule\": ");
        push_json_string(&mut output, &binding.root_rule);
        output.push('}');
    }
    close_array(&mut output, &manifest.domains);
    output.push_str(",\n  \"resources\": [");
    for (index, resource) in manifest.resources.iter().enumerate() {
        push_array_separator(&mut output, index);
        output.push_str("    {\n      \"coordinate\": ");
        push_json_string(&mut output, &resource.coordinate);
        output.push_str(",\n      \"canonicalBytesHex\": ");
        push_json_string(&mut output, &hex_bytes(&resource.canonical_bytes));
        output.push_str(",\n      \"rawSha256\": ");
        push_json_string(&mut output, &hex_bytes(&resource.raw_sha256));
        output.push_str(",\n      \"domainFramedDigest\": ");
        push_json_string(&mut output, &resource.domain_framed_digest);
        output.push_str(",\n      \"provenance\": {\n        \"repository\": ");
        push_json_string(&mut output, &resource.provenance.repository);
        output.push_str(",\n        \"sourcePath\": ");
        push_json_string(&mut output, &resource.provenance.source_path);
        output.push_str("\n      }\n    }");
    }
    close_array(&mut output, &manifest.resources);
    output.push_str("\n}\n");
    output
}

fn push_array_separator(output: &mut String, index: usize) {
    if index == 0 {
        output.push('\n');
    } else {
        output.push_str(",\n");
    }
}

fn close_array<T>(output: &mut String, values: &[T]) {
    if !values.is_empty() {
        output.push('\n');
        output.push_str("  ");
    }
    output.push(']');
}

fn push_json_string(output: &mut String, value: &str) {
    output.push('"');
    for character in value.chars() {
        match character {
            '"' => output.push_str("\\\""),
            '\\' => output.push_str("\\\\"),
            '\u{08}' => output.push_str("\\b"),
            '\u{0c}' => output.push_str("\\f"),
            '\n' => output.push_str("\\n"),
            '\r' => output.push_str("\\r"),
            '\t' => output.push_str("\\t"),
            control if control <= '\u{1f}' => {
                use std::fmt::Write as _;
                write!(output, "\\u{:04x}", u32::from(control))
                    .expect("writing to a String cannot fail");
            }
            character => output.push(character),
        }
    }
    output.push('"');
}

fn hex_bytes(bytes: &[u8]) -> String {
    use std::fmt::Write as _;

    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        write!(output, "{byte:02x}").expect("writing to a String cannot fail");
    }
    output
}

#[cfg(test)]
mod tests {
    use super::bindings_are_strictly_sorted_by_key;

    #[test]
    fn static_binding_guard_rejects_unsorted_and_duplicate_keys() {
        assert!(bindings_are_strictly_sorted_by_key(&[
            ("alpha", "root-a"),
            ("beta", "root-b"),
        ]));
        assert!(!bindings_are_strictly_sorted_by_key(&[
            ("beta", "root-b"),
            ("alpha", "root-a"),
        ]));
        assert!(!bindings_are_strictly_sorted_by_key(&[
            ("alpha", "root-a"),
            ("alpha", "root-b"),
        ]));
    }
}

//! Canonical Edict-owned resources used by runtime-owned target profiles.
//!
//! The resources in this module are explicit authority inputs. Validation uses
//! only caller-supplied in-memory values and the compiled Edict contract; it
//! performs no file, registry, network, environment, or mutable-name lookup.

use std::collections::BTreeMap;

use crate::canonical::{
    decode_canonical_cbor, digest_canonical_value, encode_canonical_cbor, CanonicalValue,
};
use crate::core_ir::ResourceRef;
use crate::target_profile::TargetProfileManifest;

/// Canonical document ABI shared by Edict-owned target-profile resources.
pub const TARGET_PROFILE_CONTRACT_RESOURCE_API_VERSION: &str =
    "edict.target-profile.contract-resource/v1";

/// Repository whose checked fixture paths identify the reviewed artifacts.
pub const TARGET_PROFILE_CONTRACT_RESOURCE_REPOSITORY: &str =
    "https://github.com/flyingrobots/edict";

/// Edict-owned target-profile canonical-encoding coordinate.
pub const CANONICAL_CBOR_CONTRACT_COORDINATE: &str = "edict.canonical-cbor/v1";
/// Edict-owned target-profile deterministic-execution coordinate.
pub const DETERMINISM_CONTRACT_COORDINATE: &str = "edict.determinism/v1";
/// Edict-owned target-profile diagnostics coordinate.
pub const DIAGNOSTICS_CONTRACT_COORDINATE: &str = "edict.diagnostics/v1";
/// Edict-owned target-profile fuel-accounting coordinate.
pub const FUEL_CONTRACT_COORDINATE: &str = "edict.fuel/v1";
/// Edict-owned target-profile component-sandbox coordinate.
pub const WASM_COMPONENT_CONTRACT_COORDINATE: &str = "edict.wasm-component/v1";

const RESOURCE_FIXTURE_ROOT: &str = "fixtures/target-profile/contract-resources";

/// Review provenance for one checked canonical resource.
///
/// The path is evidence for reviewers and packaging tools; validation never
/// reads it. Exact artifact bytes and their coordinate-framed digest carry the
/// content identity.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct TargetProfileContractResourceProvenance {
    pub repository: String,
    pub source_path: String,
}

/// One explicitly supplied Edict target-profile contract resource.
///
/// Public fields make this an untrusted transport value. Only
/// [`ValidatedTargetProfileContractResources`] carries binding authority.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TargetProfileContractResource {
    pub coordinate: String,
    pub provenance: TargetProfileContractResourceProvenance,
    pub canonical_bytes: Vec<u8>,
    pub digest: String,
}

/// Stable rejection categories for the contract-resource authority boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum TargetProfileContractResourceFailureKind {
    MissingResource,
    UnknownResource,
    DuplicateResource,
    InvalidCanonicalArtifact,
    ArtifactBytesMismatch,
    ArtifactDigestMismatch,
    ProvenanceMismatch,
}

/// One rejected contract-resource identity claim.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TargetProfileContractResourceFailure {
    pub kind: TargetProfileContractResourceFailureKind,
    pub coordinate: String,
}

/// Complete, coordinate-ordered Edict authority for the five owned slots.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidatedTargetProfileContractResources {
    resources: Vec<TargetProfileContractResource>,
}

impl ValidatedTargetProfileContractResources {
    /// Borrow the complete resource set in coordinate order.
    #[must_use]
    pub fn resources(&self) -> &[TargetProfileContractResource] {
        &self.resources
    }

    /// Borrow one validated resource by exact coordinate.
    #[must_use]
    pub fn resource(&self, coordinate: &str) -> Option<&TargetProfileContractResource> {
        self.resources
            .binary_search_by_key(&coordinate, |resource| resource.coordinate.as_str())
            .ok()
            .map(|index| &self.resources[index])
    }

    /// Apply all five exact Edict resource references to a runtime-owned profile.
    ///
    /// Runtime-owned semantics and provider component selection remain
    /// unchanged. This method is available only after the complete explicit
    /// input set has passed authority validation.
    #[must_use]
    pub fn bind_manifest(&self, mut manifest: TargetProfileManifest) -> TargetProfileManifest {
        manifest.canonical_encoding_rules = self.resource_ref(CANONICAL_CBOR_CONTRACT_COORDINATE);
        manifest.deterministic_execution = self.resource_ref(DETERMINISM_CONTRACT_COORDINATE);
        manifest.diagnostic_abi = self.resource_ref(DIAGNOSTICS_CONTRACT_COORDINATE);
        manifest.fuel_model = self.resource_ref(FUEL_CONTRACT_COORDINATE);
        manifest.sandbox = self.resource_ref(WASM_COMPONENT_CONTRACT_COORDINATE);
        manifest
    }

    fn resource_ref(&self, coordinate: &str) -> ResourceRef {
        let resource = self
            .resource(coordinate)
            .expect("validated contract resource set is complete");
        ResourceRef {
            coordinate: resource.coordinate.clone(),
            digest: Some(resource.digest.clone()),
        }
    }
}

/// Generate the complete reviewed Edict target-profile resource set.
///
/// The returned values are explicit inputs suitable for passing to a
/// runtime-owned generator. Callers should still validate the values at their
/// trust boundary before binding them to a profile.
///
/// # Panics
///
/// Panics only if Edict's compiled built-in semantic model violates the
/// canonical value or non-empty digest-domain invariants. That is a source-code
/// defect, not a caller-controlled input failure.
#[must_use]
pub fn canonical_target_profile_contract_resources() -> Vec<TargetProfileContractResource> {
    resource_specs()
        .into_iter()
        .map(|spec| {
            let value = contract_resource_value(spec.coordinate, spec.contract);
            let canonical_bytes = encode_canonical_cbor(&value)
                .expect("built-in target-profile contract resource must encode");
            let digest = digest_review_string(
                &digest_canonical_value(spec.coordinate, &value)
                    .expect("built-in target-profile contract resource must digest"),
            );
            TargetProfileContractResource {
                coordinate: spec.coordinate.to_owned(),
                provenance: TargetProfileContractResourceProvenance {
                    repository: TARGET_PROFILE_CONTRACT_RESOURCE_REPOSITORY.to_owned(),
                    source_path: format!("{RESOURCE_FIXTURE_ROOT}/{}.cbor", spec.fixture_stem),
                },
                canonical_bytes,
                digest,
            }
        })
        .collect()
}

/// Compute the coordinate-framed identity of canonical resource bytes.
///
/// This computes identity only. It does not authorize replacement policy;
/// [`validate_target_profile_contract_resources`] separately requires exact
/// agreement with Edict's published artifact.
///
/// # Errors
///
/// Returns `UnknownResource` for a coordinate outside the five owned slots and
/// `InvalidCanonicalArtifact` when the bytes are not canonical CBOR.
pub fn digest_target_profile_contract_resource(
    coordinate: &str,
    canonical_bytes: &[u8],
) -> Result<String, TargetProfileContractResourceFailure> {
    if resource_spec(coordinate).is_none() {
        return Err(failure(
            TargetProfileContractResourceFailureKind::UnknownResource,
            coordinate,
        ));
    }
    let value = decode_canonical_cbor(canonical_bytes).map_err(|_err| {
        failure(
            TargetProfileContractResourceFailureKind::InvalidCanonicalArtifact,
            coordinate,
        )
    })?;
    let digest = digest_canonical_value(coordinate, &value).map_err(|_err| {
        failure(
            TargetProfileContractResourceFailureKind::InvalidCanonicalArtifact,
            coordinate,
        )
    })?;
    Ok(digest_review_string(&digest))
}

/// Validate explicit resources against Edict's complete built-in authority set.
///
/// Validation is all-or-nothing and input-order independent. A successful
/// value can bind target-profile slots; failures return no partial authority.
///
/// # Errors
///
/// Returns structured failures in stable failure-kind-then-coordinate order for
/// incomplete, unknown, repeated, non-canonical, byte-mismatched,
/// digest-mismatched, or provenance-mismatched inputs.
pub fn validate_target_profile_contract_resources(
    resources: impl IntoIterator<Item = TargetProfileContractResource>,
) -> Result<ValidatedTargetProfileContractResources, Vec<TargetProfileContractResourceFailure>> {
    let authoritative = canonical_target_profile_contract_resources()
        .into_iter()
        .map(|resource| (resource.coordinate.clone(), resource))
        .collect::<BTreeMap<_, _>>();
    let mut supplied = BTreeMap::<String, Vec<TargetProfileContractResource>>::new();
    for resource in resources {
        supplied
            .entry(resource.coordinate.clone())
            .or_default()
            .push(resource);
    }

    let mut failures = Vec::new();
    for coordinate in authoritative.keys() {
        if !supplied.contains_key(coordinate) {
            failures.push(failure(
                TargetProfileContractResourceFailureKind::MissingResource,
                coordinate,
            ));
        }
    }
    for coordinate in supplied.keys() {
        if !authoritative.contains_key(coordinate) {
            failures.push(failure(
                TargetProfileContractResourceFailureKind::UnknownResource,
                coordinate,
            ));
        }
    }

    let mut validated = Vec::with_capacity(authoritative.len());
    for (coordinate, expected) in &authoritative {
        let Some(candidates) = supplied.get(coordinate) else {
            continue;
        };
        if candidates.len() != 1 {
            failures.push(failure(
                TargetProfileContractResourceFailureKind::DuplicateResource,
                coordinate,
            ));
            continue;
        }
        let candidate = &candidates[0];
        if candidate.provenance != expected.provenance {
            failures.push(failure(
                TargetProfileContractResourceFailureKind::ProvenanceMismatch,
                coordinate,
            ));
        }
        let actual_digest =
            match digest_target_profile_contract_resource(coordinate, &candidate.canonical_bytes) {
                Ok(digest) => Some(digest),
                Err(err) => {
                    failures.push(err);
                    None
                }
            };
        if let Some(actual_digest) = actual_digest {
            if candidate.canonical_bytes != expected.canonical_bytes {
                failures.push(failure(
                    TargetProfileContractResourceFailureKind::ArtifactBytesMismatch,
                    coordinate,
                ));
            }
            if candidate.digest != actual_digest {
                failures.push(failure(
                    TargetProfileContractResourceFailureKind::ArtifactDigestMismatch,
                    coordinate,
                ));
            }
        }
        validated.push(candidate.clone());
    }

    if failures.is_empty() {
        Ok(ValidatedTargetProfileContractResources {
            resources: validated,
        })
    } else {
        failures.sort_by(|left, right| {
            left.kind
                .cmp(&right.kind)
                .then_with(|| left.coordinate.cmp(&right.coordinate))
        });
        Err(failures)
    }
}

#[derive(Clone, Copy)]
struct ResourceSpec {
    coordinate: &'static str,
    fixture_stem: &'static str,
    contract: fn() -> CanonicalValue,
}

fn resource_specs() -> [ResourceSpec; 5] {
    [
        ResourceSpec {
            coordinate: CANONICAL_CBOR_CONTRACT_COORDINATE,
            fixture_stem: "canonical-cbor",
            contract: canonical_cbor_contract,
        },
        ResourceSpec {
            coordinate: DETERMINISM_CONTRACT_COORDINATE,
            fixture_stem: "determinism",
            contract: determinism_contract,
        },
        ResourceSpec {
            coordinate: DIAGNOSTICS_CONTRACT_COORDINATE,
            fixture_stem: "diagnostics",
            contract: diagnostics_contract,
        },
        ResourceSpec {
            coordinate: FUEL_CONTRACT_COORDINATE,
            fixture_stem: "fuel",
            contract: fuel_contract,
        },
        ResourceSpec {
            coordinate: WASM_COMPONENT_CONTRACT_COORDINATE,
            fixture_stem: "wasm-component",
            contract: wasm_component_contract,
        },
    ]
}

fn resource_spec(coordinate: &str) -> Option<ResourceSpec> {
    resource_specs()
        .into_iter()
        .find(|spec| spec.coordinate == coordinate)
}

fn contract_resource_value(coordinate: &str, contract: fn() -> CanonicalValue) -> CanonicalValue {
    cbor_map([
        (
            "apiVersion",
            text(TARGET_PROFILE_CONTRACT_RESOURCE_API_VERSION),
        ),
        ("contract", contract()),
        ("coordinate", text(coordinate)),
    ])
}

fn canonical_cbor_contract() -> CanonicalValue {
    cbor_map([
        ("containerLengths", text("definite")),
        ("digestFrame", text("edict.digest/v1")),
        ("duplicateMapKeys", text("reject")),
        ("integerEncoding", text("shortest")),
        ("mapOrdering", text("canonical-encoded-key-bytes")),
        ("maximumNestingDepth", CanonicalValue::Integer(128)),
        ("textEncoding", text("utf-8")),
        ("trailingBytes", text("reject")),
        (
            "valueModel",
            text_array(["array", "bool", "bytes", "integer", "map", "null", "text"]),
        ),
    ])
}

fn determinism_contract() -> CanonicalValue {
    cbor_map([
        ("ambientClock", CanonicalValue::Bool(false)),
        ("ambientEnvironment", CanonicalValue::Bool(false)),
        ("ambientFilesystem", CanonicalValue::Bool(false)),
        ("ambientNetwork", CanonicalValue::Bool(false)),
        ("ambientRandomness", CanonicalValue::Bool(false)),
        (
            "canonicalResultIndependentOfLimits",
            CanonicalValue::Bool(true),
        ),
        ("freshStorePerInvocation", CanonicalValue::Bool(true)),
        ("mutableNameResolution", CanonicalValue::Bool(false)),
        ("outputIdentity", text("host-computed-after-validation")),
        ("registryLookup", CanonicalValue::Bool(false)),
    ])
}

fn diagnostics_contract() -> CanonicalValue {
    cbor_map([
        ("aggregateByteBound", CanonicalValue::Bool(true)),
        (
            "fields",
            text_array(["code", "severity", "message", "repair"]),
        ),
        ("listCountBound", CanonicalValue::Bool(true)),
        ("ordering", text("code-severity-message-repair")),
        (
            "recordAbi",
            text("edict:target-provider/protocol@1.0.0#diagnostic"),
        ),
        ("severityOrder", text_array(["error", "warning", "info"])),
        ("truncation", text("forbidden")),
    ])
}

fn fuel_contract() -> CanonicalValue {
    cbor_map([
        ("accounting", text("deterministic-guest-work")),
        ("budgetUnit", text("fuel")),
        ("epochInterruption", text("not-a-replay-budget")),
        ("exhaustion", text("host-owned-failure")),
        ("perInvocation", CanonicalValue::Bool(true)),
        ("required", CanonicalValue::Bool(true)),
    ])
}

fn wasm_component_contract() -> CanonicalValue {
    cbor_map([
        ("callableImports", text("forbidden")),
        ("componentDigestVerification", text("before-decode")),
        ("contractIdentity", text("exact")),
        ("freshStorePerInvocation", CanonicalValue::Bool(true)),
        ("protocolImport", text("type-only")),
        ("providerContract", text("edict:target-provider@1.0.0")),
        ("wasi", text("forbidden")),
    ])
}

fn cbor_map<const N: usize>(entries: [(&str, CanonicalValue); N]) -> CanonicalValue {
    CanonicalValue::Map(
        entries
            .into_iter()
            .map(|(key, value)| (text(key), value))
            .collect(),
    )
}

fn text_array<const N: usize>(values: [&str; N]) -> CanonicalValue {
    CanonicalValue::Array(values.into_iter().map(text).collect())
}

fn text(value: &str) -> CanonicalValue {
    CanonicalValue::Text(value.to_owned())
}

fn digest_review_string(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut review = String::with_capacity(7 + bytes.len() * 2);
    review.push_str("sha256:");
    for byte in bytes {
        review.push(char::from(HEX[usize::from(byte >> 4)]));
        review.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    review
}

fn failure(
    kind: TargetProfileContractResourceFailureKind,
    coordinate: &str,
) -> TargetProfileContractResourceFailure {
    TargetProfileContractResourceFailure {
        kind,
        coordinate: coordinate.to_owned(),
    }
}

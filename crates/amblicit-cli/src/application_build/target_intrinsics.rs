//! Explicit catalog loading at the application I/O boundary. The selected,
//! already-bound target profile owns the pin; application config supplies only
//! a provider-relative location and exact contract requirements.
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use amblicit_syntax::{
    admit_target_pure_intrinsics, decode_canonical_cbor, CanonicalValue, ProviderBoundArtifact,
    RequiredPureIntrinsic, ResourceRef, TargetPureIntrinsicFact, TargetPureIntrinsicFailureKind,
};
use serde::Deserialize;

use super::{
    confined_existing_path, failure, read, rendered_digest, validate_application_path,
    ApplicationBuildFailure,
};

const MAX_REQUIRED_INTRINSICS: usize = 64;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct ApplicationPureIntrinsics {
    catalog: PathBuf,
    required: Vec<Requirement>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Requirement {
    coordinate: String,
    contract_version: String,
}

impl ApplicationPureIntrinsics {
    pub(super) fn validate(&self) -> Result<(), ApplicationBuildFailure> {
        validate_application_path("target.pureIntrinsics.catalog", &self.catalog)?;
        if self.required.is_empty()
            || self.required.len() > MAX_REQUIRED_INTRINSICS
            || self
                .required
                .iter()
                .any(|r| r.coordinate.is_empty() || r.contract_version.is_empty())
        {
            return Err(failure(
                "InvalidApplicationConfig",
                "target.pureIntrinsics requires 1..=64 non-empty coordinate/contractVersion pairs",
            ));
        }
        Ok(())
    }

    pub(super) fn load_from_provider(
        &self,
        provider_root: &Path,
        profile: &ProviderBoundArtifact,
    ) -> Result<Vec<TargetPureIntrinsicFact>, ApplicationBuildFailure> {
        let catalog = catalog_reference(&profile.artifact.bytes)?;
        let path = confined_existing_path(
            provider_root,
            &self.catalog,
            "target.pureIntrinsics.catalog",
            "TargetIntrinsicCatalogReadFailed",
            "intrinsic catalog",
        )?;
        let bytes = read(
            &path,
            "TargetIntrinsicCatalogReadFailed",
            "intrinsic catalog",
        )?;
        let target = ResourceRef {
            coordinate: profile.reference.coordinate.clone(),
            digest: Some(rendered_digest(&profile.reference.digest)),
        };
        let required = self
            .required
            .iter()
            .map(|r| RequiredPureIntrinsic {
                coordinate: r.coordinate.clone(),
                contract_version: r.contract_version.clone(),
            })
            .collect::<Vec<_>>();
        admit_target_pure_intrinsics(&target, &catalog, &bytes, &required).map_err(|errors| {
            // Admission returns all failed obligations deterministically. The
            // public category names the first; the message retains the full set.
            let kind = errors
                .first()
                .map_or("TargetIntrinsicAdmissionFailed", |e| {
                    diagnostic_kind(e.kind)
                });
            failure(
                kind,
                format!("target pure intrinsic admission failed: {errors:?}"),
            )
        })
    }
}

fn catalog_reference(bytes: &[u8]) -> Result<ResourceRef, ApplicationBuildFailure> {
    let invalid =
        || {
            failure("InvalidTargetIntrinsicReference",
        "selected target profile must carry one digest-locked intrinsics resource reference")
        };
    let profile = decode_canonical_cbor(bytes).map_err(|_| invalid())?;
    let CanonicalValue::Map(fields) = profile else {
        return Err(invalid());
    };
    let mut references = fields
        .iter()
        .filter(|(key, _)| key == &CanonicalValue::Text("intrinsics".into()));
    let Some((_, CanonicalValue::Map(reference))) = references.next() else {
        return Err(invalid());
    };
    if references.next().is_some() || reference.len() != 2 {
        return Err(invalid());
    }
    let mut coordinate = None;
    let mut digest = None;
    for (key, value) in reference {
        match (key, value) {
            (CanonicalValue::Text(key), CanonicalValue::Text(id))
                if key == "id" && !id.is_empty() =>
            {
                coordinate = Some(id.clone());
            }
            (CanonicalValue::Text(key), CanonicalValue::Array(parts)) if key == "digest" => {
                let [CanonicalValue::Text(algorithm), CanonicalValue::Bytes(bytes)] =
                    parts.as_slice()
                else {
                    return Err(invalid());
                };
                if algorithm != "sha256" || bytes.len() != 32 {
                    return Err(invalid());
                }
                let mut rendered = String::from("sha256:");
                for byte in bytes {
                    write!(&mut rendered, "{byte:02x}").map_err(|_| invalid())?;
                }
                digest = Some(rendered);
            }
            _ => return Err(invalid()),
        }
    }
    Ok(ResourceRef {
        coordinate: coordinate.ok_or_else(invalid)?,
        digest: Some(digest.ok_or_else(invalid)?),
    })
}

fn diagnostic_kind(kind: TargetPureIntrinsicFailureKind) -> &'static str {
    use TargetPureIntrinsicFailureKind as K;
    match kind {
        K::UnpinnedResource => "TargetIntrinsicUnpinnedResource",
        K::NonCanonicalCatalog => "TargetIntrinsicNonCanonicalCatalog",
        K::DigestMismatch => "TargetIntrinsicDigestMismatch",
        K::MalformedCatalog => "TargetIntrinsicMalformedCatalog",
        K::MissingIntrinsic => "TargetIntrinsicMissingIntrinsic",
        K::DuplicateRequirement => "TargetIntrinsicDuplicateRequirement",
        K::NotPure => "TargetIntrinsicNotPure",
        K::VersionMismatch => "TargetIntrinsicVersionMismatch",
        K::MalformedContract => "TargetIntrinsicMalformedContract",
        K::OutsideNamespace => "TargetIntrinsicOutsideNamespace",
        K::UnresolvedType => "TargetIntrinsicUnresolvedType",
        K::InvalidProviderType => "TargetIntrinsicInvalidProviderType",
        K::InvalidCostRule => "TargetIntrinsicInvalidCostRule",
    }
}

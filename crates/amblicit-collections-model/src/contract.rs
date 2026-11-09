//! This module owns pure agreement on collection artifact contract metadata.
//!
//! Agreement is neither decoding nor authorization. The module has no world,
//! snapshot, candidate, provider, or I/O access.

/// An unvalidated view of the common requirement/offer header fields.
#[derive(Clone, Copy, Debug)]
pub struct RawContract<'a> {
    /// Exact artifact version discriminator, not a compatible-version range.
    pub api_version: &'a str,
    /// Digest-locked semantic resource coordinate.
    pub semantics_id: &'a str,
    /// Raw SHA-256 identity bytes; length is checked before copying.
    pub semantics_digest: &'a [u8],
    /// At most three recognized names in canonical UTF-8 text order.
    pub features: &'a [&'a str],
}

/// Known v1 features in canonical text order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Feature {
    /// Typed immutable content resolution and ensure obligations.
    Content,
    /// Witnessed optional-slot reads and proposed mutations.
    Slot,
    /// Bounded snapshot entry traversal; requires slot support.
    Traversal,
}

/// Which header or comparison caused a contract refusal.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ContractSide {
    /// The operation's declared requirement.
    Requirement,
    /// The target's offer, including mismatches against the requirement.
    Offer,
}

/// Contract refusal, distinct from application-domain and observation failures.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ContractErrorKind {
    /// The artifact version is unsupported, even if both sides name it.
    UnsupportedVersion,
    /// The semantic resource coordinate is not the v1 coordinate.
    UnknownSemantics,
    /// The digest length differs from 32 bytes; payload is observed length.
    InvalidDigestLength(usize),
    /// The feature list exceeds the v1 envelope of three entries.
    TooManyFeatures,
    /// The feature at this bounded index has an unsupported name.
    UnknownFeature(usize),
    /// This feature repeats or precedes the previous one in canonical order.
    NonCanonicalFeatures(usize),
    /// Traversal was declared without its required slot feature.
    TraversalNeedsSlot,
    /// The offer and requirement lock different semantic resource bytes.
    SemanticsMismatch,
    /// The target does not offer this required feature.
    UnsupportedFeature(Feature),
}

/// Structured refusal of metadata agreement; no operation was admitted.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ContractError {
    /// Header or comparison that failed.
    pub side: ContractSide,
    /// Stable reason and, where relevant, offending feature or index.
    pub kind: ContractErrorKind,
}

/// Exact identity of the semantic resource, supplied by the fixture or producer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SemanticDigest([u8; 32]);

impl SemanticDigest {
    /// Return the checked identity bytes; this does not compute or verify a hash.
    #[must_use]
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

/// Validated agreement metadata. It grants no observation or write authority.
#[derive(Clone, Debug, PartialEq, Eq)]
#[must_use]
pub struct ContractAgreement {
    semantics_digest: SemanticDigest,
    features: Vec<Feature>,
}

impl ContractAgreement {
    /// The operation's required features, excluding unused target offers.
    #[must_use]
    pub fn features(&self) -> &[Feature] {
        &self.features
    }

    /// The exact semantic identity shared by requirement and offer.
    #[must_use]
    pub fn semantics_digest(&self) -> SemanticDigest {
        self.semantics_digest
    }
}

/// Check requirement first, offer second, then semantic identity and coverage.
///
/// # Errors
/// Returns a structured contract refusal for unsupported, malformed, or
/// incompatible headers. This function performs no stored reads or mutations.
pub fn negotiate_contract(
    required: RawContract<'_>,
    offered: RawContract<'_>,
) -> Result<ContractAgreement, ContractError> {
    let required = required.check(ContractSide::Requirement)?;
    let offered = offered.check(ContractSide::Offer)?;
    if required.semantics_digest != offered.semantics_digest {
        return Err(ContractSide::Offer.failure(ContractErrorKind::SemanticsMismatch));
    }
    if let Some(feature) = required
        .features
        .iter()
        .find(|feature| !offered.features.contains(feature))
    {
        return Err(ContractSide::Offer.failure(ContractErrorKind::UnsupportedFeature(*feature)));
    }
    Ok(ContractAgreement {
        semantics_digest: required.semantics_digest,
        features: required.features,
    })
}

struct CheckedContract {
    semantics_digest: SemanticDigest,
    features: Vec<Feature>,
}

impl ContractSide {
    fn failure(self, kind: ContractErrorKind) -> ContractError {
        ContractError { side: self, kind }
    }
}

impl RawContract<'_> {
    fn check(self, side: ContractSide) -> Result<CheckedContract, ContractError> {
        if self.api_version != "amblicit.collections.artifact/v1" {
            return Err(side.failure(ContractErrorKind::UnsupportedVersion));
        }
        if self.semantics_id != "amblicit.collections/v1" {
            return Err(side.failure(ContractErrorKind::UnknownSemantics));
        }
        let digest = self.semantics_digest.try_into().map_err(|_| {
            side.failure(ContractErrorKind::InvalidDigestLength(
                self.semantics_digest.len(),
            ))
        })?;
        let features = check_features(self.features, side)?;
        Ok(CheckedContract {
            semantics_digest: SemanticDigest(digest),
            features,
        })
    }
}

fn check_features(raw: &[&str], side: ContractSide) -> Result<Vec<Feature>, ContractError> {
    if raw.len() > 3 {
        return Err(side.failure(ContractErrorKind::TooManyFeatures));
    }
    let mut features = Vec::with_capacity(raw.len());
    for (index, name) in raw.iter().enumerate() {
        let feature = match *name {
            "content" => Feature::Content,
            "slot" => Feature::Slot,
            "traversal" => Feature::Traversal,
            _ => return Err(side.failure(ContractErrorKind::UnknownFeature(index))),
        };
        if features.last().is_some_and(|previous| *previous >= feature) {
            return Err(side.failure(ContractErrorKind::NonCanonicalFeatures(index)));
        }
        features.push(feature);
    }
    if features.contains(&Feature::Traversal) && !features.contains(&Feature::Slot) {
        return Err(side.failure(ContractErrorKind::TraversalNeedsSlot));
    }
    Ok(features)
}

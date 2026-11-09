//! Small pure-boundary fixtures. Expected versions, digest lengths, features,
//! and refusal precedence are literals from the wire contract, not model output.

use amblicit_collections_model::contract::{
    negotiate_contract, ContractError, ContractErrorKind, ContractSide, Feature, RawContract,
};

const VERSION: &str = "amblicit.collections.artifact/v1";
const SEMANTICS: &str = "amblicit.collections/v1";
const DIGEST: [u8; 32] = [7; 32];

fn header<'a>(features: &'a [&'a str]) -> RawContract<'a> {
    RawContract {
        api_version: VERSION,
        semantics_id: SEMANTICS,
        semantics_digest: &DIGEST,
        features,
    }
}

fn refusal(
    required: RawContract<'_>,
    offered: RawContract<'_>,
    side: ContractSide,
    kind: ContractErrorKind,
) {
    assert_eq!(
        negotiate_contract(required, offered),
        Err(ContractError { side, kind })
    );
}

#[test]
fn a_target_may_offer_more_features_than_the_operation_requires() {
    let agreement =
        negotiate_contract(header(&["slot"]), header(&["content", "slot", "traversal"])).unwrap();
    assert_eq!(agreement.features(), &[Feature::Slot]);
    assert_eq!(agreement.semantics_digest().as_bytes(), &DIGEST);
}

#[test]
fn empty_feature_agreement_is_valid_metadata() {
    let agreement = negotiate_contract(header(&[]), header(&[])).unwrap();
    assert_eq!(agreement.features(), &[]);
    assert_eq!(agreement.semantics_digest().as_bytes(), &DIGEST);
}

#[test]
fn immutable_content_does_not_require_mutable_slot_authority() {
    let agreement = negotiate_contract(header(&["content"]), header(&["content"])).unwrap();
    assert_eq!(agreement.features(), &[Feature::Content]);
}

#[test]
fn traversal_agreement_keeps_its_slot_prerequisite() {
    let agreement = negotiate_contract(
        header(&["slot", "traversal"]),
        header(&["slot", "traversal"]),
    )
    .unwrap();
    assert_eq!(agreement.features(), &[Feature::Slot, Feature::Traversal]);
}

#[test]
fn matching_unknown_versions_are_still_unsupported() {
    let unsupported = RawContract {
        api_version: "amblicit.collections.artifact/v2",
        ..header(&[])
    };
    refusal(
        unsupported,
        unsupported,
        ContractSide::Requirement,
        ContractErrorKind::UnsupportedVersion,
    );
}

#[test]
fn unsupported_offer_version_refuses() {
    refusal(
        header(&[]),
        RawContract {
            api_version: "amblicit.collections.artifact/v2",
            ..header(&[])
        },
        ContractSide::Offer,
        ContractErrorKind::UnsupportedVersion,
    );
}

#[test]
fn required_version_is_checked_before_other_header_fields() {
    refusal(
        RawContract {
            api_version: "future",
            semantics_id: "wrong",
            semantics_digest: &[],
            features: &["unknown"],
        },
        header(&[]),
        ContractSide::Requirement,
        ContractErrorKind::UnsupportedVersion,
    );
}

#[test]
fn semantic_coordinate_is_checked_before_digest_shape() {
    refusal(
        RawContract {
            semantics_id: "unrelated/v1",
            semantics_digest: &[],
            ..header(&[])
        },
        header(&[]),
        ContractSide::Requirement,
        ContractErrorKind::UnknownSemantics,
    );
}

#[test]
fn unknown_offered_semantics_refuse() {
    refusal(
        header(&[]),
        RawContract {
            semantics_id: "amblicit.collections/v2",
            ..header(&[])
        },
        ContractSide::Offer,
        ContractErrorKind::UnknownSemantics,
    );
}

#[test]
fn a_short_required_digest_is_not_an_identity() {
    refusal(
        RawContract {
            semantics_digest: &[7; 31],
            ..header(&[])
        },
        header(&[]),
        ContractSide::Requirement,
        ContractErrorKind::InvalidDigestLength(31),
    );
}

#[test]
fn an_oversized_offered_digest_is_not_truncated() {
    refusal(
        header(&[]),
        RawContract {
            semantics_digest: &[7; 33],
            ..header(&[])
        },
        ContractSide::Offer,
        ContractErrorKind::InvalidDigestLength(33),
    );
}

#[test]
fn equal_versions_do_not_hide_different_semantics() {
    refusal(
        header(&[]),
        RawContract {
            semantics_digest: &[8; 32],
            ..header(&[])
        },
        ContractSide::Offer,
        ContractErrorKind::SemanticsMismatch,
    );
}

#[test]
fn unknown_required_feature_refuses() {
    refusal(
        header(&["slot", "teleport"]),
        header(&["slot"]),
        ContractSide::Requirement,
        ContractErrorKind::UnknownFeature(1),
    );
}

#[test]
fn unknown_offered_features_are_not_silently_ignored() {
    refusal(
        header(&[]),
        header(&["future"]),
        ContractSide::Offer,
        ContractErrorKind::UnknownFeature(0),
    );
}

#[test]
fn feature_count_is_bounded_before_inspecting_unknown_values() {
    refusal(
        header(&["a", "b", "c", "d"]),
        header(&[]),
        ContractSide::Requirement,
        ContractErrorKind::TooManyFeatures,
    );
}

#[test]
fn repeated_features_are_not_a_canonical_set() {
    refusal(
        header(&["slot", "slot"]),
        header(&["slot"]),
        ContractSide::Requirement,
        ContractErrorKind::NonCanonicalFeatures(1),
    );
}

#[test]
fn unsorted_offer_features_refuse_even_when_unused() {
    refusal(
        header(&[]),
        header(&["slot", "content"]),
        ContractSide::Offer,
        ContractErrorKind::NonCanonicalFeatures(1),
    );
}

#[test]
fn a_traversal_requirement_cannot_omit_slot() {
    refusal(
        header(&["traversal"]),
        header(&["slot", "traversal"]),
        ContractSide::Requirement,
        ContractErrorKind::TraversalNeedsSlot,
    );
}

#[test]
fn a_traversal_offer_cannot_omit_slot() {
    refusal(
        header(&[]),
        header(&["traversal"]),
        ContractSide::Offer,
        ContractErrorKind::TraversalNeedsSlot,
    );
}

#[test]
fn a_known_but_unoffered_feature_refuses() {
    refusal(
        header(&["slot"]),
        header(&["content"]),
        ContractSide::Offer,
        ContractErrorKind::UnsupportedFeature(Feature::Slot),
    );
}

#[test]
fn missing_features_are_reported_in_canonical_order() {
    refusal(
        header(&["content", "slot", "traversal"]),
        header(&[]),
        ContractSide::Offer,
        ContractErrorKind::UnsupportedFeature(Feature::Content),
    );
}

#[test]
fn malformed_required_header_precedes_malformed_offer() {
    refusal(
        header(&["slot", "slot"]),
        RawContract {
            api_version: "future",
            ..header(&[])
        },
        ContractSide::Requirement,
        ContractErrorKind::NonCanonicalFeatures(1),
    );
}

//! Canonical authority-facts ABI behavior.

use amblicit_syntax::{
    compile_to_core, compiler_context_from_authority_facts, decode_authority_facts_cbor,
    digest_authority_facts_document, encode_authority_facts_cbor, AuthorityFactSource,
    AuthorityFactSourceKind, AuthorityFactsDocument, AuthorityFactsLoadFailure,
    AuthorityFactsLoadFailureKind, BudgetFact, CanonicalValue, CoreBudget, EffectWriteClassFact,
    OperationProfileFact, WriteClass, AUTHORITY_FACTS_API_VERSION,
};

const EFFECTFUL_SOURCE: &str = "package a.b@1;\n\
    type Input = { id: String<max=16>, };\n\
    type Receipt = { id: String<max=16>, };\n\
    type Output = { id: String<max=16>, };\n\
    intent t(input: Input) returns Output\n\
      profile p.effectful\n\
      basis none\n\
      budget <= p.tiny {\n\
      let receipt: Receipt = target.replace(input.id)\n\
        else { rejected(reason) => domain.WriteRejected };\n\
      return { id: receipt.id };\n\
    }";

#[test]
fn canonical_authority_facts_decode_into_existing_compiler_context() {
    let document = complete_document();
    let bytes = encode_authority_facts_cbor(&document).expect("authority facts encode");
    let decoded = decode_authority_facts_cbor(&bytes).expect("authority facts decode");

    assert_eq!(decoded, document);

    let context = compiler_context_from_authority_facts(&[decoded]).expect("facts merge");
    let module = amblicit_syntax::parse_module(EFFECTFUL_SOURCE).expect("fixture parses");
    let core = compile_to_core(&module, &context).expect("decoded facts compile fixture");
    let intent = core.intents.get("t").expect("compiled intent");

    assert_eq!(
        intent.required_operation_profile,
        "continuum.profile.write/v1"
    );
    assert_eq!(intent.core_evaluation_budget.max_steps, 8);
    assert_eq!(intent.core_evaluation_budget.max_allocated_bytes, 1024);
    assert_eq!(intent.core_evaluation_budget.max_output_bytes, 256);
}

#[test]
fn canonical_authority_facts_encoding_is_insertion_order_independent() {
    let mut first = complete_document();
    first.operation_profiles.push(OperationProfileFact {
        source: "a.readOnly".to_owned(),
        core: "continuum.profile.read-only/v1".to_owned(),
        allowed_write_classes: vec![WriteClass::Read],
    });
    first.effect_write_classes.push(EffectWriteClassFact {
        effect: "a.read".to_owned(),
        write_class: WriteClass::Read,
    });
    first.budgets.push(BudgetFact {
        source: "a.small".to_owned(),
        budget: CoreBudget {
            max_steps: 1,
            max_allocated_bytes: 2,
            max_output_bytes: 3,
        },
    });

    let mut reordered = first.clone();
    reordered.operation_profiles.reverse();
    reordered.effect_write_classes.reverse();
    reordered.budgets.reverse();
    reordered.operation_profiles[0]
        .allowed_write_classes
        .reverse();
    reordered.operation_profiles[1]
        .allowed_write_classes
        .reverse();

    assert_eq!(
        encode_authority_facts_cbor(&first).expect("first document encodes"),
        encode_authority_facts_cbor(&reordered).expect("reordered document encodes")
    );
    assert_eq!(
        digest_authority_facts_document(&first).expect("first document digests"),
        digest_authority_facts_document(&reordered).expect("reordered document digests")
    );

    let mut changed = first;
    changed.budgets[0].budget.max_steps += 1;
    assert_ne!(
        digest_authority_facts_document(&changed).expect("changed document digests"),
        digest_authority_facts_document(&reordered).expect("reordered document digests")
    );
}

#[test]
fn digest_review_hex_case_does_not_create_a_source_conflict() {
    let mut upper = complete_document();
    upper.source.digest = format!("sha256:{}", "A".repeat(64));
    let decoded = decode_authority_facts_cbor(
        &encode_authority_facts_cbor(&upper).expect("uppercase review digest encodes"),
    )
    .expect("canonical authority facts decode");

    assert_eq!(decoded.source.digest, format!("sha256:{}", "a".repeat(64)));
    compiler_context_from_authority_facts(&[upper, decoded])
        .expect("digest review hex case denotes the same source identity");
}

#[test]
fn canonical_authority_facts_rejections_have_stable_kinds() {
    let noncanonical =
        decode_authority_facts_cbor(&[0x18, 0x00]).expect_err("non-minimal CBOR rejects");
    assert_exact_kind(
        &noncanonical,
        AuthorityFactsLoadFailureKind::InvalidCanonicalCbor,
    );

    let malformed_root = amblicit_syntax::encode_canonical_cbor(&CanonicalValue::Null)
        .expect("canonical null encodes");
    let malformed =
        decode_authority_facts_cbor(&malformed_root).expect_err("non-document root rejects");
    assert_exact_kind(&malformed, AuthorityFactsLoadFailureKind::InvalidCborShape);

    let mut unknown_field = canonical_value(&complete_document());
    let CanonicalValue::Map(root) = &mut unknown_field else {
        panic!("authority-facts encoder must produce a map");
    };
    root.push((
        CanonicalValue::Text("unknown".to_owned()),
        CanonicalValue::Null,
    ));
    let unknown_field = amblicit_syntax::encode_canonical_cbor(&unknown_field)
        .expect("unknown-field value is canonical CBOR");
    let unknown_field = decode_authority_facts_cbor(&unknown_field)
        .expect_err("unknown authority-facts field rejects");
    assert_exact_kind(
        &unknown_field,
        AuthorityFactsLoadFailureKind::InvalidCborShape,
    );

    let mut invalid_digest = canonical_value(&complete_document());
    *nested_map_value_mut(&mut invalid_digest, &["source", "digest"]) =
        CanonicalValue::Array(vec![
            CanonicalValue::Text("sha256".to_owned()),
            CanonicalValue::Bytes(vec![0; 31]),
        ]);
    let invalid_digest = amblicit_syntax::encode_canonical_cbor(&invalid_digest)
        .expect("invalid typed digest remains canonical CBOR");
    let invalid_digest =
        decode_authority_facts_cbor(&invalid_digest).expect_err("wrong-width typed digest rejects");
    assert_exact_kind(
        &invalid_digest,
        AuthorityFactsLoadFailureKind::NonDigestLockedSource,
    );

    let mut invalid_source = canonical_value(&complete_document());
    *nested_map_value_mut(&mut invalid_source, &["source", "kind"]) =
        CanonicalValue::Text("runtime".to_owned());
    let invalid_source = amblicit_syntax::encode_canonical_cbor(&invalid_source)
        .expect("unsupported source kind remains canonical CBOR");
    let invalid_source = decode_authority_facts_cbor(&invalid_source)
        .expect_err("unsupported authority source kind rejects");
    assert_exact_kind(
        &invalid_source,
        AuthorityFactsLoadFailureKind::InvalidSourceKind,
    );

    let mut legacy_array_set = canonical_value(&complete_document());
    *nested_map_value_mut(
        &mut legacy_array_set,
        &["operationProfiles", "p.effectful", "allowedWriteClasses"],
    ) = CanonicalValue::Array(vec![
        CanonicalValue::Text("read".to_owned()),
        CanonicalValue::Text("replace".to_owned()),
    ]);
    let legacy_array_set = amblicit_syntax::encode_canonical_cbor(&legacy_array_set)
        .expect("legacy array set remains canonical CBOR");
    let legacy_array_set = decode_authority_facts_cbor(&legacy_array_set)
        .expect_err("array-shaped write-class set rejects");
    assert_exact_kind(
        &legacy_array_set,
        AuthorityFactsLoadFailureKind::InvalidCborShape,
    );

    let mut duplicate = complete_document();
    duplicate
        .operation_profiles
        .push(duplicate.operation_profiles[0].clone());
    let duplicate =
        encode_authority_facts_cbor(&duplicate).expect_err("duplicate fact coordinate rejects");
    assert_exact_kind(&duplicate, AuthorityFactsLoadFailureKind::DuplicateFact);

    let mut invalid = complete_document();
    invalid.budgets[0].source = "invalid budget coordinate".to_owned();
    let invalid = encode_authority_facts_cbor(&invalid).expect_err("invalid semantic fact rejects");
    assert_exact_kind(&invalid, AuthorityFactsLoadFailureKind::InvalidCoordinate);
}

fn canonical_value(document: &AuthorityFactsDocument) -> CanonicalValue {
    let bytes = encode_authority_facts_cbor(document).expect("authority facts encode");
    amblicit_syntax::decode_canonical_cbor(&bytes).expect("canonical authority facts decode")
}

fn nested_map_value_mut<'a>(
    value: &'a mut CanonicalValue,
    path: &[&str],
) -> &'a mut CanonicalValue {
    let Some((field, tail)) = path.split_first() else {
        return value;
    };
    let CanonicalValue::Map(entries) = value else {
        panic!("{field} parent must be a map");
    };
    let child = entries
        .iter_mut()
        .find_map(|(key, value)| {
            (key == &CanonicalValue::Text((*field).to_owned())).then_some(value)
        })
        .unwrap_or_else(|| panic!("missing map field {field}"));
    nested_map_value_mut(child, tail)
}

fn complete_document() -> AuthorityFactsDocument {
    AuthorityFactsDocument {
        api_version: AUTHORITY_FACTS_API_VERSION.to_owned(),
        source: AuthorityFactSource {
            kind: AuthorityFactSourceKind::TargetProfile,
            coordinate: "example.target@1".to_owned(),
            digest: format!("sha256:{}", "1".repeat(64)),
        },
        operation_profiles: vec![OperationProfileFact {
            source: "p.effectful".to_owned(),
            core: "continuum.profile.write/v1".to_owned(),
            allowed_write_classes: vec![WriteClass::Read, WriteClass::Replace],
        }],
        effect_write_classes: vec![EffectWriteClassFact {
            effect: "target.replace".to_owned(),
            write_class: WriteClass::Replace,
        }],
        budgets: vec![BudgetFact {
            source: "p.tiny".to_owned(),
            budget: CoreBudget {
                max_steps: 8,
                max_allocated_bytes: 1024,
                max_output_bytes: 256,
            },
        }],
    }
}

fn assert_exact_kind(
    failures: &[AuthorityFactsLoadFailure],
    expected: AuthorityFactsLoadFailureKind,
) {
    assert_eq!(
        failures
            .iter()
            .map(|failure| failure.kind)
            .collect::<Vec<_>>(),
        vec![expected]
    );
}

use super::*;

fn ordered_facts() -> TargetIrLoweringFacts {
    let mut facts = echo_facts();
    "echo.span-ir/v2".clone_into(&mut facts.target_ir_domain);
    facts
}

fn ordered_core(source: &str) -> CoreModule {
    let module = edict_syntax::parse_module(source).expect("ordered source parses");
    compile_to_core(&module, &effectful_context()).expect("ordered source compiles")
}

fn ordered_artifact(source: &str) -> TargetIrArtifact {
    let report = lower_to_target_ir(&ordered_core(source), &ordered_facts());
    assert_eq!(report.status, TargetLoweringStatus::Lowered, "{report:?}");
    report.artifact.expect("ordered artifact")
}

fn encoded_order(artifact: &TargetIrArtifact) -> Vec<String> {
    let bytes = encode_target_ir_artifact(artifact).expect("ordered encoding");
    let value = decode_canonical_cbor(&bytes).expect("canonical decoding");
    let intent = map_entry(map_entry(&value, "intents"), "t");
    let CanonicalValue::Array(order) = map_entry(intent, "executionOrder") else {
        panic!("execution order is an array");
    };
    order
        .iter()
        .map(|value| match value {
            CanonicalValue::Text(id) => id.clone(),
            _ => panic!("execution order contains instruction ids"),
        })
        .collect()
}

fn map_entry<'a>(value: &'a CanonicalValue, key: &str) -> &'a CanonicalValue {
    let CanonicalValue::Map(entries) = value else {
        panic!("map expected")
    };
    entries
        .iter()
        .find_map(|(name, value)| (name == &CanonicalValue::Text(key.to_owned())).then_some(value))
        .expect("map key exists")
}

#[test]
fn effect_result_guard_preserves_execution_order() {
    let artifact = ordered_artifact(ECHO_EFFECT_OUTPUT_DEPENDENT_REQUIRE);
    assert_eq!(encoded_order(&artifact), ["t.step.0", "t.require.0"]);
    assert_eq!(artifact.intents["t"].requirements.len(), 1);
    assert_eq!(artifact.intents["t"].steps.len(), 1);
}

#[test]
fn input_only_guard_after_effect_stays_after_effect() {
    let artifact = ordered_artifact(ECHO_POST_STEP_INPUT_REQUIRE);
    assert_eq!(encoded_order(&artifact), ["t.step.0", "t.require.0"]);
}

#[test]
fn effect_pure_binding_and_guard_keep_interleaving() {
    let source = ECHO_EFFECT_OUTPUT_DEPENDENT_REQUIRE.replace(
        "require receipt.id != \"\"",
        "let observed: String<max=16> = receipt.id; require observed != \"\"",
    );
    let artifact = ordered_artifact(&source);
    assert_eq!(
        encoded_order(&artifact),
        ["t.step.0", "t.binding.0", "t.require.0"]
    );
}

#[test]
fn guard_between_effects_is_not_hoisted_or_delayed() {
    let source = CHAINED_EFFECT_RESULTS.replace(
        "let second: Receipt",
        "require first.id != \"\" else domain.WriteRejected; let second: Receipt",
    );
    let artifact = ordered_artifact(&source);
    assert_eq!(
        encoded_order(&artifact),
        ["t.step.0", "t.require.0", "t.step.1"]
    );
}

#[test]
fn ordered_selection_keeps_legacy_guard_refusal() {
    let core = ordered_core(ECHO_EFFECT_OUTPUT_DEPENDENT_REQUIRE);
    let report = lower_to_target_ir(&core, &echo_facts());
    assert_eq!(report.status, TargetLoweringStatus::Unsupported);
    assert_eq!(
        report.failures[0].kind,
        TargetLoweringFailureKind::UnsupportedTargetFeature
    );
    assert!(report.artifact.is_none());
}

#[test]
fn ordered_selection_still_rejects_forward_and_duplicate_producers() {
    let valid = ordered_core(ECHO_EFFECT_OUTPUT_DEPENDENT_REQUIRE);
    let mut forward = valid.clone();
    forward
        .intents
        .get_mut("t")
        .expect("intent")
        .body
        .nodes
        .swap(0, 1);
    let mut duplicate = valid;
    let body = &mut duplicate.intents.get_mut("t").expect("intent").body;
    body.nodes.push(body.nodes[0].clone());
    for core in [forward, duplicate] {
        let report = lower_to_target_ir(&core, &ordered_facts());
        assert_eq!(report.status, TargetLoweringStatus::Unsupported);
        assert_eq!(
            report.failures[0].kind,
            TargetLoweringFailureKind::InvalidCoreIdentity
        );
        assert!(report.artifact.is_none());
    }
}

#[test]
fn encoder_refuses_missing_duplicate_foreign_and_forward_order_entries() {
    let baseline = ordered_artifact(ECHO_EFFECT_OUTPUT_DEPENDENT_REQUIRE);
    let invalid_orders = [
        None,
        Some(vec!["t.step.0".to_owned()]),
        Some(vec!["t.step.0".to_owned(), "t.step.0".to_owned()]),
        Some(vec!["t.step.0".to_owned(), "other.require.0".to_owned()]),
        Some(vec!["t.require.0".to_owned(), "t.step.0".to_owned()]),
    ];
    for order in invalid_orders {
        let mut artifact = baseline.clone();
        artifact
            .intents
            .get_mut("t")
            .expect("intent")
            .execution_order = order;
        assert_eq!(
            encode_target_ir_artifact(&artifact)
                .expect_err("bad order")
                .kind(),
            CanonicalErrorKind::UnsupportedValue
        );
    }
    let mut legacy_domain = baseline;
    legacy_domain.domain = ECHO_SPAN_IR_DOMAIN.to_owned();
    assert_eq!(
        encode_target_ir_artifact(&legacy_domain)
            .expect_err("v1 has no order")
            .kind(),
        CanonicalErrorKind::UnsupportedValue
    );
}

#[test]
fn encoder_binds_valid_independent_orderings_into_identity() {
    let mut artifact = ordered_artifact(ECHO_POST_STEP_INPUT_REQUIRE);
    let original = digest_target_ir_artifact(&artifact).expect("digest");
    artifact
        .intents
        .get_mut("t")
        .expect("intent")
        .execution_order
        .as_mut()
        .expect("order")
        .swap(0, 1);
    assert_ne!(
        digest_target_ir_artifact(&artifact).expect("valid independent reorder"),
        original
    );
}

#[test]
fn payload_only_effect_dependency_keeps_guard_after_effect() {
    let source =
        ECHO_EFFECT_OUTPUT_DEPENDENT_REQUIRE.replace("require receipt.id != \"\"", "require true");
    let mut artifact = ordered_artifact(&source);
    assert_eq!(encoded_order(&artifact), ["t.step.0", "t.require.0"]);
    artifact
        .intents
        .get_mut("t")
        .expect("intent")
        .execution_order
        .as_mut()
        .expect("order")
        .swap(0, 1);
    assert_eq!(
        encode_target_ir_artifact(&artifact)
            .expect_err("payload dependency")
            .kind(),
        CanonicalErrorKind::UnsupportedValue
    );
}

#[test]
fn ordered_encoder_rejects_cross_table_ids_and_inconsistent_local_references() {
    let baseline = ordered_artifact(ECHO_EFFECT_OUTPUT_DEPENDENT_REQUIRE);
    let mut duplicate_id = baseline.clone();
    duplicate_id
        .intents
        .get_mut("t")
        .expect("intent")
        .requirements[0]
        .id = "t.step.0".to_owned();
    let mut inconsistent = baseline.clone();
    let guard = &mut inconsistent
        .intents
        .get_mut("t")
        .expect("intent")
        .requirements[0];
    assert!(mutate_first_predicate_local_type(
        &mut guard.predicate,
        "U64"
    ));
    let mut missing_result = baseline;
    missing_result.intents.get_mut("t").expect("intent").result = CoreExpr::Local {
        reference: LocalRef {
            id: "not.produced".to_owned(),
            alpha_name: "$absent".to_owned(),
            ty: "U64".to_owned(),
        },
    };
    for artifact in [duplicate_id, inconsistent, missing_result] {
        assert_eq!(
            encode_target_ir_artifact(&artifact)
                .expect_err("invalid order authority")
                .kind(),
            CanonicalErrorKind::UnsupportedValue
        );
    }
}

#[test]
fn ordered_contract_does_not_silently_drop_external_requests() {
    let core = workspace_request_core();
    let mut facts = ordered_facts();
    facts
        .operation_profiles
        .push("continuum.profile.read-only/v1".to_owned());
    let report = lower_to_target_ir(&core, &facts);
    assert_eq!(report.status, TargetLoweringStatus::Unsupported);
    assert!(report
        .failures
        .iter()
        .any(|failure| failure.kind == TargetLoweringFailureKind::UnsupportedCoreNode));
    assert!(report.artifact.is_none());
}

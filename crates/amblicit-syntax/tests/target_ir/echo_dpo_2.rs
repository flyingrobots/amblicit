//! Target profile `echo.dpo@2` selects the ordered `echo.span-ir/v2` contract
//! with its own intrinsic namespace (issue #310).

use super::*;

const ECHO_DPO_2: &str = "echo.dpo@2";
const ORDERED: &str = "echo.span-ir/v2";
const STRUCTURED: &str = "echo.span-ir/v3";

fn echo_dpo_2_facts(target_intrinsic: &str) -> TargetIrLoweringFacts {
    let mut facts = echo_facts();
    ECHO_DPO_2.clone_into(&mut facts.target_profile.coordinate);
    ORDERED.clone_into(&mut facts.target_ir_domain);
    target_intrinsic.clone_into(&mut facts.effect_lowerings[0].target_intrinsic);
    facts
}

fn guarded_effect_core() -> CoreModule {
    let module = amblicit_syntax::parse_module(ECHO_EFFECT_OUTPUT_DEPENDENT_REQUIRE)
        .expect("guarded effect source parses");
    compile_to_core(&module, &effectful_context()).expect("guarded effect source compiles")
}

fn echo_dpo_2_artifact() -> TargetIrArtifact {
    let report = lower_to_target_ir(
        &guarded_effect_core(),
        &echo_dpo_2_facts("echo.dpo@2.replace"),
    );
    assert_eq!(report.status, TargetLoweringStatus::Lowered, "{report:?}");
    assert!(report.failures.is_empty(), "{report:?}");
    report.artifact.expect("echo.dpo@2 artifact")
}

fn text_entry<'a>(value: &'a CanonicalValue, key: &str) -> &'a CanonicalValue {
    let CanonicalValue::Map(entries) = value else {
        panic!("map expected")
    };
    entries
        .iter()
        .find_map(|(name, value)| (name == &CanonicalValue::Text(key.to_owned())).then_some(value))
        .unwrap_or_else(|| panic!("map key `{key}` exists"))
}

fn text(value: &str) -> CanonicalValue {
    CanonicalValue::Text(value.to_owned())
}

#[test]
fn echo_dpo_2_lowers_effects_to_ordered_target_ir() {
    let facts = echo_dpo_2_facts("echo.dpo@2.replace");
    let artifact = echo_dpo_2_artifact();

    assert_eq!(artifact.domain, ORDERED);
    assert_eq!(artifact.target_profile, facts.target_profile);
    let intent = &artifact.intents["t"];
    assert_eq!(intent.steps.len(), 1);
    assert_eq!(intent.steps[0].target_intrinsic, "echo.dpo@2.replace");
    // The guard reads the effect result, which only the ordered model admits.
    assert_eq!(intent.requirements.len(), 1);

    let bytes = encode_target_ir_artifact(&artifact).expect("ordered echo.dpo@2 encoding");
    let value = decode_canonical_cbor(&bytes).expect("canonical decoding");
    assert_eq!(text_entry(&value, "kind"), &text("orderedTargetIrArtifact"));
    assert_eq!(text_entry(&value, "domain"), &text(ORDERED));
    assert_eq!(
        text_entry(text_entry(&value, "targetProfile"), "coordinate"),
        &text(ECHO_DPO_2)
    );
    let encoded_intent = text_entry(text_entry(&value, "intents"), "t");
    assert_eq!(
        text_entry(encoded_intent, "executionOrder"),
        &CanonicalValue::Array(vec![text("t.step.0"), text("t.require.0")])
    );
}

#[test]
fn echo_dpo_2_and_echo_dpo_1_refuse_each_others_intrinsics() {
    let mut echo_dpo_1_ordered = echo_facts();
    ORDERED.clone_into(&mut echo_dpo_1_ordered.target_ir_domain);
    "echo.dpo@2.replace".clone_into(&mut echo_dpo_1_ordered.effect_lowerings[0].target_intrinsic);

    for (facts, refused) in [
        (echo_dpo_2_facts("echo.dpo@1.replace"), "echo.dpo@1.replace"),
        (
            echo_dpo_2_facts("echo.dpo@20.replace"),
            "echo.dpo@20.replace",
        ),
        (echo_dpo_1_ordered, "echo.dpo@2.replace"),
    ] {
        let report = lower_to_target_ir(&guarded_effect_core(), &facts);

        assert_eq!(
            report.status,
            TargetLoweringStatus::Unsupported,
            "{refused}"
        );
        assert!(report.artifact.is_none(), "{refused}");
        assert_eq!(
            failure_kinds(&report),
            vec![TargetLoweringFailureKind::UnsupportedTargetIntrinsic],
            "{refused}"
        );
        assert_eq!(report.failures[0].detail, refused);
    }
}

#[test]
fn echo_dpo_2_selects_only_the_ordered_target_ir() {
    for domain in [
        ECHO_SPAN_IR_DOMAIN,
        STRUCTURED,
        GITWARP_COMMIT_REDUCER_IR_DOMAIN,
    ] {
        let mut facts = echo_dpo_2_facts("echo.dpo@2.replace");
        domain.clone_into(&mut facts.target_ir_domain);

        let report = lower_to_target_ir(&guarded_effect_core(), &facts);

        assert_eq!(report.status, TargetLoweringStatus::Unsupported, "{domain}");
        assert!(report.artifact.is_none(), "{domain}");
        assert_eq!(
            failure_kinds(&report),
            vec![TargetLoweringFailureKind::UnsupportedTargetIrDomain],
            "{domain}"
        );
    }
}

#[test]
fn ordered_encoding_requires_a_profile_that_selects_its_domain() {
    let ordered = echo_dpo_2_artifact();

    let mut structured_under_echo_dpo_2 = ordered.clone();
    STRUCTURED.clone_into(&mut structured_under_echo_dpo_2.domain);
    let mut ordered_under_gitwarp = ordered.clone();
    GITWARP_REF_CRDT_TARGET_PROFILE
        .clone_into(&mut ordered_under_gitwarp.target_profile.coordinate);
    for refused in [structured_under_echo_dpo_2, ordered_under_gitwarp] {
        let error = encode_target_ir_artifact(&refused)
            .expect_err("profile does not select this ordered domain");
        assert_eq!(error.kind(), CanonicalErrorKind::UnsupportedValue);
    }

    // Control: the same instruction tree is a valid structured artifact under
    // the profile that does select `echo.span-ir/v3`.
    let mut structured_under_echo_dpo_1 = ordered;
    STRUCTURED.clone_into(&mut structured_under_echo_dpo_1.domain);
    ECHO_DPO_TARGET_PROFILE.clone_into(&mut structured_under_echo_dpo_1.target_profile.coordinate);
    encode_target_ir_artifact(&structured_under_echo_dpo_1)
        .expect("echo.dpo@1 selects the structured domain");
}

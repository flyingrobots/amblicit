//! End-to-end compiler and Target IR evidence for executable-operation prerequisites.
//!
//! This fixture remains application-neutral. It proves Edict preserves exact
//! operation inputs and semantic-resource identity; it does not claim Echo
//! admission, execution, commitment, or receipt evidence.

use amblicit_syntax::ast::IntentClause;
use amblicit_syntax::{
    compile_to_core, decode_canonical_cbor, digest_core_module, digest_target_ir_artifact,
    encode_core_module, encode_target_ir_artifact, lower_to_target_ir, parse_module,
    resolve_module, type_check, CanonicalErrorKind, CanonicalValue, CompilerContext,
    CompilerErrorKind, CompilerStage, CoreBudget, CoreExpr, CoreImport, CoreImportKind, CoreNode,
    CorePredicate, CoreType, CoreValue, ResourceRef, TargetEffectLowering, TargetIrArtifact,
    TargetIrLoweringFacts, TargetLoweringFailureKind, TargetLoweringStatus, WriteClass,
    ECHO_DPO_TARGET_PROFILE, ECHO_SPAN_IR_DOMAIN,
};

const OPERATION_SOURCE: &str =
    include_str!("../../../fixtures/lang/operations/explicit-basis-u64.amblicit");

fn operation_context() -> CompilerContext {
    CompilerContext::new()
        .with_operation_profile("sequence.splice", "continuum.profile.write/v1")
        .with_operation_profile_write_classes("sequence.splice", [WriteClass::Replace])
        .with_effect_write_class("sequence.splice", WriteClass::Replace)
        .with_budget(
            "sequence.small",
            CoreBudget {
                max_steps: 64,
                max_allocated_bytes: 16 * 1024,
                max_output_bytes: 4096,
            },
        )
}

fn digest(hex: char) -> String {
    format!("sha256:{}", hex.to_string().repeat(64))
}

fn target_facts() -> TargetIrLoweringFacts {
    TargetIrLoweringFacts {
        target_profile: ResourceRef {
            coordinate: ECHO_DPO_TARGET_PROFILE.to_owned(),
            digest: Some(digest('2')),
        },
        target_ir_domain: ECHO_SPAN_IR_DOMAIN.to_owned(),
        operation_profiles: vec!["continuum.profile.write/v1".to_owned()],
        obstruction_coordinates: vec!["rejected".to_owned()],
        effect_lowerings: vec![TargetEffectLowering {
            effect: "sequence.splice".to_owned(),
            target_intrinsic: "echo.dpo@1.splice".to_owned(),
            failure_mappings: std::collections::BTreeMap::new(),
        }],
        effect_signatures: Vec::new(),
        pure_functions: Vec::new(),
    }
}

fn compile_operation(source: &str) -> amblicit_syntax::CoreModule {
    let module = parse_module(source).expect("operation prerequisite source parses");
    compile_to_core(&module, &operation_context()).expect("operation prerequisite compiles")
}

fn lower_operation(source: &str) -> (amblicit_syntax::CoreModule, TargetIrArtifact) {
    let core = compile_operation(source);
    let report = lower_to_target_ir(&core, &target_facts());
    assert_eq!(report.status, TargetLoweringStatus::Lowered);
    let artifact = report.artifact.expect("operation prerequisite lowers");
    (core, artifact)
}

fn map_field<'a>(value: &'a CanonicalValue, field: &str) -> &'a CanonicalValue {
    let CanonicalValue::Map(entries) = value else {
        panic!("expected map while looking up {field:?}, got {value:?}");
    };
    entries
        .iter()
        .find_map(|(key, value)| match key {
            CanonicalValue::Text(key) if key == field => Some(value),
            _ => None,
        })
        .unwrap_or_else(|| panic!("missing canonical field {field:?}"))
}

fn text_field<'a>(value: &'a CanonicalValue, field: &str) -> &'a str {
    let CanonicalValue::Text(value) = map_field(value, field) else {
        panic!("canonical field {field:?} is not text");
    };
    value
}

fn digest_bytes(value: &CanonicalValue) -> &[u8] {
    let CanonicalValue::Array(parts) = value else {
        panic!("digest is not the canonical algorithm/payload pair");
    };
    let [CanonicalValue::Text(algorithm), CanonicalValue::Bytes(bytes)] = parts.as_slice() else {
        panic!("digest pair has the wrong canonical shape");
    };
    assert_eq!(algorithm, "sha256");
    bytes
}

#[test]
fn operation_prerequisite_fixture_preserves_fixed_width_basis_and_lawpack_closure() {
    let (core, target) = lower_operation(OPERATION_SOURCE);

    let Some(CoreType::Record { fields }) = core.types.get("SpliceInput") else {
        panic!("SpliceInput remains a named record");
    };
    assert_eq!(fields.get("start").map(String::as_str), Some("U64"));
    assert_eq!(fields.get("end").map(String::as_str), Some("U64"));
    assert_eq!(
        fields.get("replacement").map(String::as_str),
        Some("Bytes<max=4096>")
    );
    assert!(core
        .types
        .keys()
        .all(|coordinate| !coordinate.starts_with("SpliceInput.")));
    let lawpack = core
        .imports
        .iter()
        .find(|import| import.kind == CoreImportKind::Lawpack)
        .expect("lawpack import reaches Core");
    assert_eq!(lawpack.resource.coordinate, "sequence.edit@1");
    let expected_lawpack_digest = digest('1');
    assert_eq!(
        lawpack.resource.digest.as_deref(),
        Some(expected_lawpack_digest.as_str())
    );

    let core_value = decode_canonical_cbor(&encode_core_module(&core).expect("Core encodes"))
        .expect("Core bytes decode");
    let core_intent = map_field(map_field(&core_value, "intents"), "splice");
    let core_basis = map_field(core_intent, "basis");
    assert_eq!(text_field(core_basis, "kind"), "field");
    assert_eq!(text_field(core_basis, "field"), "basis");

    let target_value =
        decode_canonical_cbor(&encode_target_ir_artifact(&target).expect("Target IR encodes"))
            .expect("Target IR bytes decode");
    let target_intent = map_field(map_field(&target_value, "intents"), "splice");
    assert_eq!(map_field(target_intent, "basis"), core_basis);

    let closure = map_field(&target_value, "semanticClosure");
    let source_core = map_field(closure, "sourceCore");
    assert_eq!(text_field(source_core, "id"), core.coordinate);
    assert_eq!(
        digest_bytes(map_field(source_core, "digest")),
        digest_core_module(&core).expect("Core digest").bytes()
    );
    let CanonicalValue::Array(lawpacks) = map_field(closure, "lawpacks") else {
        panic!("semantic closure lawpacks are not an array");
    };
    assert_eq!(lawpacks.len(), 1);
    assert_eq!(text_field(&lawpacks[0], "id"), "sequence.edit@1");
    assert_eq!(digest_bytes(map_field(&lawpacks[0], "digest")), &[0x11; 32]);
}

#[test]
fn out_of_range_u64_and_cross_width_values_reject_before_core() {
    for replacement in ["18446744073709551616u64", "-1u64", "1i64"] {
        let source = OPERATION_SOURCE.replace("18446744073709551615u64", replacement);
        let module = parse_module(&source).expect("mutated source parses");
        let errors = compile_to_core(&module, &operation_context())
            .expect_err("invalid fixed-width value must reject");
        assert_eq!(
            errors
                .iter()
                .map(|error| (error.stage, error.kind))
                .collect::<Vec<_>>(),
            vec![(CompilerStage::TypeCheck, CompilerErrorKind::TypeMismatch)]
        );
    }
}

#[test]
fn fixed_width_integer_types_and_suffixes_preserve_exact_domains() {
    for (width, suffix, maximum) in [
        ("I32", "i32", "2147483647"),
        ("I64", "i64", "9223372036854775807"),
        ("U8", "u8", "255"),
        ("U32", "u32", "4294967295"),
        ("U64", "u64", "18446744073709551615"),
    ] {
        let source = OPERATION_SOURCE
            .replace("U64", width)
            .replace("18446744073709551615u64", &format!("{maximum}{suffix}"));
        let core = compile_operation(&source);
        let Some(CoreType::Record { fields }) = core.types.get("SpliceInput") else {
            panic!("SpliceInput remains a named record");
        };
        assert_eq!(fields.get("start").map(String::as_str), Some(width));
        assert!(!core.types.contains_key("SpliceInput.start"));
        let CorePredicate::Compare { right, .. } = &core
            .intents
            .get("splice")
            .expect("splice intent")
            .input_constraints[0]
            .predicate
        else {
            panic!("first input constraint is not a comparison");
        };
        assert_eq!(
            right,
            &CoreExpr::Const(CoreValue::Int {
                width: width.to_owned(),
                value: maximum.to_owned(),
            })
        );
    }
}

#[test]
fn u8_context_and_width_preserve_canonical_identity() {
    let source = OPERATION_SOURCE
        .replace("U64", "U8")
        .replace("18446744073709551615u64", "255");
    let core = compile_operation(&source);
    let CorePredicate::Compare { right, .. } =
        &core.intents["splice"].input_constraints[0].predicate
    else {
        panic!("expected a comparison");
    };
    assert_eq!(
        right,
        &CoreExpr::Const(CoreValue::Int {
            width: "U8".into(),
            value: "255".into(),
        })
    );
    let wider = compile_operation(&source.replace("U8", "U32"));
    assert_ne!(
        digest_core_module(&core).unwrap(),
        digest_core_module(&wider).unwrap()
    );
    let report = lower_to_target_ir(&core, &target_facts());
    assert_eq!(report.status, TargetLoweringStatus::Lowered);
}

#[test]
fn u8_literals_refuse_out_of_range_and_cross_width_values() {
    for literal in ["256u8", "-1u8", "256", "-1", "1u32"] {
        let source = OPERATION_SOURCE
            .replace("U64", "U8")
            .replace("18446744073709551615u64", literal);
        let module = parse_module(&source).expect("U8 source syntax");
        let errors = compile_to_core(&module, &operation_context()).expect_err("invalid U8 domain");
        assert_eq!(
            errors
                .iter()
                .map(|error| (error.stage, error.kind))
                .collect::<Vec<_>>(),
            vec![(CompilerStage::TypeCheck, CompilerErrorKind::TypeMismatch)],
            "{literal}"
        );
    }
}

#[test]
fn u8_source_helper_preserves_its_result_width() {
    let source = OPERATION_SOURCE.replace(
        "intent splice",
        "fn byteMarker() -> U8 { return 255u8; }\nintent splice",
    );
    let context = operation_context().with_budget(
        "sequence.small",
        CoreBudget {
            max_steps: 4096,
            max_allocated_bytes: 65536,
            max_output_bytes: 4096,
        },
    );
    let core = compile_to_core(&parse_module(&source).expect("U8 helper syntax"), &context)
        .expect("U8 helper with sufficient whole-operation budget");
    assert_eq!(
        core.functions["byteMarker"].body.result,
        CoreExpr::Const(CoreValue::Int {
            width: "U8".into(),
            value: "255".into(),
        })
    );
}

#[test]
fn bare_integer_literals_inherit_unambiguous_fixed_width_context() {
    let comparison = compile_operation(&OPERATION_SOURCE.replace("18446744073709551615u64", "1"));
    let CorePredicate::Compare { right, .. } =
        &comparison.intents["splice"].input_constraints[0].predicate
    else {
        panic!("first input constraint is not a comparison");
    };
    assert_eq!(
        right,
        &CoreExpr::Const(CoreValue::Int {
            width: "U64".to_owned(),
            value: "1".to_owned(),
        })
    );

    let annotated = compile_operation(&OPERATION_SOURCE.replace(
        "  let receipt: SpliceReceipt",
        "  let marker: U32 = 1;\n  let receipt: SpliceReceipt",
    ));
    let CoreNode::Let { value, .. } = &annotated.intents["splice"].body.nodes[0] else {
        panic!("annotated bare integer did not lower as a pure let");
    };
    assert_eq!(
        value,
        &CoreExpr::Const(CoreValue::Int {
            width: "U32".to_owned(),
            value: "1".to_owned(),
        })
    );

    let returned = compile_operation(&OPERATION_SOURCE.replace(
        "return { version: receipt.version };",
        "return { version: 1 };",
    ));
    let CoreExpr::Record { fields } = &returned.intents["splice"].body.result else {
        panic!("operation result is not a record");
    };
    assert_eq!(
        fields["version"],
        CoreExpr::Const(CoreValue::Int {
            width: "U64".to_owned(),
            value: "1".to_owned(),
        })
    );
}

#[test]
fn unconstrained_bare_integer_literal_refuses_before_core() {
    let source = OPERATION_SOURCE.replace(
        "  let receipt: SpliceReceipt",
        "  let marker = 1;\n  let receipt: SpliceReceipt",
    );
    let module = parse_module(&source).expect("mutated source parses");
    let errors = compile_to_core(&module, &operation_context())
        .expect_err("a bare integer without an expected width must reject");
    assert_eq!(
        errors
            .iter()
            .map(|error| (error.stage, error.kind))
            .collect::<Vec<_>>(),
        vec![(CompilerStage::TypeCheck, CompilerErrorKind::TypeMismatch)]
    );
}

#[test]
fn signed_fixed_width_minima_preserve_exact_domains() {
    for (width, suffix, minimum) in [
        ("I32", "i32", "-2147483648"),
        ("I64", "i64", "-9223372036854775808"),
    ] {
        let source = OPERATION_SOURCE
            .replace("U64", width)
            .replace("18446744073709551615u64", &format!("{minimum}{suffix}"));
        let core = compile_operation(&source);
        let CorePredicate::Compare { right, .. } = &core
            .intents
            .get("splice")
            .expect("splice intent")
            .input_constraints[0]
            .predicate
        else {
            panic!("first input constraint is not a comparison");
        };
        assert_eq!(
            right,
            &CoreExpr::Const(CoreValue::Int {
                width: width.to_owned(),
                value: minimum.to_owned(),
            })
        );
    }
}

#[test]
fn canonical_core_rejects_values_outside_their_declared_integer_domain() {
    for (width, value) in [
        ("I32", "2147483648"),
        ("I64", "9223372036854775808"),
        ("U32", "4294967296"),
        ("U32", "-1"),
        ("U64", "18446744073709551616"),
        ("unknown", "0"),
    ] {
        let mut core = compile_operation(OPERATION_SOURCE);
        let CorePredicate::Compare { right, .. } = &mut core
            .intents
            .get_mut("splice")
            .expect("splice intent")
            .input_constraints[0]
            .predicate
        else {
            panic!("first input constraint is not a comparison");
        };
        *right = CoreExpr::Const(CoreValue::Int {
            width: width.to_owned(),
            value: value.to_owned(),
        });
        assert_eq!(
            encode_core_module(&core)
                .expect_err("declared integer domain must guard canonical identity")
                .kind(),
            CanonicalErrorKind::InvalidInteger,
            "{width} accepted {value}"
        );
    }
}

#[test]
fn body_local_cannot_become_an_intent_basis() {
    let source = OPERATION_SOURCE.replace("basis input.basis", "basis receipt.version");
    let module = parse_module(&source).expect("mutated source parses");
    let errors = compile_to_core(&module, &operation_context())
        .expect_err("body-local basis must reject before Core");
    assert_eq!(
        errors
            .iter()
            .map(|error| (error.stage, error.kind))
            .collect::<Vec<_>>(),
        vec![(CompilerStage::TypeCheck, CompilerErrorKind::UnresolvedType)]
    );
}

#[test]
fn direct_type_check_refuses_a_missing_basis_without_panicking() {
    let source = OPERATION_SOURCE.replace("  basis input.basis\n", "");
    let module = parse_module(&source).expect("mutated source parses");
    let resolved = resolve_module(&module, &operation_context()).expect("source resolves");
    let errors = type_check(&resolved).expect_err("missing basis must reject during type checking");
    assert_eq!(
        errors
            .iter()
            .map(|error| (error.stage, error.kind))
            .collect::<Vec<_>>(),
        vec![(
            CompilerStage::TypeCheck,
            CompilerErrorKind::UnsupportedSourceShape
        )]
    );
}

#[test]
fn direct_type_check_refuses_duplicate_basis_clauses() {
    let module = parse_module(OPERATION_SOURCE).expect("operation prerequisite source parses");
    let mut resolved =
        resolve_module(&module, &operation_context()).expect("operation prerequisite resolves");
    resolved.intents[0]
        .source
        .clauses
        .push(IntentClause::Basis(None));

    let errors = type_check(&resolved).expect_err("duplicate basis clauses must reject");
    assert_eq!(
        errors
            .iter()
            .map(|error| (error.stage, error.kind))
            .collect::<Vec<_>>(),
        vec![(
            CompilerStage::TypeCheck,
            CompilerErrorKind::UnsupportedSourceShape
        )]
    );
}

#[test]
fn explicit_basis_and_semantic_input_mutations_move_target_identity() {
    let (_, baseline) = lower_operation(OPERATION_SOURCE);
    let baseline_digest = digest_target_ir_artifact(&baseline).expect("baseline Target IR digest");

    for mutated in [
        OPERATION_SOURCE.replace("basis input.basis", "basis input.replacement"),
        OPERATION_SOURCE.replace("sequence.edit@1", "sequence.alternate@1"),
        OPERATION_SOURCE.replace(&digest('1'), &digest('3')),
        OPERATION_SOURCE.replace("18446744073709551615u64", "18446744073709551614u64"),
    ] {
        let (_, artifact) = lower_operation(&mutated);
        assert_ne!(
            digest_target_ir_artifact(&artifact).expect("mutated Target IR digest"),
            baseline_digest
        );
    }
}

#[test]
fn target_lowering_refuses_an_unidentifiable_semantic_closure() {
    let mut core = compile_operation(OPERATION_SOURCE);
    let CorePredicate::Compare { right, .. } = &mut core
        .intents
        .get_mut("splice")
        .expect("splice intent")
        .input_constraints[0]
        .predicate
    else {
        panic!("first input constraint is not a comparison");
    };
    *right = CoreExpr::Const(CoreValue::Int {
        width: "U32".to_owned(),
        value: "4294967296".to_owned(),
    });

    let report = lower_to_target_ir(&core, &target_facts());
    assert_eq!(report.status, TargetLoweringStatus::Unsupported);
    assert!(report.artifact.is_none());
    assert_eq!(
        report
            .failures
            .iter()
            .map(|failure| failure.kind)
            .collect::<Vec<_>>(),
        vec![TargetLoweringFailureKind::InvalidCoreIdentity]
    );
}

#[test]
fn target_lowering_refuses_an_empty_source_core_coordinate() {
    let mut core = compile_operation(OPERATION_SOURCE);
    core.coordinate.clear();

    let report = lower_to_target_ir(&core, &target_facts());
    assert_eq!(report.status, TargetLoweringStatus::Unsupported);
    assert!(report.artifact.is_none());
    assert_eq!(
        report
            .failures
            .iter()
            .map(|failure| failure.kind)
            .collect::<Vec<_>>(),
        vec![TargetLoweringFailureKind::InvalidCoreIdentity]
    );
}

#[test]
fn legacy_target_ir_encoder_rejects_an_empty_source_core_coordinate() {
    let (_, mut artifact) = lower_operation(OPERATION_SOURCE);
    artifact.source_core_coordinate.clear();
    artifact.semantic_closure = None;
    for intent in artifact.intents.values_mut() {
        intent.basis = None;
    }

    assert_eq!(
        encode_target_ir_artifact(&artifact)
            .expect_err("legacy Target IR with an empty Core coordinate must not encode")
            .kind(),
        CanonicalErrorKind::UnsupportedValue
    );
}

#[test]
fn semantic_closure_lawpack_set_is_order_invariant() {
    let core = compile_operation(OPERATION_SOURCE);
    let primary = core
        .imports
        .iter()
        .find(|import| import.kind == CoreImportKind::Lawpack)
        .expect("primary lawpack")
        .clone();
    let alternate = CoreImport {
        kind: CoreImportKind::Lawpack,
        resource: ResourceRef {
            coordinate: "sequence.alternate@1".to_owned(),
            digest: Some(digest('4')),
        },
        alias: Some("alternate".to_owned()),
    };

    let mut forward = core.clone();
    forward.imports = vec![primary.clone(), alternate.clone()];
    let forward = lower_to_target_ir(&forward, &target_facts())
        .artifact
        .expect("forward import order lowers");
    let mut reverse = core;
    reverse.imports = vec![alternate, primary];
    let reverse = lower_to_target_ir(&reverse, &target_facts())
        .artifact
        .expect("reverse import order lowers");

    let closure = forward
        .semantic_closure
        .as_ref()
        .expect("operation has semantic closure");
    assert_eq!(
        closure
            .lawpacks
            .iter()
            .map(|lawpack| lawpack.coordinate.as_str())
            .collect::<Vec<_>>(),
        vec!["sequence.alternate@1", "sequence.edit@1"]
    );
    assert_eq!(
        digest_target_ir_artifact(&forward).expect("forward digest"),
        digest_target_ir_artifact(&reverse).expect("reverse digest")
    );

    let mut duplicate = forward.clone();
    let lawpacks = &mut duplicate
        .semantic_closure
        .as_mut()
        .expect("operation has semantic closure")
        .lawpacks;
    lawpacks.push(lawpacks[0].clone());
    assert_eq!(
        digest_target_ir_artifact(&duplicate).expect("set duplicate canonicalizes"),
        digest_target_ir_artifact(&forward).expect("forward digest")
    );
}

#[test]
fn invalid_or_conflicting_lawpack_resources_refuse_before_target_artifact() {
    let mut uppercase = compile_operation(OPERATION_SOURCE);
    uppercase.imports[0].resource.digest = Some(format!("sha256:{}", "A".repeat(64)));
    let report = lower_to_target_ir(&uppercase, &target_facts());
    assert!(report.artifact.is_none());
    assert_eq!(
        report
            .failures
            .iter()
            .map(|failure| failure.kind)
            .collect::<Vec<_>>(),
        vec![TargetLoweringFailureKind::UndigestedCoreImport]
    );

    let mut conflicting = compile_operation(OPERATION_SOURCE);
    let mut conflict = conflicting.imports[0].clone();
    conflict.resource.digest = Some(digest('5'));
    conflict.alias = Some("conflict".to_owned());
    conflicting.imports.push(conflict);
    let report = lower_to_target_ir(&conflicting, &target_facts());
    assert!(report.artifact.is_none());
    assert_eq!(
        report
            .failures
            .iter()
            .map(|failure| failure.kind)
            .collect::<Vec<_>>(),
        vec![TargetLoweringFailureKind::InvalidCoreIdentity]
    );

    let (_, mut artifact) = lower_operation(OPERATION_SOURCE);
    let lawpacks = &mut artifact
        .semantic_closure
        .as_mut()
        .expect("operation has semantic closure")
        .lawpacks;
    let mut conflict = lawpacks[0].clone();
    conflict.digest = Some(digest('5'));
    lawpacks.push(conflict);
    assert_eq!(
        digest_target_ir_artifact(&artifact)
            .expect_err("conflicting public artifact must not gain identity")
            .kind(),
        CanonicalErrorKind::UnsupportedValue
    );
}

#[test]
fn semantic_closure_cannot_substitute_a_different_source_core_coordinate() {
    let (_, mut artifact) = lower_operation(OPERATION_SOURCE);
    artifact
        .semantic_closure
        .as_mut()
        .expect("operation has semantic closure")
        .source_core
        .coordinate = "examples.substituted@1".to_owned();

    assert_eq!(
        digest_target_ir_artifact(&artifact)
            .expect_err("contradictory source Core coordinates must not gain identity")
            .kind(),
        CanonicalErrorKind::UnsupportedValue
    );
}

#[test]
fn explicit_basis_target_ir_cannot_drop_its_semantic_closure() {
    let (_, mut artifact) = lower_operation(OPERATION_SOURCE);
    artifact.semantic_closure = None;

    assert_eq!(
        digest_target_ir_artifact(&artifact)
            .expect_err("basis-bearing Target IR without a closure must not gain identity")
            .kind(),
        CanonicalErrorKind::UnsupportedValue
    );
}

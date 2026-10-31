//! Exhaustive nonrecursive variants: source and canonical Core contracts.
use amblicit_syntax::{
    compile_to_core, decode_canonical_cbor, encode_core_module, parse_module, CanonicalValue as V,
    CompilerContext, CompilerErrorKind, CoreModule,
};

fn compile(body: &str) -> Result<CoreModule, Vec<amblicit_syntax::CompilerError>> {
    let source =
        format!("package choices.matching@1; type Choice = variant {{ Empty, Data(U8) }}; {body}");
    compile_to_core(
        &parse_module(&source).expect("valid source syntax"),
        &CompilerContext::new(),
    )
}
fn field<'a>(value: &'a V, key: &str) -> &'a V {
    let V::Map(fields) = value else {
        panic!("map expected")
    };
    fields
        .iter()
        .find_map(|(k, v)| (k == &V::Text(key.into())).then_some(v))
        .expect("field")
}
fn encoded(core: &CoreModule) -> V {
    decode_canonical_cbor(&encode_core_module(core).expect("canonical encoding"))
        .expect("canonical decoding")
}
#[test]
fn variant_matches_preserve_scrutinee_and_arm_local_payloads() {
    let core = compile("fn unwrap(value: Choice) -> U8 { return match value { Empty => 0u8, Data(payload) => payload, }; }").expect("exhaustive variant match");
    let wire = encoded(&core);
    let expr = field(
        field(field(field(&wire, "functions"), "unwrap"), "body"),
        "result",
    );
    assert_eq!(field(expr, "kind"), &V::Text("match".into()));
    assert_eq!(
        field(field(expr, "scrutinee"), "kind"),
        &V::Text("local".into())
    );
    let V::Array(arms) = field(expr, "arms") else {
        panic!("arms array")
    };
    assert_eq!(arms.len(), 2);
    let data = arms
        .iter()
        .find(|arm| field(arm, "case") == &V::Text("Data".into()))
        .expect("Data arm");
    assert_eq!(field(field(data, "body"), "kind"), &V::Text("local".into()));
    assert_eq!(field(data, "binder"), field(field(data, "body"), "ref"));
    let empty = arms
        .iter()
        .find(|arm| field(arm, "case") == &V::Text("Empty".into()))
        .expect("Empty arm");
    let V::Map(fields) = empty else {
        panic!("arm map")
    };
    assert!(!fields
        .iter()
        .any(|(key, _)| key == &V::Text("binder".into())));
}
#[test]
fn variant_matches_are_alpha_stable() {
    let first=compile("fn unwrap(value: Choice) -> U8 { return match value { Empty => 0u8, Data(payload) => payload, }; }").expect("match");
    let renamed=compile("fn unwrap(input: Choice) -> U8 { return match input { Empty => 0u8, Data(item) => item, }; }").expect("renamed match");
    assert_eq!(
        encode_core_module(&first).unwrap(),
        encode_core_module(&renamed).unwrap()
    );
}
#[test]
fn variant_matches_reject_invalid_coverage_payloads_and_results() {
    for expression in [
        "match value { Empty => 0u8 }",
        "match value { Empty => 0u8, Empty => 1u8, Data(p) => p }",
        "match value { Empty => 0u8, Missing => 1u8, Data(p) => p }",
        "match value { Empty(p) => p, Data(p) => p }",
        "match value { Empty => 0u8, Data(p) => true }",
        "match 0u8 { Empty => 0u8, Data(p) => p }",
    ] {
        let errors = compile(&format!(
            "fn unwrap(value: Choice) -> U8 {{ return {expression}; }}"
        ))
        .expect_err("invalid match");
        assert!(
            errors.iter().any(|e| matches!(
                e.kind,
                CompilerErrorKind::TypeMismatch | CompilerErrorKind::UnknownField
            )),
            "{expression}: {errors:?}"
        );
    }
}
#[test]
fn variant_match_payloads_do_not_escape_their_arm() {
    let errors=compile("fn unwrap(value: Choice) -> U8 { return match value { Data(payload) => payload, Empty => payload, }; }").expect_err("payload is out of scope");
    assert!(
        errors
            .iter()
            .any(|e| e.kind == CompilerErrorKind::UnresolvedType),
        "{errors:?}"
    );
}

fn operation() -> CoreModule {
    let source = "package choices.matching@1; type Choice = variant { Empty, Data(U8) }; fn unwrap(value: Choice) -> U8 { return match value { Empty => 0u8, Data(payload) => payload, }; } type Input = { value: U8, }; type Output = { value: U8, }; intent run(input: Input) returns Output profile p.read basis none budget <= p.small { let value = unwrap(Choice::Data(input.value)); return { value }; }";
    let context = CompilerContext::new()
        .with_operation_profile("p.read", "continuum.profile.read-only/v1")
        .with_budget(
            "p.small",
            amblicit_syntax::CoreBudget {
                max_steps: 65_536,
                max_allocated_bytes: 1_048_576,
                max_output_bytes: 65_536,
            },
        );
    compile_to_core(&parse_module(source).unwrap(), &context).expect("matched operation")
}
fn facts() -> amblicit_syntax::TargetIrLoweringFacts {
    amblicit_syntax::TargetIrLoweringFacts {
        target_profile: amblicit_syntax::ResourceRef {
            coordinate: "echo.dpo@1".into(),
            digest: Some(format!("sha256:{}", "1".repeat(64))),
        },
        target_ir_domain: "echo.span-ir/v1".into(),
        operation_profiles: vec!["continuum.profile.read-only/v1".into()],
        obstruction_coordinates: vec![],
        effect_lowerings: vec![],
        effect_signatures: vec![],
        pure_functions: vec![],
    }
}
#[test]
fn target_independently_validates_exhaustive_match_arms() {
    let original = operation();
    let report = amblicit_syntax::lower_to_target_ir(&original, &facts());
    assert_eq!(
        report.status,
        amblicit_syntax::TargetLoweringStatus::Lowered,
        "{report:?}"
    );
    assert!(report.artifact.is_some());
    for mutation in 0..5 {
        let mut core = original.clone();
        let amblicit_syntax::CoreExpr::Match { arms, .. } =
            &mut core.functions.get_mut("unwrap").unwrap().body.result
        else {
            panic!("match")
        };
        match mutation {
            0 => {
                arms.pop();
            }
            1 => {
                arms[1].case = "Missing".into();
            }
            2 => {
                arms[1].binder.as_mut().unwrap().ty = "Bool".into();
            }
            3 => {
                arms[1].body =
                    amblicit_syntax::CoreExpr::Const(amblicit_syntax::CoreValue::Bool(true));
            }
            4 => {
                arms[0].body = arms[1].body.clone();
            }
            _ => unreachable!(),
        }
        let report = amblicit_syntax::lower_to_target_ir(&core, &facts());
        assert_eq!(
            report.status,
            amblicit_syntax::TargetLoweringStatus::Unsupported,
            "mutation {mutation}: {report:?}"
        );
        assert!(report.artifact.is_none());
        assert!(
            report.failures.iter().any(|failure| failure.kind
                == amblicit_syntax::TargetLoweringFailureKind::InvalidCoreIdentity),
            "mutation {mutation}: {report:?}"
        );
    }
}

#[test]
fn nested_matches_keep_outer_payloads_and_boolean_predicates_scoped() {
    compile("fn nested(value: Choice, other: Choice) -> U8 { return match value { Empty => 0u8, Data(outer) => match other { Empty => outer, Data(inner) => if match value { Empty => false, Data(ignored) => true } then inner else outer, }, }; }").expect("nested lexical frames and Boolean matching");
    compile(
        "fn ignored(value: Choice) -> U8 { return match value { Empty => 0u8, Data => 1u8, }; }",
    )
    .expect("payload binding is optional");
}

#[test]
fn match_arms_cannot_hide_recursive_calls() {
    let errors=compile("fn looped(value: Choice) -> U8 { return match value { Empty => 0u8, Data(payload) => looped(value), }; }").expect_err("cycle through an arm");
    assert!(
        errors
            .iter()
            .any(|e| e.kind == CompilerErrorKind::UnsupportedSourceShape),
        "{errors:?}"
    );
}

#[test]
fn direct_matches_charge_dispatch_without_a_helper_declaration() {
    let cases = (0..128)
        .map(|i| format!("Case{i}"))
        .collect::<Vec<_>>()
        .join(",");
    let arms = (0..128)
        .map(|i| format!("Case{i} => 0u8"))
        .collect::<Vec<_>>()
        .join(",");
    let source=format!("package choices.matching@1; type Choice = variant {{ {cases} }}; type Input = {{ value: U8, }}; type Output = {{ value: U8, }}; intent run(input: Input) returns Output profile p.read basis none budget <= p.small {{ let result = match Choice::Case0 {{ {arms} }}; return {{ value: result }}; }}");
    let context = CompilerContext::new()
        .with_operation_profile("p.read", "continuum.profile.read-only/v1")
        .with_budget(
            "p.small",
            amblicit_syntax::CoreBudget {
                max_steps: 64,
                max_allocated_bytes: 1_048_576,
                max_output_bytes: 65_536,
            },
        );
    let control = source.replace(&format!("match Choice::Case0 {{ {arms} }}"), "input.value");
    compile_to_core(&parse_module(&control).unwrap(), &context)
        .expect("same frame without dispatch fits");
    let errors = compile_to_core(&parse_module(&source).unwrap(), &context)
        .expect_err("128-case dispatch cannot fit 64 steps");
    assert!(
        errors
            .iter()
            .any(|e| e.kind == CompilerErrorKind::InvalidBound),
        "{errors:?}"
    );
}

#[test]
fn target_checks_fields_of_record_valued_matches() {
    use amblicit_syntax::CoreExpr;
    let mut core = operation();
    let result = &mut core.functions.get_mut("unwrap").unwrap().body.result;
    let mut matched = result.clone();
    let CoreExpr::Match { arms, .. } = &mut matched else {
        panic!("match")
    };
    for arm in arms {
        arm.body = CoreExpr::Record {
            fields: std::collections::BTreeMap::from([("value".into(), arm.body.clone())]),
        };
    }
    *result = CoreExpr::Field {
        base: Box::new(matched),
        field: "value".into(),
    };
    let report = amblicit_syntax::lower_to_target_ir(&core, &facts());
    assert_eq!(
        report.status,
        amblicit_syntax::TargetLoweringStatus::Lowered,
        "{report:?}"
    );
    assert!(report.artifact.is_some());
}

#[test]
fn structured_target_encodes_integer_match_comparisons() {
    use amblicit_syntax::{
        CompareOp, CoreExpr, CoreMatchArm, CorePredicate, CoreValue, InputConstraint,
        InputConstraintSource,
    };
    let mut core = operation();
    let integer = |value: &str| {
        CoreExpr::Const(CoreValue::Int {
            width: "U8".into(),
            value: value.into(),
        })
    };
    let selected = CoreExpr::Match {
        scrutinee: Box::new(CoreExpr::Variant {
            ty: "choices.matching@1.Choice".into(),
            case: "Empty".into(),
            payload: None,
        }),
        arms: vec![
            CoreMatchArm {
                case: "Empty".into(),
                binder: None,
                body: integer("1"),
            },
            CoreMatchArm {
                case: "Data".into(),
                binder: None,
                body: integer("2"),
            },
        ],
    };
    core.intents
        .get_mut("run")
        .unwrap()
        .input_constraints
        .push(InputConstraint {
            coordinate: "where.0".into(),
            source: InputConstraintSource::Where,
            predicate: CorePredicate::Compare {
                op: CompareOp::Gt,
                left: selected,
                right: integer("0"),
            },
        });
    let mut facts = facts();
    facts.target_ir_domain = "echo.span-ir/v3".into();
    let report = amblicit_syntax::lower_to_target_ir(&core, &facts);
    assert_eq!(
        report.status,
        amblicit_syntax::TargetLoweringStatus::Lowered,
        "{report:?}"
    );
    amblicit_syntax::encode_target_ir_artifact(&report.artifact.unwrap())
        .expect("integer comparison remains encodable");
}

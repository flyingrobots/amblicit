//! Raw byte lookup requires strict bounds on the exact evaluated operand.
use amblicit_syntax::{
    compile_to_core, digest_core_module, lower_to_target_ir, parse_module, CompilerContext,
    CompilerErrorKind, CoreBudget, CoreExpr, CoreModule, CoreNode, CorePredicate, CoreValue,
    ResourceRef, TargetIrLoweringFacts, TargetLoweringStatus,
};

fn context() -> CompilerContext {
    CompilerContext::new()
        .with_operation_profile("p.read", "continuum.profile.read-only/v1")
        .with_budget(
            "p.small",
            CoreBudget {
                max_steps: 4096,
                max_allocated_bytes: 65536,
                max_output_bytes: 65536,
            },
        )
}

fn source(ty: &str, guard: &str, expression: &str, output: &str) -> String {
    format!(
        "package byte.lookup@1;
        type Input = {{ bytes: {ty}, other: {ty}, index: U64, otherIndex: U64, }};
        type Output = {{ value: {output}, }};
        intent read(input: Input) returns Output
          profile p.read basis none budget <= p.small {guard} {{
          let value: {output} = {expression}; return {{ value }};
        }}"
    )
}

fn compile(ty: &str, guard: &str, expression: &str) -> CoreModule {
    compile_to_core(
        &parse_module(&source(ty, guard, expression, "U8")).unwrap(),
        &context(),
    )
    .expect("proven lookup compiles")
}

fn facts() -> TargetIrLoweringFacts {
    TargetIrLoweringFacts {
        target_profile: ResourceRef {
            coordinate: "echo.dpo@1".into(),
            digest: Some(format!("sha256:{}", "1".repeat(64))),
        },
        target_ir_domain: "echo.span-ir/v1".into(),
        operation_profiles: vec!["continuum.profile.read-only/v1".into()],
        obstruction_coordinates: Vec::new(),
        effect_lowerings: Vec::new(),
        effect_signatures: Vec::new(),
        pure_functions: Vec::new(),
    }
}

const GUARD: &str = "where input.index < len(input.bytes)";
const LOOKUP: &str = "at(input.bytes, input.index)";

#[test]
fn byte_at_preserves_exact_operand_and_u8_result() {
    for guard in [GUARD, "where len(input.bytes) > input.index"] {
        let core = compile("Bytes<max=32>", guard, LOOKUP);
        let CoreNode::Let {
            binding,
            value:
                CoreExpr::Call {
                    callee,
                    type_args,
                    args,
                },
        } = &core.intents["read"].body.nodes[0]
        else {
            panic!("lookup binding")
        };
        assert_eq!(binding.ty, "U8");
        assert_eq!(callee, "core.bytes.at");
        assert_eq!(type_args, &["Bytes<max=32>"]);
        assert!(
            matches!(args.as_slice(), [CoreExpr::Field { field: a, .. }, CoreExpr::Field { field: b, .. }] if a == "bytes" && b == "index")
        );
        assert_eq!(
            lower_to_target_ir(&core, &facts()).status,
            TargetLoweringStatus::Lowered
        );
        let other = compile(
            "Bytes<max=32>",
            "where input.index < len(input.other)",
            "at(input.other, input.index)",
        );
        assert_ne!(
            digest_core_module(&core).unwrap(),
            digest_core_module(&other).unwrap()
        );
    }
}

#[test]
fn byte_at_rejects_unsafe_domains_and_signatures() {
    for (ty, guard, expr, output, kind) in [
        (
            "Bytes<max=32>",
            "",
            LOOKUP,
            "U8",
            CompilerErrorKind::UnsupportedSourceShape,
        ),
        (
            "Bytes<max=32>",
            "where input.index <= len(input.bytes)",
            LOOKUP,
            "U8",
            CompilerErrorKind::UnsupportedSourceShape,
        ),
        (
            "Bytes<max=32>",
            "where input.index < len(input.other)",
            LOOKUP,
            "U8",
            CompilerErrorKind::UnsupportedSourceShape,
        ),
        (
            "Bytes<max=32>",
            "where input.otherIndex < len(input.bytes)",
            LOOKUP,
            "U8",
            CompilerErrorKind::UnsupportedSourceShape,
        ),
        (
            "Bytes<max=32>",
            "where input.index < len(input.bytes) || input.index == 0u64",
            LOOKUP,
            "U8",
            CompilerErrorKind::UnsupportedSourceShape,
        ),
        (
            "Bytes<max=32>",
            GUARD,
            "at(input.bytes, 0u32)",
            "U8",
            CompilerErrorKind::TypeMismatch,
        ),
        (
            "Bytes<max=32>",
            GUARD,
            "at(input.bytes, -1i64)",
            "U8",
            CompilerErrorKind::TypeMismatch,
        ),
        (
            "Bytes<max=32>",
            GUARD,
            "at(input.bytes)",
            "U8",
            CompilerErrorKind::UnsupportedSourceShape,
        ),
        (
            "Bytes<max=32>",
            GUARD,
            "at<U8>(input.bytes, input.index)",
            "U8",
            CompilerErrorKind::UnsupportedSourceShape,
        ),
        (
            "Bytes<max=32>",
            GUARD,
            LOOKUP,
            "U64",
            CompilerErrorKind::TypeMismatch,
        ),
        ("U64", "", LOOKUP, "U8", CompilerErrorKind::TypeMismatch),
        (
            "String<max=32>",
            "",
            LOOKUP,
            "U8",
            CompilerErrorKind::TypeMismatch,
        ),
    ] {
        let errors = compile_to_core(
            &parse_module(&source(ty, guard, expr, output)).unwrap(),
            &context(),
        )
        .unwrap_err();
        assert!(
            errors.iter().any(|e| e.kind == kind),
            "{ty} {guard} {expr}: {errors:?}"
        );
    }
}

fn helper(expression: &str) -> String {
    source("Bytes<max=32>", "", "lookup(input.bytes, input.index)", "U8")
        .replace("type Input", &format!("fn lookup(bytes: Bytes<max=32>, index: U64) -> U8 {{ return {expression}; }} type Input"))
}

#[test]
fn byte_at_helper_guard_is_scoped_to_its_arm() {
    for expr in [
        "if index < len(bytes) then at(bytes, index) else 0u8",
        "if len(bytes) > index then at(bytes, index) else 0u8",
        "if index >= len(bytes) then 0u8 else at(bytes, index)",
        "if index < len(bytes) && index < 10u64 then at(bytes, index) else 0u8",
    ] {
        let core = compile_to_core(&parse_module(&helper(expr)).unwrap(), &context()).unwrap();
        let report = lower_to_target_ir(&core, &facts());
        assert_eq!(report.status, TargetLoweringStatus::Lowered, "{report:?}");
    }
    for expr in [
        "if index <= len(bytes) then at(bytes, index) else 0u8",
        "if index < len(bytes) then 0u8 else at(bytes, index)",
        "if index < len(bytes) then at(bytes, index) else at(bytes, index)",
    ] {
        let errors =
            compile_to_core(&parse_module(&helper(expr)).unwrap(), &context()).unwrap_err();
        assert!(
            errors
                .iter()
                .any(|e| e.kind == CompilerErrorKind::UnsupportedSourceShape),
            "{expr}: {errors:?}"
        );
    }
}

#[test]
fn target_rechecks_byte_at_signature_and_strict_totality() {
    let original = compile("Bytes<max=32>", GUARD, LOOKUP);
    for case in 0..7 {
        let mut core = original.clone();
        let intent = core.intents.get_mut("read").unwrap();
        match case {
            0 => intent.input_constraints.clear(),
            1 => intent.input_constraints[0].predicate = CorePredicate::True,
            2 => {
                let CorePredicate::Compare { op, .. } = &mut intent.input_constraints[0].predicate
                else {
                    panic!("comparison")
                };
                *op = amblicit_syntax::CompareOp::Le;
            }
            _ => {
                let CoreNode::Let {
                    binding,
                    value:
                        CoreExpr::Call {
                            type_args, args, ..
                        },
                } = &mut intent.body.nodes[0]
                else {
                    panic!("call")
                };
                match case {
                    3 => binding.ty = "U64".into(),
                    4 => {
                        let CoreExpr::Field { field, .. } = &mut args[0] else {
                            panic!("field")
                        };
                        *field = "other".into();
                    }
                    5 => {
                        args[1] = CoreExpr::Const(CoreValue::Int {
                            width: "U32".into(),
                            value: "0".into(),
                        });
                    }
                    6 => type_args.push("U8".into()),
                    _ => unreachable!(),
                }
            }
        }
        let report = lower_to_target_ir(&core, &facts());
        assert_eq!(
            report.status,
            TargetLoweringStatus::Unsupported,
            "case {case}: {report:?}"
        );
        assert!(report.artifact.is_none());
    }
}

#[test]
fn byte_at_uses_minimum_length_never_maximum_as_presence() {
    for (ty, expr) in [
        ("Bytes<exact=32>", "at(input.bytes, 31u64)"),
        ("Bytes<max=32>", "at(b\"\\x00\\xff\", 1u64)"),
    ] {
        let core = compile(ty, "", expr);
        let target = lower_to_target_ir(&core, &facts());
        assert_eq!(target.status, TargetLoweringStatus::Lowered, "{target:?}");
    }
    for (ty, expr) in [
        ("Bytes<exact=32>", "at(input.bytes, 32u64)"),
        ("Bytes<max=32>", "at(input.bytes, 0u64)"),
        ("Bytes<exact=0>", "at(input.bytes, 0u64)"),
    ] {
        let errors = compile_to_core(
            &parse_module(&source(ty, "", expr, "U8")).unwrap(),
            &context(),
        )
        .unwrap_err();
        assert!(
            errors
                .iter()
                .any(|e| e.kind == CompilerErrorKind::UnsupportedSourceShape),
            "{errors:?}"
        );
    }
}

#[test]
fn target_rechecks_conditional_helper_dominance() {
    let text = helper("if index < len(bytes) then at(bytes, index) else 0u8");
    let original = compile_to_core(&parse_module(&text).unwrap(), &context()).unwrap();
    for case in 0..3 {
        let mut core = original.clone();
        let function = core.functions.get_mut("lookup").unwrap();
        let CoreExpr::If {
            predicate,
            then_value,
            else_value,
        } = &mut function.body.result
        else {
            panic!("conditional")
        };
        match case {
            0 => **predicate = CorePredicate::True,
            1 => std::mem::swap(then_value, else_value),
            2 => {
                let CorePredicate::Compare { op, .. } = predicate.as_mut() else {
                    panic!("compare")
                };
                *op = amblicit_syntax::CompareOp::Le;
            }
            _ => unreachable!(),
        }
        let report = lower_to_target_ir(&core, &facts());
        assert_eq!(
            report.status,
            TargetLoweringStatus::Unsupported,
            "case {case}: {report:?}"
        );
        assert!(report.artifact.is_none());
    }
}

#[test]
fn byte_at_prelude_cannot_be_shadowed() {
    let text = source("Bytes<max=32>", GUARD, LOOKUP, "U8")
        .replace("let value:", "let at:")
        .replace("return { value }", "return { value: at }");
    let errors = compile_to_core(&parse_module(&text).unwrap(), &context()).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|e| e.kind == CompilerErrorKind::SurfaceValidation),
        "{errors:?}"
    );
}

#[test]
fn byte_at_short_circuit_guards_cover_only_evaluated_operands() {
    for expression in [
        "if index < len(bytes) && at(bytes, index) < 128u8 then 1u8 else 0u8",
        "if index >= len(bytes) || at(bytes, index) < 128u8 then 1u8 else 0u8",
        "if index >= len(bytes) || at(bytes, index) < 128u8 || at(bytes, index) >= 192u8 then 1u8 else 0u8",
    ] {
        let core = compile_to_core(&parse_module(&helper(expression)).unwrap(), &context()).unwrap();
        let target = lower_to_target_ir(&core, &facts());
        assert_eq!(target.status, TargetLoweringStatus::Lowered, "{target:?}");
    }
    for expression in [
        "if index < len(bytes) || at(bytes, index) < 128u8 then 1u8 else 0u8",
        "if index >= len(bytes) && at(bytes, index) < 128u8 then 1u8 else 0u8",
        "if at(bytes, index) < 128u8 || index >= len(bytes) then 1u8 else 0u8",
    ] {
        let errors =
            compile_to_core(&parse_module(&helper(expression)).unwrap(), &context()).unwrap_err();
        assert!(
            errors
                .iter()
                .any(|e| e.kind == CompilerErrorKind::UnsupportedSourceShape),
            "{expression}: {errors:?}"
        );
    }
}

fn conditional_partial_operation(output: &str, guard: &str, operation: &str, fallback: &str) {
    let source = |then_value: &str, else_value: &str| {
        format!(
            "package proof.scope@1;
             type Input = {{ bytes: Bytes<max=32>, start: U64, end: U64,
                             left: U64, right: U64, }};
             intent evaluate(input: Input) returns {output}
               profile p.read basis none budget <= p.small {{
               let value: {output} = if {guard} then {then_value} else {else_value};
               return value;
             }}"
        )
    };
    let core = compile_to_core(
        &parse_module(&source(operation, fallback)).unwrap(),
        &context(),
    )
    .expect("the positive conditional arm supplies the proof");
    let report = lower_to_target_ir(&core, &facts());
    assert_eq!(report.status, TargetLoweringStatus::Lowered, "{report:?}");

    let errors = compile_to_core(
        &parse_module(&source(fallback, operation)).unwrap(),
        &context(),
    )
    .unwrap_err();
    assert!(errors
        .iter()
        .any(|error| error.kind == CompilerErrorKind::UnsupportedSourceShape));

    let mut forged = core;
    let CoreNode::Let {
        value:
            CoreExpr::If {
                then_value,
                else_value,
                ..
            },
        ..
    } = &mut forged.intents.get_mut("evaluate").unwrap().body.nodes[0]
    else {
        panic!("conditional binding")
    };
    std::mem::swap(then_value, else_value);
    let report = lower_to_target_ir(&forged, &facts());
    assert_eq!(
        report.status,
        TargetLoweringStatus::Unsupported,
        "{report:?}"
    );
    assert!(report.artifact.is_none());
}

#[test]
fn conditional_proof_covers_unsigned_subtraction_without_leaking() {
    conditional_partial_operation(
        "U64",
        "input.left >= input.right",
        "input.left - input.right",
        "0u64",
    );
}

#[test]
fn conditional_proof_covers_unsigned_addition_without_leaking() {
    conditional_partial_operation(
        "U64",
        "input.left <= 10u64 && input.right <= 20u64",
        "input.left + input.right",
        "0u64",
    );
}

#[test]
fn conditional_proof_covers_byte_slice_without_leaking() {
    conditional_partial_operation(
        "Bytes<max=32>",
        "input.start <= input.end && input.end <= len(input.bytes)",
        "slice(input.bytes, input.start, input.end)",
        "b\"\"",
    );
}

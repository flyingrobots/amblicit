//! A half-open byte slice is total only within its proven input domain.
use amblicit_syntax::{
    compile_to_core, digest_core_module, lower_to_target_ir, parse_module, CompilerContext,
    CompilerErrorKind, CoreBudget, CoreExpr, CoreModule, CoreNode, CorePredicate, CoreValue,
    ResourceRef, TargetIrLoweringFacts, TargetLoweringFailureKind, TargetLoweringStatus,
};

const GUARD: &str = "where input.start <= input.end && input.end <= len(input.bytes)";
const SLICE: &str = "slice(input.bytes, input.start, input.end)";

fn context() -> CompilerContext {
    CompilerContext::new()
        .with_operation_profile("p.read", "continuum.profile.read-only/v1")
        .with_budget(
            "p.small",
            CoreBudget {
                max_steps: 64,
                max_allocated_bytes: 4096,
                max_output_bytes: 2 * 1024 * 1024,
            },
        )
}

fn source(ty: &str, guard: &str, expression: &str, output: &str) -> String {
    format!(
        "package bytes.example@1;
        type Input = {{ bytes: {ty}, other: {ty}, start: U64, end: U64, }};
        type Output = {{ fragment: {output}, }};
        intent cut(input: Input) returns Output
          profile p.read basis none budget <= p.small {guard} {{
          let fragment: {output} = {expression};
          return {{ fragment }};
        }}"
    )
}

fn compile(ty: &str, guard: &str, expression: &str, output: &str) -> CoreModule {
    compile_to_core(
        &parse_module(&source(ty, guard, expression, output)).unwrap(),
        &context(),
    )
    .expect("proven slice compiles")
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

#[test]
fn byte_slice_preserves_operand_identity_and_weakens_length_bound() {
    for (ty, output) in [
        ("Bytes<max=1048576>", "Bytes<max=1048576>"),
        ("Bytes<exact=32>", "Bytes<max=32>"),
        ("Bytes<exact=0>", "Bytes<exact=0>"),
    ] {
        let core = compile(ty, GUARD, SLICE, output);
        let CoreNode::Let {
            binding,
            value:
                CoreExpr::Call {
                    callee,
                    type_args,
                    args,
                },
        } = &core.intents["cut"].body.nodes[0]
        else {
            panic!("slice binding")
        };
        assert_eq!(binding.ty, output);
        assert_eq!(callee, "core.bytes.slice");
        assert_eq!(type_args, &[ty]);
        assert_eq!(args.len(), 3);
        for (arg, field_name) in args.iter().zip(["bytes", "start", "end"]) {
            assert!(matches!(arg, CoreExpr::Field { field, .. } if field == field_name));
        }
        let report = lower_to_target_ir(&core, &facts());
        assert_eq!(report.status, TargetLoweringStatus::Lowered, "{report:?}");
        assert_eq!(
            report.artifact.unwrap().intents["cut"].pure_bindings[0].value,
            match &core.intents["cut"].body.nodes[0] {
                CoreNode::Let { value, .. } => value.clone(),
                _ => unreachable!(),
            }
        );
        assert!(report.result_projections.contains_key("cut"));
    }
}

#[test]
fn byte_slice_accepts_full_and_empty_ranges_without_assumptions() {
    for expression in [
        "slice(input.bytes, 0, len(input.bytes))",
        "slice(input.bytes, 0, 0)",
        "slice(input.bytes, len(input.bytes), len(input.bytes))",
    ] {
        let core = compile("Bytes<max=32>", "", expression, "Bytes<max=32>");
        let report = lower_to_target_ir(&core, &facts());
        assert_eq!(report.status, TargetLoweringStatus::Lowered, "{report:?}");
    }
}

#[test]
fn byte_slice_accepts_equivalent_exact_order_evidence() {
    for guard in [
        "where input.end >= input.start, len(input.bytes) >= input.end",
        "where input.start < input.end, input.end < len(input.bytes)",
        "where input.start == input.end, input.end == len(input.bytes)",
        "where input.end == input.start, len(input.bytes) == input.end",
        "where true && input.start <= input.end && input.end <= len(input.bytes)",
    ] {
        let core = compile("Bytes<max=32>", guard, SLICE, "Bytes<max=32>");
        let report = lower_to_target_ir(&core, &facts());
        assert_eq!(report.status, TargetLoweringStatus::Lowered, "{report:?}");
    }
}

fn assert_source_rejected(
    ty: &str,
    guard: &str,
    expression: &str,
    output: &str,
    expected: CompilerErrorKind,
) {
    let result = compile_to_core(
        &parse_module(&source(ty, guard, expression, output)).unwrap(),
        &context(),
    );
    assert!(
        result
            .unwrap_err()
            .iter()
            .any(|error| error.kind == expected),
        "{ty}: {guard} {expression}"
    );
}

#[test]
fn byte_slice_rejects_unproven_ranges() {
    for guard in [
        "",
        "where input.start <= input.end",
        "where input.end <= len(input.bytes)",
        "where input.start <= input.end || input.end <= len(input.bytes)",
        "where !(input.start > input.end), input.end <= len(input.bytes)",
        "where input.start <= input.end, input.end <= len(input.other)",
    ] {
        assert_source_rejected(
            "Bytes<max=32>",
            guard,
            SLICE,
            "Bytes<max=32>",
            CompilerErrorKind::UnsupportedSourceShape,
        );
    }
    assert_source_rejected(
        "Bytes<max=32>",
        GUARD,
        "slice(input.bytes, input.end, input.start)",
        "Bytes<max=32>",
        CompilerErrorKind::UnsupportedSourceShape,
    );
}

#[test]
fn byte_slice_rejects_wrong_call_shapes_and_types() {
    for (ty, guard, expression, output, expected) in [
        (
            "Bytes<exact=32>",
            GUARD,
            SLICE,
            "Bytes<exact=32>",
            CompilerErrorKind::TypeMismatch,
        ),
        (
            "Bytes<max=32>",
            GUARD,
            SLICE,
            "Bytes<max=16>",
            CompilerErrorKind::TypeMismatch,
        ),
        (
            "U64",
            "",
            "slice(input.bytes, 0, 0)",
            "Bytes<max=32>",
            CompilerErrorKind::TypeMismatch,
        ),
        (
            "String<max=32>",
            "",
            "slice(input.bytes, 0, 0)",
            "Bytes<max=32>",
            CompilerErrorKind::TypeMismatch,
        ),
        (
            "Bytes<max=32>",
            GUARD,
            "slice(input.bytes, 0u32, input.end)",
            "Bytes<max=32>",
            CompilerErrorKind::TypeMismatch,
        ),
        (
            "Bytes<max=32>",
            GUARD,
            "slice(input.bytes, 0i64, input.end)",
            "Bytes<max=32>",
            CompilerErrorKind::TypeMismatch,
        ),
        (
            "Bytes<max=32>",
            GUARD,
            "slice(input.bytes, input.start)",
            "Bytes<max=32>",
            CompilerErrorKind::UnsupportedSourceShape,
        ),
        (
            "Bytes<max=32>",
            GUARD,
            "slice<Bytes<max=32>>(input.bytes, input.start, input.end)",
            "Bytes<max=32>",
            CompilerErrorKind::UnsupportedSourceShape,
        ),
    ] {
        assert_source_rejected(ty, guard, expression, output, expected);
    }
}

#[test]
fn byte_slice_type_errors_identify_each_invalid_operand() {
    for (ty, expression, operands) in [
        ("U64", "slice(input.bytes, 0, 0)", vec!["input.bytes"]),
        ("Bytes<max=32>", "slice(input.bytes, 0u32, 0)", vec!["0u32"]),
        ("Bytes<max=32>", "slice(input.bytes, 0, 0i64)", vec!["0i64"]),
        (
            "Bytes<max=32>",
            "slice(input.bytes, 0u32, 1i64)",
            vec!["0u32", "1i64"],
        ),
    ] {
        let text = source(ty, "", expression, "Bytes<max=32>");
        let call_start = text.find(expression).unwrap();
        let call_end = call_start + expression.len();
        let errors = compile_to_core(&parse_module(&text).unwrap(), &context()).unwrap_err();
        let actual = errors
            .iter()
            .filter(|error| {
                error.kind == CompilerErrorKind::TypeMismatch
                    && error.span.start >= call_start
                    && error.span.end <= call_end
            })
            .map(|error| (error.span.start, error.span.end))
            .collect::<Vec<_>>();
        let expected = operands
            .iter()
            .map(|operand| {
                let start = call_start + expression.find(operand).unwrap();
                (start, start + operand.len())
            })
            .collect::<Vec<_>>();
        assert_eq!(actual, expected, "{expression}");
    }
}

#[test]
fn byte_slice_proofs_do_not_leak_into_basis_constraints_or_other_intents() {
    let original = source("Bytes<max=32>", GUARD, SLICE, "Bytes<max=32>");
    compile_to_core(&parse_module(&original).unwrap(), &context()).unwrap();
    let second_intent = original[original.find("intent cut").unwrap()..]
        .replace("intent cut", "intent unguarded")
        .replace(GUARD, "");
    for (invalid, use_last_slice) in [
        (
            original.replace("basis none", &format!("basis {SLICE}")),
            false,
        ),
        (
            original.replace(GUARD, &format!("{GUARD}, len({SLICE}) >= 0u64")),
            false,
        ),
        (
            original.replace(GUARD, &format!("where len({SLICE}) >= 0u64, input.start <= input.end, input.end <= len(input.bytes)")),
            false,
        ),
        (format!("{original}\n{second_intent}"), true),
    ] {
        let start = if use_last_slice {
            invalid.rfind(SLICE)
        } else {
            invalid.find(SLICE)
        }
        .unwrap();
        let errors = compile_to_core(&parse_module(&invalid).unwrap(), &context()).unwrap_err();
        let spans = errors
            .iter()
            .filter(|error| error.kind == CompilerErrorKind::UnsupportedSourceShape)
            .map(|error| (error.span.start, error.span.end))
            .collect::<Vec<_>>();
        assert_eq!(spans, vec![(start, start + SLICE.len())]);
    }
}

#[test]
fn byte_slice_source_name_cannot_be_shadowed() {
    let invalid = source("Bytes<max=32>", GUARD, SLICE, "Bytes<max=32>")
        .replace("let fragment:", "let slice:")
        .replace("return { fragment }", "return { fragment: slice }");
    let errors = compile_to_core(&parse_module(&invalid).unwrap(), &context()).unwrap_err();
    assert!(errors
        .iter()
        .any(|error| error.kind == CompilerErrorKind::SurfaceValidation));
}

#[test]
fn byte_slice_source_mutation_changes_core_identity() {
    let original = compile("Bytes<max=32>", GUARD, SLICE, "Bytes<max=32>");
    for changed in [
        compile(
            "Bytes<max=32>",
            GUARD,
            "slice(input.bytes, 0, input.end)",
            "Bytes<max=32>",
        ),
        compile(
            "Bytes<max=32>",
            GUARD,
            "slice(input.bytes, input.end, input.end)",
            "Bytes<max=32>",
        ),
        compile(
            "Bytes<max=32>",
            &GUARD.replace("input.bytes", "input.other"),
            &SLICE.replace("input.bytes", "input.other"),
            "Bytes<max=32>",
        ),
    ] {
        assert_ne!(
            digest_core_module(&original).unwrap(),
            digest_core_module(&changed).unwrap()
        );
    }
}

#[test]
fn target_rejects_forged_byte_slice_signatures_and_proofs() {
    for case in 0..14 {
        let mut core = compile("Bytes<max=32>", GUARD, SLICE, "Bytes<max=32>");
        let intent = core.intents.get_mut("cut").unwrap();
        let CoreNode::Let {
            binding,
            value: CoreExpr::Call {
                type_args, args, ..
            },
        } = &mut intent.body.nodes[0]
        else {
            panic!("slice binding")
        };
        match case {
            0 => type_args.clear(),
            1 => type_args.push("Bytes<max=32>".into()),
            2 => type_args[0] = "U64".into(),
            3 => type_args[0] = "Bytes<max=64>".into(),
            4 => {
                args.pop();
            }
            5 => args.push(args[0].clone()),
            6 => {
                args[1] = CoreExpr::Const(CoreValue::Int {
                    width: "U32".into(),
                    value: "0".into(),
                });
            }
            7 => binding.ty = "Bytes<exact=32>".into(),
            8 => {
                let CoreExpr::Field { base, .. } = &mut args[0] else {
                    panic!("field")
                };
                let CoreExpr::Local { reference } = base.as_mut() else {
                    panic!("local")
                };
                reference.id = "foreign-input".into();
            }
            9 => intent.input_constraints.clear(),
            10 => {
                let CorePredicate::All(items) = &mut intent.input_constraints[0].predicate else {
                    panic!("all")
                };
                items.pop();
            }
            11 => {
                let predicate = &mut intent.input_constraints[0].predicate;
                let CorePredicate::All(items) = predicate else {
                    panic!("all")
                };
                *predicate = CorePredicate::Any(items.clone());
            }
            12 => {
                intent.basis = Some(CoreExpr::Call {
                    callee: "core.bytes.slice".into(),
                    type_args: type_args.clone(),
                    args: args.clone(),
                });
            }
            13 => {
                let call = CoreExpr::Call {
                    callee: "core.bytes.slice".into(),
                    type_args: type_args.clone(),
                    args: args.clone(),
                };
                intent.input_constraints[0].predicate = CorePredicate::Compare {
                    op: amblicit_syntax::CompareOp::Eq,
                    left: call.clone(),
                    right: call,
                };
            }
            _ => unreachable!(),
        }
        let report = lower_to_target_ir(&core, &facts());
        assert_eq!(
            report.status,
            TargetLoweringStatus::Unsupported,
            "case {case}"
        );
        assert!(report.artifact.is_none());
        assert!(
            report
                .failures
                .iter()
                .any(|failure| failure.kind == TargetLoweringFailureKind::InvalidCoreIdentity),
            "case {case}: {report:?}"
        );
    }
}

#[test]
fn conjunctive_prefix_guards_prove_later_slices_and_unsigned_differences() {
    let predicates = [
        "input.start <= input.end",
        "input.end <= len(input.bytes)",
        "len(slice(input.bytes, input.start, input.end)) <= len(input.bytes)",
        "input.end - input.start <= len(input.bytes)",
    ];
    for guard in [
        format!("where {}", predicates.join(" && ")),
        format!(
            "where {} && ({} && ({} && {}))",
            predicates[0], predicates[1], predicates[2], predicates[3]
        ),
    ] {
        let text = source("Bytes<max=32>", &guard, SLICE, "Bytes<max=32>");
        let core = compile_to_core(&parse_module(&text).unwrap(), &context())
            .expect("a checked left conjunction proves the right expression domain");
        let report = lower_to_target_ir(&core, &facts());
        assert_eq!(report.status, TargetLoweringStatus::Lowered, "{report:?}");
        let mut reordered = core.clone();
        let CorePredicate::All(items) =
            &mut reordered.intents.get_mut("cut").unwrap().input_constraints[0].predicate
        else {
            panic!("conjunction")
        };
        items.swap(0, 1);
        let report = lower_to_target_ir(&reordered, &facts());
        assert_eq!(report.status, TargetLoweringStatus::Unsupported);
        assert!(report.artifact.is_none());
        assert!(report
            .failures
            .iter()
            .any(|failure| failure.kind == TargetLoweringFailureKind::InvalidCoreIdentity));
        assert_ne!(
            digest_core_module(&core).unwrap(),
            digest_core_module(&reordered).unwrap(),
            "conjunction order is semantic"
        );
    }
}

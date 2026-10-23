//! Unsigned sums need a dominating, independently checked no-overflow proof.
use amblicit_syntax::{
    compile_to_core, lower_to_target_ir, parse_module, CompilerContext, CompilerErrorKind,
    CoreBudget, CoreExpr, CoreModule, CoreNode, CorePredicate, ResourceRef, TargetIrLoweringFacts,
    TargetLoweringFailureKind, TargetLoweringStatus,
};

fn compile(
    width: &str,
    guard: &str,
    expression: &str,
) -> Result<CoreModule, Vec<amblicit_syntax::CompilerError>> {
    compile_with_annotation(width, guard, expression, true)
}

fn compile_with_annotation(
    width: &str,
    guard: &str,
    expression: &str,
    annotated: bool,
) -> Result<CoreModule, Vec<amblicit_syntax::CompilerError>> {
    compile_with_input_widths(width, width, guard, expression, annotated)
}

fn compile_with_input_widths(
    width: &str,
    right_width: &str,
    guard: &str,
    expression: &str,
    annotated: bool,
) -> Result<CoreModule, Vec<amblicit_syntax::CompilerError>> {
    let annotation = if annotated {
        format!(": {width}")
    } else {
        String::new()
    };
    let source = format!(
        "package arithmetic.sum@1;
        type Input = {{ left: {width}, right: {right_width}, }};
        type Output = {{ sum: {width}, }};
        intent measure(input: Input) returns Output
          profile p.read basis none budget <= p.small {guard} {{
          let sum{annotation} = {expression}; return {{ sum }};
        }}"
    );
    let context = CompilerContext::new()
        .with_operation_profile("p.read", "continuum.profile.read-only/v1")
        .with_budget(
            "p.small",
            CoreBudget {
                max_steps: 64,
                max_allocated_bytes: 4096,
                max_output_bytes: 1024,
            },
        );
    compile_to_core(
        &parse_module(&source).expect("valid source syntax"),
        &context,
    )
}

#[test]
fn nested_unsigned_addition_infers_its_width_without_an_annotation() {
    for width in ["U32", "U64"] {
        let core = compile_with_annotation(
            width,
            "where input.left <= 10 && input.right <= 20",
            "(input.left + input.right) + 3",
            false,
        )
        .expect("inferred nested bounded sum");
        assert_eq!(
            lower_to_target_ir(&core, &facts()).status,
            TargetLoweringStatus::Lowered
        );
    }
}

#[test]
fn nested_bare_literals_inherit_an_unsigned_peers_width() {
    for width in ["U32", "U64"] {
        for expression in ["(input.left + 1) + 2", "2 + (1 + input.left)"] {
            let core = compile_with_annotation(width, "where input.left <= 10", expression, false)
                .expect("the known operand supplies the nested literals width");
            assert_eq!(
                lower_to_target_ir(&core, &facts()).status,
                TargetLoweringStatus::Lowered
            );
        }
    }
}

#[test]
fn a_typed_right_operand_supplies_the_left_sums_width() {
    for width in ["U32", "U64"] {
        let core = compile_with_annotation(
            width,
            "where input.left <= 10",
            "(1 + 2) + input.left",
            false,
        )
        .expect("a typed right operand supplies the literal sums width");
        assert_eq!(
            lower_to_target_ir(&core, &facts()).status,
            TargetLoweringStatus::Lowered
        );
    }
}

#[test]
fn alternating_unsigned_arithmetic_completes_at_normal_expression_depth() {
    let mut expression = "input.left".to_owned();
    for _ in 0..32 {
        expression = format!("({expression} + 0) - 0");
    }
    let core = compile_with_annotation("U64", "", &expression, false)
        .expect("all alternating operations are total");
    assert!(matches!(
        &core.intents["measure"].body.nodes[0],
        CoreNode::Let { value: CoreExpr::Call { callee, .. }, .. }
            if callee == "core.integer.subtract"
    ));
}

#[test]
fn signed_sums_and_negated_bound_evidence_are_rejected_as_unsupported() {
    for (width, guard) in [
        ("I32", ""),
        ("U64", "where !(input.left > 10) && !(input.right > 20)"),
    ] {
        let errors = compile(width, guard, "input.left + input.right")
            .expect_err("not proved unsigned addition");
        assert!(
            errors
                .iter()
                .any(|e| e.kind == CompilerErrorKind::UnsupportedSourceShape),
            "{errors:?}"
        );
    }
}

#[test]
fn mixed_width_addition_has_a_stable_type_mismatch() {
    for (left, right) in [("U32", "U64"), ("U64", "U32")] {
        let errors = compile_with_input_widths(left, right, "", "input.left + input.right", true)
            .expect_err("mixed widths must refuse");
        assert!(errors
            .iter()
            .any(|e| e.kind == CompilerErrorKind::TypeMismatch));
    }
}

#[test]
fn enclosing_sums_preserve_a_contextual_literal_peers_width() {
    for width in ["U32", "U64"] {
        for expression in ["((1 + 2) + input.left) + 3", "3 + (input.left + (2 + 1))"] {
            let core = compile_with_annotation(width, "where input.left <= 10", expression, false)
                .expect("enclosing sums retain the known unsigned width");
            assert_eq!(
                lower_to_target_ir(&core, &facts()).status,
                TargetLoweringStatus::Lowered
            );
        }
    }
}

fn facts() -> TargetIrLoweringFacts {
    TargetIrLoweringFacts {
        target_profile: ResourceRef {
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
fn proven_unsigned_addition_preserves_operands_and_lowers_independently() {
    for width in ["U32", "U64"] {
        let core = compile(
            width,
            "where input.left <= 10 && input.right <= 20",
            "input.left + input.right",
        )
        .expect("the sum is bounded above by 30");
        let CoreNode::Let {
            binding,
            value:
                CoreExpr::Call {
                    callee,
                    type_args,
                    args,
                },
        } = &core.intents["measure"].body.nodes[0]
        else {
            panic!("arithmetic call")
        };
        assert_eq!(binding.ty, width);
        assert_eq!(callee, "core.integer.add");
        assert_eq!(type_args, &[width]);
        assert!(
            matches!(&args[..], [CoreExpr::Field {field: a, ..}, CoreExpr::Field {field: b, ..}] if a == "left" && b == "right")
        );
        let report = lower_to_target_ir(&core, &facts());
        assert_eq!(report.status, TargetLoweringStatus::Lowered);
        assert!(report.artifact.is_some());
    }
}

#[test]
fn unsigned_addition_accepts_the_width_boundary_and_zero_identity() {
    for (width, max) in [("U32", "4294967295"), ("U64", "18446744073709551615")] {
        for expression in [
            format!("{max} + 0"),
            "0 + input.left".into(),
            "input.left + 0".into(),
        ] {
            let core = compile(width, "", &expression).expect("total sum");
            assert_eq!(
                lower_to_target_ir(&core, &facts()).status,
                TargetLoweringStatus::Lowered
            );
        }
    }
}

#[test]
fn unsigned_addition_uses_upper_bounds_with_both_comparison_directions() {
    for width in ["U32", "U64"] {
        for guard in [
            "where input.left < 11 && input.right < 21",
            "where 10 >= input.left && 20 >= input.right",
            "where 11 > input.left && 21 > input.right",
            "where input.left == 10 && 20 == input.right",
        ] {
            let core = compile(width, guard, "input.left + input.right").unwrap();
            assert_eq!(
                lower_to_target_ir(&core, &facts()).status,
                TargetLoweringStatus::Lowered
            );
        }
        let ceiling = if width == "U32" {
            "4294967294"
        } else {
            "18446744073709551614"
        };
        let core = compile(
            width,
            &format!("where input.left <= {ceiling} && input.right <= 1"),
            "input.left + input.right",
        )
        .unwrap();
        assert_eq!(
            lower_to_target_ir(&core, &facts()).status,
            TargetLoweringStatus::Lowered
        );
        let core = compile(
            width,
            "where input.left <= 10 && input.right <= 20",
            "(input.left + input.right) + 3",
        )
        .unwrap();
        assert_eq!(
            lower_to_target_ir(&core, &facts()).status,
            TargetLoweringStatus::Lowered
        );
    }
}

#[test]
fn target_refuses_forged_addition_width_arity_and_overflowing_constants() {
    let core = compile(
        "U64",
        "where input.left <= 10 && input.right <= 20",
        "input.left + input.right",
    )
    .unwrap();
    for case in 0..4 {
        let mut forged = core.clone();
        let CoreNode::Let {
            value: CoreExpr::Call {
                type_args, args, ..
            },
            ..
        } = &mut forged.intents.get_mut("measure").unwrap().body.nodes[0]
        else {
            panic!("sum")
        };
        match case {
            0 => {
                type_args[0] = "I64".into();
            }
            1 => {
                args.pop();
            }
            2 => {
                args.push(args[0].clone());
            }
            _ => {
                *args = vec![
                    CoreExpr::Const(amblicit_syntax::CoreValue::Int {
                        width: "U64".into(),
                        value: u64::MAX.to_string(),
                    }),
                    CoreExpr::Const(amblicit_syntax::CoreValue::Int {
                        width: "U64".into(),
                        value: "1".into(),
                    }),
                ];
            }
        }
        let report = lower_to_target_ir(&forged, &facts());
        assert_eq!(report.status, TargetLoweringStatus::Unsupported);
        assert!(report.artifact.is_none());
        assert!(report
            .failures
            .iter()
            .any(|failure| failure.kind == TargetLoweringFailureKind::InvalidCoreIdentity));
    }
}

#[test]
fn unsigned_addition_refuses_possible_overflow_and_disjunctive_evidence() {
    for (width, max) in [("U32", "4294967295"), ("U64", "18446744073709551615")] {
        for (guard, expression) in [
            ("", "input.left + input.right".to_owned()),
            ("", format!("{max} + 1")),
            (
                "where input.left <= 10 || input.right <= 20",
                "input.left + input.right".into(),
            ),
        ] {
            let errors = compile(width, guard, &expression).expect_err("no totality proof");
            assert!(errors
                .iter()
                .any(|error| error.kind == CompilerErrorKind::UnsupportedSourceShape));
        }
    }
}

#[test]
fn target_rejects_unsigned_addition_when_core_proof_is_removed_or_weakened() {
    let core = compile(
        "U64",
        "where input.left <= 10 && input.right <= 20",
        "input.left + input.right",
    )
    .unwrap();
    for disjunctive in [false, true] {
        let mut forged = core.clone();
        let constraints = &mut forged.intents.get_mut("measure").unwrap().input_constraints;
        if disjunctive {
            constraints[0].predicate =
                CorePredicate::Any(vec![constraints[0].predicate.clone(), CorePredicate::True]);
        } else {
            constraints.clear();
        }
        let report = lower_to_target_ir(&forged, &facts());
        assert_eq!(report.status, TargetLoweringStatus::Unsupported);
        assert!(report.artifact.is_none());
        assert!(report
            .failures
            .iter()
            .any(|failure| failure.kind == TargetLoweringFailureKind::InvalidCoreIdentity));
    }
}

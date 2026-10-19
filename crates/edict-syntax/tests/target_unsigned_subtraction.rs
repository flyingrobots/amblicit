//! The target boundary must validate totality without trusting source compilation.
use edict_syntax::{
    compile_to_core, lower_to_target_ir, parse_module, CompilerContext, CoreBudget, CoreExpr,
    CoreModule, CoreNode, CorePredicate, ResourceRef, TargetIrLoweringFacts,
    TargetLoweringFailureKind, TargetLoweringStatus,
};

fn core() -> CoreModule {
    core_width("U64")
}

fn core_width(width: &str) -> CoreModule {
    let source = "package arithmetic.target@1;
        type Input = { lower: U64, upper: U64, };
        type Output = { distance: U64, };
        intent measure(input: Input) returns Output
          profile p.read basis none budget <= p.small
          where input.lower <= input.upper {
          let distance: U64 = input.upper - input.lower;
          return { distance };
        }";
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
        &parse_module(&source.replace("U64", width)).unwrap(),
        &context,
    )
    .unwrap()
}

fn facts() -> TargetIrLoweringFacts {
    TargetIrLoweringFacts {
        target_profile: ResourceRef {
            coordinate: "echo.dpo@1".to_owned(),
            digest: Some(format!("sha256:{}", "1".repeat(64))),
        },
        target_ir_domain: "echo.span-ir/v1".to_owned(),
        operation_profiles: vec!["continuum.profile.read-only/v1".to_owned()],
        obstruction_coordinates: Vec::new(),
        effect_lowerings: Vec::new(),
        effect_signatures: Vec::new(),
        pure_functions: Vec::new(),
    }
}

#[test]
fn target_lowers_proven_unsigned_difference_without_changing_operands() {
    let core = core();
    let report = lower_to_target_ir(&core, &facts());
    assert_eq!(
        report.status,
        TargetLoweringStatus::Lowered,
        "{:?}",
        report.failures
    );
    let artifact = report.artifact.unwrap();
    let binding = &artifact.intents["measure"].pure_bindings[0];
    let CoreNode::Let { value, .. } = &core.intents["measure"].body.nodes[0] else {
        panic!("difference binding");
    };
    assert_eq!(&binding.value, value);
    assert!(report.result_projections.contains_key("measure"));
}

#[test]
fn target_rejects_unsigned_difference_when_core_guard_is_stripped() {
    let mut core = core();
    core.intents
        .get_mut("measure")
        .unwrap()
        .input_constraints
        .clear();
    rejects(&core);
}

#[test]
fn target_rejects_unsigned_difference_when_core_operands_are_reversed() {
    let mut core = core();
    let CoreNode::Let {
        value: CoreExpr::Call { args, .. },
        ..
    } = &mut core.intents.get_mut("measure").unwrap().body.nodes[0]
    else {
        panic!("difference call");
    };
    args.swap(0, 1);
    rejects(&core);
}

#[test]
fn target_rejects_unsigned_difference_with_disjunctive_evidence() {
    let mut core = core();
    let predicate = &mut core.intents.get_mut("measure").unwrap().input_constraints[0].predicate;
    *predicate = CorePredicate::Any(vec![predicate.clone(), CorePredicate::True]);
    rejects(&core);
}

fn rejects(core: &CoreModule) {
    let report = lower_to_target_ir(core, &facts());
    assert_eq!(report.status, TargetLoweringStatus::Unsupported);
    assert!(report.artifact.is_none());
    assert!(report
        .failures
        .iter()
        .any(|failure| failure.kind == TargetLoweringFailureKind::InvalidCoreIdentity));
}

#[test]
fn target_rejects_malformed_subtraction_signatures_and_literals() {
    for mutation in 0..5 {
        let mut core = core();
        let CoreNode::Let {
            value: CoreExpr::Call {
                type_args, args, ..
            },
            ..
        } = &mut core.intents.get_mut("measure").unwrap().body.nodes[0]
        else {
            panic!("difference call");
        };
        match mutation {
            0 => type_args[0] = "U32".to_owned(),
            1 => {
                args.pop();
            }
            2 => args.push(args[0].clone()),
            3 => {
                let invalid = CoreExpr::Const(edict_syntax::CoreValue::Int {
                    width: "U64".to_owned(),
                    value: "18446744073709551616".to_owned(),
                });
                *args = vec![invalid.clone(), invalid];
            }
            4 => {
                type_args[0] = "I64".to_owned();
                let signed = CoreExpr::Const(edict_syntax::CoreValue::Int {
                    width: "I64".to_owned(),
                    value: "1".to_owned(),
                });
                *args = vec![signed.clone(), signed];
            }
            _ => unreachable!(),
        }
        rejects(&core);
    }
}

fn difference(core: &CoreModule) -> CoreExpr {
    let CoreNode::Let { value, .. } = &core.intents["measure"].body.nodes[0] else {
        panic!("difference binding");
    };
    value.clone()
}

#[test]
fn target_does_not_use_body_evidence_to_authorize_metadata_arithmetic() {
    let mut basis = core();
    basis.intents.get_mut("measure").unwrap().basis = Some(difference(&core()));
    rejects(&basis);
    let mut constraint = core();
    constraint
        .intents
        .get_mut("measure")
        .unwrap()
        .input_constraints
        .push(edict_syntax::InputConstraint {
            coordinate: "where.1".to_owned(),
            source: edict_syntax::InputConstraintSource::Where,
            predicate: CorePredicate::Compare {
                op: edict_syntax::CompareOp::Ge,
                left: difference(&core()),
                right: CoreExpr::Const(edict_syntax::CoreValue::Int {
                    width: "U64".to_owned(),
                    value: "0".to_owned(),
                }),
            },
        });
    rejects(&constraint);
}

#[test]
fn target_checks_nested_result_arithmetic_and_isolates_intents() {
    let mut result = core();
    let mut value = result.intents["measure"].clone();
    let computed = difference(&result);
    let CoreExpr::Record { fields } = &mut value.body.result else {
        panic!("record result")
    };
    fields.insert("distance".to_owned(), computed);
    value.input_constraints.clear();
    value.body.nodes.clear();
    value.body.locals.truncate(1);
    result.intents.insert("unguarded".to_owned(), value);
    let report = lower_to_target_ir(&result, &facts());
    assert_eq!(report.status, TargetLoweringStatus::Unsupported);
    assert!(report.artifact.is_none());
    assert_eq!(report.failures.len(), 1);
    assert_eq!(
        report.failures[0].kind,
        TargetLoweringFailureKind::InvalidCoreIdentity
    );
    assert_eq!(report.failures[0].intent.as_deref(), Some("unguarded"));
}

#[test]
fn target_accepts_both_unsigned_widths_and_unconditional_constant_bounds() {
    for width in ["U32", "U64"] {
        let guarded = core_width(width);
        assert_eq!(
            lower_to_target_ir(&guarded, &facts()).status,
            TargetLoweringStatus::Lowered
        );
        for (left, right) in [("9", "4"), ("0", "0")] {
            let mut core = core_width(width);
            let intent = core.intents.get_mut("measure").unwrap();
            intent.input_constraints.clear();
            let CoreNode::Let {
                value: CoreExpr::Call { args, .. },
                ..
            } = &mut intent.body.nodes[0]
            else {
                panic!("difference call");
            };
            *args = [left, right]
                .iter()
                .map(|value| {
                    CoreExpr::Const(edict_syntax::CoreValue::Int {
                        width: width.to_owned(),
                        value: (*value).to_owned(),
                    })
                })
                .collect();
            assert_eq!(
                lower_to_target_ir(&core, &facts()).status,
                TargetLoweringStatus::Lowered
            );
        }
    }
}

#[test]
fn target_rejects_negated_evidence_and_underflowing_literals() {
    let mut negated = core();
    let predicate = &mut negated
        .intents
        .get_mut("measure")
        .unwrap()
        .input_constraints[0]
        .predicate;
    *predicate = CorePredicate::Not(Box::new(predicate.clone()));
    rejects(&negated);
    let mut constants = core();
    let CoreNode::Let {
        value: CoreExpr::Call { args, .. },
        ..
    } = &mut constants.intents.get_mut("measure").unwrap().body.nodes[0]
    else {
        panic!("difference call");
    };
    *args = ["4", "9"]
        .iter()
        .map(|value| {
            CoreExpr::Const(edict_syntax::CoreValue::Int {
                width: "U64".to_owned(),
                value: (*value).to_owned(),
            })
        })
        .collect();
    rejects(&constants);
}

#[test]
fn target_checks_nested_operands_without_repeated_validation() {
    let wrappers: [fn(CoreExpr) -> CoreExpr; 3] = [
        std::convert::identity,
        |value| CoreExpr::If {
            predicate: Box::new(CorePredicate::True),
            then_value: Box::new(value),
            else_value: Box::new(CoreExpr::Const(edict_syntax::CoreValue::Int {
                width: "U64".to_owned(),
                value: "9".to_owned(),
            })),
        },
        |value| CoreExpr::Field {
            base: Box::new(CoreExpr::Record {
                fields: std::collections::BTreeMap::from([("value".to_owned(), value)]),
            }),
            field: "value".to_owned(),
        },
    ];
    for wrap in wrappers {
        for literal in ["9", "18446744073709551616", "-1", "invalid"] {
            let mut core = core();
            let mut expression = CoreExpr::Const(edict_syntax::CoreValue::Int {
                width: "U64".to_owned(),
                value: literal.to_owned(),
            });
            for _ in 0..8 {
                expression = wrap(CoreExpr::Call {
                    callee: "core.integer.subtract".to_owned(),
                    type_args: vec!["U64".to_owned()],
                    args: vec![
                        expression,
                        CoreExpr::Const(edict_syntax::CoreValue::Int {
                            width: "U64".to_owned(),
                            value: "0".to_owned(),
                        }),
                    ],
                });
            }
            let CoreNode::Let { value, .. } =
                &mut core.intents.get_mut("measure").unwrap().body.nodes[0]
            else {
                panic!("difference binding");
            };
            *value = expression;
            if literal == "9" {
                let report = lower_to_target_ir(&core, &facts());
                assert_eq!(report.status, TargetLoweringStatus::Lowered, "{report:?}");
                assert!(report.artifact.is_some());
            } else {
                rejects(&core);
            }
        }
    }
}

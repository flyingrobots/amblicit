//! Unsigned difference is total only after its input-domain proof.
use amblicit_syntax::{
    compile_to_core, digest_core_module, parse_module, CompilerContext, CoreBudget, CoreExpr,
    CoreModule, CoreNode, CoreValue,
};

fn context() -> CompilerContext {
    CompilerContext::new()
        .with_operation_profile("p.read", "continuum.profile.read-only/v1")
        .with_budget(
            "p.small",
            CoreBudget {
                max_steps: 64,
                max_allocated_bytes: 4096,
                max_output_bytes: 1024,
            },
        )
}

fn source(width: &str, guard: &str, expression: &str) -> String {
    format!(
        "package arithmetic.example@1;
         type Input = {{ lower: {width}, upper: {width}, other: {width}, }};
         type Output = {{ distance: {width}, }};
         intent measure(input: Input) returns Output
           profile p.read basis none budget <= p.small {guard} {{
           let distance: {width} = {expression};
           return {{ distance }};
         }}"
    )
}

fn compile(width: &str, guard: &str, expression: &str) -> CoreModule {
    let module = parse_module(&source(width, guard, expression)).expect("valid source");
    compile_to_core(&module, &context()).expect("proven difference must compile")
}

#[test]
fn guarded_unsigned_difference_preserves_width_order_and_guard() {
    for width in ["U32", "U64"] {
        for guard in [
            "input.lower <= input.upper",
            "input.lower < input.upper",
            "input.upper >= input.lower",
            "input.upper > input.lower",
            "input.lower == input.upper",
            "input.upper == input.lower",
            "true && input.lower <= input.upper",
        ] {
            let core = compile(
                width,
                &format!("where {guard}"),
                "input.upper - input.lower",
            );
            let intent = &core.intents["measure"];
            assert_eq!(intent.input_constraints.len(), 1);
            let CoreNode::Let { binding, value } = &intent.body.nodes[0] else {
                panic!("authored let must remain a binding")
            };
            assert_eq!(binding.ty, width);
            let CoreExpr::Call {
                callee,
                type_args,
                args,
            } = value
            else {
                panic!("difference must lower to the generic prelude operation")
            };
            assert_eq!(callee, "core.integer.subtract");
            assert_eq!(type_args, &[width]);
            assert_eq!(args.len(), 2);
            for (argument, expected) in args.iter().zip(["upper", "lower"]) {
                assert!(matches!(argument, CoreExpr::Field { field, .. } if field == expected));
            }
        }
    }
}

#[test]
fn unsigned_difference_accepts_self_zero_and_ordered_constants() {
    for width in ["U32", "U64"] {
        for expression in ["input.upper - input.upper", "input.upper - 0", "9 - 4"] {
            let core = compile(width, "", expression);
            let CoreNode::Let {
                binding,
                value: CoreExpr::Call { args, .. },
            } = &core.intents["measure"].body.nodes[0]
            else {
                panic!("total arithmetic binding")
            };
            assert_eq!(binding.ty, width);
            if expression == "9 - 4" {
                assert_eq!(
                    args,
                    &[
                        CoreExpr::Const(CoreValue::Int {
                            width: width.into(),
                            value: "9".into()
                        }),
                        CoreExpr::Const(CoreValue::Int {
                            width: width.into(),
                            value: "4".into()
                        }),
                    ]
                );
            }
        }
    }
}

#[test]
fn unsigned_difference_identity_tracks_semantics() {
    let original = compile(
        "U64",
        "where input.lower <= input.upper",
        "input.upper - input.lower",
    );
    let original_digest = digest_core_module(&original).unwrap();
    for changed in [
        compile(
            "U64",
            "where input.lower <= input.upper",
            "input.upper - input.upper",
        ),
        compile(
            "U64",
            "where input.lower < input.upper",
            "input.upper - input.lower",
        ),
        compile(
            "U32",
            "where input.lower <= input.upper",
            "input.upper - input.lower",
        ),
    ] {
        assert_ne!(original_digest, digest_core_module(&changed).unwrap());
    }
}

fn rejects(text: &str, kind: amblicit_syntax::CompilerErrorKind) {
    let module = parse_module(text).expect("valid rejection fixture");
    let errors = compile_to_core(&module, &context()).expect_err("unsafe difference must reject");
    assert!(
        errors.iter().any(
            |error| error.stage == amblicit_syntax::CompilerStage::TypeCheck && error.kind == kind
        ),
        "{errors:?}"
    );
}

#[test]
fn unsigned_difference_rejects_unproven_or_wrong_evidence() {
    use amblicit_syntax::CompilerErrorKind::UnsupportedSourceShape;
    for guard in [
        "",
        "where input.upper <= input.lower",
        "where input.lower <= input.other",
        "where input.lower <= input.upper || true",
    ] {
        rejects(
            &source("U64", guard, "input.upper - input.lower"),
            UnsupportedSourceShape,
        );
    }
    rejects(&source("U64", "", "4 - 9"), UnsupportedSourceShape);
    rejects(
        &source(
            "I64",
            "where input.lower <= input.upper",
            "input.upper - input.lower",
        ),
        UnsupportedSourceShape,
    );
    rejects(
        &source("U64", "", "9u64 - 4u32"),
        amblicit_syntax::CompilerErrorKind::TypeMismatch,
    );
}

#[test]
fn unsigned_difference_proof_does_not_leak_between_intents() {
    let first = source(
        "U64",
        "where input.lower <= input.upper",
        "input.upper - input.lower",
    );
    let second = source("U64", "", "input.upper - input.lower");
    let second = second[second.find("intent measure").unwrap()..]
        .replace("intent measure", "intent unguarded");
    compile_to_core(&parse_module(&first).unwrap(), &context()).expect("guarded control compiles");
    let unguarded_start = first.len();
    let combined = first + &second;
    let errors = compile_to_core(&parse_module(&combined).unwrap(), &context()).unwrap_err();
    let unsupported = errors
        .iter()
        .filter(|error| {
            error.stage == amblicit_syntax::CompilerStage::TypeCheck
                && error.kind == amblicit_syntax::CompilerErrorKind::UnsupportedSourceShape
        })
        .collect::<Vec<_>>();
    assert_eq!(unsupported.len(), 1);
    assert!(unsupported[0].span.start >= unguarded_start);
    assert!(unsupported[0].span.end <= combined.len());
}

#[test]
fn unsigned_difference_cannot_use_guards_to_evaluate_basis_or_guards() {
    let text = source(
        "U64",
        "where input.lower <= input.upper",
        "input.upper - input.lower",
    );
    rejects(
        &text.replace("basis none", "basis input.upper - input.lower"),
        amblicit_syntax::CompilerErrorKind::UnsupportedSourceShape,
    );
    let text = source(
        "U64",
        "where input.upper - input.lower >= 0u64 && input.lower <= input.upper",
        "input.upper - input.lower",
    );
    rejects(
        &text,
        amblicit_syntax::CompilerErrorKind::UnsupportedSourceShape,
    );
}

#[test]
fn boolean_connectives_preserve_predicate_structure_in_core() {
    for (guard, conjunction) in [
        ("where input.lower <= input.upper && true", true),
        ("where input.lower <= input.upper || false", false),
    ] {
        let core = compile("U64", guard, "input.upper - input.upper");
        let predicate = &core.intents["measure"].input_constraints[0].predicate;
        let items = match predicate {
            amblicit_syntax::CorePredicate::All(items) if conjunction => items,
            amblicit_syntax::CorePredicate::Any(items) if !conjunction => items,
            _ => panic!("logical connective must retain its Core meaning"),
        };
        assert_eq!(items.len(), 2);
        assert_eq!(
            items[0],
            amblicit_syntax::CorePredicate::Compare {
                op: amblicit_syntax::CompareOp::Le,
                left: CoreExpr::Field {
                    base: Box::new(CoreExpr::Local {
                        reference: core.intents["measure"].body.locals[0].clone(),
                    }),
                    field: "lower".into(),
                },
                right: CoreExpr::Field {
                    base: Box::new(CoreExpr::Local {
                        reference: core.intents["measure"].body.locals[0].clone(),
                    }),
                    field: "upper".into(),
                },
            }
        );
        assert_eq!(
            items[1],
            if conjunction {
                amblicit_syntax::CorePredicate::True
            } else {
                amblicit_syntax::CorePredicate::False
            }
        );
    }
}

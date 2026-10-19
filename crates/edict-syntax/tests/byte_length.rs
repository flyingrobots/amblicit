//! Bounded byte length is a prelude operation, not an imported application helper.
use edict_syntax::{
    compile_to_core, digest_core_module, lower_to_target_ir, parse_module, CompilerContext,
    CompilerErrorKind, CoreBudget, CoreExpr, CoreModule, CoreNode, CoreValue, ResourceRef,
    TargetIrLoweringFacts, TargetLoweringFailureKind, TargetLoweringStatus,
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

fn source(ty: &str, expression: &str, output: &str) -> String {
    format!(
        "package length.example@1;
        type Input = {{ bytes: {ty}, other: {ty}, }};
        type Output = {{ length: {output}, }};
        intent measure(input: Input) returns Output
          profile p.read basis none budget <= p.small {{
          let length: {output} = {expression};
          return {{ length }};
        }}"
    )
}

fn compile(ty: &str, expression: &str) -> CoreModule {
    compile_to_core(
        &parse_module(&source(ty, expression, "U64")).unwrap(),
        &context(),
    )
    .unwrap()
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
fn bounded_byte_length_preserves_operand_type_and_u64_result() {
    for ty in ["Bytes<max=1048576>", "Bytes<exact=32>", "Bytes<exact=0>"] {
        let core = compile(ty, "len(input.bytes)");
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
            panic!("length binding")
        };
        assert_eq!(binding.ty, "U64");
        assert_eq!(callee, "core.bytes.length");
        assert_eq!(type_args, &[ty]);
        assert!(matches!(args.as_slice(), [CoreExpr::Field { field, .. }] if field == "bytes"));
        assert_ne!(
            digest_core_module(&core).unwrap(),
            digest_core_module(&compile(ty, "len(input.other)")).unwrap()
        );
    }
}

#[test]
fn byte_length_rejects_wrong_source_operands_and_call_shapes() {
    for (ty, expression, output, expected) in [
        (
            "U64",
            "len(input.bytes)",
            "U64",
            CompilerErrorKind::TypeMismatch,
        ),
        (
            "String<max=32>",
            "len(input.bytes)",
            "U64",
            CompilerErrorKind::TypeMismatch,
        ),
        (
            "Bytes<max=32>",
            "len()",
            "U64",
            CompilerErrorKind::UnsupportedSourceShape,
        ),
        (
            "Bytes<max=32>",
            "len(input.bytes, input.other)",
            "U64",
            CompilerErrorKind::UnsupportedSourceShape,
        ),
        (
            "Bytes<max=32>",
            "len<U64>(input.bytes)",
            "U64",
            CompilerErrorKind::UnsupportedSourceShape,
        ),
        (
            "Bytes<max=32>",
            "len(input.bytes)",
            "U32",
            CompilerErrorKind::TypeMismatch,
        ),
    ] {
        let result = compile_to_core(
            &parse_module(&source(ty, expression, output)).unwrap(),
            &context(),
        );
        assert!(
            result
                .unwrap_err()
                .iter()
                .any(|error| error.kind == expected),
            "{ty} {expression}"
        );
    }
}

#[test]
fn target_independently_accepts_byte_length_and_preserves_projection() {
    for ty in ["Bytes<max=1048576>", "Bytes<exact=32>", "Bytes<exact=0>"] {
        let core = compile(ty, "len(input.bytes)");
        let report = lower_to_target_ir(&core, &facts());
        assert_eq!(
            report.status,
            TargetLoweringStatus::Lowered,
            "{:?}",
            report.failures
        );
        let CoreNode::Let { value, .. } = &core.intents["measure"].body.nodes[0] else {
            panic!("binding")
        };
        assert_eq!(
            &report.artifact.unwrap().intents["measure"].pure_bindings[0].value,
            value
        );
        assert!(report.result_projections.contains_key("measure"));
    }
}

#[test]
fn target_rejects_forged_byte_length_signatures_and_authority() {
    for case in 0..9 {
        let mut core = compile("Bytes<max=32>", "len(input.bytes)");
        let CoreNode::Let {
            binding,
            value: CoreExpr::Call {
                type_args, args, ..
            },
        } = &mut core.intents.get_mut("measure").unwrap().body.nodes[0]
        else {
            panic!("binding")
        };
        match case {
            0 => type_args.clear(),
            1 => type_args.push("Bytes<max=32>".into()),
            2 => type_args[0] = "U64".into(),
            3 => type_args[0] = "Bytes<max=64>".into(),
            4 => args.clear(),
            5 => {
                args[0] = CoreExpr::Const(CoreValue::Int {
                    width: "U64".into(),
                    value: "32".into(),
                });
            }
            6 => binding.ty = "U32".into(),
            7 => args.push(args[0].clone()),
            8 => {
                let CoreExpr::Field { base, .. } = &mut args[0] else {
                    panic!("field")
                };
                let CoreExpr::Local { reference } = base.as_mut() else {
                    panic!("local")
                };
                reference.id = "unavailable-input".into();
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
        assert!(report
            .failures
            .iter()
            .any(|failure| failure.kind == TargetLoweringFailureKind::InvalidCoreIdentity));
    }
}

#[test]
fn target_checks_byte_length_authority_inside_nested_predicates() {
    for dangling in [false, true] {
        let mut core = compile("Bytes<max=32>", "len(input.bytes)");
        let CoreNode::Let { value, .. } =
            &mut core.intents.get_mut("measure").unwrap().body.nodes[0]
        else {
            panic!("length binding");
        };
        let CoreExpr::Call { args, .. } = value else {
            panic!("length call");
        };
        let operand = args[0].clone();
        if dangling {
            let CoreExpr::Field { base, .. } = &mut args[0] else {
                panic!("byte operand field");
            };
            let CoreExpr::Local { reference } = base.as_mut() else {
                panic!("byte operand local");
            };
            reference.id = "unavailable-inner-input".into();
        }
        for _ in 1..8 {
            *value = CoreExpr::Call {
                callee: "core.bytes.length".into(),
                type_args: vec!["Bytes<max=32>".into()],
                args: vec![CoreExpr::If {
                    predicate: Box::new(edict_syntax::CorePredicate::Compare {
                        op: edict_syntax::CompareOp::Ge,
                        left: value.clone(),
                        right: CoreExpr::Const(CoreValue::Int {
                            width: "U64".into(),
                            value: "0".into(),
                        }),
                    }),
                    then_value: Box::new(operand.clone()),
                    else_value: Box::new(operand.clone()),
                }],
            };
        }
        let report = lower_to_target_ir(&core, &facts());
        if dangling {
            assert_eq!(report.status, TargetLoweringStatus::Unsupported);
            assert!(report.artifact.is_none());
            assert!(report
                .failures
                .iter()
                .any(|failure| { failure.kind == TargetLoweringFailureKind::InvalidCoreIdentity }));
        } else {
            assert_eq!(report.status, TargetLoweringStatus::Lowered, "{report:?}");
            assert!(report.artifact.is_some());
        }
    }
}

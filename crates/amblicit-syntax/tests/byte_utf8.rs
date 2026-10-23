//! UTF-8 validation is a generic bounded byte predicate.
use amblicit_syntax::{
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
                max_steps: 4_194_304,
                max_allocated_bytes: 4096,
                max_output_bytes: 1024,
            },
        )
}

fn source(ty: &str, expression: &str, output: &str) -> String {
    format!(
        "package utf8.example@1;
        type Input = {{ bytes: {ty}, other: {ty}, }};
        type Output = {{ valid: {output}, }};
        intent measure(input: Input) returns Output
          profile p.read basis none budget <= p.small {{
          let valid: {output} = {expression};
          return {{ valid }};
        }}"
    )
}

fn compile(ty: &str, expression: &str) -> CoreModule {
    compile_to_core(
        &parse_module(&source(ty, expression, "Bool")).unwrap(),
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
fn byte_utf8_call_retains_operand_identity_and_boolean_projection() {
    for ty in ["Bytes<max=1048576>", "Bytes<exact=32>", "Bytes<exact=0>"] {
        let core = compile(ty, "isUtf8(input.bytes)");
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
            panic!("UTF8 binding")
        };
        assert_eq!(binding.ty, "Bool");
        assert_eq!(callee, "core.bytes.isUtf8");
        assert_eq!(type_args, &[ty]);
        assert!(matches!(args.as_slice(),[CoreExpr::Field{field,..}] if field=="bytes"));
        assert_ne!(
            digest_core_module(&core).unwrap(),
            digest_core_module(&compile(ty, "isUtf8(input.other)")).unwrap()
        );
        let report = lower_to_target_ir(&core, &facts());
        assert_eq!(report.status, TargetLoweringStatus::Lowered, "{report:?}");
        assert_eq!(
            report.artifact.unwrap().intents["measure"].pure_bindings[0].value,
            *match &core.intents["measure"].body.nodes[0] {
                CoreNode::Let { value, .. } => value,
                _ => unreachable!(),
            }
        );
        assert!(report.result_projections.contains_key("measure"));
    }
}
#[test]
fn byte_utf8_rejects_wrong_operands_signatures_and_result_type() {
    for (ty, expression, output, expected) in [
        (
            "U64",
            "isUtf8(input.bytes)",
            "Bool",
            CompilerErrorKind::TypeMismatch,
        ),
        (
            "String<max=32>",
            "isUtf8(input.bytes)",
            "Bool",
            CompilerErrorKind::TypeMismatch,
        ),
        (
            "Bytes<max=32>",
            "isUtf8()",
            "Bool",
            CompilerErrorKind::UnsupportedSourceShape,
        ),
        (
            "Bytes<max=32>",
            "isUtf8(input.bytes,input.other)",
            "Bool",
            CompilerErrorKind::UnsupportedSourceShape,
        ),
        (
            "Bytes<max=32>",
            "isUtf8<U64>(input.bytes)",
            "Bool",
            CompilerErrorKind::UnsupportedSourceShape,
        ),
        (
            "Bytes<max=32>",
            "isUtf8(input.bytes)",
            "U64",
            CompilerErrorKind::TypeMismatch,
        ),
    ] {
        let errors = compile_to_core(
            &parse_module(&source(ty, expression, output)).unwrap(),
            &context(),
        )
        .unwrap_err();
        assert!(
            errors.iter().any(|e| e.kind == expected),
            "{ty} {expression}: {errors:?}"
        );
    }
}
#[test]
fn byte_utf8_target_refuses_forged_signature_or_operand_authority() {
    for case in 0..9 {
        let mut core = compile("Bytes<max=32>", "isUtf8(input.bytes)");
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
            5 => args[0] = CoreExpr::Const(CoreValue::Bool(true)),
            6 => binding.ty = "U64".into(),
            7 => args.push(args[0].clone()),
            8 => {
                let CoreExpr::Field { base, .. } = &mut args[0] else {
                    panic!("field")
                };
                let CoreExpr::Local { reference } = base.as_mut() else {
                    panic!("local")
                };
                reference.id = "foreign-input".into();
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
            .any(|f| f.kind == TargetLoweringFailureKind::InvalidCoreIdentity));
    }
}
#[test]
fn byte_utf8_predicate_accepts_proven_prefix_and_rejects_unproven_slice() {
    for (guard, accepted) in [
        (
            "input.start <= len(input.bytes) && isUtf8(slice(input.bytes,0u64,input.start))",
            true,
        ),
        (
            "isUtf8(slice(input.bytes,0u64,input.start)) && input.start <= len(input.bytes)",
            false,
        ),
    ] {
        let text=format!("package utf8.guards@1; type Input={{bytes:Bytes<max=32>,start:U64,}}; intent measure(input:Input) returns Bool profile p.read basis none budget <= p.small where {guard} {{let valid = isUtf8(input.bytes); return valid;}}");
        let result = compile_to_core(&parse_module(&text).unwrap(), &context());
        if accepted {
            let report = lower_to_target_ir(&result.unwrap(), &facts());
            assert_eq!(report.status, TargetLoweringStatus::Lowered, "{report:?}");
        } else {
            assert!(result
                .unwrap_err()
                .iter()
                .any(|e| e.kind == CompilerErrorKind::UnsupportedSourceShape));
        }
    }
}

#[test]
fn byte_utf8_prelude_name_cannot_be_shadowed() {
    let text = source("Bytes<max=32>", "isUtf8(input.bytes)", "Bool")
        .replace("let valid:", "let isUtf8:")
        .replace("return { valid }", "return { valid: isUtf8 }");
    let errors = compile_to_core(&parse_module(&text).unwrap(), &context()).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|e| e.kind == CompilerErrorKind::SurfaceValidation),
        "{errors:?}"
    );
}
#[test]
fn byte_utf8_source_function_budget_includes_bounded_scan() {
    let text="package utf8.cost@1; fn inspect(value:Bytes<max=1024>) -> Bool {return isUtf8(value);} type Input={bytes:Bytes<max=1024>,}; intent measure(input:Input) returns Bool profile p.read basis none budget <= p.small {let valid = inspect(input.bytes); return valid;}";
    let parsed = parse_module(text).unwrap();
    let budget_for = |steps| {
        CompilerContext::new()
            .with_operation_profile("p.read", "continuum.profile.read-only/v1")
            .with_budget(
                "p.small",
                CoreBudget {
                    max_steps: steps,
                    max_allocated_bytes: u64::MAX,
                    max_output_bytes: u64::MAX,
                },
            )
    };
    let errors = compile_to_core(&parsed, &budget_for(100)).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|e| e.kind == CompilerErrorKind::InvalidBound),
        "{errors:?}"
    );
    let generous = budget_for(4096);
    let core = compile_to_core(&parsed, &generous).unwrap();
    let report = lower_to_target_ir(&core, &facts());
    assert_eq!(report.status, TargetLoweringStatus::Lowered, "{report:?}");
}

fn scan_budget(steps: u64) -> CompilerContext {
    CompilerContext::new()
        .with_operation_profile("p.read", "continuum.profile.read-only/v1")
        .with_budget(
            "p.small",
            CoreBudget {
                max_steps: steps,
                max_allocated_bytes: u64::MAX,
                max_output_bytes: u64::MAX,
            },
        )
}
#[test]
fn direct_utf8_scans_are_bounded_in_bodies_guards_and_lazy_branches() {
    let simple = source("Bytes<max=1024>", "isUtf8(input.bytes)", "Bool");
    let conditional = source(
        "Bytes<max=1024>",
        "if input.flag then isUtf8(input.bytes) else isUtf8(input.other)",
        "Bool",
    )
    .replace(
        "other: Bytes<max=1024>,",
        "other: Bytes<max=1024>, flag: Bool,",
    );
    let guarded = simple
        .replace(
            "budget <= p.small {",
            "budget <= p.small where isUtf8(input.bytes) {",
        )
        .replace(
            "let valid: Bool = isUtf8(input.bytes);",
            "let valid: Bool = true;",
        );
    for text in [&simple, &conditional, &guarded] {
        let module = parse_module(text).unwrap();
        let errors = compile_to_core(&module, &scan_budget(100)).unwrap_err();
        assert!(
            errors
                .iter()
                .any(|e| e.kind == CompilerErrorKind::InvalidBound),
            "{errors:?}"
        );
        compile_to_core(&module, &scan_budget(1600))
            .expect("one complete scan or one selected conditional branch fits");
    }
    let repeated = simple.replace(
        "return { valid };",
        "let second = isUtf8(input.other); return { valid };",
    );
    let errors =
        compile_to_core(&parse_module(&repeated).unwrap(), &scan_budget(1600)).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|e| e.kind == CompilerErrorKind::InvalidBound),
        "{errors:?}"
    );
}
#[test]
fn utf8_helper_scan_is_not_charged_twice_at_its_call_site() {
    let text="package utf8.cost@1; fn inspect(value:Bytes<max=1024>) -> Bool {return isUtf8(value);} type Input={bytes:Bytes<max=1024>,}; intent measure(input:Input) returns Bool profile p.read basis none budget <= p.small {let valid = inspect(input.bytes); return valid;}";
    compile_to_core(&parse_module(text).unwrap(), &scan_budget(1600))
        .expect("single helper scan fits; duplicate scan accounting would exceed this ceiling");
}

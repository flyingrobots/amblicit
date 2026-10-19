//! Generic bounded raw-byte concatenation, independent of application semantics.
use edict_syntax::{
    compile_to_core, digest_core_module, lower_to_target_ir, parse_module,
    validate_core_module_type_integrity, CompilerContext, CompilerErrorKind, CoreBudget, CoreExpr,
    CoreModule, CoreNode, CoreType, CoreValue, LocalRef, ResourceRef, TargetIrLoweringFacts,
    TargetLoweringFailureKind, TargetLoweringStatus,
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

fn source(left: &str, right: &str, output: &str, expression: &str) -> String {
    format!(
        "package concat.example@1;
        type Input = {{ left: {left}, right: {right}, }};
        type Output = {{ bytes: {output}, }};
        intent concatenate(input: Input) returns Output
          profile p.read basis none budget <= p.small {{
          let bytes = {expression};
          return {{ bytes }};
        }}"
    )
}

fn compile(left: &str, right: &str, output: &str, expression: &str) -> CoreModule {
    compile_to_core(
        &parse_module(&source(left, right, output, expression)).unwrap(),
        &context(),
    )
    .expect("bounded byte concatenation compiles")
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

fn call(core: &mut CoreModule) -> (&mut LocalRef, &mut Vec<String>, &mut Vec<CoreExpr>) {
    let CoreNode::Let {
        binding,
        value: CoreExpr::Call {
            type_args, args, ..
        },
    } = &mut core.intents.get_mut("concatenate").unwrap().body.nodes[0]
    else {
        panic!("concatenation binding")
    };
    (binding, type_args, args)
}

#[test]
fn bounded_byte_concat_preserves_order_coordinates_and_summed_maximum() {
    for (left, right, max) in [
        ("Bytes<max=8>", "Bytes<max=8>", 16_u64),
        ("Bytes<max=3>", "Bytes<max=5>", 8),
        ("Bytes<exact=2>", "Bytes<exact=3>", 5),
        ("Bytes<exact=0>", "Bytes<max=8>", 8),
        ("Bytes<exact=0>", "Bytes<exact=0>", 0),
        (
            "Bytes<max=18446744073709551615>",
            "Bytes<exact=0>",
            u64::MAX,
        ),
    ] {
        let output = format!("Bytes<max={max}>");
        let mut core = compile(left, right, &output, "input.left + input.right");
        let (binding, type_args, args) = call(&mut core);
        assert_eq!(binding.ty, output);
        assert_eq!(type_args, &[left, right]);
        assert_eq!(args.len(), 2);
        for (argument, expected) in args.iter().zip(["left", "right"]) {
            assert!(matches!(argument, CoreExpr::Field { field, .. } if field == expected));
        }
        let CoreNode::Let { value, .. } = &core.intents["concatenate"].body.nodes[0] else {
            unreachable!()
        };
        assert!(matches!(value, CoreExpr::Call { callee, .. } if callee == "core.bytes.concat"));
        let report = lower_to_target_ir(&core, &facts());
        assert_eq!(report.status, TargetLoweringStatus::Lowered, "{report:?}");
        assert_eq!(
            report.artifact.unwrap().intents["concatenate"].pure_bindings[0].value,
            *value
        );
    }
}

#[test]
fn byte_concat_order_and_nesting_are_explicit_in_core_identity() {
    let forward = compile(
        "Bytes<max=3>",
        "Bytes<max=5>",
        "Bytes<max=8>",
        "input.left + input.right",
    );
    let reverse = compile(
        "Bytes<max=3>",
        "Bytes<max=5>",
        "Bytes<max=8>",
        "input.right + input.left",
    );
    assert_ne!(
        digest_core_module(&forward).unwrap(),
        digest_core_module(&reverse).unwrap()
    );
    let nested = compile(
        "Bytes<max=3>",
        "Bytes<max=5>",
        "Bytes<max=11>",
        "input.left + (input.right + input.left)",
    );
    assert_eq!(
        lower_to_target_ir(&nested, &facts()).status,
        TargetLoweringStatus::Lowered
    );
}

fn assert_source_rejected(left: &str, right: &str, output: &str, kind: CompilerErrorKind) {
    let text = source(left, right, output, "input.left + input.right");
    let errors = compile_to_core(&parse_module(&text).unwrap(), &context()).unwrap_err();
    assert!(errors.iter().any(|error| error.kind == kind), "{errors:?}");
}

#[test]
fn byte_concat_rejects_overflowing_static_sum_without_wrapping() {
    assert_source_rejected(
        "Bytes<max=18446744073709551615>",
        "Bytes<max=1>",
        "Bytes<max=18446744073709551615>",
        CompilerErrorKind::InvalidBound,
    );
}

#[test]
fn byte_concat_rejects_narrow_destinations_and_wrong_operand_families() {
    for (left, right, output) in [
        ("Bytes<max=8>", "Bytes<max=8>", "Bytes<max=15>"),
        ("Bytes<exact=2>", "Bytes<exact=3>", "Bytes<exact=5>"),
        ("Bytes<max=8>", "U64", "Bytes<max=8>"),
        ("U64", "Bytes<max=8>", "Bytes<max=8>"),
        ("Bytes<max=8>", "String<max=8>", "Bytes<max=16>"),
        ("String<max=8>", "Bytes<max=8>", "Bytes<max=16>"),
        ("U64", "U64", "U64"),
    ] {
        assert_source_rejected(left, right, output, CompilerErrorKind::TypeMismatch);
    }
}

#[test]
fn existing_string_concat_remains_unchanged() {
    let core = compile(
        "String<max=3>",
        "String<max=5>",
        "String<max=8>",
        "input.left + input.right",
    );
    let CoreNode::Let {
        value: CoreExpr::Call {
            callee, type_args, ..
        },
        ..
    } = &core.intents["concatenate"].body.nodes[0]
    else {
        panic!("string concatenation")
    };
    assert_eq!(callee, "core.string.concat");
    assert_eq!(type_args, &Vec::<String>::new());
    assert_eq!(
        lower_to_target_ir(&core, &facts()).status,
        TargetLoweringStatus::Lowered
    );
}

fn assert_target_rejected(core: &CoreModule) {
    let report = lower_to_target_ir(core, &facts());
    assert_eq!(report.status, TargetLoweringStatus::Unsupported);
    assert!(report.artifact.is_none());
    assert!(
        report
            .failures
            .iter()
            .any(|failure| failure.kind == TargetLoweringFailureKind::InvalidCoreIdentity),
        "{report:?}"
    );
}

#[test]
fn target_rejects_forged_byte_concat_calls() {
    for case in 0..10 {
        let mut core = compile(
            "Bytes<max=8>",
            "Bytes<max=8>",
            "Bytes<max=16>",
            "input.left + input.right",
        );
        let (binding, type_args, args) = call(&mut core);
        match case {
            0 => type_args.clear(),
            1 => {
                type_args.pop();
            }
            2 => type_args.push("Bytes<max=8>".into()),
            3 => type_args[0] = "Bytes<max=9>".into(),
            4 => type_args[1] = "U64".into(),
            5 => {
                args.pop();
            }
            6 => args.push(args[0].clone()),
            7 => args[1] = CoreExpr::Const(CoreValue::Bool(false)),
            8 => binding.ty = "Bytes<max=8>".into(),
            9 => {
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
        assert_target_rejected(&core);
    }
}

fn change_input_field(core: &mut CoreModule, field: &str, coordinate: &str) {
    let CoreType::Record { fields } = core.types.get_mut("Input").unwrap() else {
        panic!("input record")
    };
    fields.insert(field.into(), coordinate.into());
}

#[test]
fn target_rejects_byte_concat_overflow_and_nominal_unwrapping() {
    let mut overflow = compile(
        "Bytes<max=18446744073709551615>",
        "Bytes<max=0>",
        "Bytes<max=18446744073709551615>",
        "input.left + input.right",
    );
    change_input_field(&mut overflow, "right", "Bytes<max=1>");
    call(&mut overflow).1[1] = "Bytes<max=1>".into();
    validate_core_module_type_integrity(&overflow).unwrap();
    assert_target_rejected(&overflow);

    let mut nominal = compile(
        "Bytes<max=8>",
        "Bytes<max=8>",
        "Bytes<max=16>",
        "input.left + input.right",
    );
    nominal.types.insert(
        "Sealed".into(),
        CoreType::Nominal {
            contract: "Sealed".into(),
            representation: "Bytes<max=8>".into(),
        },
    );
    change_input_field(&mut nominal, "left", "Sealed");
    call(&mut nominal).1[0] = "Sealed".into();
    validate_core_module_type_integrity(&nominal).unwrap();
    assert_target_rejected(&nominal);
}

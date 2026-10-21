//! `count(bytes, b"…")` counts occurrences of one byte in bounded bytes.
use edict_syntax::{
    compile_to_core, digest_core_module, lower_to_target_ir, parse_module, CompilerContext,
    CompilerErrorKind, CoreBudget, CoreExpr, CoreModule, CoreNode, CoreValue, ResourceRef,
    TargetIrLoweringFacts, TargetLoweringFailureKind, TargetLoweringStatus,
};

fn context() -> CompilerContext {
    scan_budget(4_194_304)
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

fn source(ty: &str, expression: &str, output: &str) -> String {
    format!(
        "package count.example@1;
        type Input = {{ bytes: {ty}, other: {ty}, }};
        type Output = {{ lines: {output}, }};
        intent measure(input: Input) returns Output
          profile p.read basis none budget <= p.small {{
          let lines: {output} = {expression};
          return {{ lines }};
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

fn first_binding(core: &CoreModule) -> &CoreExpr {
    match &core.intents["measure"].body.nodes[0] {
        CoreNode::Let { value, .. } => value,
        _ => panic!("binding"),
    }
}

#[test]
fn count_retains_operand_identity_needle_byte_and_u64_result() {
    for (ty, needle, byte) in [
        ("Bytes<max=1048576>", "b\"\\n\"", b'\n'),
        ("Bytes<exact=32>", "b\"\\x00\"", 0x00),
        ("Bytes<exact=0>", "b\"\\xff\"", 0xff),
    ] {
        let core = compile(ty, &format!("count(input.bytes, {needle})"));
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
            panic!("count binding")
        };
        assert_eq!(binding.ty, "U64");
        assert_eq!(callee, "core.bytes.count");
        assert_eq!(type_args, &[ty]);
        assert!(
            matches!(args.as_slice(), [CoreExpr::Field { field, .. }, CoreExpr::Const(CoreValue::Bytes(needle))] if field == "bytes" && needle == &[byte])
        );
        assert_ne!(
            digest_core_module(&core).unwrap(),
            digest_core_module(&compile(ty, &format!("count(input.other, {needle})"))).unwrap()
        );
        let report = lower_to_target_ir(&core, &facts());
        assert_eq!(report.status, TargetLoweringStatus::Lowered, "{report:?}");
        assert_eq!(
            report.artifact.unwrap().intents["measure"].pure_bindings[0].value,
            *first_binding(&core)
        );
    }
    let other = compile("Bytes<max=32>", "count(input.bytes, b\"\\r\")");
    assert_ne!(
        digest_core_module(&other).unwrap(),
        digest_core_module(&compile("Bytes<max=32>", "count(input.bytes, b\"\\n\")")).unwrap(),
        "the counted byte is hash-significant"
    );
}

#[test]
fn count_rejects_wrong_operands_needles_signatures_and_result_type() {
    for (ty, expression, output, expected) in [
        (
            "U64",
            "count(input.bytes, b\"\\n\")",
            "U64",
            CompilerErrorKind::TypeMismatch,
        ),
        (
            "String<max=32>",
            "count(input.bytes, b\"\\n\")",
            "U64",
            CompilerErrorKind::TypeMismatch,
        ),
        (
            "Bytes<max=32>",
            "count(input.bytes)",
            "U64",
            CompilerErrorKind::UnsupportedSourceShape,
        ),
        (
            "Bytes<max=32>",
            "count(input.bytes, b\"\\n\", b\"\\n\")",
            "U64",
            CompilerErrorKind::UnsupportedSourceShape,
        ),
        (
            "Bytes<max=32>",
            "count<U64>(input.bytes, b\"\\n\")",
            "U64",
            CompilerErrorKind::UnsupportedSourceShape,
        ),
        (
            "Bytes<max=32>",
            "count(input.bytes, b\"\")",
            "U64",
            CompilerErrorKind::UnsupportedSourceShape,
        ),
        (
            "Bytes<max=32>",
            "count(input.bytes, b\"ab\")",
            "U64",
            CompilerErrorKind::UnsupportedSourceShape,
        ),
        (
            "Bytes<max=32>",
            "count(input.bytes, \"\\n\")",
            "U64",
            CompilerErrorKind::UnsupportedSourceShape,
        ),
        (
            "Bytes<max=32>",
            "count(input.bytes, input.other)",
            "U64",
            CompilerErrorKind::UnsupportedSourceShape,
        ),
        (
            "Bytes<max=32>",
            "count(input.bytes, 10u64)",
            "U64",
            CompilerErrorKind::UnsupportedSourceShape,
        ),
        (
            "Bytes<max=32>",
            "count(input.bytes, b\"\\n\")",
            "Bool",
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
            "{ty} {expression} -> {output}: {errors:?}"
        );
    }
}

#[test]
fn count_target_refuses_forged_signature_needle_or_operand_authority() {
    for case in 0..12 {
        let mut core = compile("Bytes<max=32>", "count(input.bytes, b\"\\n\")");
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
            5 => args.truncate(1),
            6 => args.push(args[1].clone()),
            7 => args[1] = CoreExpr::Const(CoreValue::Bytes(vec![])),
            8 => args[1] = CoreExpr::Const(CoreValue::Bytes(vec![b'\r', b'\n'])),
            9 => args[1] = args[0].clone(),
            10 => binding.ty = "Bool".into(),
            11 => {
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
        assert!(
            report
                .failures
                .iter()
                .any(|f| f.kind == TargetLoweringFailureKind::InvalidCoreIdentity),
            "case {case}: {:?}",
            report.failures
        );
    }
}

#[test]
fn count_prelude_name_cannot_be_shadowed() {
    let text = source("Bytes<max=32>", "count(input.bytes, b\"\\n\")", "U64")
        .replace("let lines:", "let count:")
        .replace("return { lines }", "return { lines: count }");
    let errors = compile_to_core(&parse_module(&text).unwrap(), &context()).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|e| e.kind == CompilerErrorKind::SurfaceValidation),
        "{errors:?}"
    );
}

#[test]
fn direct_count_scans_charge_the_operand_maximum() {
    let simple = source("Bytes<max=1024>", "count(input.bytes, b\"\\n\")", "U64");
    let errors = compile_to_core(&parse_module(&simple).unwrap(), &scan_budget(100)).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|e| e.kind == CompilerErrorKind::InvalidBound),
        "{errors:?}"
    );
    compile_to_core(&parse_module(&simple).unwrap(), &scan_budget(1600))
        .expect("one complete scan fits");
    let repeated = simple.replace(
        "return { lines };",
        "let more = count(input.other, b\"\\n\"); return { lines };",
    );
    let errors =
        compile_to_core(&parse_module(&repeated).unwrap(), &scan_budget(1600)).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|e| e.kind == CompilerErrorKind::InvalidBound),
        "sequential scans add: {errors:?}"
    );
}

#[test]
fn count_helper_scan_is_charged_once() {
    let text = "package count.cost@1; fn lineBreaks(value:Bytes<max=1024>) -> U64 {return count(value, b\"\\n\");} type Input={bytes:Bytes<max=1024>,}; intent measure(input:Input) returns U64 profile p.read basis none budget <= p.small {let lines = lineBreaks(input.bytes); return lines;}";
    let parsed = parse_module(text).unwrap();
    let errors = compile_to_core(&parsed, &scan_budget(100)).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|e| e.kind == CompilerErrorKind::InvalidBound),
        "{errors:?}"
    );
    let core = compile_to_core(&parsed, &scan_budget(1600))
        .expect("single helper scan fits; double charging would exceed this ceiling");
    let report = lower_to_target_ir(&core, &facts());
    assert_eq!(report.status, TargetLoweringStatus::Lowered, "{report:?}");
}

#[test]
fn the_count_bound_proves_jims_line_count_addition() {
    // Jim's head records lineCount = lineBreaks + 1. The count is at most the
    // operand's byte maximum, so the sum cannot overflow U64.
    let core = compile("Bytes<max=1048576>", "count(input.bytes, b\"\\n\") + 1u64");
    let report = lower_to_target_ir(&core, &facts());
    assert_eq!(report.status, TargetLoweringStatus::Lowered, "{report:?}");
}

#[test]
fn the_count_bound_is_the_operand_maximum_not_zero() {
    // 2^64 - 1 - 1048576 + 1 overflows only when the count can reach the
    // operand maximum, so source must refuse it.
    let text = source(
        "Bytes<max=1048576>",
        "count(input.bytes, b\"\\n\") + 18446744073708503040u64",
        "U64",
    );
    let errors = compile_to_core(&parse_module(&text).unwrap(), &context()).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|e| e.kind == CompilerErrorKind::UnsupportedSourceShape),
        "{errors:?}"
    );
    compile_to_core(
        &parse_module(&source(
            "Bytes<max=1048576>",
            "count(input.bytes, b\"\\n\") + 18446744073708503039u64",
            "U64",
        ))
        .unwrap(),
        &context(),
    )
    .expect("the largest total addend is accepted");
}

#[test]
fn target_independently_refuses_a_forged_overflowing_count_sum() {
    let mut core = compile("Bytes<max=1048576>", "count(input.bytes, b\"\\n\") + 1u64");
    let CoreNode::Let {
        value: CoreExpr::Call { args, .. },
        ..
    } = &mut core.intents.get_mut("measure").unwrap().body.nodes[0]
    else {
        panic!("sum")
    };
    args[1] = CoreExpr::Const(CoreValue::Int {
        width: "U64".into(),
        value: "18446744073708503040".into(),
    });
    let report = lower_to_target_ir(&core, &facts());
    assert_eq!(
        report.status,
        TargetLoweringStatus::Unsupported,
        "{report:?}"
    );
    assert!(report.artifact.is_none());
}

#[test]
fn jims_line_metrics_compile_through_a_helper_fact_and_field_access() {
    // Jim's path: bounded bytes -> helper -> fact construction -> field access
    // -> Head.lineCount. The helper proves lineCount beside the count, where the
    // count's local binding still carries its bound.
    let text = "package count.path@1;
        type Metrics = { lineBreaks: U64, lineCount: U64, };
        fn measure(value: Bytes<max=1048576>) -> Metrics {
          let lineBreaks = count(value, b\"\\n\");
          let lineCount = lineBreaks + 1u64;
          return { lineBreaks, lineCount };
        }
        type Input = { text: Bytes<max=1048576>, };
        type Head = { lineCount: U64, };
        intent head(input: Input) returns Head
          profile p.read basis none budget <= p.small {
          let fact = measure(input.text);
          return { lineCount: fact.lineCount };
        }";
    let core = compile_to_core(&parse_module(text).unwrap(), &context()).expect("source proof");
    let report = lower_to_target_ir(&core, &facts());
    assert_eq!(report.status, TargetLoweringStatus::Lowered, "{report:?}");
}

#[test]
fn a_let_bound_count_keeps_its_bound_in_intent_bodies() {
    let text = source("Bytes<max=1048576>", "count(input.bytes, b\"\\n\")", "U64").replace(
        "return { lines };",
        "let total: U64 = lines + 1u64; return { lines: total };",
    );
    let core = compile_to_core(&parse_module(&text).unwrap(), &context()).expect("source proof");
    let report = lower_to_target_ir(&core, &facts());
    assert_eq!(report.status, TargetLoweringStatus::Lowered, "{report:?}");
}

#[test]
fn target_refuses_a_forged_binding_bound() {
    // Raise the addend by one past what the bound local proves.
    let text = source("Bytes<max=1048576>", "count(input.bytes, b\"\\n\")", "U64").replace(
        "return { lines };",
        "let total: U64 = lines + 18446744073708503039u64; return { lines: total };",
    );
    let mut core = compile_to_core(&parse_module(&text).unwrap(), &context()).unwrap();
    let CoreNode::Let {
        value: CoreExpr::Call { args, .. },
        ..
    } = &mut core.intents.get_mut("measure").unwrap().body.nodes[1]
    else {
        panic!("sum")
    };
    args[1] = CoreExpr::Const(CoreValue::Int {
        width: "U64".into(),
        value: "18446744073708503040".into(),
    });
    let report = lower_to_target_ir(&core, &facts());
    assert_eq!(
        report.status,
        TargetLoweringStatus::Unsupported,
        "{report:?}"
    );
}

#[test]
fn a_count_read_back_through_a_record_field_has_no_bound() {
    // Bounds travel with local bindings only, not through helper results or
    // record fields; callers compute derived metrics beside the count.
    let text = "package count.field@1;
        type Metrics = { lineBreaks: U64, };
        fn measure(value: Bytes<max=1048576>) -> Metrics {
          return { lineBreaks: count(value, b\"\\n\") };
        }
        type Input = { text: Bytes<max=1048576>, };
        type Head = { lineCount: U64, };
        intent head(input: Input) returns Head
          profile p.read basis none budget <= p.small {
          let fact = measure(input.text);
          let lineCount: U64 = fact.lineBreaks + 1u64;
          return { lineCount };
        }";
    let errors = compile_to_core(&parse_module(text).unwrap(), &context()).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|e| e.kind == CompilerErrorKind::UnsupportedSourceShape),
        "{errors:?}"
    );
}

/// The smallest budget component at which a module compiles.
fn smallest_budget(text: &str, budget: impl Fn(u64) -> CoreBudget) -> u64 {
    let parsed = parse_module(text).unwrap();
    let fits = |limit: u64| {
        let context = CompilerContext::new()
            .with_operation_profile("p.read", "continuum.profile.read-only/v1")
            .with_budget("p.small", budget(limit));
        compile_to_core(&parsed, &context).is_ok()
    };
    let (mut low, mut high) = (0_u64, 1_u64 << 40);
    while low < high {
        let middle = low + (high - low) / 2;
        if fits(middle) {
            high = middle;
        } else {
            low = middle + 1;
        }
    }
    low
}

#[test]
fn a_helper_count_charges_its_needle_literal() {
    // `isUtf8` scans the same operand with the same cost rule and has no
    // needle, so a helper `count` must cost strictly more in steps and storage.
    let helper = |body: &str, result: &str| {
        format!("package count.needle@1; fn h(value:Bytes<max=1024>) -> {result} {{return {body};}} type Input={{bytes:Bytes<max=1024>,}}; intent measure(input:Input) returns {result} profile p.read basis none budget <= p.small {{let r = h(input.bytes); return r;}}")
    };
    let count = helper("count(value, b\"\\n\")", "U64");
    let scan = helper("isUtf8(value)", "Bool");
    let steps = |limit| CoreBudget {
        max_steps: limit,
        max_allocated_bytes: u64::MAX,
        max_output_bytes: u64::MAX,
    };
    let storage = |limit| CoreBudget {
        max_steps: u64::MAX,
        max_allocated_bytes: limit,
        max_output_bytes: u64::MAX,
    };
    assert!(smallest_budget(&count, steps) > smallest_budget(&scan, steps));
    assert!(smallest_budget(&count, storage) > smallest_budget(&scan, storage));
}

#[test]
fn count_operand_errors_surface_alongside_signature_errors() {
    for expression in [
        "count(input.missing)",
        "count(input.missing, b\"ab\")",
        "count<U64>(input.missing, b\"\\n\")",
        "count(input.missing, input.other)",
    ] {
        let errors = compile_to_core(
            &parse_module(&source("Bytes<max=32>", expression, "U64")).unwrap(),
            &context(),
        )
        .unwrap_err();
        assert!(
            errors
                .iter()
                .any(|e| e.kind == CompilerErrorKind::UnsupportedSourceShape),
            "{expression}: {errors:?}"
        );
        assert!(
            errors
                .iter()
                .any(|e| e.kind != CompilerErrorKind::UnsupportedSourceShape),
            "{expression}: the unknown operand field is diagnosed too: {errors:?}"
        );
    }
}

//! First-order bounded state iteration needed by authored iterative algorithms.
use amblicit_syntax::{
    compile_to_core, decode_canonical_cbor, encode_core_module, parse_module, CanonicalValue,
    CompilerContext,
};

fn field<'a>(value: &'a CanonicalValue, key: &str) -> &'a CanonicalValue {
    let CanonicalValue::Map(fields) = value else {
        panic!("expected canonical map")
    };
    fields
        .iter()
        .find_map(|(name, value)| (name == &CanonicalValue::Text(key.into())).then_some(value))
        .expect("required contract field")
}

#[test]
fn bounded_fold_preserves_initial_state_static_step_and_iteration_bound() {
    let source = r#"
package iteration.state@1;
type State = { done: Bool };
fn step(state: State) -> State { return { done: !state.done }; }
fn run(initial: State) -> State { return fold(initial, step, 3u64); }
"#;
    let module = parse_module(source).expect("ordinary first-order call syntax");
    let core = compile_to_core(&module, &CompilerContext::new())
        .expect("a statically bounded state fold must compile");
    let bytes = encode_core_module(&core).expect("canonical Core encoding");
    let wire = decode_canonical_cbor(&bytes).expect("canonical Core decoding");
    let result = field(
        field(field(field(&wire, "functions"), "run"), "body"),
        "result",
    );
    assert_eq!(field(result, "kind"), &CanonicalValue::Text("fold".into()));
    assert_eq!(field(result, "bound"), &CanonicalValue::Integer(3));
    assert_eq!(
        field(result, "step"),
        &CanonicalValue::Text("iteration.state@1.step".into())
    );
    assert_eq!(
        field(field(result, "initial"), "kind"),
        &CanonicalValue::Text("local".into())
    );
}

fn compile_source(
    body: &str,
) -> Result<amblicit_syntax::CoreModule, Vec<amblicit_syntax::CompilerError>> {
    let source = format!("package iteration.state@1; {body}");
    compile_to_core(
        &parse_module(&source).expect("parse fixture"),
        &CompilerContext::new(),
    )
}

#[test]
fn fold_rejects_dynamic_bounds_callback_values_and_state_changes() {
    use amblicit_syntax::CompilerErrorKind as K;
    for (body, kind) in [
        ("fn step(s: Bool) -> Bool { return !s; } fn run(s: Bool, n: U64) -> Bool { return fold(s, step, n); }", K::InvalidBound),
        ("fn run(s: Bool, callback: Bool) -> Bool { return fold(s, callback, 3u64); }", K::UnsupportedSourceShape),
        ("fn step(s: Bool) -> U8 { return 0u8; } fn run(s: Bool) -> U8 { return fold(s, step, 3u64); }", K::TypeMismatch),
        ("fn step(a: Bool, b: Bool) -> Bool { return a; } fn run(s: Bool) -> Bool { return fold(s, step, 3u64); }", K::TypeMismatch),
        ("fn step(s: Bool) -> Bool { return !s; } fn run(s: U8) -> Bool { return fold(s, step, 3u64); }", K::TypeMismatch),
        ("fn step(s: Bool) -> Bool { return !s; } fn run(s: Bool) -> Bool { return fold(s, step, 18446744073709551615u64); }", K::InvalidBound),
    ] {
        let errors = compile_source(body).expect_err("invalid fold must refuse");
        assert!(errors.iter().any(|e| e.kind == kind), "{body}: {errors:?}");
    }
}

#[test]
fn zero_bound_does_not_hide_recursive_step_authority() {
    let errors = compile_source("fn step(s: Bool) -> Bool { return fold(s, step, 0u64); }")
        .expect_err("zero iterations still require a nonrecursive helper");
    assert!(
        errors
            .iter()
            .any(|e| e.kind == amblicit_syntax::CompilerErrorKind::UnsupportedSourceShape),
        "{errors:?}"
    );
}

#[test]
fn fold_core_requires_capability_and_declared_step_state() {
    let original = compile_source("fn step(s: Bool) -> Bool { return !s; } fn run(s: Bool) -> Bool { return fold(s, step, 0u64); }").expect("zero bound");
    assert!(original
        .required_core_capabilities
        .iter()
        .any(|c| c == "core.fold/v1"));
    for mutation in 0..3 {
        let mut core = original.clone();
        if mutation == 0 {
            core.required_core_capabilities.clear();
        }
        let amblicit_syntax::CoreExpr::Fold {
            step, state_type, ..
        } = &mut core.functions.get_mut("run").unwrap().body.result
        else {
            panic!("fold");
        };
        if mutation == 1 {
            *step = "untrusted.step".into();
        }
        if mutation == 2 {
            *state_type = "U8".into();
        }
        assert!(
            encode_core_module(&core).is_err(),
            "mutated fold authority must not encode"
        );
    }
}

fn operation(
    bound: u64,
    budget: u64,
) -> Result<amblicit_syntax::CoreModule, Vec<amblicit_syntax::CompilerError>> {
    let source = format!("package iteration.state@1; type State = {{ done: Bool }}; fn step(s: State) -> State {{ return {{ done: !s.done }}; }} fn run(s: State) -> State {{ return fold(s, step, {bound}u64); }} intent runIntent(input: State) returns State profile p.read basis none budget <= p.small {{ let result = run(input); return {{ done: result.done }}; }}");
    let context = CompilerContext::new()
        .with_operation_profile("p.read", "continuum.profile.read-only/v1")
        .with_budget(
            "p.small",
            amblicit_syntax::CoreBudget {
                max_steps: budget,
                max_allocated_bytes: 1_048_576,
                max_output_bytes: 65_536,
            },
        );
    compile_to_core(&parse_module(&source).unwrap(), &context)
}

#[test]
fn target_rechecks_fold_initial_state_and_static_helper() {
    let facts = lowering_facts();
    let original = operation(3, 65_536).expect("fold operation");
    let valid = amblicit_syntax::lower_to_target_ir(&original, &facts);
    assert_eq!(
        valid.status,
        amblicit_syntax::TargetLoweringStatus::Lowered,
        "{valid:?}"
    );
    for mutation in 0..3 {
        let mut core = original.clone();
        let amblicit_syntax::CoreExpr::Fold {
            initial,
            step,
            state_type,
            ..
        } = &mut core.functions.get_mut("run").unwrap().body.result
        else {
            panic!("fold");
        };
        match mutation {
            0 => {
                *initial = Box::new(amblicit_syntax::CoreExpr::Const(
                    amblicit_syntax::CoreValue::Bool(false),
                ))
            }
            1 => *step = "untrusted.step".into(),
            _ => *state_type = "Bool".into(),
        }
        let report = amblicit_syntax::lower_to_target_ir(&core, &facts);
        assert_ne!(
            report.status,
            amblicit_syntax::TargetLoweringStatus::Lowered,
            "{report:?}"
        );
        assert!(report.artifact.is_none());
    }
}

#[test]
fn fold_iteration_count_changes_admission_work() {
    // One shared ceiling separates zero from substantial iteration. This is a
    // differential work witness, not an independently derived exact-cost oracle.
    operation(0, 256).expect("zero-iteration control fits");
    let errors = operation(100, 256).expect_err("repeated state work exceeds ceiling");
    assert!(
        errors
            .iter()
            .any(|e| e.kind == amblicit_syntax::CompilerErrorKind::InvalidBound),
        "{errors:?}"
    );
}

#[test]
fn fold_name_cannot_be_redefined_as_a_source_helper() {
    let errors = compile_source("fn fold(s: Bool) -> Bool { return s; }")
        .expect_err("fold is a closed prelude form");
    assert!(
        errors
            .iter()
            .any(|e| e.kind == amblicit_syntax::CompilerErrorKind::SurfaceValidation),
        "{errors:?}"
    );
}

fn lowering_facts() -> amblicit_syntax::TargetIrLoweringFacts {
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
fn fold_accepts_initial_bytes_within_state_bound() {
    use amblicit_syntax::{CoreBudget, CoreExpr, TargetLoweringStatus};
    let context = CompilerContext::new()
        .with_operation_profile("p.read", "continuum.profile.read-only/v1")
        .with_budget(
            "p.small",
            CoreBudget {
                max_steps: 65_536,
                max_allocated_bytes: 1_048_576,
                max_output_bytes: 65_536,
            },
        );
    for initial in ["", "12345678"] {
        let source = format!(
            r#"
package iteration.state@1;
type State = {{ value: Bytes<max=8> }};
fn step(s: Bytes<max=8>) -> Bytes<max=8> {{ return s; }}
fn run() -> Bytes<max=8> {{ return fold(b"{initial}", step, 2u64); }}
intent runIntent(input: State) returns State profile p.read basis none budget <= p.small {{
    let result = run();
    return {{ value: result }};
}}
"#
        );
        let core = compile_to_core(&parse_module(&source).unwrap(), &context)
            .expect("initial bytes within the state bound must compile");
        encode_core_module(&core).expect("accepted fold must encode");
        let CoreExpr::Fold { state_type, .. } = &core.functions["run"].body.result else {
            panic!("explicit fold required");
        };
        assert_eq!(state_type, &core.functions["step"].return_type);
        let report = amblicit_syntax::lower_to_target_ir(&core, &lowering_facts());
        assert_eq!(report.status, TargetLoweringStatus::Lowered, "{report:?}");
    }
    let errors = compile_source(r#"fn step(s: Bytes<max=8>) -> Bytes<max=8> { return s; } fn run() -> Bytes<max=8> { return fold(b"123456789", step, 2u64); }"#)
        .expect_err("initial bytes larger than the state bound must refuse");
    assert!(
        errors
            .iter()
            .any(|e| e.kind == amblicit_syntax::CompilerErrorKind::TypeMismatch),
        "{errors:?}"
    );
}

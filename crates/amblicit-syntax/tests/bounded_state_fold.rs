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
    let source = r"
package iteration.state@1;
type State = { done: Bool };
fn step(state: State) -> State { return { done: !state.done }; }
fn run(initial: State) -> State { return fold(initial, step, 3u64); }
";
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
        field(result, "stateType"),
        &CanonicalValue::Text("iteration.state@1.State".into())
    );
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
    encode_core_module(&original).expect("unmutated zero-bound fold encodes");
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
        let failure = amblicit_syntax::validate_core_module_type_integrity(&core)
            .expect_err("mutated fold authority must fail Core integrity");
        assert_eq!(
            failure.kind(),
            amblicit_syntax::CoreTypeIntegrityFailureKind::InvalidDefinition
        );
        assert_eq!(failure.path(), "functions.run.body.result");
        assert_eq!(
            encode_core_module(&core)
                .expect_err("invalid fold must not encode")
                .kind(),
            amblicit_syntax::CanonicalErrorKind::UnsupportedValue
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
                **initial =
                    amblicit_syntax::CoreExpr::Const(amblicit_syntax::CoreValue::Bool(false));
            }
            1 => *step = "untrusted.step".into(),
            _ => *state_type = "Bool".into(),
        }
        // The initial-value mismatch must reach the Target-local type check.
        // Bad helper/state coordinates instead stop at shared Core integrity.
        if mutation == 0 {
            amblicit_syntax::validate_core_module_type_integrity(&core)
                .expect("initial-value mismatch is structurally valid Core");
            encode_core_module(&core).expect("initial-value mismatch encodes structurally");
        } else {
            let failure = amblicit_syntax::validate_core_module_type_integrity(&core)
                .expect_err("helper/state forgery stops at shared Core integrity");
            assert_eq!(
                failure.kind(),
                amblicit_syntax::CoreTypeIntegrityFailureKind::InvalidDefinition
            );
            assert_eq!(failure.path(), "functions.run.body.result");
        }
        let report = amblicit_syntax::lower_to_target_ir(&core, &facts);
        assert_eq!(
            report.status,
            amblicit_syntax::TargetLoweringStatus::Unsupported
        );
        assert_eq!(report.failures.len(), 1, "{report:?}");
        assert_eq!(
            report.failures[0].kind,
            amblicit_syntax::TargetLoweringFailureKind::InvalidCoreIdentity
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

#[test]
fn fold_initial_state_uses_existing_record_and_variant_compatibility() {
    let core = compile_source("type A = { done: Bool }; type B = { done: Bool }; fn step(s: A) -> A { return s; } fn run(s: B) -> A { return fold(s, step, 3u64); }")
        .expect("same-shaped named records retain structural compatibility");
    encode_core_module(&core).expect("structurally compatible record entry encodes");
    let amblicit_syntax::CoreExpr::Fold {
        state_type,
        initial,
        ..
    } = &core.functions["run"].body.result
    else {
        panic!("fold");
    };
    assert_eq!(state_type, "iteration.state@1.A");
    let amblicit_syntax::CoreExpr::Local { reference } = initial.as_ref() else {
        panic!("initial local");
    };
    assert_eq!(reference.ty, "iteration.state@1.B");
    let errors = compile_source("type A = variant { Done }; type B = variant { Done }; fn step(s: A) -> A { return s; } fn run(s: B) -> A { return fold(s, step, 3u64); }")
        .expect_err("distinct nominal variants must not substitute at fold entry");
    assert!(
        errors
            .iter()
            .any(|error| error.kind == amblicit_syntax::CompilerErrorKind::TypeMismatch),
        "{errors:?}"
    );
}

#[test]
fn fold_name_cannot_be_shadowed_by_parameters_or_locals() {
    for source in [
        "fn run(fold: Bool) -> Bool { return fold; }",
        "fn run(s: Bool) -> Bool { let fold = s; return fold; }",
    ] {
        let errors = compile_source(source).expect_err("fold is a protected prelude name");
        assert!(
            errors
                .iter()
                .any(|error| error.kind == amblicit_syntax::CompilerErrorKind::SurfaceValidation),
            "{errors:?}"
        );
    }
}

#[test]
fn target_rejects_forged_fold_work_without_intrinsic_facts() {
    use amblicit_syntax::{CoreExpr, TargetLoweringFailureKind, TargetLoweringStatus};
    let facts = lowering_facts();
    assert!(facts.pure_functions.is_empty());
    let original = operation(3, 65_536).expect("valid bounded operation");
    assert_eq!(
        amblicit_syntax::lower_to_target_ir(&original, &facts).status,
        TargetLoweringStatus::Lowered
    );
    for forged in [65_537, u64::MAX] {
        let mut core = original.clone();
        let CoreExpr::Fold { bound, .. } = &mut core.functions.get_mut("run").unwrap().body.result
        else {
            panic!("fold")
        };
        *bound = forged;
        encode_core_module(&core).expect("forged bound is structurally valid Core");
        let report = amblicit_syntax::lower_to_target_ir(&core, &facts);
        assert_eq!(
            report.status,
            TargetLoweringStatus::Unsupported,
            "bound {forged}: {report:?}"
        );
        assert_eq!(report.failures.len(), 1);
        assert_eq!(
            report.failures[0].kind,
            TargetLoweringFailureKind::InvalidCoreIdentity
        );
        assert_eq!(report.failures[0].intent.as_deref(), Some("runIntent"));
        assert!(report.artifact.is_none());
    }
}

fn boolean_work_operation(step: &str, bound: u64) -> amblicit_syntax::CoreModule {
    let source = format!("package iteration.state@1; type Input = {{ value: Bool }}; {step} fn run(s: Bool) -> Bool {{ return fold(s, step, {bound}u64); }} intent runIntent(input: Input) returns Input profile p.read basis none budget <= p.small {{ let result = run(input.value); return {{ value: result }}; }}");
    let context = CompilerContext::new()
        .with_operation_profile("p.read", "continuum.profile.read-only/v1")
        .with_budget(
            "p.small",
            amblicit_syntax::CoreBudget {
                max_steps: 1_000_000,
                max_allocated_bytes: 1_048_576,
                max_output_bytes: 65_536,
            },
        );
    compile_to_core(&parse_module(&source).unwrap(), &context).unwrap()
}

fn assert_work_boundary(mut core: amblicit_syntax::CoreModule, steps: u64, bytes: u64) {
    use amblicit_syntax::{TargetLoweringFailureKind, TargetLoweringStatus};
    let facts = lowering_facts();
    for (step_limit, byte_limit, accepted) in [
        (steps, bytes, true),
        (steps - 1, bytes, false),
        (steps, bytes - 1, false),
    ] {
        let budget = &mut core
            .intents
            .get_mut("runIntent")
            .unwrap()
            .core_evaluation_budget;
        budget.max_steps = step_limit;
        budget.max_allocated_bytes = byte_limit;
        encode_core_module(&core).expect("budget mutation is structurally valid");
        let report = amblicit_syntax::lower_to_target_ir(&core, &facts);
        if accepted {
            assert_eq!(report.status, TargetLoweringStatus::Lowered, "{report:?}");
        } else {
            assert_eq!(
                report.status,
                TargetLoweringStatus::Unsupported,
                "limits {step_limit}/{byte_limit}: {report:?}"
            );
            assert_eq!(report.failures.len(), 1);
            assert_eq!(
                report.failures[0].kind,
                TargetLoweringFailureKind::InvalidCoreIdentity
            );
            assert_eq!(report.failures[0].intent.as_deref(), Some("runIntent"));
            assert!(report.artifact.is_none());
        }
    }
}

#[test]
fn target_fold_work_has_exact_step_and_storage_boundaries() {
    // Hand-derived portable costs: the one-field Input has 3 cells / 197 bytes.
    // Frame + argument + run wrapper + binding + result = 34 steps / 1108 bytes.
    // Each identity iteration: helper entry 1, parameter/result checks 2,
    // local value 2 steps / 64 bytes, and iteration overhead 1 => 6 / 64.
    for n in [0, 1, 3] {
        assert_work_boundary(
            boolean_work_operation("fn step(s: Bool) -> Bool { return s; }", n),
            34 + 6 * n,
            1108 + 64 * n,
        );
    }
}

#[test]
fn target_fold_work_counts_transitive_and_nested_steps() {
    // A transitive identity call adds 7 steps / 128 bytes per outer step.
    let transitive =
        "fn leaf(s: Bool) -> Bool { return s; } fn step(s: Bool) -> Bool { return leaf(s); }";
    assert_work_boundary(
        boolean_work_operation(transitive, 3),
        34 + 13 * 3,
        1108 + 192 * 3,
    );
    // Inner fold initializer/result = 4 / 128, two inner iterations = 12 / 128;
    // step entry/checks = 3 and outer overhead = 1 => 20 / 256 per outer step.
    let nested = "fn leaf(s: Bool) -> Bool { return s; } fn step(s: Bool) -> Bool { return fold(s, leaf, 2u64); }";
    assert_work_boundary(
        boolean_work_operation(nested, 3),
        34 + 20 * 3,
        1108 + 256 * 3,
    );
    let mut forged = boolean_work_operation(nested, 3);
    for name in ["step", "run"] {
        let amblicit_syntax::CoreExpr::Fold { bound, .. } =
            &mut forged.functions.get_mut(name).unwrap().body.result
        else {
            panic!("fold")
        };
        *bound = 1_u64 << 32;
    }
    let report = amblicit_syntax::lower_to_target_ir(&forged, &lowering_facts());
    assert_eq!(
        report.status,
        amblicit_syntax::TargetLoweringStatus::Unsupported
    );
    assert_eq!(
        report.failures[0].kind,
        amblicit_syntax::TargetLoweringFailureKind::InvalidCoreIdentity
    );
    assert!(report.artifact.is_none());
}

#[test]
fn target_fold_allocation_overflow_refuses_even_when_steps_fit() {
    let mut core = boolean_work_operation("fn step(s: Bool) -> Bool { return s; }", 1);
    let amblicit_syntax::CoreExpr::Fold { bound, .. } =
        &mut core.functions.get_mut("run").unwrap().body.result
    else {
        panic!("fold")
    };
    *bound = 1_u64 << 60;
    // 34 + 6*N fits u64; 1108 + 64*N does not. This isolates storage overflow.
    assert!(6_u64.checked_mul(*bound).unwrap().checked_add(34).is_some());
    let budget = &mut core
        .intents
        .get_mut("runIntent")
        .unwrap()
        .core_evaluation_budget;
    budget.max_steps = u64::MAX;
    budget.max_allocated_bytes = u64::MAX;
    encode_core_module(&core).expect("structurally valid overflow witness");
    let report = amblicit_syntax::lower_to_target_ir(&core, &lowering_facts());
    assert_eq!(
        report.status,
        amblicit_syntax::TargetLoweringStatus::Unsupported
    );
    assert_eq!(report.failures.len(), 1);
    assert_eq!(
        report.failures[0].kind,
        amblicit_syntax::TargetLoweringFailureKind::InvalidCoreIdentity
    );
    assert_eq!(report.failures[0].intent.as_deref(), Some("runIntent"));
    assert!(report.artifact.is_none());
}

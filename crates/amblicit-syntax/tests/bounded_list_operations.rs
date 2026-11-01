//! Public compiler and independent Target contracts for immutable worklists.
use amblicit_syntax::{
    compile_to_core, encode_core_module, lower_to_target_ir, parse_module,
    validate_core_module_type_integrity, CompilerContext, CompilerErrorKind, CoreBudget, CoreExpr,
    CoreModule, CorePredicate, CoreTypeIntegrityFailureKind, ResourceRef, TargetIrLoweringFacts,
    TargetLoweringFailureKind, TargetLoweringStatus,
};

fn compile(body: &str) -> Result<CoreModule, Vec<amblicit_syntax::CompilerError>> {
    compile_to_core(
        &parse_module(&format!("package work.lists@1; {body}")).expect("parse specimen"),
        &CompilerContext::new()
            .with_operation_profile("p.read", "continuum.profile.read-only/v1")
            .with_budget(
                "p.large",
                CoreBudget {
                    max_steps: 1_000_000,
                    max_allocated_bytes: 1_000_000,
                    max_output_bytes: 1_000_000,
                },
            ),
    )
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

const FOLD: &str = "
fn step(xs: List<U8, max=3>) -> List<U8, max=3> {
  return if listLen(xs) < 3u64 then listAppend(xs, 7u8) else xs;
}
fn build() -> List<U8, max=3> { let empty: List<U8, max=3> = []; return fold(empty, step, 5u64); }
type Input = { marker: U8 }; type Output = { items: List<U8, max=3> };
intent run(input: Input) returns Output profile p.read basis none budget <= p.large {
  let items = build(); return { items };
}";

#[test]
fn bounded_lists_accumulate_through_invariant_fold_state() {
    let core = compile(FOLD).expect("guarded bounded accumulation");
    assert!(core
        .required_core_capabilities
        .iter()
        .any(|c| c == "core.list/v1"));
    assert!(core
        .required_core_capabilities
        .iter()
        .any(|c| c == "core.fold/v1"));
    let report = lower_to_target_ir(&core, &facts());
    assert_eq!(report.status, TargetLoweringStatus::Lowered, "{report:?}");
    assert_eq!(
        encode_core_module(&core).unwrap(),
        encode_core_module(&compile(FOLD).unwrap()).unwrap()
    );
}

#[test]
fn bounded_lists_preserve_empty_maximum_and_nominal_element_types() {
    for body in [
        "fn length() -> U64 { let xs: List<U8, max=0> = []; return listLen(xs); }",
        "fn length() -> U64 { let xs: List<U8, max=2> = [1u8, 2u8]; return listLen(xs); }",
        "fn select(xs: List<U8, max=2>, i: U64) -> U8 { return if i < listLen(xs) then listAt(xs, i) else 0u8; }",
        "fn select(xs: List<List<U8, max=2>, max=2>, i: U64) -> List<U8, max=2> { return if i < listLen(xs) then listAt(xs, i) else []; }",
        "type Item = variant { Empty, Value(U8) }; fn select(xs: List<Item, max=2>, i: U64) -> Item { return if i < listLen(xs) then listAt(xs, i) else Item::Empty; }",
        "fn join(a: List<U8, max=2>, b: List<U8, max=3>) -> List<U8, max=5> { return listConcat(a, b); }",
    ] {
        let specimen = format!("{body} type Input = {{ marker: U8 }}; type Output = {{ marker: U8 }}; intent witness(input: Input) returns Output profile p.read basis none budget <= p.large {{ let marker = input.marker; return {{ marker }}; }}");
        let core = compile(&specimen).expect(body);
        let report = lower_to_target_ir(&core, &facts());
        assert_eq!(report.status, TargetLoweringStatus::Lowered, "{body}: {report:?}");
    }
}

#[test]
fn bounded_lists_reject_unsafe_guards_signatures_and_result_bounds() {
    use CompilerErrorKind as K;
    for (body, kind) in [
        ("fn f(xs: List<U8, max=2>, i: U64) -> U8 { return listAt(xs, i); }", K::UnsupportedSourceShape),
        ("fn f(xs: List<U8, max=2>, i: U64) -> U8 { return if i <= listLen(xs) then listAt(xs, i) else 0u8; }", K::UnsupportedSourceShape),
        ("fn f(xs: List<U8, max=2>, ys: List<U8, max=2>, i: U64) -> U8 { return if i < listLen(ys) then listAt(xs, i) else 0u8; }", K::UnsupportedSourceShape),
        ("fn f(xs: List<U8, max=2>) -> List<U8, max=2> { return listAppend(xs, 1u8); }", K::UnsupportedSourceShape),
        ("fn f(xs: List<U8, max=2>) -> List<U8, max=2> { return if listLen(xs) <= 2u64 then listAppend(xs, 1u8) else xs; }", K::UnsupportedSourceShape),
        ("fn f(xs: List<U8, max=2>) -> U64 { return len(xs); }", K::TypeMismatch),
        ("fn f(xs: Bytes<max=2>) -> U64 { return listLen(xs); }", K::TypeMismatch),
        ("fn f(a: List<U8, max=2>, b: List<U8, max=3>) -> List<U8, max=4> { return listConcat(a, b); }", K::TypeMismatch),
        ("fn f(a: List<U8, max=18446744073709551615>, b: List<U8, max=1>) -> List<U8, max=1> { return listConcat(a, b); }", K::InvalidBound),
    ] {
        let errors = compile(body).expect_err(body);
        assert!(errors.iter().any(|e| e.kind == kind), "{body}: {errors:?}");
    }
}

#[test]
fn bounded_list_capability_is_required_for_canonical_encoding() {
    let mut core = compile(FOLD).unwrap();
    core.required_core_capabilities
        .retain(|c| c != "core.list/v1");
    let failure = validate_core_module_type_integrity(&core).expect_err("missing capability");
    assert_eq!(
        failure.kind(),
        CoreTypeIntegrityFailureKind::InvalidDefinition
    );
}

#[test]
fn target_rechecks_list_append_guard_and_signature() {
    let original = compile(FOLD).unwrap();
    for mutation in 0..3 {
        let mut core = original.clone();
        let CoreExpr::If {
            predicate,
            then_value,
            ..
        } = &mut core.functions.get_mut("step").unwrap().body.result
        else {
            panic!("guard")
        };
        if mutation == 0 {
            **predicate = CorePredicate::True;
        }
        let CoreExpr::Call {
            type_args, args, ..
        } = then_value.as_mut()
        else {
            panic!("append")
        };
        if mutation == 1 {
            type_args[0] = "List<U8,max=4>".into();
        }
        if mutation == 2 {
            args.pop();
        }
        let report = lower_to_target_ir(&core, &facts());
        assert_eq!(
            report.status,
            TargetLoweringStatus::Unsupported,
            "{report:?}"
        );
        assert!(
            report
                .failures
                .iter()
                .any(|f| f.kind == TargetLoweringFailureKind::InvalidCoreIdentity),
            "{report:?}"
        );
    }
}

#[test]
fn target_reconstructs_list_fold_work_without_source_estimates() {
    let original = compile(FOLD).unwrap();
    for allocation in [false, true] {
        let mut core = original.clone();
        let budget = &mut core.intents.get_mut("run").unwrap().core_evaluation_budget;
        if allocation {
            budget.max_allocated_bytes = 1;
        } else {
            budget.max_steps = 1;
        }
        let report = lower_to_target_ir(&core, &facts());
        assert_eq!(
            report.status,
            TargetLoweringStatus::Unsupported,
            "{report:?}"
        );
        assert!(
            report
                .failures
                .iter()
                .any(|f| f.kind == TargetLoweringFailureKind::InvalidCoreIdentity),
            "{report:?}"
        );
    }
}

#[test]
fn bounded_list_guards_follow_short_circuit_and_branch_dominance() {
    for expression in [
        "if i < listLen(xs) then listAt(xs, i) else 0u8",
        "if listLen(xs) > i then listAt(xs, i) else 0u8",
        "if i >= listLen(xs) then 0u8 else listAt(xs, i)",
        "if i < listLen(xs) && listAt(xs, i) == 1u8 then 1u8 else 0u8",
        "if i >= listLen(xs) || listAt(xs, i) == 1u8 then 1u8 else 0u8",
    ] {
        compile(&format!(
            "fn select(xs: List<U8, max=2>, i: U64) -> U8 {{ return {expression}; }}"
        ))
        .expect(expression);
    }
    for expression in [
        "if i < listLen(xs) then 0u8 else listAt(xs, i)",
        "if i < listLen(xs) || i == 0u64 then listAt(xs, i) else 0u8",
        "if i < listLen(xs) || listAt(xs, i) == 1u8 then 1u8 else 0u8",
    ] {
        let errors = compile(&format!(
            "fn select(xs: List<U8, max=2>, i: U64) -> U8 {{ return {expression}; }}"
        ))
        .expect_err(expression);
        assert!(
            errors
                .iter()
                .any(|e| e.kind == CompilerErrorKind::UnsupportedSourceShape),
            "{errors:?}"
        );
    }
}

#[test]
fn bounded_lists_preserve_typed_missing_and_full_outcomes() {
    compile("type Found = variant { Missing, Value(U8) }; type Added = variant { Full, Value(List<U8, max=2>) };
      fn lookup(xs: List<U8, max=2>, i: U64) -> Found { return if i < listLen(xs) then Found::Value(listAt(xs,i)) else Found::Missing; }
      fn append(xs: List<U8, max=2>, x: U8) -> Added { return if listLen(xs) < 2u64 then Added::Value(listAppend(xs,x)) else Added::Full; }").expect("closed typed outcomes");
    for body in [
        "type A = variant { Value(U8) }; type B = variant { Value(U8) }; fn f(xs: List<A,max=2>, y: B) -> List<A,max=2> { return if listLen(xs) < 2u64 then listAppend(xs,y) else xs; }",
        "type A = variant { Value(U8) }; type B = variant { Value(U8) }; fn f(xs: List<A,max=2>, ys: List<B,max=2>) -> List<A,max=4> { return listConcat(xs,ys); }",
        "fn f(xs: List<U8,max=2>, i: U32) -> U8 { return listAt(xs,i); }",
        "fn f(xs: List<U8,max=2>) -> List<U8,max=2> { return if listLen(xs) < 3u64 then listAppend(xs,1u8) else xs; }",
        "fn f(xs: List<U8,max=0>) -> List<U8,max=0> { return listAppend(xs,1u8); }",
        "fn f(xs: List<U8,max=2>) -> U64 { return listLen<U8>(xs); }",
        "fn f(xs: List<U8,max=2>) -> U8 { return listAt(xs); }",
    ] {
        let errors = compile(body).expect_err(body);
        assert!(errors.iter().any(|e| matches!(e.kind, CompilerErrorKind::TypeMismatch | CompilerErrorKind::UnsupportedSourceShape)), "{errors:?}");
    }
}

const LENGTH_INTENT: &str = "
type Input = { xs: List<U8,max=2> }; type Output = { n: U64 };
intent run(input: Input) returns Output profile p.read basis none budget <= p.large {
  let n = listLen(input.xs); return { n };
}";

#[test]
fn target_list_work_has_exact_steps_and_storage_without_a_fold() {
    let original = compile(LENGTH_INTENT).unwrap();
    assert_eq!(original.required_core_capabilities, vec!["core.list/v1"]);
    // Portable frame: 8 cells, 515 bytes. len expression: 271 steps,
    // 578 bytes. Let overhead: 1 step. Result: 6 steps, 257 bytes.
    // This includes evaluation of the full record/list operand, not just length.
    for (steps, bytes, accepted) in [(294, 1350, true), (293, 1350, false), (294, 1349, false)] {
        let context = CompilerContext::new()
            .with_operation_profile("p.read", "continuum.profile.read-only/v1")
            .with_budget(
                "p.large",
                CoreBudget {
                    max_steps: steps,
                    max_allocated_bytes: bytes,
                    max_output_bytes: 1_000_000,
                },
            );
        let authored = compile_to_core(
            &parse_module(&format!("package work.lists@1; {LENGTH_INTENT}")).unwrap(),
            &context,
        );
        assert_eq!(
            authored.is_ok(),
            accepted,
            "source {steps}/{bytes}: {authored:?}"
        );
        if let Err(errors) = authored {
            assert!(errors
                .iter()
                .any(|e| e.kind == CompilerErrorKind::InvalidBound));
        }
        let mut core = original.clone();
        let budget = &mut core.intents.get_mut("run").unwrap().core_evaluation_budget;
        budget.max_steps = steps;
        budget.max_allocated_bytes = bytes;
        let report = lower_to_target_ir(&core, &facts());
        assert_eq!(
            report.status == TargetLoweringStatus::Lowered,
            accepted,
            "{steps}/{bytes}: {report:?}"
        );
        if !accepted {
            assert!(report
                .failures
                .iter()
                .any(|f| f.kind == TargetLoweringFailureKind::InvalidCoreIdentity));
        }
    }
}

#[test]
fn source_list_work_accounts_for_intents_before_capability_discovery() {
    for source in [LENGTH_INTENT.to_owned(), format!("{} {LENGTH_INTENT}", "intent earlier(input: Bytes<max=4096>) returns Bytes<max=4096> profile p.read basis none budget <= p.large { let x = input; return x; }")] {
        let context = CompilerContext::new().with_operation_profile("p.read", "continuum.profile.read-only/v1").with_budget("p.large", CoreBudget { max_steps: 1_000_000, max_allocated_bytes: 1300, max_output_bytes: 1_000_000 });
        let errors = compile_to_core(&parse_module(&format!("package work.lists@1; {source}")).unwrap(), &context).expect_err("allocation budget");
        assert!(errors.iter().any(|e| e.kind == CompilerErrorKind::InvalidBound), "{errors:?}");
    }
    // Give only the earlier, list-free intent a small budget, so a later list
    // operation cannot mask missing accounting on that first intent.
    let earlier = "intent earlier(input: Bytes<max=4096>) returns Bytes<max=4096> profile p.read basis none budget <= p.tiny { let x = input; return x; }";
    let source = format!("package work.lists@1; {earlier} {LENGTH_INTENT}");
    let context = CompilerContext::new()
        .with_operation_profile("p.read", "continuum.profile.read-only/v1")
        .with_budget(
            "p.large",
            CoreBudget {
                max_steps: 1_000_000,
                max_allocated_bytes: 1_000_000,
                max_output_bytes: 1_000_000,
            },
        )
        .with_budget(
            "p.tiny",
            CoreBudget {
                max_steps: 1_000_000,
                max_allocated_bytes: 1300,
                max_output_bytes: 1_000_000,
            },
        );
    let errors = compile_to_core(&parse_module(&source).unwrap(), &context)
        .expect_err("earlier intent budget");
    assert!(
        errors
            .iter()
            .any(|e| e.kind == CompilerErrorKind::InvalidBound
                && e.span.end <= source.find(LENGTH_INTENT).unwrap()),
        "{errors:?}"
    );
}

#[test]
fn target_rechecks_list_selection_and_nominal_element_coordinates() {
    let source = format!("type A = variant {{ Value(U8) }}; type B = variant {{ Value(U8) }};
      fn lookup(xs: List<A,max=2>, other: List<A,max=2>, i: U64) -> A {{ return if i < listLen(xs) then listAt(xs,i) else A::Value(0u8); }} {LENGTH_INTENT}");
    let original = compile(&source).unwrap();
    assert_eq!(
        lower_to_target_ir(&original, &facts()).status,
        TargetLoweringStatus::Lowered
    );
    for mutation in 0..4 {
        let mut core = original.clone();
        let function = core.functions.get_mut("lookup").unwrap();
        let other = function.params[1].clone();
        let CoreExpr::If {
            predicate,
            then_value,
            ..
        } = &mut function.body.result
        else {
            panic!("if")
        };
        if mutation == 0 {
            **predicate = CorePredicate::True;
        }
        if mutation == 1 {
            let CorePredicate::Compare { op, .. } = predicate.as_mut() else {
                panic!("comparison")
            };
            *op = amblicit_syntax::CompareOp::Le;
        }
        let CoreExpr::Call {
            type_args, args, ..
        } = then_value.as_mut()
        else {
            panic!("at")
        };
        if mutation == 2 {
            args[0] = CoreExpr::Local { reference: other };
        }
        if mutation == 3 {
            type_args[0] = "List<work.lists@1.B,max=2>".into();
        }
        validate_core_module_type_integrity(&core).expect("structural Core control");
        let report = lower_to_target_ir(&core, &facts());
        assert_eq!(
            report.status,
            TargetLoweringStatus::Unsupported,
            "{report:?}"
        );
        assert!(report
            .failures
            .iter()
            .any(|f| f.kind == TargetLoweringFailureKind::InvalidCoreIdentity));
    }
}

#[test]
fn list_prelude_names_cannot_be_shadowed() {
    for name in ["listLen", "listAt", "listAppend", "listConcat"] {
        for source in [
            format!("fn {name}(x: U8) -> U8 {{ return x; }}"),
            format!("fn f({name}: U8) -> U8 {{ return {name}; }}"),
        ] {
            let errors = compile(&source).expect_err("prelude authority");
            assert!(
                errors
                    .iter()
                    .any(|e| e.kind == CompilerErrorKind::SurfaceValidation),
                "{errors:?}"
            );
        }
    }
}

#[test]
fn bounded_list_identity_changes_with_operands_and_unknown_capabilities_refuse() {
    let original = compile(FOLD).unwrap();
    assert_ne!(
        encode_core_module(&original).unwrap(),
        encode_core_module(&compile(&FOLD.replace("7u8", "8u8")).unwrap()).unwrap()
    );
    let mut future = original;
    future
        .required_core_capabilities
        .push("core.list/v2".into());
    let report = lower_to_target_ir(&future, &facts());
    assert_eq!(report.status, TargetLoweringStatus::Unsupported);
    assert!(report.failures.iter().any(|f| f.kind
        == TargetLoweringFailureKind::UnsupportedCoreCapability
        && f.detail == "core.list/v2"));
}

#[test]
fn target_rechecks_list_concatenation_bounds() {
    let original = compile(&format!("fn join(a: List<U8,max=2>, b: List<U8,max=3>) -> List<U8,max=5> {{ return listConcat(a,b); }} {LENGTH_INTENT}")).unwrap();
    assert_eq!(
        lower_to_target_ir(&original, &facts()).status,
        TargetLoweringStatus::Lowered
    );
    for coordinate in [
        "List<U8,max=2>",
        "List<U8,max=18446744073709551615>",
        "List<U64,max=3>",
    ] {
        let mut core = original.clone();
        let CoreExpr::Call { type_args, .. } =
            &mut core.functions.get_mut("join").unwrap().body.result
        else {
            panic!("concat")
        };
        type_args[1] = coordinate.into();
        validate_core_module_type_integrity(&core).expect("valid type reference");
        let report = lower_to_target_ir(&core, &facts());
        assert_eq!(
            report.status,
            TargetLoweringStatus::Unsupported,
            "{report:?}"
        );
        assert!(report
            .failures
            .iter()
            .any(|f| f.kind == TargetLoweringFailureKind::InvalidCoreIdentity));
    }
}

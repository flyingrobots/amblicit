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
        "type Items = List<U8,max=2>; fn identity(xs: Items) -> Items { return xs; } fn append(xs: Items) -> Items { return if listLen(identity(xs)) < 2u64 then listAppend(identity(xs),1u8) else xs; }",
        "fn append(xs: List<U8,max=2>, ys: List<U8,max=2>, flag: Bool) -> List<U8,max=2> { return if listLen(if flag then xs else ys) < 2u64 then listAppend(if flag then xs else ys,1u8) else xs; }",
    ] {
        let specimen = format!("{body} type Input = {{ marker: U8 }}; type Output = {{ marker: U8 }}; intent witness(input: Input) returns Output profile p.read basis none budget <= p.large {{ let marker = input.marker; return {{ marker }}; }}");
        let core = compile(&specimen).expect(body);
        let report = lower_to_target_ir(&core, &facts());
        assert_eq!(report.status, TargetLoweringStatus::Lowered, "{body}: {report:?}");
    }
}

#[test]
fn conditional_append_preserves_record_item_coordinates() {
    for (item, list) in [
        ("Item", "List<Item,max=2>"),
        ("Item", "Items"),
        ("List<Item,max=1>", "List<List<Item,max=1>,max=2>"),
    ] {
        let body = format!(
            "type Item = {{ x: U8 }}; type Items = List<Item,max=2>;
            fn identity(xs: {list}) -> {list} {{ return xs; }}
            fn append(xs: {list}, ys: {list}, item: {item}, flag: Bool) -> {list} {{
                return if listLen(if flag then identity(xs) else ys) < 2u64
                    then listAppend(if flag then identity(xs) else ys,item) else xs;
            }} {LENGTH_INTENT}"
        );
        let core = compile(&body).expect("record list conditional compiles");
        let report = lower_to_target_ir(&core, &facts());
        assert_eq!(
            report.status,
            TargetLoweringStatus::Lowered,
            "{item}/{list}: {report:?}"
        );
    }
}

#[test]
fn matched_append_preserves_record_item_coordinates() {
    for (item, list) in [
        ("Item", "List<Item,max=2>"),
        ("Item", "Items"),
        ("List<Item,max=1>", "List<List<Item,max=1>,max=2>"),
    ] {
        for matched in [
            "match flag { A => identity(xs), B => ys }",
            "match flag { A => if true then identity(xs) else ys, B => ys }",
        ] {
            let body = format!(
                "type Item = {{ x: U8 }}; type Items = List<Item,max=2>;
                 type Flag = variant {{ A, B }};
                 fn identity(xs: {list}) -> {list} {{ return xs; }}
                 fn append(xs: {list}, ys: {list}, item: {item}, flag: Flag) -> {list} {{
                    return if listLen({matched}) < 2u64 then listAppend({matched},item) else xs;
                 }} {LENGTH_INTENT}"
            );
            let core = compile(&body).expect("record list match compiles");
            let mut target = facts();
            target.target_ir_domain = "echo.span-ir/v3".into();
            let report = lower_to_target_ir(&core, &target);
            assert_eq!(
                report.status,
                TargetLoweringStatus::Lowered,
                "{item}/{list}: {report:?}"
            );
        }
    }
}

#[test]
fn structural_record_joins_preserve_list_operations() {
    for definitions in [
        "type A = { x: U8 }; type B = { x: U8 };",
        "type A = { x: Bytes<max=1>, y: Bytes<max=3> }; type B = { x: Bytes<max=3>, y: Bytes<max=1> };",
    ] {
        for join in [
            "if input.flag then input.xs else input.ys",
            "match input.choice { Left => input.xs, Right => input.ys }",
            "(if input.flag then { items: input.xs } else { items: input.ys }).items",
        ] {
            for bind_first in [true, false] {
                let (setup, operand) = if bind_first {
                    (format!("let selected = {join};"), "selected")
                } else {
                    (String::new(), join)
                };
                let source = format!("{definitions} type Choice = variant {{ Left, Right }};
                  type Input = {{ xs: List<A,max=2>, ys: List<B,max=2>, flag: Bool, choice: Choice }};
                  type Output = {{ n: U64 }};
                  intent run(input: Input) returns Output profile p.read basis none budget <= p.large {{
                    {setup} let combined = listConcat({operand}, {operand});
                    let n = listLen(combined); return {{ n }};
                  }}");
                let core = compile(&source).expect("compatible structural record joins");
                let mut target = facts(); target.target_ir_domain = "echo.span-ir/v3".into();
                let report = lower_to_target_ir(&core, &target);
                assert_eq!(report.status, TargetLoweringStatus::Lowered, "bound={bind_first}, {source}: {report:?}");
            }
        }
    }
}

#[test]
fn structural_record_function_joins_preserve_list_operations() {
    for bind_first in [true, false] {
        let join = "if flag then xs else ys";
        let (setup, operand) = if bind_first {
            (format!("let selected = {join};"), "selected")
        } else {
            (String::new(), join)
        };
        let source = format!(
            "type A = {{ x: U8 }}; type B = {{ x: U8 }};
          fn merge(xs: List<A,max=2>, ys: List<B,max=2>, flag: Bool) -> List<A,max=4> {{
            {setup} return listConcat({operand}, {operand});
          }} {LENGTH_INTENT}"
        );
        let core = compile(&source).unwrap();
        let mut target = facts();
        target.target_ir_domain = "echo.span-ir/v3".into();
        let report = lower_to_target_ir(&core, &target);
        assert_eq!(
            report.status,
            TargetLoweringStatus::Lowered,
            "bound={bind_first}: {report:?}"
        );
    }
}

#[test]
fn concat_equivalence_retains_nominal_variant_identity() {
    let source = format!("type A = variant {{ Value(U8) }}; type B = variant {{ Value(U8) }};
      fn join(a: List<A,max=2>, b: List<A,max=2>) -> List<A,max=4> {{ return listConcat(a,b); }} {LENGTH_INTENT}");
    let mut core = compile(&source).unwrap();
    assert_eq!(
        lower_to_target_ir(&core, &facts()).status,
        TargetLoweringStatus::Lowered
    );
    let function = core.functions.get_mut("join").unwrap();
    function.return_type = "List<work.lists@1.B,max=4>".into();
    let CoreExpr::Call { type_args, .. } = &mut function.body.result else {
        panic!("concat");
    };
    type_args.fill("List<work.lists@1.B,max=2>".into());
    validate_core_module_type_integrity(&core).expect("well-formed nominal substitution");
    let report = lower_to_target_ir(&core, &facts());
    assert_eq!(report.status, TargetLoweringStatus::Unsupported);
    assert!(report.artifact.is_none());
    assert!(report
        .failures
        .iter()
        .any(|failure| failure.kind == TargetLoweringFailureKind::InvalidCoreIdentity));
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
fn target_rejects_append_capacity_widening_with_a_coherent_guard() {
    for (operand, output, item, capacity) in [
        ("List<U8,max=2>", "List<U8,max=3>", "7u8", 3),
        (
            "List<Bytes<max=1>,max=2>",
            "List<Bytes<max=2>,max=2>",
            "b\"x\"",
            2,
        ),
    ] {
        let source = format!("fn widen(xs: {operand}) -> {output} {{ return if listLen(xs) < 2u64 then listAppend(xs,{item}) else xs; }} {LENGTH_INTENT}");
        let mut core = compile(&source).unwrap();
        assert_eq!(
            lower_to_target_ir(&core, &facts()).status,
            TargetLoweringStatus::Lowered
        );
        let CoreExpr::If {
            predicate,
            then_value,
            ..
        } = &mut core.functions.get_mut("widen").unwrap().body.result
        else {
            panic!("guard")
        };
        let CorePredicate::Compare { left, right, .. } = predicate.as_mut() else {
            panic!("comparison")
        };
        let CoreExpr::Call { type_args, .. } = left else {
            panic!("length")
        };
        type_args[0] = output.into();
        *right = CoreExpr::Const(amblicit_syntax::CoreValue::Int {
            width: "U64".into(),
            value: capacity.to_string(),
        });
        let CoreExpr::Call { type_args, .. } = then_value.as_mut() else {
            panic!("append")
        };
        type_args[0] = output.into();
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

fn compile_with_steps(
    body: &str,
    steps: u64,
) -> Result<CoreModule, Vec<amblicit_syntax::CompilerError>> {
    compile_to_core(
        &parse_module(&format!("package work.lists@1; {body}")).expect("parse mixed specimen"),
        &CompilerContext::new()
            .with_operation_profile("p.read", "continuum.profile.read-only/v1")
            .with_budget(
                "p.large",
                CoreBudget {
                    max_steps: steps,
                    max_allocated_bytes: 1_000_000,
                    max_output_bytes: 1_000_000,
                },
            ),
    )
}

#[test]
fn list_work_includes_each_requirement_node() {
    let body = LENGTH_INTENT.replace("let n =", "require true else example.No; let n =");
    let mut ordered = facts();
    ordered.target_ir_domain = "echo.span-ir/v3".into();
    ordered.obstruction_coordinates.push("example.No".into());
    for (steps, accepted) in [(296, true), (295, false)] {
        let source = compile_with_steps(&body, steps);
        assert_eq!(source.is_ok(), accepted, "source {steps}: {source:?}");
        if let Err(errors) = source {
            assert!(errors
                .iter()
                .any(|e| e.kind == CompilerErrorKind::InvalidBound));
        }
        let mut core = compile(&body).unwrap();
        core.intents
            .get_mut("run")
            .unwrap()
            .core_evaluation_budget
            .max_steps = steps;
        let report = lower_to_target_ir(&core, &ordered);
        assert_eq!(
            report.status == TargetLoweringStatus::Lowered,
            accepted,
            "target {steps}: {report:?}"
        );
    }
}

#[test]
fn list_work_multiplies_requirements_inside_nested_loops() {
    for requirements in [
        "require true else example.No; require false else example.No;",
        "for item in input.xs bounded 2 { require true else example.No; }",
        "for a in input.xs bounded 2 { for b in input.xs bounded 2 { require true else example.No; } }",
        "if true { require true else example.No; } else { require false else example.No; }",
    ] {
        let body = LENGTH_INTENT.replace("let n =", &format!("{requirements} let n ="));
        compile(&body).expect("valid requirement control");
        let (mut low, mut high) = (0, 1_000_000);
        while low < high {
            let middle = low + (high - low) / 2;
            if compile_with_steps(&body, middle).is_ok() { high = middle; } else { low = middle + 1; }
        }
        let core = compile_with_steps(&body, high).unwrap();
        let mut ordered = facts();
        ordered.target_ir_domain = "echo.span-ir/v3".into();
        ordered.obstruction_coordinates.push("example.No".into());
        let report = lower_to_target_ir(&core, &ordered);
        assert_eq!(report.status, TargetLoweringStatus::Lowered,
            "{requirements}: source steps {high}: {report:?}");
    }
}

#[test]
fn bounded_for_list_work_includes_generated_frames() {
    let body = "type Input = { xs: List<U8,max=2> }; type Output = { n: U64 };
      intent run(input: Input) returns Output profile p.read basis none budget <= p.large {
        for item in input.xs bounded 2 { let x = item; }
        let n = listLen(input.xs); return { n };
      }";
    let mut ordered = facts();
    ordered.target_ir_domain = "echo.span-ir/v3".into();
    // Baseline 294/1350 + iterable 10/514 + two loop frames 7/192 each
    // + one enclosing node step. Each frame includes its binder and null result.
    for (steps, bytes, accepted) in [
        (319, 2248, true),
        (318, 2248, false),
        (319, 2247, false),
        (312, 2248, false),
    ] {
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
            &parse_module(&format!("package work.lists@1; {body}")).unwrap(),
            &context,
        );
        assert_eq!(
            authored.is_ok(),
            accepted,
            "source {steps}/{bytes}: {authored:?}"
        );
        let mut core = compile(body).unwrap();
        let budget = &mut core.intents.get_mut("run").unwrap().core_evaluation_budget;
        budget.max_steps = steps;
        budget.max_allocated_bytes = bytes;
        let report = lower_to_target_ir(&core, &ordered);
        assert_eq!(
            report.status == TargetLoweringStatus::Lowered,
            accepted,
            "target {steps}/{bytes}: {report:?}"
        );
    }
}

#[test]
fn bounded_for_list_work_covers_empty_nested_and_record_items() {
    for (input, loops) in [
        (
            "List<U8,max=0>",
            "for item in input.xs bounded 0 { let x = item; }",
        ),
        (
            "List<List<U8,max=2>,max=2>",
            "for batch in input.xs bounded 2 { for item in batch bounded 2 { let x = item; } }",
        ),
        (
            "List<Item,max=2>",
            "for item in input.xs bounded 2 { let x = item; }",
        ),
        (
            "List<Item,max=2>",
            "for item in input.xs bounded 2 { if input.flag { let x = item; } }",
        ),
    ] {
        let body = format!("type Item = {{ bytes: Bytes<max=4> }}; type Input = {{ xs: {input}, flag: Bool }}; type Output = {{ n: U64 }};
          intent run(input: Input) returns Output profile p.read basis none budget <= p.large {{ {loops} let n = listLen(input.xs); return {{ n }}; }}");
        compile(&body).expect("valid loop control");
        let (mut low, mut high) = (0, 1_000_000);
        while low < high {
            let middle = low + (high - low) / 2;
            if compile_with_steps(&body, middle).is_ok() {
                high = middle;
            } else {
                low = middle + 1;
            }
        }
        let core = compile_with_steps(&body, high).unwrap();
        let mut ordered = facts();
        ordered.target_ir_domain = "echo.span-ir/v3".into();
        let report = lower_to_target_ir(&core, &ordered);
        assert_eq!(
            report.status,
            TargetLoweringStatus::Lowered,
            "{input}, {loops}: source steps {high}: {report:?}"
        );
    }
}

#[test]
fn mixed_list_and_byte_length_recheck_exact_work() {
    let body = "type Input = { xs: List<U8,max=2>, bs: Bytes<max=100> }; type Output = { n: U64 };
      intent run(input: Input) returns Output profile p.read basis none budget <= p.large {
        let dummy = listLen(input.xs); let n = len(input.bs); return { n };
      }";
    // Frame 20 + list-length 274 + byte-length 240 + two lets 2 + result 6.
    // Byte length includes 64 result bytes plus 164 bounded operand bytes.
    for (steps, accepted) in [(542, true), (541, false), (314, false)] {
        let source = compile_with_steps(body, steps);
        assert_eq!(source.is_ok(), accepted, "source {steps}: {source:?}");
        if let Err(errors) = source {
            assert!(errors
                .iter()
                .any(|e| e.kind == CompilerErrorKind::InvalidBound));
        }
        let mut core = compile(body).unwrap();
        core.intents
            .get_mut("run")
            .unwrap()
            .core_evaluation_budget
            .max_steps = steps;
        let report = lower_to_target_ir(&core, &facts());
        assert_eq!(
            report.status == TargetLoweringStatus::Lowered,
            accepted,
            "target {steps}: {report:?}"
        );
    }
}

#[test]
fn mixed_list_and_byte_text_work_is_sufficient_for_target() {
    for operation in [
        "len(input.bs)",
        "slice(input.bs, 0u64, 0u64)",
        "isUtf8(input.bs)",
        "count(input.bs, b\"x\")",
        "if 0u64 < len(input.bs) then at(input.bs, 0u64) else 0u8",
        "input.bs + input.bs",
        "input.text + input.text",
    ] {
        for list_first in [true, false] {
            let list = "let n = listLen(input.xs);";
            let other = format!("let other = {operation};");
            let bindings = if list_first {
                format!("{list} {other}")
            } else {
                format!("{other} {list}")
            };
            let body = format!("type Input = {{ xs: List<U8,max=2>, bs: Bytes<max=100>, text: String<max=10> }}; type Output = {{ n: U64 }};
              intent run(input: Input) returns Output profile p.read basis none budget <= p.large {{ {bindings} return {{ n }}; }}");
            compile_with_steps(&body, 1_000_000).expect("valid mixed specimen");
            // Find the tight source-admitted budget, then ask the independent
            // native meter whether that budget actually suffices.
            let (mut low, mut high) = (0, 1_000_000);
            while low < high {
                let middle = low + (high - low) / 2;
                if compile_with_steps(&body, middle).is_ok() {
                    high = middle;
                } else {
                    low = middle + 1;
                }
            }
            let core = compile_with_steps(&body, high).unwrap();
            let report = lower_to_target_ir(&core, &facts());
            assert_eq!(
                report.status,
                TargetLoweringStatus::Lowered,
                "{operation}, list first={list_first}, source steps={high}: {report:?}"
            );
        }
    }
}

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

#[test]
fn target_rejects_concat_operand_coordinate_widening() {
    let mut accepted = Vec::new();
    for (operand, claimed, output) in [
        ("List<U8,max=2>", "List<U8,max=3>", "List<U8,max=6>"),
        (
            "List<Bytes<max=1>,max=2>",
            "List<Bytes<max=2>,max=2>",
            "List<Bytes<max=2>,max=4>",
        ),
    ] {
        let original = compile(&format!("fn join(a: {operand}, b: {operand}) -> {output} {{ return listConcat(a,b); }} {LENGTH_INTENT}")).unwrap();
        assert_eq!(
            lower_to_target_ir(&original, &facts()).status,
            TargetLoweringStatus::Lowered
        );
        for indices in [vec![0], vec![1], vec![0, 1]] {
            let mut core = original.clone();
            let CoreExpr::Call { type_args, .. } =
                &mut core.functions.get_mut("join").unwrap().body.result
            else {
                panic!("concat");
            };
            for index in &indices {
                type_args[*index] = claimed.into();
            }
            validate_core_module_type_integrity(&core).expect("well-formed forged coordinates");
            let report = lower_to_target_ir(&core, &facts());
            if report.status == TargetLoweringStatus::Lowered {
                accepted.push(format!("{operand} -> {claimed}, operands {indices:?}"));
                continue;
            }
            assert_eq!(report.status, TargetLoweringStatus::Unsupported);
            assert!(report.artifact.is_none());
            assert!(report
                .failures
                .iter()
                .any(|failure| failure.kind == TargetLoweringFailureKind::InvalidCoreIdentity));
        }
    }
    assert!(
        accepted.is_empty(),
        "accepted forged operand coordinates: {accepted:?}"
    );
}

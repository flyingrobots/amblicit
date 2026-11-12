//! `Option<T>` values, the closed Option prelude and the `isSome` refinement.
//!
//! Oracle: SPEC Edict Language v1, Standard Functions (`some`, `none`,
//! `default`, `isSome`, `unwrap`) and Option Refinement
//! (`EDICT-LANG-OPTION-REFINE-001`). The Core spelling `core.option.*` under
//! `core.option/v1` is the compiler's Option encoding, recorded in the
//! compiler-spine shelf.
use amblicit_syntax::{
    compile_to_core, digest_core_module, encode_core_module, lower_to_target_ir, parse_module,
    validate_core_module_type_integrity, CompilerContext, CompilerError, CompilerErrorKind,
    CoreBudget, CoreExpr, CoreModule, CoreNode, CorePredicate, CoreType,
    CoreTypeIntegrityFailureKind, ResourceRef, TargetIrLoweringFacts, TargetLoweringFailureKind,
    TargetLoweringStatus, ECHO_DPO_TARGET_PROFILE, ECHO_SPAN_IR_DOMAIN,
    ECHO_STRUCTURED_SPAN_IR_DOMAIN,
};

fn context() -> CompilerContext {
    CompilerContext::new()
        .with_operation_profile("p.read", "continuum.profile.read-only/v1")
        .with_budget(
            "p.large",
            CoreBudget {
                max_steps: 1_000_000,
                max_allocated_bytes: 1_000_000,
                max_output_bytes: 1_000_000,
            },
        )
}

fn compile(body: &str) -> Result<CoreModule, Vec<CompilerError>> {
    let source = format!("package jedit.options@1; {body}");
    compile_to_core(&parse_module(&source).expect("valid syntax"), &context())
}

fn facts() -> TargetIrLoweringFacts {
    TargetIrLoweringFacts {
        target_profile: ResourceRef {
            coordinate: ECHO_DPO_TARGET_PROFILE.to_owned(),
            digest: Some(format!("sha256:{}", "1".repeat(64))),
        },
        target_ir_domain: ECHO_SPAN_IR_DOMAIN.to_owned(),
        operation_profiles: vec!["continuum.profile.read-only/v1".to_owned()],
        obstruction_coordinates: Vec::new(),
        effect_lowerings: Vec::new(),
        effect_signatures: Vec::new(),
        pure_functions: Vec::new(),
    }
}

/// A scalar intent gives Target a step to lower beside the helpers under test.
const WITNESS: &str = "type Input = { marker: U8 }; type Output = { marker: U8 }; intent witness(input: Input) returns Output profile p.read basis none budget <= p.large { let marker = input.marker; return { marker }; }";

const EXPANSION: &str = "
type Visit = { lo: U64, hi: U64 };
type Expansion = { left: Option<Visit>, right: Option<Visit> };
fn both(v: Visit) -> Expansion { return { left: some(v), right: none() }; }
fn present(e: Expansion) -> Bool { return isSome(e.left); }
fn fallback(o: Option<U64>) -> U64 { return default(o, 0u64); }
fn explicit() -> Option<U64> { return none<U64>(); }
fn typed(v: U64) -> Option<U64> { return some<U64>(v); }
fn guarded(o: Option<U64>) -> U64 { return if isSome(o) then unwrap(o) else 0u64; }
";

fn call(expr: &CoreExpr) -> (&str, &[String], &[CoreExpr]) {
    let CoreExpr::Call {
        callee,
        type_args,
        args,
    } = expr
    else {
        panic!("typed Option call expected, got {expr:?}")
    };
    (callee, type_args, args)
}

fn lowered(core: &CoreModule) {
    lowered_with(core, &facts());
}

/// Statement and branch-yield branches need the structured Target domain.
fn structured_facts() -> TargetIrLoweringFacts {
    TargetIrLoweringFacts {
        target_ir_domain: ECHO_STRUCTURED_SPAN_IR_DOMAIN.to_owned(),
        ..facts()
    }
}

fn lowered_with(core: &CoreModule, facts: &TargetIrLoweringFacts) {
    let report = lower_to_target_ir(core, facts);
    assert_eq!(report.status, TargetLoweringStatus::Lowered, "{report:?}");
    assert!(report.artifact.is_some());
}

fn refused_by_target(core: &CoreModule, label: &str) {
    refused_with(core, &facts(), label);
}

fn refused_with(core: &CoreModule, facts: &TargetIrLoweringFacts, label: &str) {
    let report = lower_to_target_ir(core, facts);
    assert_eq!(
        report.status,
        TargetLoweringStatus::Unsupported,
        "{label}: {report:?}"
    );
    assert!(report.artifact.is_none(), "{label}");
    assert!(
        report
            .failures
            .iter()
            .any(|failure| failure.kind == TargetLoweringFailureKind::InvalidCoreIdentity),
        "{label}: {report:?}"
    );
}

#[test]
fn option_prelude_calls_lower_to_typed_core_option_calls() {
    let core = compile(&format!("{EXPANSION} {WITNESS}")).expect("Option prelude compiles");
    assert!(core
        .required_core_capabilities
        .iter()
        .any(|capability| capability == "core.option/v1"));
    let CoreType::Record { fields } = &core.types["Expansion"] else {
        panic!("record")
    };
    assert_eq!(fields["left"], "Option<jedit.options@1.Visit>");

    let CoreExpr::Record { fields } = &core.functions["both"].body.result else {
        panic!("record literal")
    };
    let (callee, type_args, args) = call(&fields["left"]);
    assert_eq!(callee, "core.option.some");
    assert_eq!(type_args, ["jedit.options@1.Visit"]);
    assert!(matches!(args, [CoreExpr::Local { .. }]));
    let (callee, type_args, args) = call(&fields["right"]);
    assert_eq!(callee, "core.option.none");
    assert_eq!(type_args, ["jedit.options@1.Visit"]);
    assert!(matches!(args, []), "{args:?}");

    let (callee, type_args, args) = call(&core.functions["present"].body.result);
    assert_eq!(
        (callee, type_args),
        (
            "core.option.isSome",
            &["jedit.options@1.Visit".to_owned()][..]
        )
    );
    assert!(matches!(args, [CoreExpr::Field { .. }]));

    let (callee, type_args, args) = call(&core.functions["fallback"].body.result);
    assert_eq!(
        (callee, type_args),
        ("core.option.default", &["U64".to_owned()][..])
    );
    assert_eq!(args.len(), 2);

    let (callee, type_args, _) = call(&core.functions["explicit"].body.result);
    assert_eq!(
        (callee, type_args),
        ("core.option.none", &["U64".to_owned()][..])
    );
    let (callee, type_args, _) = call(&core.functions["typed"].body.result);
    assert_eq!(
        (callee, type_args),
        ("core.option.some", &["U64".to_owned()][..])
    );

    let CoreExpr::If {
        predicate,
        then_value,
        ..
    } = &core.functions["guarded"].body.result
    else {
        panic!("guard")
    };
    let CorePredicate::Compare { left, .. } = predicate.as_ref() else {
        panic!("isSome predicate")
    };
    assert_eq!(call(left).0, "core.option.isSome");
    let (callee, type_args, args) = call(then_value);
    assert_eq!(
        (callee, type_args),
        ("core.option.unwrap", &["U64".to_owned()][..])
    );
    assert!(matches!(args, [CoreExpr::Local { .. }]));
    lowered(&core);
}

#[test]
fn option_identity_is_repeatable_and_capability_bound() {
    let source = format!("{EXPANSION} {WITNESS}");
    let core = compile(&source).unwrap();
    assert_eq!(
        encode_core_module(&core).unwrap(),
        encode_core_module(&compile(&source).unwrap()).unwrap()
    );
    let mut changed = core.clone();
    let CoreExpr::Call { type_args, .. } =
        &mut changed.functions.get_mut("explicit").unwrap().body.result
    else {
        panic!("none call")
    };
    type_args[0] = "U32".into();
    changed.functions.get_mut("explicit").unwrap().return_type = "Option<U32>".into();
    assert_ne!(
        digest_core_module(&core).unwrap(),
        digest_core_module(&changed).unwrap()
    );
    let mut stripped = core;
    stripped
        .required_core_capabilities
        .retain(|capability| capability != "core.option/v1");
    assert_eq!(
        validate_core_module_type_integrity(&stripped)
            .expect_err("Option calls need their capability")
            .kind(),
        CoreTypeIntegrityFailureKind::InvalidDefinition
    );
}

#[test]
fn none_takes_its_item_type_from_a_typed_conditional_peer() {
    for arms in ["none() else some(v)", "some(v) else none()"] {
        let core = compile(&format!(
            "fn pick(flag: Bool, v: U64) -> U64 {{ let x = if flag then {arms}; return default(x, 0u64); }} {WITNESS}"
        ))
        .expect(arms);
        let function = &core.functions["pick"];
        let CoreExpr::If {
            then_value,
            else_value,
            ..
        } = &function.body.bindings[0].value
        else {
            panic!("conditional binding")
        };
        for value in [then_value, else_value] {
            assert_eq!(call(value).1, ["U64"], "{arms}");
        }
        lowered(&core);
    }
}

#[test]
fn option_typing_refuses_mismatches_and_untyped_none() {
    use CompilerErrorKind as K;
    for (body, kind) in [
        (
            "fn f() -> Option<U64> { return some(true); }",
            K::TypeMismatch,
        ),
        (
            "fn f() -> Option<U64> { return some<U64>(true); }",
            K::TypeMismatch,
        ),
        (
            "fn f() -> Option<U64> { return some<U32>(1u32); }",
            K::TypeMismatch,
        ),
        (
            "fn f(o: Option<U64>) -> U64 { return default(o, true); }",
            K::TypeMismatch,
        ),
        (
            "fn f(v: U64) -> Bool { return isSome(v); }",
            K::TypeMismatch,
        ),
        (
            "fn f(v: U64) -> U64 { return default(v, 1u64); }",
            K::TypeMismatch,
        ),
        (
            "fn f(o: Option<U64>) -> Option<U32> { return o; }",
            K::TypeMismatch,
        ),
        (
            "fn f() -> U64 { let n = none(); return 0u64; }",
            K::UnsupportedSourceShape,
        ),
        (
            "fn f() -> Option<U64> { return some(); }",
            K::UnsupportedSourceShape,
        ),
        (
            "fn f(o: Option<U64>) -> Bool { return isSome(o, o); }",
            K::UnsupportedSourceShape,
        ),
        (
            "fn f(o: Option<U64>) -> Bool { return isSome<U64>(o); }",
            K::UnsupportedSourceShape,
        ),
        (
            "fn f(o: Option<U64>) -> U64 { return default(o); }",
            K::UnsupportedSourceShape,
        ),
        // The spec names no Option case labels, so Option `match` is refused.
        (
            "fn f(o: Option<U64>) -> U64 { return match o { Some(v) => v, None => 0u64 }; }",
            K::TypeMismatch,
        ),
    ] {
        let errors = compile(body).expect_err(body);
        assert!(
            errors.iter().any(|error| error.kind == kind),
            "{body}: expected {kind:?}, got {errors:?}"
        );
    }
}

#[test]
fn option_prelude_names_cannot_be_redeclared() {
    for name in ["isSome", "unwrap", "default"] {
        let errors = compile(&format!("fn {name}(x: U64) -> U64 {{ return x; }}")).expect_err(name);
        assert!(
            errors
                .iter()
                .any(|error| error.kind == CompilerErrorKind::SurfaceValidation),
            "{name}: {errors:?}"
        );
    }
}

#[test]
fn unwrap_compiles_in_every_refined_region() {
    for body in [
        "fn f(o: Option<U64>) -> U64 { return if isSome(o) then unwrap(o) else 0u64; }",
        "fn f(o: Option<U64>) -> Bool { return isSome(o) && unwrap(o) > 3u64; }",
        "fn f(o: Option<U64>, p: Option<U64>) -> U64 { return if isSome(o) && isSome(p) then unwrap(p) else 0u64; }",
        "fn f(o: Option<U64>, p: Option<U64>) -> Bool { return isSome(o) && isSome(p) && unwrap(o) < unwrap(p); }",
        "fn f(o: Option<U64>) -> U64 { return if isSome(o) then if unwrap(o) > 3u64 then unwrap(o) else 1u64 else 0u64; }",
    ] {
        let core = compile(&format!("{body} {WITNESS}")).expect(body);
        lowered(&core);
    }
}

const OPTIONAL_INPUT: &str = "type Input = { value: Option<U64> }; type Output = { value: U64 };";

#[test]
fn unwrap_compiles_in_statement_and_branch_yield_then_blocks() {
    for (body, target) in [
        ("let o = input.value; let v = if isSome(o) { yield unwrap(o); } else { yield 0u64; }; return { value: v };", false),
        ("let o = input.value; if isSome(o) { let inner = unwrap(o); } return { value: 0u64 };", true),
    ] {
        let source = format!("{OPTIONAL_INPUT} intent run(input: Input) returns Output profile p.read basis none budget <= p.large {{ {body} }}");
        let core = compile(&source).expect(body);
        assert!(core.intents["run"]
            .body
            .nodes
            .iter()
            .any(|node| matches!(node, CoreNode::Branch { .. })));
        // Native Target refuses every branch-yield binding (`branch_binding`),
        // independently of Options; only the statement form can lower.
        if target {
            lowered_with(&core, &structured_facts());
        }
    }
}

#[test]
fn unwrap_refuses_outside_refined_regions() {
    for body in [
        "fn f(o: Option<U64>) -> U64 { return unwrap(o); }",
        "fn f(o: Option<U64>) -> U64 { return if isSome(o) then 0u64 else unwrap(o); }",
        "fn f(o: Option<U64>, p: Option<U64>) -> U64 { return if isSome(p) then unwrap(o) else 0u64; }",
        "fn f(o: Option<U64>) -> U64 { let q = o; return if isSome(o) then unwrap(q) else 0u64; }",
        "fn f(o: Option<U64>) -> Bool { return isSome(o) || unwrap(o) > 3u64; }",
        "fn f(o: Option<U64>) -> Bool { return !isSome(o) && unwrap(o) > 3u64; }",
        "fn f(o: Option<U64>) -> U64 { return if !isSome(o) then 0u64 else unwrap(o); }",
        "fn f(o: Option<U64>) -> U64 { return if isSome(o) || false then unwrap(o) else 0u64; }",
        "type Box = { inner: Option<U64> }; fn f(b: Box) -> U64 { return if isSome(b.inner) then unwrap(b.inner) else 0u64; }",
        "fn get(o: Option<U64>) -> U64 { return unwrap(o); } fn f(o: Option<U64>) -> U64 { return if isSome(o) then get(o) else 0u64; }",
    ] {
        let errors = compile(body).expect_err(body);
        assert!(
            errors
                .iter()
                .any(|error| error.kind == CompilerErrorKind::UnsupportedSourceShape),
            "{body}: {errors:?}"
        );
    }
    for body in [
        "let o = input.value; if isSome(o) { let inner = 1u64; } let v = unwrap(o); return { value: v };",
        "let o = input.value; if isSome(o) { let inner = 1u64; } else { let inner = unwrap(o); } return { value: 0u64 };",
        "let o = input.value; let v = if isSome(o) { yield 0u64; } else { yield unwrap(o); }; return { value: v };",
    ] {
        let source = format!("{OPTIONAL_INPUT} intent run(input: Input) returns Output profile p.read basis none budget <= p.large {{ {body} }}");
        let errors = compile(&source).expect_err(body);
        assert!(
            errors
                .iter()
                .any(|error| error.kind == CompilerErrorKind::UnsupportedSourceShape),
            "{body}: {errors:?}"
        );
    }
}

fn guarded_core() -> CoreModule {
    compile(&format!(
        "fn guarded(o: Option<U64>, p: Option<U64>) -> U64 {{ return if isSome(o) then unwrap(o) else default(p, 1u64); }} {WITNESS}"
    ))
    .expect("guarded unwrap")
}

#[test]
fn target_rechecks_unwrap_presence_on_the_exact_local() {
    let original = guarded_core();
    lowered(&original);
    for mutation in 0..4 {
        let mut core = original.clone();
        let CoreExpr::If {
            predicate,
            then_value,
            else_value,
        } = &mut core.functions.get_mut("guarded").unwrap().body.result
        else {
            panic!("guard")
        };
        match mutation {
            // Remove the guard.
            0 => **predicate = CorePredicate::True,
            // Guard the other local.
            1 => {
                let CorePredicate::Compare { left, .. } = predicate.as_mut() else {
                    panic!("compare")
                };
                let CoreExpr::Call { args, .. } = left else {
                    panic!("isSome")
                };
                let CoreExpr::Call { args: fallback, .. } = else_value.as_ref() else {
                    panic!("default")
                };
                args[0] = fallback[0].clone();
            }
            // Move unwrap to the unguarded arm.
            2 => std::mem::swap(then_value, else_value),
            // Unwrap a non-local operand.
            3 => {
                let CoreExpr::Call { args, .. } = then_value.as_mut() else {
                    panic!("unwrap")
                };
                args[0] = CoreExpr::Call {
                    callee: "core.option.some".into(),
                    type_args: vec!["U64".into()],
                    args: vec![CoreExpr::Const(amblicit_syntax::CoreValue::Int {
                        width: "U64".into(),
                        value: "1".into(),
                    })],
                };
            }
            _ => unreachable!(),
        }
        refused_by_target(&core, &format!("mutation {mutation}"));
    }
}

/// Target reads presence through `!=`, negation and disjunction itself; it
/// does not rely on the compiler emitting only `isSome(x) == true`.
#[test]
fn target_presence_follows_negated_and_disjunctive_guards() {
    use amblicit_syntax::{CompareOp, CoreValue};
    let original = guarded_core();
    let CoreExpr::If { predicate, .. } = &original.functions["guarded"].body.result else {
        panic!("guard")
    };
    let CorePredicate::Compare { left: probe, .. } = predicate.as_ref() else {
        panic!("isSome comparison")
    };
    let test = |op, value| CorePredicate::Compare {
        op,
        left: probe.clone(),
        right: CoreExpr::Const(CoreValue::Bool(value)),
    };
    let not = |inner| CorePredicate::Not(Box::new(inner));
    let guarded = |guard: CorePredicate| {
        let mut core = original.clone();
        let CoreExpr::If { predicate, .. } =
            &mut core.functions.get_mut("guarded").unwrap().body.result
        else {
            panic!("guard")
        };
        **predicate = guard;
        core
    };
    for (label, guard) in [
        ("isSome != false", test(CompareOp::Ne, false)),
        ("!(isSome == false)", not(test(CompareOp::Eq, false))),
        (
            "!(isSome == false || false)",
            not(CorePredicate::Any(vec![
                test(CompareOp::Eq, false),
                CorePredicate::False,
            ])),
        ),
    ] {
        let report = lower_to_target_ir(&guarded(guard), &facts());
        assert_eq!(
            report.status,
            TargetLoweringStatus::Lowered,
            "{label}: {report:?}"
        );
    }
    for (label, guard) in [
        ("isSome != true", test(CompareOp::Ne, true)),
        ("isSome == false", test(CompareOp::Eq, false)),
        (
            "isSome == true || true",
            CorePredicate::Any(vec![test(CompareOp::Eq, true), CorePredicate::True]),
        ),
        (
            "!(isSome == true && true)",
            not(CorePredicate::All(vec![
                test(CompareOp::Eq, true),
                CorePredicate::True,
            ])),
        ),
    ] {
        refused_by_target(&guarded(guard), label);
    }
}

#[test]
fn target_rechecks_option_signatures_and_capability() {
    let original = compile(&format!("{EXPANSION} {WITNESS}")).unwrap();
    lowered(&original);
    for mutation in 0..6 {
        let mut core = original.clone();
        match mutation {
            0 => {
                core.required_core_capabilities
                    .retain(|capability| capability != "core.option/v1");
            }
            1 => {
                let CoreExpr::Call { type_args, .. } =
                    &mut core.functions.get_mut("typed").unwrap().body.result
                else {
                    panic!("some")
                };
                type_args[0] = "U32".into();
            }
            2 => {
                let CoreExpr::Call { args, .. } =
                    &mut core.functions.get_mut("fallback").unwrap().body.result
                else {
                    panic!("default")
                };
                args[1] = CoreExpr::Const(amblicit_syntax::CoreValue::Bool(true));
            }
            3 => {
                let CoreExpr::Call { args, .. } =
                    &mut core.functions.get_mut("fallback").unwrap().body.result
                else {
                    panic!("default")
                };
                args.pop();
            }
            4 => {
                let CoreExpr::Call { args, .. } =
                    &mut core.functions.get_mut("present").unwrap().body.result
                else {
                    panic!("isSome")
                };
                args[0] = CoreExpr::Const(amblicit_syntax::CoreValue::Int {
                    width: "U64".into(),
                    value: "1".into(),
                });
            }
            // A payload that does not fit the declared item type.
            5 => {
                let CoreExpr::Call { args, .. } =
                    &mut core.functions.get_mut("typed").unwrap().body.result
                else {
                    panic!("some")
                };
                args[0] = CoreExpr::Const(amblicit_syntax::CoreValue::Bool(true));
            }
            _ => unreachable!(),
        }
        refused_by_target(&core, &format!("mutation {mutation}"));
    }
}

#[test]
fn target_honours_statement_branch_presence_only_in_the_then_block() {
    let source = format!("{OPTIONAL_INPUT} intent run(input: Input) returns Output profile p.read basis none budget <= p.large {{ let o = input.value; if isSome(o) {{ let inner = unwrap(o); }} return {{ value: 0u64 }}; }}");
    let original = compile(&source).unwrap();
    lowered_with(&original, &structured_facts());
    let mut core = original;
    let intent = core.intents.get_mut("run").unwrap();
    let CoreNode::Branch {
        then_block,
        else_block,
        ..
    } = intent
        .body
        .nodes
        .iter_mut()
        .find(|node| matches!(node, CoreNode::Branch { .. }))
        .unwrap()
    else {
        unreachable!()
    };
    std::mem::swap(then_block, else_block);
    refused_with(&core, &structured_facts(), "unwrap moved to the else block");
}

/// The root-presence shape of Jedit's `ReplaceRange` draft (`rootId`).
#[test]
fn draft_shaped_root_presence_compiles_and_lowers() {
    let source = "type Input = { byteLength: U64, id: U64 }; type Edge = { edgeType: U64, to: U64 }; type Output = { root: U64, edges: List<Edge, max=2> };
intent run(input: Input) returns Output profile p.read basis none budget <= p.large {
  let rootId = if input.byteLength == 0u64 then none() else some(input.id);
  let anchorEdge = { edgeType: 1u64, to: 2u64 };
  let headEdges: List<Edge, max=2> = if isSome(rootId) then [{ edgeType: 3u64, to: unwrap(rootId) }, anchorEdge] else [anchorEdge];
  if isSome(rootId) { let root = unwrap(rootId); }
  return { root: default(rootId, 0u64), edges: headEdges };
}";
    let core = compile(source).expect("draft-shaped Option use compiles");
    assert!(core
        .required_core_capabilities
        .iter()
        .any(|capability| capability == "core.option/v1"));
    lowered_with(&core, &structured_facts());
}

#[test]
fn where_clauses_do_not_refine_and_option_results_do_not_project() {
    for source in [
        "type Output = { value: U64 }; intent run(input: Option<U64>) returns Output profile p.read basis none budget <= p.large where isSome(input) { return { value: unwrap(input) }; }",
        "type Input = { value: U64 }; type Output = { value: Option<U64> }; intent run(input: Input) returns Output profile p.read basis none budget <= p.large { return { value: some(input.value) }; }",
    ] {
        let errors = compile(source).expect_err(source);
        assert!(
            errors
                .iter()
                .any(|error| error.kind == CompilerErrorKind::UnsupportedSourceShape),
            "{source}: {errors:?}"
        );
    }
}

const OPTION_FOLD: &str = "
type State = { best: Option<U64>, seen: U64 };
fn step(s: State) -> State { let b = s.best; return { best: if isSome(b) then some(unwrap(b)) else some(1u64), seen: s.seen }; }
fn run() -> State { let init: State = { best: none(), seen: 0u64 }; return fold(init, step, 3u64); }
type Input = { marker: U8 }; type Output = { seen: U64 };
intent witness(input: Input) returns Output profile p.read basis none budget <= p.large { let state = run(); let seen = state.seen; return { seen }; }";

#[test]
fn option_fold_state_is_metered_by_target() {
    let core = compile(OPTION_FOLD).expect("fold over Option state compiles");
    lowered(&core);
    let mut starved = core;
    starved
        .intents
        .get_mut("witness")
        .unwrap()
        .core_evaluation_budget
        .max_steps = 1;
    refused_by_target(&starved, "one-step budget");
}

#[test]
fn recursive_types_through_option_remain_rejected() {
    for body in [
        "type Tree = variant { Leaf, Node(Option<Tree>) };",
        "type Tree = variant { Leaf, Node(Branch) }; type Branch = { left: Option<Tree>, right: Option<Tree> };",
        "type Chain = { next: Option<Chain> };",
    ] {
        let errors = compile(body).expect_err(body);
        assert!(
            errors
                .iter()
                .any(|error| error.kind == CompilerErrorKind::InvalidCoreTypeIntegrity),
            "{body}: {errors:?}"
        );
    }
}

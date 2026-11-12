//! Forged-Core and call-position witnesses for function preconditions
//! (EDICT-TARGET-FN-PRECONDITION-001). Each case starts from Core the
//! compiler accepted, changes one thing, and expects Target to refuse.
//! First written as probes by safety-review round 2 on PR #318.
use amblicit_syntax::{
    compile_to_core, lower_to_target_ir, parse_module, CompareOp, CompilerContext, CompilerError,
    CompilerErrorKind, CoreBudget, CoreExpr, CoreModule, CoreNode, CorePredicate, CoreValue,
    InputConstraint, InputConstraintSource, ResourceRef, TargetIrLoweringFacts,
    TargetLoweringFailureKind, TargetLoweringReport, TargetLoweringStatus,
};

const SPAN: &str = "fn span(start: U64, end: U64) -> U64 where start <= end {
  return end - start;
}";
const FIRST: &str = "fn first(start: U64, end: U64) -> U64 where start <= end { return start; }";
const PLAIN_FIRST: &str = "fn first(start: U64, end: U64) -> U64 { return start; }";
const ORDERED: &str =
    "fn ordered(start: U64, end: U64) -> Bool where start <= end { return end - start <= 4u64; }";

fn context() -> CompilerContext {
    let budget = CoreBudget {
        max_steps: 4096,
        max_allocated_bytes: 1 << 20,
        max_output_bytes: 1 << 20,
    };
    CompilerContext::new()
        .with_operation_profile("p.read", "continuum.profile.read-only/v1")
        .with_budget("p.small", budget)
}

fn program(functions: &str, guard: &str, value_type: &str, value: &str) -> String {
    format!(
        "package preconditions.example@1;
{functions}
type Input = {{ bytes: Bytes<max=8>, start: U64, end: U64, }};
type Output = {{ value: {value_type}, }};
intent evaluate(input: Input) returns Output profile p.read basis none budget <= p.small {guard} {{
  let value: {value_type} = {value};
  return {{ value }};
}}"
    )
}

fn compile(source: &str) -> Result<CoreModule, Vec<CompilerError>> {
    compile_to_core(&parse_module(source).expect("source parses"), &context())
}

fn accepted(source: &str) -> CoreModule {
    compile(source).unwrap_or_else(|errors| panic!("expected acceptance: {errors:?}"))
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

fn lower(core: &CoreModule) -> TargetLoweringReport {
    lower_to_target_ir(core, &facts())
}

fn assert_lowers(core: &CoreModule) {
    let report = lower(core);
    assert_eq!(report.status, TargetLoweringStatus::Lowered, "{report:?}");
}

fn assert_refused(core: &CoreModule) {
    let report = lower(core);
    assert_eq!(
        report.failures.first().map(|failure| failure.kind),
        Some(TargetLoweringFailureKind::InvalidCoreIdentity),
        "BYPASS: {report:?}"
    );
}

/// Pre-order rewrite; stops at the first node for which `f` returns true.
fn rewrite(e: &mut CoreExpr, f: &mut impl FnMut(&mut CoreExpr) -> bool) -> bool {
    if f(e) {
        return true;
    }
    match e {
        CoreExpr::Local { .. } | CoreExpr::Const(_) => false,
        CoreExpr::Variant { payload, .. } => payload.as_deref_mut().is_some_and(|p| rewrite(p, f)),
        CoreExpr::Fold { initial, .. } => rewrite(initial, f),
        CoreExpr::Match { scrutinee, arms } => {
            rewrite(scrutinee, f) || arms.iter_mut().any(|arm| rewrite(&mut arm.body, f))
        }
        CoreExpr::Record { fields } => fields.values_mut().any(|v| rewrite(v, f)),
        CoreExpr::List { values } => values.iter_mut().any(|v| rewrite(v, f)),
        CoreExpr::Field { base, .. } => rewrite(base, f),
        CoreExpr::Call { args, .. } => args.iter_mut().any(|v| rewrite(v, f)),
        CoreExpr::If {
            then_value,
            else_value,
            ..
        } => rewrite(then_value, f) || rewrite(else_value, f),
    }
}

fn body_value(core: &mut CoreModule) -> &mut CoreExpr {
    let intent = core.intents.get_mut("evaluate").unwrap();
    match &mut intent.body.nodes[0] {
        CoreNode::Let { value, .. } => value,
        other => panic!("unexpected first node {other:?}"),
    }
}

fn find_call(e: &CoreExpr, suffix: &str) -> Option<CoreExpr> {
    let mut found = None;
    let mut probe = e.clone();
    rewrite(&mut probe, &mut |node| {
        if matches!(node, CoreExpr::Call { callee, .. } if callee.ends_with(suffix)) {
            found = Some(node.clone());
            true
        } else {
            false
        }
    });
    found
}

fn u64_lit(v: &str) -> CoreExpr {
    CoreExpr::Const(CoreValue::Int {
        width: "U64".into(),
        value: v.into(),
    })
}

// simultaneous substitution. Caller function swaps its own params into
// the callee; both use arg.N ids, so a sequential substitution would capture.
#[test]
fn swapped_parameter_arguments_do_not_capture() {
    let nested = format!(
        "{SPAN}\nfn width(start: U64, end: U64) -> U64 where start <= end {{ return span(start, end); }}"
    );
    let mut core = accepted(&program(
        &nested,
        "where input.start <= input.end",
        "U64",
        "width(input.start, input.end)",
    ));
    assert_lowers(&core);
    let width = core.functions.get_mut("width").unwrap();
    let swapped = rewrite(&mut width.body.result, &mut |node| {
        if let CoreExpr::Call { callee, args, .. } = node {
            if callee == "preconditions.example@1.span" {
                args.swap(0, 1);
                return true;
            }
        }
        false
    });
    assert!(swapped, "found the span call in width");
    assert_refused(&core);
}

// a call moved into the basis sees no intent `where` facts.
#[test]
fn a_call_in_the_basis_sees_no_where_facts() {
    for (function, expect_lowered) in [(PLAIN_FIRST, true), (FIRST, false)] {
        let mut core = accepted(&program(
            function,
            "where input.start <= input.end",
            "U64",
            "first(input.start, input.end)",
        ));
        let call = find_call(body_value(&mut core), ".first").unwrap();
        core.intents.get_mut("evaluate").unwrap().basis = Some(call);
        let report = lower(&core);
        if expect_lowered {
            assert_eq!(
                report.status,
                TargetLoweringStatus::Lowered,
                "control: {report:?}"
            );
        } else {
            assert_eq!(
                report.failures.first().map(|failure| failure.kind),
                Some(TargetLoweringFailureKind::InvalidCoreIdentity),
                "BYPASS: {report:?}"
            );
        }
    }
}

// separate input constraints cannot justify one another.
#[test]
fn separate_input_constraints_do_not_justify_each_other() {
    let mut core = accepted(&program(
        ORDERED,
        "where input.start <= input.end && ordered(input.start, input.end)",
        "U64",
        "input.start",
    ));
    assert_lowers(&core);
    let intent = core.intents.get_mut("evaluate").unwrap();
    let CorePredicate::All(items) = intent.input_constraints[0].predicate.clone() else {
        panic!("expected All")
    };
    intent.input_constraints = items
        .into_iter()
        .enumerate()
        .map(|(index, predicate)| InputConstraint {
            coordinate: format!("where.{index}"),
            source: InputConstraintSource::Where,
            predicate,
        })
        .collect();
    assert_refused(&core);
}

// the same split, as `||`: the right operand sees only !(left).
#[test]
fn a_disjunction_does_not_give_an_order_fact() {
    let mut core = accepted(&program(
        ORDERED,
        "where input.start <= input.end && ordered(input.start, input.end)",
        "U64",
        "input.start",
    ));
    let intent = core.intents.get_mut("evaluate").unwrap();
    let CorePredicate::All(items) = intent.input_constraints[0].predicate.clone() else {
        panic!("expected All")
    };
    intent.input_constraints[0].predicate = CorePredicate::Any(items);
    assert_refused(&core);
    let error = compile(&program(
        ORDERED,
        "where input.end < input.start || ordered(input.start, input.end)",
        "U64",
        "input.start",
    ))
    .unwrap_err();
    assert_eq!(
        error[0].kind,
        CompilerErrorKind::UnprovenPrecondition,
        "{error:?}"
    );
}

// the else arm sees the negation, which is never an order.
#[test]
fn a_call_moved_to_the_else_arm_is_refused() {
    let mut core = accepted(&program(
        SPAN,
        "",
        "U64",
        "if input.start <= input.end then span(input.start, input.end) else 0u64",
    ));
    assert_lowers(&core);
    let value = body_value(&mut core);
    let CoreExpr::If {
        then_value,
        else_value,
        ..
    } = value
    else {
        panic!("expected If")
    };
    std::mem::swap(then_value, else_value);
    assert_refused(&core);
}

// comparison kinds in the dominating fact.
#[test]
fn only_order_implying_facts_prove_an_order() {
    for (op, flip, expect_lowered) in [
        (CompareOp::Le, false, true),
        (CompareOp::Lt, false, true),
        (CompareOp::Eq, false, true),
        (CompareOp::Eq, true, true),
        (CompareOp::Ge, true, true),
        (CompareOp::Gt, true, true),
        (CompareOp::Ne, false, false),
        (CompareOp::Ge, false, false),
        (CompareOp::Gt, false, false),
        (CompareOp::Le, true, false),
        (CompareOp::Lt, true, false),
    ] {
        let mut core = accepted(&program(
            FIRST,
            "where input.start <= input.end",
            "U64",
            "first(input.start, input.end)",
        ));
        let intent = core.intents.get_mut("evaluate").unwrap();
        let CorePredicate::Compare { left, right, .. } =
            intent.input_constraints[0].predicate.clone()
        else {
            panic!("compare")
        };
        let (left, right) = if flip { (right, left) } else { (left, right) };
        intent.input_constraints[0].predicate = CorePredicate::Compare { op, left, right };
        let report = lower(&core);
        assert_eq!(
            report.status == TargetLoweringStatus::Lowered,
            expect_lowered,
            "op={op:?} flip={flip}: {report:?}"
        );
    }
}

// literal order rule.
#[test]
fn literal_arguments_follow_the_literal_order_rule() {
    for (a, b, expect_lowered) in [("3", "5", true), ("5", "5", true), ("5", "3", false)] {
        let mut core = accepted(&program(FIRST, "", "U64", "first(0u64, input.end)"));
        let value = body_value(&mut core);
        let CoreExpr::Call { args, .. } = value else {
            panic!("call")
        };
        args[0] = u64_lit(a);
        args[1] = u64_lit(b);
        let report = lower(&core);
        assert_eq!(
            report.status == TargetLoweringStatus::Lowered,
            expect_lowered,
            "{a} <= {b}: {report:?}"
        );
    }
    // A U64 literal above u64::MAX is not a literal for the order proof.
    let mut core = accepted(&program(FIRST, "", "U64", "first(0u64, input.end)"));
    let CoreExpr::Call { args, .. } = body_value(&mut core) else {
        panic!("call")
    };
    args[0] = u64_lit("18446744073709551616");
    args[1] = u64_lit("5");
    assert_refused(&core);
}

// drop a caller function's precondition; its body call becomes unproven.
#[test]
fn dropping_a_calling_functions_precondition_is_refused() {
    let nested = format!(
        "{SPAN}\nfn width(start: U64, end: U64) -> U64 where start <= end {{ return span(start, end); }}"
    );
    let mut core = accepted(&program(
        &nested,
        "where input.start <= input.end",
        "U64",
        "width(input.start, input.end)",
    ));
    core.functions
        .get_mut("width")
        .unwrap()
        .preconditions
        .clear();
    assert_refused(&core);
}

// precondition order matters for totality; drop or reverse refuses.
#[test]
fn each_precondition_is_total_only_given_the_ones_before_it() {
    let gap = "fn gap(start: U64, end: U64) -> U64 where start <= end, end - start <= 4u64 { return start; }";
    let base = accepted(&program(
        gap,
        "where input.start <= input.end && input.end - input.start <= 4u64",
        "U64",
        "gap(input.start, input.end)",
    ));
    assert_lowers(&base);
    let mut reversed = base.clone();
    reversed
        .functions
        .get_mut("gap")
        .unwrap()
        .preconditions
        .reverse();
    assert_refused(&reversed);
    let mut dropped = base.clone();
    dropped
        .functions
        .get_mut("gap")
        .unwrap()
        .preconditions
        .remove(0);
    assert_refused(&dropped);
    // Compiler side: same ordering requirement.
    let gap_rev = "fn gap(start: U64, end: U64) -> U64 where end - start <= 4u64, start <= end { return start; }";
    assert!(compile(&program(
        gap_rev,
        "where input.start <= input.end && input.end - input.start <= 4u64",
        "U64",
        "gap(input.start, input.end)",
    ))
    .is_err());
}

// a call in a fold's initial value is judged with the intent facts.
#[test]
fn a_call_in_a_fold_initial_value_needs_the_intent_facts() {
    let source = "package preconditions.example@1;
fn span(start: U64, end: U64) -> U64 where start <= end { return end - start; }
fn step(state: U64) -> U64 { return state; }
type Input = { start: U64, end: U64, };
type Output = { value: U64, };
intent evaluate(input: Input) returns Output profile p.read basis none budget <= p.small where input.start <= input.end {
  let value: U64 = fold(span(input.start, input.end), step, 2u64);
  return { value };
}";
    let mut core = accepted(source);
    assert_lowers(&core);
    core.intents
        .get_mut("evaluate")
        .unwrap()
        .input_constraints
        .clear();
    assert_refused(&core);
}

// separate source `where` predicates do not see each other (compiler).
#[test]
fn compiler_separate_where_predicates_do_not_see_each_other() {
    let error = compile(&program(
        ORDERED,
        "where input.start <= input.end, ordered(input.start, input.end)",
        "U64",
        "input.start",
    ))
    .unwrap_err();
    assert_eq!(
        error[0].kind,
        CompilerErrorKind::UnprovenPrecondition,
        "{error:?}"
    );
}

// statement-level `if` gives no facts (compiler).
#[test]
fn compiler_statement_level_if_gives_no_fact() {
    let source = "package preconditions.example@1;
fn span(start: U64, end: U64) -> U64 where start <= end { return end - start; }
type Input = { start: U64, end: U64, };
type Output = { value: U64, };
intent evaluate(input: Input) returns Output profile p.read basis none budget <= p.small {
  if input.start <= input.end {
    let inner: U64 = span(input.start, input.end);
  }
  return { value: input.start };
}";
    let result = compile(source);
    let errors = result.expect_err("statement-level if must not prove the call");
    assert!(
        errors
            .iter()
            .any(|e| e.kind == CompilerErrorKind::UnprovenPrecondition),
        "{errors:?}"
    );
}

// nested `&&` conjunct as a fact (both compiler and Target accept).
#[test]
fn a_nested_conjunct_is_a_fact_at_any_depth() {
    let core = accepted(&program(
        SPAN,
        "where input.end <= 8u64 && input.start <= input.end && input.start <= 8u64",
        "U64",
        "span(input.start, input.end)",
    ));
    assert_lowers(&core);
}

// an `if` fact from the caller's own precondition position: a
// precondition-bearing call guarded only inside the precondition list.
#[test]
fn a_negated_left_operand_does_not_prove_the_right() {
    // `false && span(..)` style: forge All[ (end < start), call-compare ]
    // where the left conjunct contradicts the obligation.
    let mut core = accepted(&program(
        ORDERED,
        "where input.start <= input.end && ordered(input.start, input.end)",
        "U64",
        "input.start",
    ));
    let intent = core.intents.get_mut("evaluate").unwrap();
    let CorePredicate::All(mut items) = intent.input_constraints[0].predicate.clone() else {
        panic!("All")
    };
    items[0] = CorePredicate::Not(Box::new(items[0].clone()));
    intent.input_constraints[0].predicate = CorePredicate::All(items);
    assert_refused(&core);
}

const NE: &str = "fn pick(start: U64, end: U64) -> U64 where start != end { return start; }";
const LT: &str = "fn pick(start: U64, end: U64) -> U64 where start < end { return start; }";

/// The non-order rule (`<`, `!=`, mirrored, nested in `&&`) through the
/// compiler and Target. First written by safety-review round 3.
#[test]
fn non_order_preconditions_need_the_same_or_a_mirrored_fact() {
    for (function, guard, proven) in [
        (NE, "where input.start != input.end", true),
        (NE, "where input.end != input.start", true),
        (
            NE,
            "where input.end <= 4u64 && input.start != input.end",
            true,
        ),
        (NE, "where input.start < input.end", false),
        (NE, "", false),
        (LT, "where input.start < input.end", true),
        (LT, "where input.end > input.start", true),
        (LT, "where input.end < input.start", false),
        (LT, "where input.start <= input.end", false),
        (LT, "where input.start != input.end", false),
    ] {
        let source = program(function, guard, "U64", "pick(input.start, input.end)");
        match (proven, compile(&source)) {
            (true, Ok(core)) => assert_lowers(&core),
            (false, Err(errors)) => assert_eq!(
                errors[0].kind,
                CompilerErrorKind::UnprovenPrecondition,
                "{guard}: {errors:?}"
            ),
            (_, result) => panic!("{function} / {guard}: expected proven={proven}, got {result:?}"),
        }
    }
    // Target: a reversed strict-order fact does not prove `start < end`.
    let mut core = accepted(&program(
        LT,
        "where input.start < input.end",
        "U64",
        "pick(input.start, input.end)",
    ));
    let intent = core.intents.get_mut("evaluate").unwrap();
    let CorePredicate::Compare { left, right, .. } = intent.input_constraints[0].predicate.clone()
    else {
        panic!("compare")
    };
    intent.input_constraints[0].predicate = CorePredicate::Compare {
        op: CompareOp::Lt,
        left: right,
        right: left,
    };
    assert_refused(&core);
}

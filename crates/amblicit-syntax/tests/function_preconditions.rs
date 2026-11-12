//! Function preconditions: a static obligation at every call site and an
//! assumption inside the body (`EDICT-LANG-FN-PRECONDITION-001`..`-004`).
//!
//! Oracle: the language specification's "Function Preconditions" section. Each
//! refusal names the call or declaration the specification says is at fault.
use amblicit_syntax::{
    compile_to_core, decode_canonical_cbor, digest_core_module, encode_core_module,
    lower_to_target_ir, parse_module, validate_core_module_type_integrity, CanonicalValue,
    CompareOp, CompilerContext, CompilerError, CompilerErrorKind, CoreBudget, CoreExpr, CoreModule,
    CorePredicate, CoreTypeIntegrityFailureKind, CoreValue, LocalRef, ResourceRef,
    TargetIrLoweringFacts, TargetLoweringFailureKind, TargetLoweringStatus,
};

/// Jedit's reproducer, byte-for-byte (see `fixtures/lang/functions/README.md`).
const JEDIT_REPRODUCER: &str =
    include_str!("../../../fixtures/lang/functions/function-precondition.amblicit");

const CUT: &str = "fn cut(bytes: Bytes<max=8>, start: U64, end: U64) -> Bytes<max=8>
  where start <= end, end <= len(bytes)
{
  return slice(bytes, start, end);
}";

const SPAN: &str = "fn span(start: U64, end: U64) -> U64 where start <= end {
  return end - start;
}";

fn context() -> CompilerContext {
    let budget = CoreBudget {
        max_steps: 4096,
        max_allocated_bytes: 1 << 20,
        max_output_bytes: 1 << 20,
    };
    CompilerContext::new()
        .with_operation_profile("p.read", "continuum.profile.read-only/v1")
        .with_operation_profile("text.replaceRange", "continuum.profile.read-only/v1")
        .with_budget("p.small", budget.clone())
        .with_budget("text.replaceRangeBudget", budget)
}

/// One intent over `{ bytes, start, end }` that binds `value` and returns it.
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

fn first_error(source: &str) -> CompilerError {
    compile(source)
        .err()
        .and_then(|errors| errors.into_iter().next())
        .unwrap_or_else(|| panic!("expected refusal of:\n{source}"))
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

fn lowers(core: &CoreModule) {
    let report = lower_to_target_ir(core, &facts());
    assert_eq!(report.status, TargetLoweringStatus::Lowered, "{report:?}");
}

fn target_refuses(core: &CoreModule) {
    let report = lower_to_target_ir(core, &facts());
    assert_ne!(report.status, TargetLoweringStatus::Lowered, "{report:?}");
    assert_eq!(
        report.failures.first().map(|failure| failure.kind),
        Some(TargetLoweringFailureKind::InvalidCoreIdentity),
        "{report:?}"
    );
}

/// Target refuses, and attributes the refusal to the calling intent rather
/// than to a callee body, so the call-site judgment is what decided.
fn target_refuses_call_in(core: &CoreModule, intent: &str) {
    let report = lower_to_target_ir(core, &facts());
    let failure = report.failures.first();
    assert_eq!(
        failure.map(|failure| (failure.kind, failure.intent.as_deref())),
        Some((TargetLoweringFailureKind::InvalidCoreIdentity, Some(intent))),
        "{report:?}"
    );
}

fn param(index: usize, ty: &str) -> CoreExpr {
    CoreExpr::Local {
        reference: LocalRef {
            id: format!("arg.{index}"),
            alpha_name: format!("$arg{index}"),
            ty: ty.into(),
        },
    }
}

#[test]
fn jedit_function_precondition_reproducer_compiles_and_lowers() {
    let core = accepted(JEDIT_REPRODUCER);
    let cut = &core.functions["cut"];
    assert_eq!(cut.preconditions.len(), 3);
    assert_eq!(
        cut.preconditions[0],
        CorePredicate::Compare {
            op: CompareOp::Le,
            left: param(1, "U64"),
            right: param(2, "U64"),
        }
    );
    lowers(&core);
}

#[test]
fn preconditions_prove_subtraction_and_slice_in_the_body() {
    let both = format!("{CUT}\n{SPAN}");
    let core = accepted(&program(
        &both,
        "where input.start <= input.end, input.end <= len(input.bytes)",
        "U64",
        "span(input.start, input.end)",
    ));
    assert_eq!(core.functions["span"].preconditions.len(), 1);
    assert_eq!(core.functions["cut"].preconditions.len(), 2);
    lowers(&core);
}

#[test]
fn bodies_without_preconditions_still_refuse_unproven_subtraction_and_slice() {
    for function in [
        "fn span(start: U64, end: U64) -> U64 { return end - start; }",
        "fn cut(bytes: Bytes<max=8>, start: U64, end: U64) -> Bytes<max=8> { return slice(bytes, start, end); }",
    ] {
        let error = first_error(&program(
            function,
            "where input.start <= input.end, input.end <= len(input.bytes)",
            "U64",
            "input.start",
        ));
        assert_eq!(error.kind, CompilerErrorKind::UnsupportedSourceShape, "{error:?}");
    }
}

#[test]
fn a_call_that_does_not_establish_a_precondition_is_refused_at_the_call() {
    for (guard, call) in [
        ("", "span(input.start, input.end)"),
        (
            "where input.end <= input.start",
            "span(input.start, input.end)",
        ),
        (
            "where input.start <= input.end",
            "span(input.end, input.start)",
        ),
        (
            "where input.start != input.end",
            "span(input.start, input.end)",
        ),
        (
            "where input.start <= input.end || input.end <= len(input.bytes)",
            "span(input.start, input.end)",
        ),
    ] {
        let source = program(SPAN, guard, "U64", call);
        let error = first_error(&source);
        assert_eq!(
            error.kind,
            CompilerErrorKind::UnprovenPrecondition,
            "{guard}: {error:?}"
        );
        assert_eq!(&source[error.span.start..error.span.end], call, "{guard}");
    }
}

#[test]
fn every_conjunct_must_be_established() {
    let source = program(
        CUT,
        "where input.start <= input.end",
        "Bytes<max=8>",
        "cut(input.bytes, input.start, input.end)",
    );
    let error = first_error(&source);
    assert_eq!(
        error.kind,
        CompilerErrorKind::UnprovenPrecondition,
        "{error:?}"
    );
    assert!(error.message.contains("where.1"), "{}", error.message);
}

#[test]
fn dominating_facts_from_every_source_establish_a_precondition() {
    let nested = format!(
        "{SPAN}\nfn width(start: U64, end: U64) -> U64 where start <= end {{ return span(start, end); }}"
    );
    for (functions, guard, call) in [
        (
            SPAN.to_owned(),
            "where input.start <= input.end",
            "span(input.start, input.end)",
        ),
        (
            SPAN.to_owned(),
            "where input.end >= input.start",
            "span(input.start, input.end)",
        ),
        (
            SPAN.to_owned(),
            "where input.start < input.end",
            "span(input.start, input.end)",
        ),
        (
            SPAN.to_owned(),
            "",
            "if input.start <= input.end then span(input.start, input.end) else 0u64",
        ),
        (
            SPAN.to_owned(),
            "where input.start == input.end",
            "span(input.start, input.end)",
        ),
        (SPAN.to_owned(), "", "span(input.start, input.start)"),
        (SPAN.to_owned(), "", "span(0u64, input.end)"),
        (
            nested,
            "where input.start <= input.end",
            "width(input.start, input.end)",
        ),
    ] {
        let core = accepted(&program(&functions, guard, "U64", call));
        lowers(&core);
    }
}

#[test]
fn a_negated_guard_does_not_establish_a_precondition() {
    let source = program(
        SPAN,
        "",
        "U64",
        "if input.start <= input.end then 0u64 else span(input.start, input.end)",
    );
    assert_eq!(
        first_error(&source).kind,
        CompilerErrorKind::UnprovenPrecondition
    );
}

#[test]
fn a_call_inside_an_intent_where_clause_sees_only_earlier_conjuncts() {
    let gate = "fn ordered(start: U64, end: U64) -> Bool where start <= end { return end - start <= 4u64; }";
    let accepted_core = accepted(&program(
        gate,
        "where input.start <= input.end && ordered(input.start, input.end)",
        "U64",
        "input.start",
    ));
    lowers(&accepted_core);
    let error = first_error(&program(
        gate,
        "where ordered(input.start, input.end) && input.start <= input.end",
        "U64",
        "input.start",
    ));
    assert_eq!(error.kind, CompilerErrorKind::UnprovenPrecondition);
}

#[test]
fn preconditions_are_never_evaluated_and_do_not_change_the_caller() {
    let plain = "fn first(start: U64, end: U64) -> U64 { return start; }";
    let guarded = "fn first(start: U64, end: U64) -> U64 where start <= end { return start; }";
    let guard = "where input.start <= input.end";
    let call = "first(input.start, input.end)";
    let without = accepted(&program(plain, guard, "U64", call));
    let with = accepted(&program(guarded, guard, "U64", call));
    assert_eq!(without.intents, with.intents);
    assert_eq!(
        without.functions["first"].body,
        with.functions["first"].body
    );
}

#[test]
fn multiple_where_clauses_merge_in_source_order() {
    let split = "fn cut(bytes: Bytes<max=8>, start: U64, end: U64) -> Bytes<max=8>
      where start <= end where end <= len(bytes) { return slice(bytes, start, end); }";
    let guard = "where input.start <= input.end, input.end <= len(input.bytes)";
    let call = "cut(input.bytes, input.start, input.end)";
    let merged = accepted(&program(split, guard, "Bytes<max=8>", call));
    let listed = accepted(&program(CUT, guard, "Bytes<max=8>", call));
    assert_eq!(merged.functions["cut"], listed.functions["cut"]);
}

#[test]
fn preconditions_must_be_predicates_over_parameters_without_source_calls() {
    for (functions, kind) in [
        (
            "fn bad(start: U64) -> U64 where start { return start; }".to_owned(),
            CompilerErrorKind::ExpectedPredicate,
        ),
        (
            format!("{SPAN}\nfn bad(start: U64, end: U64) -> U64 where span(start, end) <= 4u64 {{ return start; }}"),
            CompilerErrorKind::UnsupportedSourceShape,
        ),
    ] {
        let error = first_error(&program(&functions, "", "U64", "input.start"));
        assert_eq!(error.kind, kind, "{functions}: {error:?}");
    }
}

#[test]
fn a_function_with_preconditions_cannot_be_a_fold_step() {
    let source = "package preconditions.example@1;
fn step(state: U64) -> U64 where state <= 4u64 { return state; }
type Input = { start: U64, };
type Output = { value: U64, };
intent evaluate(input: Input) returns Output profile p.read basis none budget <= p.small {
  let value: U64 = fold(0u64, step, 2u64);
  return { value };
}";
    let error = first_error(source);
    assert_eq!(
        error.kind,
        CompilerErrorKind::UnsupportedSourceShape,
        "{error:?}"
    );
}

fn canonical_function(core: &CoreModule, name: &str) -> Vec<(String, CanonicalValue)> {
    let CanonicalValue::Map(module) =
        decode_canonical_cbor(&encode_core_module(core).unwrap()).unwrap()
    else {
        panic!("Core is a map")
    };
    let functions = module
        .into_iter()
        .find_map(|(key, value)| (key == CanonicalValue::Text("functions".into())).then_some(value))
        .expect("function table");
    let CanonicalValue::Map(functions) = functions else {
        panic!("function table is a map")
    };
    let CanonicalValue::Map(function) = functions
        .into_iter()
        .find_map(|(key, value)| (key == CanonicalValue::Text(name.into())).then_some(value))
        .expect("function entry")
    else {
        panic!("function is a map")
    };
    function
        .into_iter()
        .map(|(key, value)| {
            let CanonicalValue::Text(key) = key else {
                panic!("text key")
            };
            (key, value)
        })
        .collect()
}

#[test]
fn core_preconditions_are_hash_significant_and_omitted_when_empty() {
    let guard = "where input.start <= input.end, input.end <= len(input.bytes)";
    let both = format!("{CUT}\nfn first(start: U64) -> U64 {{ return start; }}");
    let core = accepted(&program(
        &both,
        guard,
        "U64",
        "first(len(cut(input.bytes, input.start, input.end)))",
    ));
    let cut = canonical_function(&core, "cut");
    let keys: Vec<_> = cut.iter().map(|(key, _)| key.as_str()).collect();
    assert_eq!(keys, ["body", "params", "returnType", "preconditions"]);
    let Some((_, CanonicalValue::Array(preconditions))) =
        cut.iter().find(|(key, _)| key == "preconditions")
    else {
        panic!("preconditions array")
    };
    assert_eq!(preconditions.len(), 2);
    let first: Vec<_> = canonical_function(&core, "first")
        .into_iter()
        .map(|(key, _)| key)
        .collect();
    assert_eq!(first, ["body", "params", "returnType"]);

    let mut changed = core.clone();
    changed
        .functions
        .get_mut("cut")
        .unwrap()
        .preconditions
        .pop();
    assert_ne!(
        digest_core_module(&core).unwrap(),
        digest_core_module(&changed).unwrap()
    );
}

#[test]
fn core_integrity_refuses_preconditions_outside_the_parameter_scope() {
    let core = accepted(&program(
        SPAN,
        "where input.start <= input.end",
        "U64",
        "span(input.start, input.end)",
    ));
    let stranger = CoreExpr::Local {
        reference: LocalRef {
            id: "local.9".into(),
            alpha_name: "$local9".into(),
            ty: "U64".into(),
        },
    };
    let mut forged = core.clone();
    forged.functions.get_mut("span").unwrap().preconditions[0] = CorePredicate::Compare {
        op: CompareOp::Le,
        left: stranger,
        right: param(1, "U64"),
    };
    let failure = validate_core_module_type_integrity(&forged).unwrap_err();
    assert_eq!(
        failure.kind(),
        CoreTypeIntegrityFailureKind::InvalidDefinition
    );

    let mut self_call = core;
    self_call.functions.get_mut("span").unwrap().preconditions[0] = CorePredicate::Compare {
        op: CompareOp::Le,
        left: CoreExpr::Call {
            callee: "preconditions.example@1.span".into(),
            type_args: Vec::new(),
            args: vec![param(0, "U64"), param(1, "U64")],
        },
        right: param(1, "U64"),
    };
    let failure = validate_core_module_type_integrity(&self_call).unwrap_err();
    assert_eq!(
        failure.kind(),
        CoreTypeIntegrityFailureKind::InvalidDefinition
    );
}

#[test]
fn target_refuses_forged_core_that_drops_a_precondition_the_body_needs() {
    let mut core = accepted(&program(
        SPAN,
        "where input.start <= input.end",
        "U64",
        "span(input.start, input.end)",
    ));
    core.functions
        .get_mut("span")
        .unwrap()
        .preconditions
        .clear();
    target_refuses(&core);
}

#[test]
fn target_refuses_forged_core_whose_call_does_not_establish_a_precondition() {
    // The callee body never needs its precondition, so only Target's
    // call-site re-proof can refuse, and it refuses at the calling intent.
    let first = "fn first(start: U64, end: U64) -> U64 where start <= end { return start; }";
    let mut core = accepted(&program(
        first,
        "where input.start <= input.end",
        "U64",
        "first(input.start, input.end)",
    ));
    lowers(&core);
    core.intents
        .get_mut("evaluate")
        .unwrap()
        .input_constraints
        .clear();
    target_refuses_call_in(&core, "evaluate");

    let mut reversed = accepted(&program(
        first,
        "where input.start <= input.end",
        "U64",
        "first(input.start, input.end)",
    ));
    reversed.functions.get_mut("first").unwrap().preconditions[0] = CorePredicate::Compare {
        op: CompareOp::Le,
        left: param(1, "U64"),
        right: param(0, "U64"),
    };
    target_refuses_call_in(&reversed, "evaluate");
}

#[test]
fn target_refuses_forged_core_that_gives_a_fold_step_a_precondition() {
    // The step body does not need the precondition, so only the fold-step
    // rule (EDICT-LANG-FN-PRECONDITION-004) can refuse.
    let source = "package preconditions.example@1;
fn step(state: U64) -> U64 { return state; }
type Input = { start: U64, };
type Output = { value: U64, };
intent evaluate(input: Input) returns Output profile p.read basis none budget <= p.small {
  let value: U64 = fold(input.start, step, 2u64);
  return { value };
}";
    let mut core = accepted(source);
    lowers(&core);
    core.functions.get_mut("step").unwrap().preconditions = vec![CorePredicate::Compare {
        op: CompareOp::Le,
        left: param(0, "U64"),
        right: CoreExpr::Const(CoreValue::Int {
            width: "U64".into(),
            value: "4".into(),
        }),
    }];
    target_refuses_call_in(&core, "evaluate");
}

#[test]
fn target_refuses_forged_core_whose_precondition_reads_a_non_parameter() {
    let mut core = accepted(&program(
        SPAN,
        "where input.start <= input.end",
        "U64",
        "span(input.start, input.end)",
    ));
    core.functions.get_mut("span").unwrap().preconditions[0] = CorePredicate::Compare {
        op: CompareOp::Le,
        left: CoreExpr::Local {
            reference: LocalRef {
                id: "local.9".into(),
                alpha_name: "$local9".into(),
                ty: "U64".into(),
            },
        },
        right: param(1, "U64"),
    };
    target_refuses(&core);
}

/// Oracle: the v3 closed comparison-operator rule (TIR-REQ-060) applies to
/// every Core predicate, so a precondition cannot carry an ordered
/// comparison of non-integers past `echo.span-ir/v3`.
#[test]
fn structured_target_refuses_a_non_integer_ordered_comparison_in_a_precondition() {
    let structured = TargetIrLoweringFacts {
        target_ir_domain: amblicit_syntax::ECHO_STRUCTURED_SPAN_IR_DOMAIN.into(),
        ..facts()
    };
    let source = |precondition: &str| {
        format!(
            "package preconditions.example@1;
fn first(bytes: Bytes<max=8>, start: U64) -> U64 where {precondition} {{ return start; }}
type Input = {{ bytes: Bytes<max=8>, start: U64, end: U64, }};
type Output = {{ value: U64, }};
intent evaluate(input: Input) returns Output profile p.read basis none budget <= p.small {{
  let value: U64 = first(input.bytes, input.start);
  return {{ value }};
}}"
        )
    };
    let integer = accepted(&source("start <= start"));
    let report = lower_to_target_ir(&integer, &structured);
    assert_eq!(report.status, TargetLoweringStatus::Lowered, "{report:?}");

    let mut forged = integer;
    forged.functions.get_mut("first").unwrap().preconditions = vec![CorePredicate::Compare {
        op: CompareOp::Le,
        left: param(0, "Bytes<max=8>"),
        right: param(0, "Bytes<max=8>"),
    }];
    let report = lower_to_target_ir(&forged, &structured);
    assert_eq!(
        report
            .failures
            .first()
            .map(|failure| (failure.kind, failure.detail.as_str())),
        Some((
            TargetLoweringFailureKind::InvalidCoreIdentity,
            "inadmissible_comparison_operator"
        )),
        "{report:?}"
    );
}

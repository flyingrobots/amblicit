//! Checked integer arithmetic and `min`/`max` in Edict source.
//!
//! Oracle: SPEC Edict Language v1, Standard Functions and Integer Safety
//! (`EDICT-LANG-INT-SAFETY-001`). `checkedAdd`, `checkedSub`, `checkedMul`,
//! `checkedDiv` and `checkedRem` return `Option<T>` and never reject on their
//! operand values; `min` and `max` are total and return `T`. Overflow,
//! underflow and a zero divisor are the `none` case the caller must handle.
//! The Core spelling `core.integer.<operation><T>` is the compiler's encoding,
//! recorded in the compiler-spine shelf.
use amblicit_syntax::{
    compile_to_core, lower_to_target_ir, parse_module, validate_core_module_type_integrity,
    CompilerContext, CompilerError, CompilerErrorKind, CoreBudget, CoreExpr, CoreModule, CoreNode,
    CoreTypeIntegrityFailureKind, ResourceRef, TargetIrLoweringFacts, TargetLoweringFailureKind,
    TargetLoweringStatus, ECHO_DPO_TARGET_PROFILE, ECHO_SPAN_IR_DOMAIN,
};

const CHECKED: [&str; 5] = [
    "checkedAdd",
    "checkedSub",
    "checkedMul",
    "checkedDiv",
    "checkedRem",
];

fn context(steps: u64) -> CompilerContext {
    CompilerContext::new()
        .with_operation_profile("p.read", "continuum.profile.read-only/v1")
        .with_budget(
            "p.large",
            CoreBudget {
                max_steps: steps,
                max_allocated_bytes: 1_000_000,
                max_output_bytes: 1_000_000,
            },
        )
}

fn compile(body: &str) -> Result<CoreModule, Vec<CompilerError>> {
    let source = format!("package jedit.arith@1; {body}");
    compile_to_core(
        &parse_module(&source).expect("valid syntax"),
        &context(1_000_000),
    )
}

fn facts() -> TargetIrLoweringFacts {
    TargetIrLoweringFacts {
        target_profile: ResourceRef {
            coordinate: ECHO_DPO_TARGET_PROFILE.to_owned(),
            digest: Some(format!("sha256:{}", "1".repeat(64))),
        },
        target_ir_domain: ECHO_SPAN_IR_DOMAIN.to_owned(),
        operation_profiles: vec!["continuum.profile.read-only/v1".to_owned()],
        obstruction_coordinates: vec!["rope.ArithmeticOverflow".to_owned()],
        effect_lowerings: Vec::new(),
        effect_signatures: Vec::new(),
        pure_functions: Vec::new(),
    }
}

/// A scalar intent gives Target a step to lower beside the helpers under test.
const WITNESS: &str = "type Input = { marker: U8 }; type Output = { marker: U8 }; intent witness(input: Input) returns Output profile p.read basis none budget <= p.large { let marker = input.marker; return { marker }; }";

fn call(expr: &CoreExpr) -> (&str, &[String], &[CoreExpr]) {
    let CoreExpr::Call {
        callee,
        type_args,
        args,
    } = expr
    else {
        panic!("typed integer call expected, got {expr:?}")
    };
    (callee, type_args, args)
}

type CallParts<'a> = (&'a mut String, &'a mut Vec<String>, &'a mut Vec<CoreExpr>);

fn call_mut(expr: &mut CoreExpr) -> CallParts<'_> {
    let CoreExpr::Call {
        callee,
        type_args,
        args,
    } = expr
    else {
        panic!("typed integer call expected, got {expr:?}")
    };
    (callee, type_args, args)
}

fn lowered(core: &CoreModule) {
    let report = lower_to_target_ir(core, &facts());
    assert_eq!(report.status, TargetLoweringStatus::Lowered, "{report:?}");
    assert!(report.artifact.is_some());
}

fn refused_by_target(core: &CoreModule, label: &str) {
    let report = lower_to_target_ir(core, &facts());
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

fn refusal_kinds(body: &str) -> Vec<CompilerErrorKind> {
    compile(body)
        .expect_err(body)
        .into_iter()
        .map(|error| error.kind)
        .collect()
}

fn capability(core: &CoreModule) -> bool {
    core.required_core_capabilities
        .iter()
        .any(|capability| capability == "core.option/v1")
}

#[test]
fn checked_forms_lower_to_typed_core_integer_calls_returning_option() {
    for width in ["U8", "U32", "U64"] {
        for name in CHECKED {
            let body = format!(
                "fn f(a: {width}, b: {width}) -> Option<{width}> {{ return {name}(a, b); }} {WITNESS}"
            );
            let core = compile(&body).unwrap_or_else(|errors| panic!("{body}: {errors:?}"));
            let function = &core.functions["f"];
            assert_eq!(function.return_type, format!("Option<{width}>"));
            let (callee, type_args, args) = call(&function.body.result);
            assert_eq!(callee, format!("core.integer.{name}"));
            assert_eq!(type_args, [width]);
            assert!(matches!(
                args,
                [CoreExpr::Local { .. }, CoreExpr::Local { .. }]
            ));
            assert!(capability(&core), "{name}<{width}> needs core.option/v1");
            lowered(&core);
        }
    }
}

#[test]
fn checked_forms_accept_operands_at_width_bounds_where_bare_forms_refuse() {
    // Values that overflow, underflow or divide by zero are the `none` case,
    // so the checked form compiles where the bare form is a compile-time
    // rejection.
    for (width, max) in [
        ("U8", "255u8"),
        ("U32", "4294967295u32"),
        ("U64", "18446744073709551615u64"),
    ] {
        let zero = format!("0{}", width.to_lowercase());
        let one = format!("1{}", width.to_lowercase());
        for (name, left, right) in [
            ("checkedAdd", max, one.as_str()),
            ("checkedSub", zero.as_str(), one.as_str()),
            ("checkedMul", max, max),
            ("checkedDiv", max, zero.as_str()),
            ("checkedRem", max, zero.as_str()),
        ] {
            let body = format!(
                "fn f() -> Option<{width}> {{ return {name}({left}, {right}); }} {WITNESS}"
            );
            let core = compile(&body).unwrap_or_else(|errors| panic!("{body}: {errors:?}"));
            let (_, type_args, args) = call(&core.functions["f"].body.result);
            assert_eq!(type_args, [width]);
            assert!(matches!(args, [CoreExpr::Const(_), CoreExpr::Const(_)]));
            lowered(&core);
        }
    }
    for bare in [
        "18446744073709551615u64 + 1u64",
        "0u64 - 1u64",
        "4294967295u32 + 1u32",
    ] {
        let width = if bare.contains("u32") { "U32" } else { "U64" };
        assert!(
            compile(&format!("fn f() -> {width} {{ return {bare}; }} {WITNESS}")).is_err(),
            "bare `{bare}` must still refuse without a proof"
        );
    }
}

#[test]
fn bare_literal_operands_take_the_typed_peer_or_expected_width() {
    for body in [
        "fn f(a: U32) -> Option<U32> { return checkedAdd(a, 1); }",
        "fn f(a: U32) -> Option<U32> { return checkedSub(1, a); }",
        "fn f() -> Option<U32> { return checkedMul(2, 3); }",
        "fn f(a: U32) -> U32 { return max(a, 1); }",
        "fn f() -> U32 { return min(2, 3); }",
    ] {
        let core =
            compile(&format!("{body} {WITNESS}")).unwrap_or_else(|e| panic!("{body}: {e:?}"));
        let (_, type_args, args) = call(&core.functions["f"].body.result);
        assert_eq!(type_args, ["U32"], "{body}");
        for arg in args {
            if let CoreExpr::Const(value) = arg {
                assert!(format!("{value:?}").contains("U32"), "{body}: {value:?}");
            }
        }
        lowered(&core);
    }
}

#[test]
fn min_and_max_lower_to_total_core_calls_without_option() {
    for width in ["U8", "U32", "U64"] {
        for name in ["min", "max"] {
            let body = format!(
                "fn f(a: {width}, b: {width}) -> {width} {{ return {name}(a, b); }} {WITNESS}"
            );
            let core = compile(&body).unwrap_or_else(|errors| panic!("{body}: {errors:?}"));
            let (callee, type_args, args) = call(&core.functions["f"].body.result);
            assert_eq!(callee, format!("core.integer.{name}"));
            assert_eq!(type_args, [width]);
            assert_eq!(args.len(), 2);
            assert!(!capability(&core), "{name} needs no Option capability");
            lowered(&core);
        }
    }
}

#[test]
fn checked_results_must_be_handled_before_use() {
    use CompilerErrorKind as K;
    for (body, kind) in [
        (
            "fn f(a: U64, b: U64) -> U64 { return checkedAdd(a, b); }",
            K::TypeMismatch,
        ),
        (
            "fn f(a: U64, b: U64) -> U64 { return unwrap(checkedAdd(a, b)); }",
            K::UnsupportedSourceShape,
        ),
        (
            "fn f(a: U64, b: U64) -> U64 { let s = checkedAdd(a, b); return unwrap(s); }",
            K::UnsupportedSourceShape,
        ),
    ] {
        assert!(
            refusal_kinds(&format!("{body} {WITNESS}")).contains(&kind),
            "{body}"
        );
    }
    for body in [
        "fn f(a: U64, b: U64) -> U64 { let s = checkedAdd(a, b); return if isSome(s) then unwrap(s) else 0u64; }",
        "fn f(a: U64, b: U64) -> U64 { return default(checkedSub(a, b), 0u64); }",
    ] {
        lowered(&compile(&format!("{body} {WITNESS}")).expect(body));
    }
}

/// The rope law: a checked sum maps its `none` case to a declared variant the
/// caller turns into an obstruction, and heights take the larger child.
const ROPE: &str = "
type Extent = { bytes: U64, breaks: U64, height: U8 };
type Joined = variant { Joined(Extent), Overflow };
fn join(left: Extent, right: Extent) -> Joined {
  let bytes = checkedAdd(left.bytes, right.bytes);
  let breaks = checkedAdd(left.breaks, right.breaks);
  let height = checkedAdd(max(left.height, right.height), 1u8);
  return if isSome(bytes) && isSome(breaks) && isSome(height)
    then Joined::Joined({ bytes: unwrap(bytes), breaks: unwrap(breaks), height: unwrap(height) })
    else Joined::Overflow;
}
type Input = { left: U64, right: U64 };
type Output = { total: U64 };
intent measure(input: Input) returns Output profile p.read basis none budget <= p.large {
  let sum = checkedAdd(input.left, input.right);
  require isSome(sum) else rope.ArithmeticOverflow({ left: input.left });
  let total = default(sum, 0u64);
  return { total };
}
";

#[test]
fn checked_failure_maps_to_a_declared_variant_and_an_obstruction() {
    let core = compile(ROPE).unwrap_or_else(|errors| panic!("{errors:?}"));
    assert!(capability(&core));
    let bindings = &core.functions["join"].body.bindings;
    assert_eq!(call(&bindings[0].value).0, "core.integer.checkedAdd");
    let (callee, type_args, args) = call(&bindings[2].value);
    assert_eq!(
        (callee, type_args),
        ("core.integer.checkedAdd", &["U8".to_owned()][..])
    );
    assert_eq!(call(&args[0]).0, "core.integer.max");
    let intent = &core.intents["measure"];
    assert!(intent.body.nodes.iter().any(|node| matches!(
        node,
        CoreNode::Let { value: CoreExpr::Call { callee, .. }, .. } if callee == "core.integer.checkedAdd"
    )));
    lowered(&core);
}

#[test]
fn checked_and_min_max_typing_refuses_mismatches() {
    use CompilerErrorKind as K;
    for (body, kind) in [
        (
            "fn f(a: U32, b: U64) -> Option<U64> { return checkedAdd(a, b); }",
            K::TypeMismatch,
        ),
        (
            "fn f(a: U64) -> Option<U32> { return checkedAdd(a, a); }",
            K::TypeMismatch,
        ),
        (
            "fn f(a: U32, b: U64) -> U64 { return max(a, b); }",
            K::TypeMismatch,
        ),
        (
            "fn f(a: Bool) -> Option<U64> { return checkedSub(a, a); }",
            K::UnsupportedSourceShape,
        ),
        (
            "fn f(a: I64) -> Option<I64> { return checkedAdd(a, a); }",
            K::UnsupportedSourceShape,
        ),
        (
            "fn f(a: String<max=2>) -> String<max=2> { return min(a, a); }",
            K::UnsupportedSourceShape,
        ),
        (
            "fn f(a: U64) -> Option<U64> { return checkedAdd(a); }",
            K::UnsupportedSourceShape,
        ),
        (
            "fn f(a: U64) -> U64 { return max(a, a, a); }",
            K::UnsupportedSourceShape,
        ),
        (
            "fn f(a: U64) -> Option<U64> { return checkedAdd<U64>(a, a); }",
            K::UnsupportedSourceShape,
        ),
    ] {
        assert!(
            refusal_kinds(&format!("{body} {WITNESS}")).contains(&kind),
            "{body}: {:?}",
            compile(&format!("{body} {WITNESS}")).err()
        );
    }
    assert!(refusal_kinds(&format!(
        "fn g() -> Bool {{ let x = checkedAdd(1, 2); return isSome(x); }} {WITNESS}"
    ))
    .contains(&K::UnsupportedSourceShape));
}

#[test]
fn arithmetic_prelude_names_cannot_be_redeclared() {
    for name in CHECKED.into_iter().chain(["min", "max"]) {
        let errors = refusal_kinds(&format!("fn {name}() -> U64 {{ return 0u64; }} {WITNESS}"));
        assert!(
            errors.contains(&CompilerErrorKind::SurfaceValidation),
            "{name}: {errors:?}"
        );
    }
}

fn bound_intent(guard: &str, expression: &str) -> Result<CoreModule, Vec<CompilerError>> {
    compile(&format!(
        "type Input = {{ left: U32, right: U32 }};
        type Output = {{ next: U32 }};
        intent measure(input: Input) returns Output profile p.read basis none budget <= p.large {guard} {{
          let height = {expression};
          let next = height + 1;
          return {{ next }};
        }}"
    ))
}

#[test]
fn min_and_max_carry_their_operand_bounds_into_addition_proofs() {
    for (guard, expression, accepted) in [
        (
            "where input.left <= 10 && input.right <= 20",
            "max(input.left, input.right)",
            true,
        ),
        (
            "where input.left <= 10",
            "max(input.left, input.right)",
            false,
        ),
        (
            "where input.left <= 10",
            "min(input.left, input.right)",
            true,
        ),
        (
            "where input.right <= 10",
            "min(input.left, input.right)",
            true,
        ),
        ("", "min(input.left, input.right)", false),
        ("", "max(input.left, 7)", false),
        ("", "min(input.left, 7)", true),
    ] {
        let result = bound_intent(guard, expression);
        assert_eq!(
            result.is_ok(),
            accepted,
            "{guard} / {expression}: {result:?}"
        );
        if let Ok(core) = result {
            lowered(&core);
        }
    }
}

fn first_let_call(core: &mut CoreModule) -> &mut CoreExpr {
    let intent = core.intents.get_mut("measure").unwrap();
    let Some(CoreNode::Let { value, .. }) = intent.body.nodes.first_mut() else {
        panic!("first node binds the min/max result")
    };
    value
}

#[test]
fn target_recomputes_min_and_max_bounds_from_untrusted_core() {
    let core = bound_intent("where input.left <= 10", "min(input.left, input.right)")
        .expect("min control");
    lowered(&core);
    let mut forged = core.clone();
    *call_mut(first_let_call(&mut forged)).0 = "core.integer.max".to_owned();
    refused_by_target(&forged, "max of an unbounded operand cannot prove the sum");
}

type Forgery = (&'static str, fn(&mut CoreModule));

fn checked_core() -> CoreModule {
    compile(&format!(
        "fn f(a: U64, b: U64) -> Option<U64> {{ return checkedAdd(a, b); }}
         fn g(a: U64, b: U64) -> U64 {{ return max(a, b); }} {WITNESS}"
    ))
    .expect("checked control")
}

#[test]
fn target_rechecks_integer_signatures_and_capability() {
    let core = checked_core();
    lowered(&core);
    let edits: [Forgery; 7] = [
        ("checked width forged", |core| {
            call_mut(&mut core.functions.get_mut("f").unwrap().body.result).1[0] = "U32".into();
        }),
        ("checked width not unsigned", |core| {
            call_mut(&mut core.functions.get_mut("f").unwrap().body.result).1[0] = "Bool".into();
        }),
        ("checked arity", |core| {
            call_mut(&mut core.functions.get_mut("f").unwrap().body.result)
                .2
                .pop();
        }),
        ("checked result is an Option, not the item", |core| {
            core.functions.get_mut("f").unwrap().return_type = "U64".into();
        }),
        ("unknown integer operation", |core| {
            *call_mut(&mut core.functions.get_mut("f").unwrap().body.result).0 =
                "core.integer.checkedPow".into();
        }),
        ("min/max width forged", |core| {
            call_mut(&mut core.functions.get_mut("g").unwrap().body.result).1[0] = "U32".into();
        }),
        ("min/max returns the item, not an Option", |core| {
            core.functions.get_mut("g").unwrap().return_type = "Option<U64>".into();
        }),
    ];
    for (label, edit) in edits {
        let mut forged = core.clone();
        edit(&mut forged);
        refused_by_target(&forged, label);
    }
    let mut stripped = core;
    stripped
        .required_core_capabilities
        .retain(|capability| capability != "core.option/v1");
    assert_eq!(
        validate_core_module_type_integrity(&stripped)
            .expect_err("checked calls need the Option capability")
            .kind(),
        CoreTypeIntegrityFailureKind::InvalidDefinition
    );
}

/// Checked arithmetic and `min`/`max` mixed with list work agree with Target
/// over the swept budgets.
#[test]
fn checked_arithmetic_is_charged_like_target_work() {
    let body = "fn f(a: U64) -> U64 { return default(checkedMul(a, 2u64), max(a, 1u64)); }
type Input = { marker: U8, value: U64 }; type Output = { value: U64, size: U64 };
intent run(input: Input) returns Output profile p.read basis none budget <= p.large {
  let xs: List<U8, max=1> = [input.marker];
  let size = listLen(xs);
  let value = f(input.value);
  return { value, size };
}";
    let parsed = parse_module(&format!("package jedit.arith@1; {body}")).unwrap();
    let mut accepted = 0;
    for steps in 1_u64..=1_200 {
        let Ok(core) = compile_to_core(&parsed, &context(steps)) else {
            continue;
        };
        accepted += 1;
        let report = lower_to_target_ir(&core, &facts());
        assert_eq!(
            report.status,
            TargetLoweringStatus::Lowered,
            "budget {steps} accepted by the compiler but refused by Target: {report:?}"
        );
    }
    assert!(accepted > 0, "no budget compiled; the probe is vacuous");
    assert!(
        compile_to_core(&parsed, &context(1)).is_err(),
        "a one-step budget must refuse"
    );
}

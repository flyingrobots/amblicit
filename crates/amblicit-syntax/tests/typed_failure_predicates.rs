//! Boolean `expect` values retain predicate typing and failure scope checks.
//!
//! Oracle: Typed Function Failures in `docs/SPEC_edict-language-v1.md`:
//! `expect` has its callee's result type, propagates only declared failures,
//! and cannot propagate from an intent. Pure functions do not support
//! statement `if`; the statement cases below exercise the intent boundary.
use amblicit_syntax::{
    compile_to_core, encode_target_ir_artifact, lower_to_target_ir, parse_module,
    validate_core_module_type_integrity, CompilerContext, CompilerErrorKind, CompilerStage,
    CoreBudget, ResourceRef, TargetIrLoweringFacts, TargetLoweringStatus,
};

const PREFIX: &str = r"package predicate.example@1;
failure Stop;
type Input = { value: Bool, };
type Output = { value: U64, };
fn checked(value: Bool) -> Bool fails Stop {
  return if value then value else fail Stop;
}
";

fn context() -> CompilerContext {
    CompilerContext::new()
        .with_operation_profile("p.read", "continuum.profile.read-only/v1")
        .with_budget(
            "p.small",
            CoreBudget {
                max_steps: 100_000,
                max_allocated_bytes: u64::MAX,
                max_output_bytes: u64::MAX,
            },
        )
}

fn intent(body: &str) -> String {
    format!(
        "intent evaluate(input: Input) returns Output profile p.read basis none budget <= p.small {{\n{body}\n}}"
    )
}

fn selection(condition: &str, failures: &str) -> String {
    format!(
        "{PREFIX}\nfn select(value: Bool) -> U64 {failures} {{\n  return if {condition} then 1u64 else 2u64;\n}}\n{}",
        intent(
            "  let value = select(input.value) else { Stop => text.ClosureInsufficient };\n  return { value };"
        )
    )
}

fn accepted_condition(condition: &str) {
    let source = selection(condition, "fails Stop");
    let module = parse_module(&source).expect("Boolean expect condition parses");
    let core = compile_to_core(&module, &context())
        .unwrap_or_else(|errors| panic!("Boolean expect condition compiles: {errors:#?}"));
    assert_eq!(
        core.functions["select"].failures,
        ["predicate.example@1.Stop"]
    );
    validate_core_module_type_integrity(&core).expect("propagated failure Core is valid");
    for domain in ["echo.span-ir/v2", "echo.span-ir/v3"] {
        let facts = TargetIrLoweringFacts {
            target_profile: ResourceRef {
                coordinate: "echo.dpo@1".into(),
                digest: Some(format!("sha256:{}", "1".repeat(64))),
            },
            target_ir_domain: domain.into(),
            operation_profiles: vec!["continuum.profile.read-only/v1".into()],
            obstruction_coordinates: Vec::new(),
            effect_lowerings: Vec::new(),
            effect_signatures: Vec::new(),
            pure_functions: Vec::new(),
        };
        let report = lower_to_target_ir(&core, &facts);
        assert_eq!(report.status, TargetLoweringStatus::Lowered, "{report:#?}");
        encode_target_ir_artifact(&report.artifact.expect("lowered predicate artifact"))
            .expect("independent Target validation accepts the predicate artifact");
    }
}

fn refused_at(source: &str, kind: CompilerErrorKind, anchor: &str) {
    let module = parse_module(source).expect("failure-scope predicate source parses");
    let errors = compile_to_core(&module, &context())
        .expect_err("expect condition must obey the failure scope");
    let start = source.find(anchor).expect("diagnostic anchor exists");
    assert!(
        errors.iter().any(|error| {
            error.stage == CompilerStage::TypeCheck
                && error.kind == kind
                && error.span.start == start
        }),
        "expected {kind:?} at {start}, got {errors:#?}"
    );
    assert!(
        errors
            .iter()
            .all(|error| error.kind != CompilerErrorKind::ExpectedPredicate),
        "Boolean result typing must preserve the specific failure diagnostic: {errors:#?}"
    );
}

#[test]
fn direct_expect_bool_is_an_expression_condition() {
    accepted_condition("expect checked(value)");
}

#[test]
fn negated_expect_bool_is_an_expression_condition() {
    accepted_condition("!(expect checked(value))");
}

#[test]
fn expect_condition_requires_the_enclosing_function_failure() {
    for condition in ["expect checked(value)", "!(expect checked(value))"] {
        refused_at(
            &selection(condition, ""),
            CompilerErrorKind::UndeclaredFailure,
            "expect checked(value)",
        );
    }
}

#[test]
fn intent_statement_conditions_cannot_propagate_expect_failures() {
    for condition in [
        "expect checked(input.value)",
        "!(expect checked(input.value))",
    ] {
        let source = format!(
            "{PREFIX}\n{}",
            intent(&format!(
                "  if {condition} {{ let selected = 1u64; }} else {{ let selected = 2u64; }}\n  return {{ value: 3u64 }};"
            ))
        );
        refused_at(
            &source,
            CompilerErrorKind::UnhandledFailure,
            "expect checked(input.value)",
        );
    }
}

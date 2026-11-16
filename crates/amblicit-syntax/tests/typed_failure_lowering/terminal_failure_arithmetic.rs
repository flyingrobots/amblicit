//! Terminal failures remain bottom operands for typed unsigned arithmetic.
use super::{core, facts, CoreExpr, CoreModule, TargetLoweringFailureKind, TargetLoweringStatus};
use amblicit_syntax::{
    compile_to_core, lower_to_target_ir, parse_module, CompilerContext, CompilerErrorKind,
    CoreBudget,
};

fn source(width: &str, expression: &str) -> String {
    format!(
        "package rope.example@1;\nfailure Stop;\ntype Input = {{}};\ntype Output = {{ value: {width}, }};\nfn checked() -> {width} fails Stop {{ return {expression}; }}\nintent evaluate(input: Input) returns Output profile p.read basis none budget <= p.small {{\n  let value = checked() else {{ Stop => text.Stop, }};\n  return {{ value }};\n}}"
    )
}

fn lowered_core(width: &str, expression: &str) -> CoreModule {
    core(&source(width, expression))
}

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

fn call_mut(core: &mut CoreModule) -> (&mut Vec<String>, &mut Vec<CoreExpr>) {
    let function = core.functions.get_mut("checked").expect("checked function");
    let CoreExpr::Call {
        callee,
        type_args,
        args,
    } = &mut function.body.result
    else {
        panic!("arithmetic call result")
    };
    assert!(matches!(
        callee.as_str(),
        "core.integer.add" | "core.integer.subtract"
    ));
    (type_args, args)
}

fn assert_lowered_on_both_domains(core: &CoreModule) {
    for domain in ["echo.span-ir/v2", "echo.span-ir/v3"] {
        let report = lower_to_target_ir(core, &facts(domain));
        assert_eq!(
            report.status,
            TargetLoweringStatus::Lowered,
            "{domain}: {report:#?}"
        );
        assert!(
            report.artifact.is_some(),
            "{domain}: expected Target IR artifact"
        );
    }
}

fn assert_invalid_core_on_both_domains(core: &CoreModule) {
    for domain in ["echo.span-ir/v2", "echo.span-ir/v3"] {
        let report = lower_to_target_ir(core, &facts(domain));
        assert_eq!(
            report.status,
            TargetLoweringStatus::Unsupported,
            "{domain}: {report:#?}"
        );
        assert!(
            report.artifact.is_none(),
            "{domain}: invalid Core has no artifact"
        );
        assert_eq!(report.failures.len(), 1, "{domain}: {report:#?}");
        assert_eq!(
            report.failures[0].kind,
            TargetLoweringFailureKind::InvalidCoreIdentity,
            "{domain}: malformed terminal arithmetic must fail Core identity"
        );
    }
}

#[test]
fn terminal_failure_arithmetic_accepts_addition_on_either_side_and_left_subtraction_at_u32_and_u64()
{
    for (width, expression) in [
        ("U32", "fail Stop + 0u32"),
        ("U32", "0u32 + fail Stop"),
        ("U32", "fail Stop - 0u32"),
        ("U64", "fail Stop + 0u64"),
        ("U64", "0u64 + fail Stop"),
        ("U64", "fail Stop - 0u64"),
    ] {
        let compiled = lowered_core(width, expression);
        assert_lowered_on_both_domains(&compiled);
    }
}

#[test]
fn terminal_failure_does_not_bypass_the_subtraction_underflow_proof() {
    let parsed = parse_module(&source("U64", "0u64 - fail Stop"))
        .expect("right-terminal subtraction source parses");
    let errors = compile_to_core(&parsed, &context())
        .expect_err("the source proof contract still rejects unproved subtraction");
    assert!(
        errors.iter().any(|error| {
            error.kind == CompilerErrorKind::UnsupportedSourceShape
                && error.message
                    == "unsigned subtraction requires a dominating proof of no underflow"
        }),
        "expected the existing subtraction proof refusal, got {errors:#?}"
    );
}

#[test]
fn terminal_failure_arithmetic_rejects_a_wrong_width_peer() {
    let mut compiled = lowered_core("U64", "fail Stop - 0u64");
    let (type_args, _) = call_mut(&mut compiled);
    type_args[0] = "U32".to_owned();
    assert_invalid_core_on_both_domains(&compiled);
}

#[test]
fn terminal_failure_arithmetic_rejects_an_undeclared_failure() {
    let mut compiled = lowered_core("U64", "fail Stop - 0u64");
    let (_, args) = call_mut(&mut compiled);
    let terminal = args
        .iter_mut()
        .find(|argument| matches!(argument, CoreExpr::Fail { .. }))
        .expect("terminal operand");
    let CoreExpr::Fail { failure } = terminal else {
        panic!("selected terminal operand remains a fail expression")
    };
    *failure = "rope.example@1.Missing".to_owned();
    assert_invalid_core_on_both_domains(&compiled);
}

#[test]
fn terminal_failure_arithmetic_rejects_a_malformed_failure_identity() {
    let mut compiled = lowered_core("U64", "fail Stop - 0u64");
    let (_, args) = call_mut(&mut compiled);
    let terminal = args
        .iter_mut()
        .find(|argument| matches!(argument, CoreExpr::Fail { .. }))
        .expect("terminal operand");
    let CoreExpr::Fail { failure } = terminal else {
        panic!("selected terminal operand remains a fail expression")
    };
    failure.clear();
    assert_invalid_core_on_both_domains(&compiled);
}

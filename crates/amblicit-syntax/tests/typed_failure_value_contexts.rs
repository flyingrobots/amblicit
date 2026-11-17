//! Contextual failure operands and bounded values returned through `expect`.
//!
//! Oracle: the language spec gives `fail` its contextual type and gives
//! `expect` its callee's result type. Comparison typing and declared list
//! bounds still apply; neither form widens a returning operand's type.
use amblicit_syntax::{
    compile_to_core, encode_target_ir_artifact, lower_to_target_ir, parse_module,
    validate_core_module_type_integrity, CompilerContext, CompilerErrorKind, CompilerStage,
    CoreBudget, CoreModule, ResourceRef, TargetIrLoweringFacts, TargetLoweringStatus,
};

fn context() -> CompilerContext {
    CompilerContext::new()
        .with_operation_profile("p.read", "continuum.profile.read-only/v1")
        .with_budget(
            "p.small",
            CoreBudget {
                max_steps: 1_000_000,
                max_allocated_bytes: u64::MAX,
                max_output_bytes: u64::MAX,
            },
        )
}

fn compiled(source: &str) -> CoreModule {
    let module = parse_module(source).expect("typed-failure value source parses");
    compile_to_core(&module, &context())
        .unwrap_or_else(|errors| panic!("typed-failure value source compiles: {errors:#?}"))
}

fn accepted(source: &str) {
    let core = compiled(source);
    validate_core_module_type_integrity(&core).expect("typed-failure value Core is valid");
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
        encode_target_ir_artifact(&report.artifact.expect("lowered value artifact"))
            .expect("independent Target validation accepts the artifact");
    }
}

fn comparison(ty: &str, expression: &str) -> String {
    format!(
        "package value.example@1;\nfailure Stop;\n\
         type Input = {{ value: {ty}, }};\ntype Output = {{ value: Bool, }};\n\
         fn compare(value: {ty}) -> Bool fails Stop {{ return {expression}; }}\n\
         intent evaluate(input: Input) returns Output profile p.read basis none budget <= p.small {{\n\
           let value = compare(input.value) else {{ Stop => text.ClosureInsufficient }};\n\
           return {{ value }};\n}}"
    )
}

fn list_length(body: &str, input_max: u64) -> String {
    format!(
        "package list.example@1;\nfailure Stop;\n\
         type Items = List<U8,max={input_max}>;\n\
         type Input = {{ xs: Items, flag: Bool, }};\ntype Output = {{ value: U64, }};\n\
         fn checked(xs: Items, flag: Bool) -> List<U8,max=2> fails Stop {{\n\
           return if flag then xs else fail Stop;\n}}\n\
         fn length(xs: Items, flag: Bool) -> U64 fails Stop {{ {body} }}\n\
         intent evaluate(input: Input) returns Output profile p.read basis none budget <= p.small {{\n\
           let value = length(input.xs, input.flag) else {{ Stop => text.ClosureInsufficient }};\n\
           return {{ value }};\n}}"
    )
}

fn type_mismatch(source: &str) {
    let module = parse_module(source).expect("invalid typed-failure value source parses");
    let errors = compile_to_core(&module, &context()).expect_err("invalid value types refuse");
    assert!(
        errors.iter().any(|error| {
            error.stage == CompilerStage::TypeCheck && error.kind == CompilerErrorKind::TypeMismatch
        }),
        "expected a typing refusal, got {errors:#?}"
    );
}

#[test]
fn failure_comparison_operand_takes_its_peer_type() {
    for ty in ["Bool", "U64", "Bytes<max=2>", "Option<U64>"] {
        accepted(&comparison(ty, "value == fail Stop"));
    }
}

#[test]
fn left_failure_comparison_operand_takes_its_peer_type() {
    for ty in ["Bool", "U64"] {
        accepted(&comparison(ty, "fail Stop == value"));
    }
}

#[test]
fn two_failure_comparison_operands_need_a_concrete_peer_type() {
    type_mismatch(&comparison("Bool", "fail Stop == fail Stop"));
}

#[test]
fn failure_comparisons_preserve_operand_compatibility_and_ordering_rules() {
    type_mismatch(&comparison("Bool", "value == 0u64"));
    type_mismatch(&comparison("Option<U64>", "value < fail Stop"));
}

#[test]
fn inline_expected_list_result_retains_its_bound() {
    accepted(&list_length("return listLen(expect checked(xs, flag));", 2));
}

#[test]
fn bound_expected_list_result_is_a_supported_control() {
    accepted(&list_length(
        "let kept = expect checked(xs, flag); return listLen(kept);",
        2,
    ));
}

#[test]
fn expect_does_not_narrow_an_oversized_returning_list() {
    type_mismatch(&list_length("return listLen(expect checked(xs, flag));", 3));
}

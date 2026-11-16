//! Terminal failures take Bool context in predicate positions.
//!
//! Oracle: the language spec's contextual `fail` expression and exact failure
//! sets. Native ordered and structured Target judgments must also accept the
//! compiler's predicates. Cost bounds below use a fixed source ledger.
use amblicit_syntax::{
    compile_to_core, encode_target_ir_artifact, lower_to_target_ir, parse_module,
    validate_core_module_type_integrity, CompilerContext, CompilerErrorKind, CompilerStage,
    CoreBudget, CoreExpr, CoreModule, CorePredicate, CoreValue, ResourceRef, TargetIrLoweringFacts,
    TargetLoweringStatus,
};

fn context(steps: u64) -> CompilerContext {
    CompilerContext::new()
        .with_operation_profile("p.read", "continuum.profile.read-only/v1")
        .with_budget(
            "p.small",
            CoreBudget {
                max_steps: steps,
                max_allocated_bytes: u64::MAX,
                max_output_bytes: u64::MAX,
            },
        )
}

fn selection(condition: &str, failures: &str) -> String {
    format!(
        "package terminal.predicate@1;\nfailure Stop;\n\
         type Input = {{ flag: Bool, }};\ntype Output = {{ value: U64, }};\n\
         fn select(flag: Bool) -> U64 {failures} {{\n\
           return if {condition} then 1u64 else 2u64;\n}}\n\
         intent evaluate(input: Input) returns Output profile p.read basis none budget <= p.small {{\n\
           let value = select(input.flag) else {{ Stop => text.Stop }};\n\
           return {{ value }};\n}}"
    )
}

fn compile(source: &str, steps: u64) -> CoreModule {
    let module = parse_module(source).expect("terminal predicate source parses");
    compile_to_core(&module, &context(steps))
        .unwrap_or_else(|errors| panic!("terminal predicate source compiles: {errors:#?}"))
}

fn native_accepts(core: &CoreModule) {
    validate_core_module_type_integrity(core).expect("terminal predicate Core is valid");
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
        let report = lower_to_target_ir(core, &facts);
        assert_eq!(report.status, TargetLoweringStatus::Lowered, "{report:#?}");
        encode_target_ir_artifact(&report.artifact.expect("lowered terminal predicate"))
            .expect("native Target validation accepts terminal predicate");
    }
}

fn accepted(condition: &str) {
    let core = compile(&selection(condition, "fails Stop"), 1_000_000);
    assert_eq!(
        core.functions["select"].failures,
        ["terminal.predicate@1.Stop"]
    );
    native_accepts(&core);
}

fn undeclared(source: &str) {
    let module = parse_module(source).expect("failure-scope source parses");
    let errors = compile_to_core(&module, &context(1_000_000))
        .expect_err("predicate context does not authorize a failure");
    let start = source.find("fail Stop").expect("failure anchor exists");
    assert!(
        errors.iter().any(|error| {
            error.stage == CompilerStage::TypeCheck
                && error.kind == CompilerErrorKind::UndeclaredFailure
                && error.span.start == start
        }),
        "expected UndeclaredFailure at {start}, got {errors:#?}"
    );
    assert!(
        errors
            .iter()
            .all(|error| error.kind != CompilerErrorKind::ExpectedPredicate),
        "predicate typing must preserve the failure-scope diagnostic: {errors:#?}"
    );
}

#[test]
fn direct_terminal_failure_is_a_boolean_condition() {
    accepted("fail Stop");
}

#[test]
fn negated_terminal_failure_is_a_boolean_condition() {
    accepted("!(fail Stop)");
}

#[test]
fn logical_and_contextualizes_terminal_failures_on_either_side() {
    for condition in ["flag && fail Stop", "fail Stop && flag"] {
        accepted(condition);
    }
}

#[test]
fn logical_or_contextualizes_terminal_failures_on_either_side() {
    for condition in ["flag || fail Stop", "fail Stop || flag"] {
        accepted(condition);
    }
}

#[test]
fn terminal_predicates_preserve_function_and_intent_failure_scopes() {
    for condition in [
        "fail Stop",
        "!(fail Stop)",
        "flag && fail Stop",
        "flag || fail Stop",
    ] {
        undeclared(&selection(condition, ""));
    }
    undeclared(
        "package terminal.intent@1; failure Stop; type Input = {}; type Output = {};\n\
         intent evaluate(input: Input) returns Output profile p.read basis none budget <= p.small {\n\
           if fail Stop { let chosen = 1u64; }\n  return {};\n}",
    );
}

#[test]
fn terminal_predicates_charge_the_synthetic_boolean_comparison() {
    // Independent source ledger (portable cell/node rules, TIR-REQ-065):
    // intent frame4 + empty-list let3 + listLen let134 + empty result2 =143.
    // select(): invocation1 + return validation1 + If result evaluation2
    //           + predicate(contextual Fail evaluation2 + synthetic true2
    //                       + comparison work64 + predicate node1)
    //           + selected literal2 =75.
    // Mapped select let: call result2 + select75 + statement/binding1 =78.
    // Total221; predicate Not adds one. Target assigns Fail no returned cell,
    // so source and Target bounds need not coincide. No threshold is searched.
    for (condition, negated, steps) in [("fail Stop", false, 221), ("!(fail Stop)", true, 222)] {
        let source = format!(
            "package terminal.cost@1; failure Stop; type Input = {{}}; type Output = {{}};\n\
             fn select() -> U64 fails Stop {{ return if {condition} then 0u64 else 1u64; }}\n\
             intent evaluate(input: Input) returns Output profile p.read basis none budget <= p.small {{\n\
               let xs: List<U64,max=0> = []; let length = listLen(xs);\n\
               let selected = select() else {{ Stop => text.Stop }}; return {{}};\n}}"
        );
        let core = compile(&source, steps);
        assert!(core
            .required_core_capabilities
            .iter()
            .any(|c| c == "core.list/v1"));
        let CoreExpr::If { predicate, .. } = &core.functions["select"].body.result else {
            panic!("conditional helper")
        };
        let predicate = if negated {
            let CorePredicate::Not(inner) = predicate.as_ref() else {
                panic!("one predicate negation")
            };
            inner.as_ref()
        } else {
            predicate.as_ref()
        };
        assert!(matches!(predicate, CorePredicate::Compare {
            left: CoreExpr::Fail { failure },
            right: CoreExpr::Const(CoreValue::Bool(true)), ..
        } if failure == "terminal.cost@1.Stop"));
        native_accepts(&core);
        let module = parse_module(&source).expect("fixed-ledger source parses");
        let errors = compile_to_core(&module, &context(steps - 1))
            .expect_err("one step less than the independent ledger refuses");
        assert!(
            errors.iter().any(|error| {
                error.stage == CompilerStage::TypeCheck
                    && error.kind == CompilerErrorKind::InvalidBound
                    && error
                        .message
                        .starts_with(&format!("pure-helper cost steps={steps},"))
            }),
            "{condition}: {errors:#?}"
        );
    }
}

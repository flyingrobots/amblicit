//! Fixed-ledger calibration for the synthetic equality in a Boolean expect predicate.
use super::facts;
use amblicit_syntax::{
    compile_to_core, lower_to_target_ir, parse_module, validate_core_module_type_integrity,
    CompilerContext, CompilerErrorKind, CompilerStage, CoreBudget, CoreExpr, CorePredicate,
    TargetLoweringStatus,
};

fn source(condition: &str) -> String {
    format!(
        r"package predicate.cost@1;
failure Stop;
type Input = {{}};
type Output = {{}};
fn stop() -> Bool fails Stop {{ return fail Stop; }}
fn select() -> U64 fails Stop {{ return if {condition} then 0u64 else 1u64; }}
intent evaluate(input: Input) returns Output profile p.read basis none budget <= p.small {{
  let xs: List<U64, max=0> = [];
  let length = listLen(xs);
  let selected = select() else {{ Stop => text.Stop, }};
  return {{}};
}}
"
    )
}

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

#[test]
fn expect_predicates_charge_the_synthetic_boolean_comparison() {
    // Independent source ledger (portable cell/node rules, TIR-REQ-065):
    // intent frame4 + empty-list let3 + listLen let134 + empty result2 =143.
    // stop(): invocation1 + return validation1 + contextual Fail evaluation2 =4.
    // select(): invocation1 + return validation1 + If result evaluation2
    //           + predicate(stop call result2 + stop4 + synthetic true2
    //                       + comparison work64 + predicate node1)
    //           + selected literal2 =79.
    // mapped select let: call result2 + select79 + statement/binding1 =82.
    // Total source cost225. Target's Fail has no result cell, so its bound224
    // is deliberately one lower. A predicate Not adds one step to each bound.
    // No threshold is searched and no production meter supplies these numbers.
    for (condition, negated, source_steps) in [
        ("expect stop()", false, 225_u64),
        ("!(expect stop())", true, 226_u64),
    ] {
        let source = source(condition);
        let module = parse_module(&source).expect("Boolean expect source parses");
        let compiled = compile_to_core(&module, &context(source_steps))
            .unwrap_or_else(|errors| panic!("fixed source bound accepts {condition}: {errors:#?}"));
        validate_core_module_type_integrity(&compiled).expect("predicate Core integrity");
        assert!(compiled
            .required_core_capabilities
            .iter()
            .any(|c| c == "core.list/v1"));
        let CoreExpr::If { predicate, .. } = &compiled.functions["select"].body.result else {
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
            left: CoreExpr::Call { callee, args, .. },
            right: CoreExpr::Const(amblicit_syntax::CoreValue::Bool(true)), ..
        } if callee == "predicate.cost@1.stop" && args.is_empty()));
        for domain in ["echo.span-ir/v2", "echo.span-ir/v3"] {
            let report = lower_to_target_ir(&compiled, &facts(domain));
            assert_eq!(
                report.status,
                TargetLoweringStatus::Lowered,
                "{domain}: {report:#?}"
            );
            assert!(report.artifact.is_some());
        }
        let errors = compile_to_core(&module, &context(source_steps - 1))
            .expect_err("the fixed source ledger refuses one step less");
        assert!(
            errors.iter().any(|error| {
                error.stage == CompilerStage::TypeCheck
                    && error.kind == CompilerErrorKind::InvalidBound
                    && error
                        .message
                        .starts_with(&format!("pure-helper cost steps={source_steps},"))
            }),
            "{condition}: {errors:#?}"
        );
    }
}

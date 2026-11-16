//! Mutually exclusive failure arms pay for one selected arm, including all its fields.
use super::{core, facts};
use amblicit_syntax::{
    lower_to_target_ir, validate_core_module_type_integrity, CoreExpr, CoreNode, CorePredicate,
    TargetLoweringFailureKind, TargetLoweringStatus,
};

const SOURCE: &str = r"package rope.example@1;
failure Left;
failure Right;
type Input = {};
type Output = {};
fn stop() -> U64 fails Left, Right {
  return if true then fail Left else fail Right;
}

intent evaluate(input: Input) returns Output profile p.read basis none budget <= p.small {
  let xs: List<U64, max=0> = [];
  let stopped = stop() else {
    Left => text.Left({ LEFT_FIELDS }),
    Right => text.Right({ RIGHT_FIELDS }),
  };
  return {};
}
";

#[test]
fn failure_arm_work_is_the_maximum_of_complete_arm_payloads() {
    // Independent ledger under TIR-REQ-065 and the typed-failure terminal rule:
    // input/output validation 4 + empty-list let 3 + empty-record result 2 = 9.
    // stop invocation: invocation 1 + return validation 1 + terminal-if own 1
    //   + true predicate 1 + selected fail evaluation 1 = 5.
    // mapped call: result evaluation 2 + invocation 5 + statement 1 = 8.
    // one listLen payload: result 2 + local evaluation 2 + argument validation 1
    //   + two 64-byte scans = 133. Thus 17 + max(fields per arm) * 133.
    // Every field in the selected payload executes; other arms do not execute.
    for (left, right, required) in [
        ("first: listLen(xs)", "first: listLen(xs)", 150),
        (
            "first: listLen(xs), second: listLen(xs)",
            "first: listLen(xs)",
            283,
        ),
        (
            "first: listLen(xs)",
            "first: listLen(xs), second: listLen(xs)",
            283,
        ),
        (
            "first: listLen(xs), second: listLen(xs)",
            "first: listLen(xs), second: listLen(xs)",
            283,
        ),
    ] {
        let source = SOURCE
            .replace("LEFT_FIELDS", left)
            .replace("RIGHT_FIELDS", right);
        let compiled = core(&source);
        validate_core_module_type_integrity(&compiled).expect("valid authored Core");
        assert!(compiled
            .required_core_capabilities
            .iter()
            .any(|c| c == "core.list/v1"));
        assert!(matches!(
            &compiled.functions["stop"].body.result,
            CoreExpr::If { predicate, then_value, else_value }
                if matches!(predicate.as_ref(), CorePredicate::True)
                    && matches!(then_value.as_ref(), CoreExpr::Fail { .. })
                    && matches!(else_value.as_ref(), CoreExpr::Fail { .. })
        ));
        assert!(
            matches!(compiled.intents["evaluate"].body.nodes.as_slice(), [
            CoreNode::Let { value: CoreExpr::List { values }, .. },
            CoreNode::Attempt { .. },
        ] if values.is_empty())
        );
        for domain in ["echo.span-ir/v2", "echo.span-ir/v3"] {
            for (steps, expected) in [
                (required, TargetLoweringStatus::Lowered),
                (required - 1, TargetLoweringStatus::Unsupported),
            ] {
                let mut candidate = compiled.clone();
                candidate
                    .intents
                    .get_mut("evaluate")
                    .expect("intent")
                    .core_evaluation_budget
                    .max_steps = steps;
                let report = lower_to_target_ir(&candidate, &facts(domain));
                assert_eq!(
                    report.status, expected,
                    "{domain}, {steps} steps: {report:#?}"
                );
                if expected == TargetLoweringStatus::Lowered {
                    assert!(report.artifact.is_some());
                } else {
                    assert!(report.artifact.is_none());
                    assert_eq!(report.failures.len(), 1, "{report:#?}");
                    assert_eq!(
                        report.failures[0].kind,
                        TargetLoweringFailureKind::InvalidCoreIdentity
                    );
                    assert_eq!(
                        report.failures[0].detail,
                        "fold-capable Core work is unestablished or exceeds the evaluation budget"
                    );
                }
            }
        }
    }
}

#[test]
fn failure_arm_step_and_allocation_maxima_can_come_from_different_arms() {
    // The surrounding work allocates 320 bytes: validation 128, list 64,
    // mapped result 64 and final result 64. listLen contributes 133 steps and
    // 128 bytes; three scalar literals contribute 6 steps and 192 bytes.
    // Select each dimension independently: (17 + 133, 320 + 192).
    for (left, right) in [
        ("count: listLen(xs)", "a: 0u64, b: 0u64, c: 0u64"),
        ("a: 0u64, b: 0u64, c: 0u64", "count: listLen(xs)"),
    ] {
        let compiled = core(
            &SOURCE
                .replace("LEFT_FIELDS", left)
                .replace("RIGHT_FIELDS", right),
        );
        for domain in ["echo.span-ir/v2", "echo.span-ir/v3"] {
            for (steps, bytes, expected) in [
                (150, 512, TargetLoweringStatus::Lowered),
                (149, 512, TargetLoweringStatus::Unsupported),
                (150, 511, TargetLoweringStatus::Unsupported),
            ] {
                let mut candidate = compiled.clone();
                let budget = &mut candidate
                    .intents
                    .get_mut("evaluate")
                    .expect("intent")
                    .core_evaluation_budget;
                budget.max_steps = steps;
                budget.max_allocated_bytes = bytes;
                let report = lower_to_target_ir(&candidate, &facts(domain));
                assert_eq!(
                    report.status, expected,
                    "{domain}, {steps} steps/{bytes} bytes: {report:#?}"
                );
                if expected == TargetLoweringStatus::Lowered {
                    assert!(report.artifact.is_some());
                } else {
                    assert!(report.artifact.is_none());
                    assert_eq!(report.failures.len(), 1, "{report:#?}");
                    assert_eq!(
                        report.failures[0].kind,
                        TargetLoweringFailureKind::InvalidCoreIdentity
                    );
                    assert_eq!(
                        report.failures[0].detail,
                        "fold-capable Core work is unestablished or exceeds the evaluation budget"
                    );
                }
            }
        }
    }
}

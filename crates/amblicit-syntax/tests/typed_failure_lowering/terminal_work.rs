//! Independent fixed-cost calibration for the explicit terminal expression.
use super::{core, facts};
use amblicit_syntax::{
    lower_to_target_ir, validate_core_module_type_integrity, CoreExpr, CoreNode,
    TargetLoweringFailureKind, TargetLoweringStatus,
};

const SOURCE: &str = r"package rope.example@1;
failure Stop;
type Input = {};
type Output = {};
fn stop() -> U64 fails Stop { return fail Stop; }
intent evaluate(input: Input) returns Output profile p.read basis none budget <= p.small {
  let xs: List<U64, max=0> = [];
  let length = listLen(xs);
  let stopped = stop() else { Stop => text.Stop, };
  return {};
}
";

#[test]
fn a_terminal_expression_retains_its_independently_counted_evaluation_step() {
    const REQUIRED_STEPS: u64 = 149;
    let compiled = core(SOURCE);
    validate_core_module_type_integrity(&compiled).expect("valid public-source Core");
    assert!(compiled
        .required_core_capabilities
        .iter()
        .any(|c| c == "core.list/v1"));
    let intent = compiled.intents.get("evaluate").expect("intent");
    assert!(intent.basis.is_none());
    assert!(intent.input_constraints.is_empty());
    assert!(matches!(intent.body.nodes.as_slice(), [
        CoreNode::Let { value: CoreExpr::List { values }, .. },
        CoreNode::Let { value: CoreExpr::Call { callee, args, .. }, .. },
        CoreNode::Attempt { .. },
    ] if values.is_empty() && callee == "core.list.length"
        && matches!(args.as_slice(), [CoreExpr::Local { .. }])));
    assert!(matches!(&intent.body.result, CoreExpr::Record { fields } if fields.is_empty()));
    let function = compiled.functions.get("stop").expect("terminal helper");
    assert!(function.params.is_empty());
    assert!(function.body.bindings.is_empty());
    assert!(matches!(&function.body.result, CoreExpr::Fail { .. }));

    // Fixed ledger from the portable cell/node rules (target-ir README,
    // TIR-REQ-065) and the terminal rule (typed-failures README):
    // input/output validation: 2 * (1 + 1) = 4;
    // empty-list let: one cell + evaluation + statement = 3;
    // listLen let: result evaluation 2 + local operand evaluation 2
    //              + operand validation 1 + bounded storage scans (64 + 64)
    //              + statement 1 = 134;
    // mapped call: result cell + evaluation + invocation + return validation
    //             + terminal evaluation + statement = 6;
    // empty-record result: one cell + evaluation = 2. Total = 149.
    // This constant is neither searched nor obtained from the meter under test.
    for domain in ["echo.span-ir/v2", "echo.span-ir/v3"] {
        for (steps, expected) in [
            (REQUIRED_STEPS, TargetLoweringStatus::Lowered),
            (REQUIRED_STEPS - 1, TargetLoweringStatus::Unsupported),
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
                assert_eq!(report.failures[0].intent.as_deref(), Some("evaluate"));
                assert_eq!(report.failures[0].node_index, None);
                assert_eq!(
                    report.failures[0].detail,
                    "fold-capable Core work is unestablished or exceeds the evaluation budget"
                );
            }
        }
    }
}

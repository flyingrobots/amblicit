use std::cell::Cell;

use crate::{
    compile_to_core, lower_to_target_ir, parse_module, validate_core_module_type_integrity,
    CompilerContext, CoreBudget, CoreExpr, CoreMatchArm, CoreModule, CoreValue, ResourceRef,
    TargetIrLoweringFacts, TargetLoweringFailureKind, TargetLoweringStatus,
};

thread_local!(static VISITS: Cell<usize> = const { Cell::new(0) });

pub(super) fn visit() {
    VISITS.with(|visits| visits.set(visits.get() + 1));
}

fn take_visits() -> usize {
    VISITS.with(|visits| visits.replace(0))
}

fn fixture() -> CoreModule {
    let source = "package work.matches@1;
      type Flag = variant { A, B };
      fn chain(flag: Flag) -> Flag { return flag; }
      type Input = { marker: U8 }; type Output = { marker: U8 };
      intent run(input: Input) returns Output profile p.read basis none budget <= p.large {
        let marker = input.marker; return { marker };
      }";
    compile_to_core(
        &parse_module(source).unwrap(),
        &CompilerContext::new()
            .with_operation_profile("p.read", "continuum.profile.read-only/v1")
            .with_budget(
                "p.large",
                CoreBudget {
                    max_steps: 1_000_000,
                    max_allocated_bytes: 1_000_000,
                    max_output_bytes: 1_000_000,
                },
            ),
    )
    .unwrap()
}

#[test]
fn nested_match_scrutinees_have_bounded_rejection_work() {
    let facts = TargetIrLoweringFacts {
        target_profile: ResourceRef {
            coordinate: "echo.dpo@1".into(),
            digest: Some(format!("sha256:{}", "1".repeat(64))),
        },
        target_ir_domain: "echo.span-ir/v3".into(),
        operation_profiles: vec!["continuum.profile.read-only/v1".into()],
        obstruction_coordinates: Vec::new(),
        effect_lowerings: Vec::new(),
        effect_signatures: Vec::new(),
        pure_functions: Vec::new(),
    };
    for levels in [4, 12] {
        for valid in [true, false] {
            let mut core = fixture();
            let function = core.functions.get_mut("chain").unwrap();
            let local = CoreExpr::Local {
                reference: function.params[0].clone(),
            };
            let leaf = if valid {
                local.clone()
            } else {
                CoreExpr::Const(CoreValue::Bool(true))
            };
            function.body.result = (0..levels).fold(leaf, |inner, _| CoreExpr::Match {
                scrutinee: Box::new(inner),
                arms: ["A", "B"]
                    .into_iter()
                    .map(|case| CoreMatchArm {
                        case: case.into(),
                        binder: None,
                        body: local.clone(),
                    })
                    .collect(),
            });
            validate_core_module_type_integrity(&core).expect("structural Core control");
            take_visits();
            let report = lower_to_target_ir(&core, &facts);
            let visits = take_visits();
            if valid {
                assert_eq!(report.status, TargetLoweringStatus::Lowered, "{report:?}");
            } else {
                assert_eq!(
                    report.status,
                    TargetLoweringStatus::Unsupported,
                    "{report:?}"
                );
                assert!(report.artifact.is_none());
                assert!(report.failures.iter().any(|failure| {
                    failure.kind == TargetLoweringFailureKind::InvalidCoreIdentity
                }));
            }
            // Successful lowering checks nested expressions in several contexts.
            // Refusal should propagate immediately from the invalid leaf.
            let budget = if valid {
                4 * (levels + 1) * (levels + 1)
            } else {
                4 * (levels + 1)
            };
            eprintln!("levels={levels}, valid={valid}: {visits} visits, budget={budget}");
            assert!(
                visits <= budget,
                "levels={levels}, valid={valid}: {visits} match-coordinate visits"
            );
        }
    }
}

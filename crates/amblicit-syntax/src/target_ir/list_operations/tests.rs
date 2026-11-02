use std::cell::Cell;

use super::{BTreeMap, CoreExpr, CoreModule, CoreValue};
use crate::{
    compile_to_core, lower_to_target_ir, parse_module, CompilerContext, CoreBudget, ResourceRef,
    TargetIrLoweringFacts, TargetLoweringStatus,
};

thread_local!(static VISITS: Cell<usize> = const { Cell::new(0) });

pub(super) fn visit() {
    VISITS.with(|visits| visits.set(visits.get() + 1));
}

fn take_visits() -> usize {
    VISITS.with(|visits| visits.replace(0))
}

fn compile(expression: &str, maximum: usize) -> CoreModule {
    let source = format!(
        "package work.inference@1;
         fn chain(xs: List<U8,max=1>) -> List<U8,max={maximum}> {{ return {expression}; }}
         type Input = {{ xs: List<U8,max=1> }}; type Output = {{ n: U64 }};
         intent run(input: Input) returns Output profile p.read basis none budget <= p.large {{
           let items = chain(input.xs); let n = listLen(items); return {{ n }};
         }}"
    );
    compile_to_core(
        &parse_module(&source).expect("valid nested list source"),
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
    .expect("bounded source compiles")
}

#[test]
fn nested_list_signatures_have_linear_inference_work() {
    const LEVELS: usize = 12;
    let core = compile("listConcat(xs,xs)", 2);
    let parameter = &core.functions["chain"].params[0];
    let scope = BTreeMap::from([(parameter.id.as_str(), parameter)]);
    let local = CoreExpr::Local {
        reference: parameter.clone(),
    };
    // Signature work is bounded even before the separate append-totality pass.
    for operation in ["append", "concat_left", "concat_right"] {
        let mut expression = local.clone();
        let mut coordinate = parameter.ty.clone();
        for level in 1..=LEVELS {
            let (callee, type_args, args) = match operation {
                "append" => (
                    "core.list.append",
                    vec![coordinate.clone()],
                    vec![
                        expression,
                        CoreExpr::Const(CoreValue::Int {
                            width: "U8".into(),
                            value: "0".into(),
                        }),
                    ],
                ),
                "concat_left" => (
                    "core.list.concat",
                    vec![coordinate.clone(), parameter.ty.clone()],
                    vec![expression, local.clone()],
                ),
                _ => (
                    "core.list.concat",
                    vec![parameter.ty.clone(), coordinate.clone()],
                    vec![local.clone(), expression],
                ),
            };
            expression = CoreExpr::Call {
                callee: callee.into(),
                type_args,
                args,
            };
            if operation != "append" {
                coordinate = format!("List<U8,max={}>", level + 1);
            }
        }
        take_visits();
        assert_eq!(
            super::expression_type_coordinate(&core, &[], &expression, &scope),
            Some(coordinate)
        );
        let visits = take_visits();
        assert!(visits <= 4 * (LEVELS + 1), "{operation}: {visits} visits");
    }
}

#[test]
fn nested_concat_sources_have_bounded_lowering_inference() {
    for levels in [4, 12] {
        for right_nested in [false, true] {
            let expression = (0..levels).fold("xs".to_owned(), |inner, _| {
                if right_nested {
                    format!("listConcat(xs,{inner})")
                } else {
                    format!("listConcat({inner},xs)")
                }
            });
            let core = compile(&expression, levels + 1);
            let facts = TargetIrLoweringFacts {
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
            };
            take_visits();
            let report = lower_to_target_ir(&core, &facts);
            let visits = take_visits();
            assert_eq!(report.status, TargetLoweringStatus::Lowered, "{report:?}");
            assert!(
                visits <= 8 * (levels + 1).pow(2),
                "levels={levels}, right_nested={right_nested}: {visits} visits"
            );
        }
    }
}

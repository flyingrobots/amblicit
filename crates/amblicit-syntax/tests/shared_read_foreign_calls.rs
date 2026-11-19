//! Shared-read schedule over target intrinsic calls (#330).
//!
//! Oracle: the shared-read rule for a foreign call (arguments copied in full,
//! result materialized in full, plus the declared cost template) from
//! `docs/topics/core-cost-schedules/README.md`, metered on actual values by the
//! reference evaluator.
#[path = "core_cost_schedules/reference.rs"]
mod reference;

use amblicit_syntax::{
    admit_target_pure_intrinsics, compile_to_core, digest_target_intrinsics_catalog,
    encode_canonical_cbor, lower_to_target_ir, parse_module, CanonicalValue as C, CompilerContext,
    CoreBudget, CoreCostSchedule, CoreModule, RequiredPureIntrinsic, ResourceRef,
    TargetIrLoweringFacts, TargetLoweringStatus, TargetPureIntrinsicFact,
};
use reference::{Evaluator, Foreign, V};
use std::collections::BTreeMap;

const CATALOG: &str = "echo.dpo@1.intrinsics";
const TOTAL: &str = "echo.dpo@1.total";
const SHARED: &str = "core.cost.shared-reads/v1";
const CEILING: u64 = u64::MAX / 4;
/// Declared cost rule of `total`: base plus a coefficient per list item.
const BASE: u64 = 5;
const PER_ITEM: u64 = 3;

fn text(value: &str) -> C {
    C::Text(value.into())
}

fn map(entries: Vec<(&str, C)>) -> C {
    C::Map(entries.into_iter().map(|(k, v)| (text(k), v)).collect())
}

fn target() -> ResourceRef {
    ResourceRef {
        coordinate: "echo.dpo@1".into(),
        digest: Some(format!("sha256:{}", "1".repeat(64))),
    }
}

fn admitted() -> Vec<TargetPureIntrinsicFact> {
    let catalog = map(vec![
        ("apiVersion", text("edict.target-profile.intrinsics/v2")),
        ("types", C::Map(Vec::new())),
        (
            "intrinsics",
            map(vec![(
                TOTAL,
                map(vec![
                    ("intrinsicClass", text("pure")),
                    ("typeParameters", C::Array(Vec::new())),
                    ("argumentTypes", C::Array(vec![text("List<U64,max=16>")])),
                    ("returnType", text("U64")),
                    ("guardSupport", C::Bool(false)),
                    ("footprintTemplate", text("total.footprint")),
                    ("costTemplate", text("total.cost")),
                    ("writeClass", text("none")),
                    ("contractVersion", text("test.total/v1")),
                    (
                        "costRule",
                        map(vec![
                            ("base", C::Integer(i128::from(BASE))),
                            (
                                "perArgumentMax",
                                C::Array(vec![C::Integer(i128::from(PER_ITEM))]),
                            ),
                        ]),
                    ),
                ]),
            )]),
        ),
    ]);
    let bytes = encode_canonical_cbor(&catalog).expect("canonical catalog");
    let pin = ResourceRef {
        coordinate: CATALOG.into(),
        digest: Some(digest_target_intrinsics_catalog(CATALOG, &bytes).expect("digest")),
    };
    let required = [RequiredPureIntrinsic {
        coordinate: TOTAL.into(),
        contract_version: "test.total/v1".into(),
    }];
    admit_target_pure_intrinsics(&target(), &pin, &bytes, &required).expect("admitted catalog")
}

const SOURCE: &str = "package app.totals@1;
use target echo.dpo@1 digest \"sha256:1111111111111111111111111111111111111111111111111111111111111111\" as echo;
type State = { xs: List<U64, max=16>, sum: U64, i: U64 };
fn step(s: State) -> State {
  return if listLen(s.xs) < 16u64 && s.i < 1000u64
    then { xs: listAppend(s.xs, s.i), sum: echo.total(s.xs), i: s.i + 1u64 }
    else { xs: s.xs, sum: echo.total(s.xs), i: s.i };
}
type In = { xs: List<U64, max=16> };
type Out = { sum: U64 };
intent run(input: In) returns Out profile p.read basis none budget <= p.b {
  let initial: State = { xs: input.xs, sum: 0u64, i: 0u64 };
  let final = fold(initial, step, 20u64);
  return { sum: echo.total(final.xs) };
}";

fn compile(budget: CoreBudget) -> Result<CoreModule, Vec<amblicit_syntax::CompilerError>> {
    compile_source(SOURCE, budget)
}

fn compile_source(
    source: &str,
    budget: CoreBudget,
) -> Result<CoreModule, Vec<amblicit_syntax::CompilerError>> {
    let mut context = CompilerContext::new()
        .with_core_cost_schedule(CoreCostSchedule::SharedReads)
        .with_operation_profile("p.read", "continuum.profile.read-only/v1")
        .with_budget("p.b", budget);
    for fact in admitted() {
        context = context.with_target_pure_intrinsic(fact);
    }
    compile_to_core(&parse_module(source).expect("parses"), &context)
}

fn budget(steps: u64, bytes: u64) -> CoreBudget {
    CoreBudget {
        max_steps: steps,
        max_allocated_bytes: bytes,
        max_output_bytes: CEILING,
    }
}

fn minimum(mut accepts: impl FnMut(u64) -> bool) -> u64 {
    assert!(accepts(CEILING), "generous-budget control");
    let (mut low, mut high) = (0, CEILING);
    while low < high {
        let middle = low + (high - low) / 2;
        if accepts(middle) {
            high = middle;
        } else {
            low = middle + 1;
        }
    }
    high
}

#[test]
fn shared_read_schedule_admits_target_intrinsic_calls() {
    let core = compile(budget(CEILING, CEILING)).unwrap_or_else(|errors| panic!("{errors:#?}"));
    assert!(core.required_core_capabilities.iter().any(|c| c == SHARED));
    let facts = TargetIrLoweringFacts {
        target_profile: target(),
        target_ir_domain: "echo.span-ir/v3".into(),
        operation_profiles: vec!["continuum.profile.read-only/v1".into()],
        obstruction_coordinates: Vec::new(),
        effect_lowerings: Vec::new(),
        effect_signatures: Vec::new(),
        pure_functions: Vec::new(),
    }
    .with_target_pure_intrinsics(&admitted());
    let report = lower_to_target_ir(&core, &facts);
    assert_eq!(report.status, TargetLoweringStatus::Lowered, "{report:#?}");
}

#[test]
fn intrinsic_calls_in_fold_state_stay_within_budget() {
    let steps = minimum(|v| compile(budget(v, CEILING)).is_ok());
    let bytes = minimum(|v| compile(budget(CEILING, v)).is_ok());
    let core = compile(budget(steps, bytes)).expect("accepted at its minimums");
    let mut foreign = BTreeMap::new();
    foreign.insert(
        TOTAL.to_owned(),
        Foreign {
            result: |args| match &args[0] {
                V::List(items) => V::Int(
                    items
                        .iter()
                        .map(|item| match item {
                            V::Int(value) => *value,
                            other => panic!("integer item, got {other:?}"),
                        })
                        .sum(),
                ),
                other => panic!("list argument, got {other:?}"),
            },
            work: |args| match &args[0] {
                V::List(items) => BASE + PER_ITEM * items.len() as u64,
                other => panic!("list argument, got {other:?}"),
            },
        },
    );
    let mut evaluator = Evaluator {
        core: &core,
        meter: reference::Meter::default(),
        foreign,
    };
    // A partly filled input list reaches capacity inside the fold.
    let input = V::Rec(BTreeMap::from([(
        "xs".to_owned(),
        V::List((0..10).map(V::Int).collect()),
    )]));
    let outcome = evaluator.intent("run", input, Clone::clone);
    let metered = evaluator.meter;
    println!("total: static steps={steps} bytes={bytes}; metered {metered:?}; {outcome:?}");
    assert!(outcome.is_ok());
    assert!(
        metered.steps <= steps,
        "metered {} > budget {steps}",
        metered.steps
    );
    assert!(
        metered.bytes <= bytes,
        "metered {} > budget {bytes}",
        metered.bytes
    );
}

/// An intrinsic reached from a bounded loop body, directly and through a
/// helper, alongside one in a fold step (#337).
const LOOP: &str = "package app.totals@1;
use target echo.dpo@1 digest \"sha256:1111111111111111111111111111111111111111111111111111111111111111\" as echo;
type State = { xs: List<U64, max=16>, sum: U64 };
fn step(s: State) -> State { return { xs: s.xs, sum: echo.total(s.xs) }; }
fn summed(xs: List<U64, max=16>) -> U64 { return echo.total(xs); }
type In = { xs: List<U64, max=16> };
type Out = { sum: U64 };
intent run(input: In) returns Out profile p.read basis none budget <= p.b {
  for x in input.xs bounded 16 {
    let direct = echo.total(input.xs);
    let helped = summed(input.xs);
  }
  let final = fold({ xs: input.xs, sum: 0u64 }, step, 4u64);
  return { sum: final.sum };
}";

fn loop_facts() -> TargetIrLoweringFacts {
    TargetIrLoweringFacts {
        target_profile: target(),
        target_ir_domain: "echo.span-ir/v3".into(),
        operation_profiles: vec!["continuum.profile.read-only/v1".into()],
        obstruction_coordinates: Vec::new(),
        effect_lowerings: Vec::new(),
        effect_signatures: Vec::new(),
        pure_functions: Vec::new(),
    }
    .with_target_pure_intrinsics(&admitted())
}

#[test]
fn native_target_lowers_loop_intrinsics_at_the_shared_read_source_minimum() {
    let steps = minimum(|v| compile_source(LOOP, budget(v, CEILING)).is_ok());
    let bytes = minimum(|v| compile_source(LOOP, budget(CEILING, v)).is_ok());
    let core = compile_source(LOOP, budget(steps, bytes)).expect("source minimum compiles");
    let report = lower_to_target_ir(&core, &loop_facts());
    assert_eq!(report.status, TargetLoweringStatus::Lowered, "{report:#?}");
}

#[test]
fn native_target_still_refuses_one_step_below_the_loop_minimum() {
    let steps = minimum(|v| compile_source(LOOP, budget(v, CEILING)).is_ok());
    let mut core = compile_source(LOOP, budget(steps, CEILING)).expect("source minimum compiles");
    core.intents.get_mut("run").unwrap().core_evaluation_budget = budget(steps - 1, CEILING);
    assert_ne!(
        lower_to_target_ir(&core, &loop_facts()).status,
        TargetLoweringStatus::Lowered
    );
}

#[test]
fn loop_and_helper_intrinsic_calls_stay_within_budget() {
    let steps = minimum(|v| compile_source(LOOP, budget(v, CEILING)).is_ok());
    let bytes = minimum(|v| compile_source(LOOP, budget(CEILING, v)).is_ok());
    let core = compile_source(LOOP, budget(steps, bytes)).expect("accepted at its minimums");
    let mut foreign = BTreeMap::new();
    foreign.insert(
        TOTAL.to_owned(),
        Foreign {
            result: |args| match &args[0] {
                V::List(items) => V::Int(items.len() as u128),
                other => panic!("list argument, got {other:?}"),
            },
            work: |args| match &args[0] {
                V::List(items) => BASE + PER_ITEM * items.len() as u64,
                other => panic!("list argument, got {other:?}"),
            },
        },
    );
    let mut evaluator = Evaluator {
        core: &core,
        meter: reference::Meter::default(),
        foreign,
    };
    // A full input list runs every loop iteration at its largest operand.
    let input = V::Rec(BTreeMap::from([(
        "xs".to_owned(),
        V::List((0..16).map(V::Int).collect()),
    )]));
    let outcome = evaluator.intent("run", input, Clone::clone);
    let metered = evaluator.meter;
    println!("loop: static steps={steps} bytes={bytes}; metered {metered:?}; {outcome:?}");
    assert!(outcome.is_ok());
    assert!(
        metered.steps <= steps,
        "metered {} > budget {steps}",
        metered.steps
    );
    assert!(
        metered.bytes <= bytes,
        "metered {} > budget {bytes}",
        metered.bytes
    );
}

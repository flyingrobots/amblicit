//! Reachable-length bounds for lists grown inside folds (#335, level 1).
//!
//! Oracles: a declared list capacity that a fold cannot reach must not change
//! the minimum budget (metamorphic); and the reference evaluator in
//! `core_cost_schedules/reference.rs`, which meters the shared-read schedule on
//! actual values, never exceeds the minimum accepted budget on inputs that
//! drive every list to its largest reachable length.
#[path = "core_cost_schedules/reference.rs"]
mod reference;

use amblicit_syntax::{
    compile_to_core, lower_to_target_ir, parse_module, CompilerContext, CoreBudget,
    CoreCostSchedule, CoreModule, ResourceRef, TargetIrLoweringFacts, TargetLoweringStatus,
};
use reference::{Evaluator, V};
use std::collections::BTreeMap;

const CEILING: u64 = u64::MAX / 4;

fn budget(steps: u64, bytes: u64) -> CoreBudget {
    CoreBudget {
        max_steps: steps,
        max_allocated_bytes: bytes,
        max_output_bytes: CEILING,
    }
}

fn compile(
    source: &str,
    budget: CoreBudget,
) -> Result<CoreModule, Vec<amblicit_syntax::CompilerError>> {
    compile_to_core(
        &parse_module(source).expect("fixture parses"),
        &CompilerContext::new()
            .with_core_cost_schedule(CoreCostSchedule::SharedReads)
            .with_operation_profile("p.read", "continuum.profile.read-only/v1")
            .with_budget("p.b", budget),
    )
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

/// Minimum accepted (steps, bytes) at source admission.
fn minimums(source: &str) -> (u64, u64) {
    if let Err(errors) = compile(source, budget(CEILING, CEILING)) {
        panic!("fixture compiles: {errors:#?}");
    }
    (
        minimum(|v| compile(source, budget(v, CEILING)).is_ok()),
        minimum(|v| compile(source, budget(CEILING, v)).is_ok()),
    )
}

fn facts() -> TargetIrLoweringFacts {
    TargetIrLoweringFacts {
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
    }
}

/// Native Target admits the source minimum and refuses one step below it.
fn assert_native_agrees(source: &str, (steps, bytes): (u64, u64)) {
    let core = compile(source, budget(steps, bytes)).expect("source minimum compiles");
    assert_eq!(
        lower_to_target_ir(&core, &facts()).status,
        TargetLoweringStatus::Lowered
    );
    let mut below = core.clone();
    for intent in below.intents.values_mut() {
        intent.core_evaluation_budget = budget(steps - 1, bytes);
    }
    assert_ne!(
        lower_to_target_ir(&below, &facts()).status,
        TargetLoweringStatus::Lowered
    );
}

/// Run at the minimum budgets; metered work must stay within both.
fn assert_sound(name: &str, source: &str, input: V) {
    let (steps, bytes) = minimums(source);
    let core = compile(source, budget(steps, bytes)).expect("accepted at its minimums");
    let mut evaluator = Evaluator {
        core: &core,
        meter: reference::Meter::default(),
        foreign: BTreeMap::new(),
    };
    let outcome = evaluator.intent("run", input, Clone::clone);
    let metered = evaluator.meter;
    println!("{name}: static steps={steps} bytes={bytes}; metered {metered:?}");
    assert!(outcome.is_ok(), "{name}: {outcome:?}");
    assert!(
        metered.steps <= steps,
        "{name}: metered {} > budget {steps}",
        metered.steps
    );
    assert!(
        metered.bytes <= bytes,
        "{name}: metered {} > budget {bytes}",
        metered.bytes
    );
}

fn input(n: u128) -> V {
    V::Rec(BTreeMap::from([(
        "xs".to_owned(),
        V::List((0..n).map(V::Int).collect()),
    )]))
}

/// One append per round for `rounds` rounds into a list of capacity `max`.
fn growing(max: u64, rounds: u64) -> String {
    format!(
        "package cost.reach@1;
type State = {{ xs: List<U64, max={max}>, i: U64 }};
fn step(s: State) -> State {{
  return if listLen(s.xs) < {max}u64 && s.i < 100000u64
    then {{ xs: listAppend(s.xs, s.i), i: s.i + 1u64 }}
    else s;
}}
type In = {{ xs: List<U64, max=4> }};
type Out = {{ n: U64 }};
intent run(input: In) returns Out profile p.read basis none budget <= p.b {{
  let initial: State = {{ xs: [], i: 0u64 }};
  let final = fold(initial, step, {rounds}u64);
  return {{ n: listLen(final.xs) }};
}}"
    )
}

/// An outer fold whose step runs an inner fold that appends to the outer state.
fn nested(max: u64) -> String {
    format!(
        "package cost.reach@1;
type Inner = {{ xs: List<U64, max={max}>, j: U64 }};
type Outer = {{ xs: List<U64, max={max}>, i: U64 }};
fn innerStep(s: Inner) -> Inner {{
  return if listLen(s.xs) < {max}u64 && s.j < 100000u64
    then {{ xs: listAppend(s.xs, s.j), j: s.j + 1u64 }}
    else s;
}}
fn outerStep(s: Outer) -> Outer {{
  let grown = fold({{ xs: s.xs, j: 0u64 }}, innerStep, 6u64);
  return if s.i < 100000u64 then {{ xs: grown.xs, i: s.i + 1u64 }} else s;
}}
type In = {{ xs: List<U64, max=4> }};
type Out = {{ n: U64 }};
intent run(input: In) returns Out profile p.read basis none budget <= p.b {{
  let initial: Outer = {{ xs: [], i: 0u64 }};
  let final = fold(initial, outerStep, 6u64);
  return {{ n: listLen(final.xs) }};
}}"
    )
}

#[test]
fn unreachable_list_capacity_does_not_change_the_fold_minimum() {
    assert_eq!(minimums(&growing(512, 8)), minimums(&growing(8, 8)));
}

#[test]
fn nested_fold_growth_is_charged_by_reachable_length() {
    assert_eq!(minimums(&nested(512)), minimums(&nested(36)));
}

#[test]
fn native_target_recomputes_the_reachable_minimum() {
    for source in [growing(512, 8), nested(512)] {
        assert_native_agrees(&source, minimums(&source));
    }
}

#[test]
fn fold_growth_to_and_past_capacity_stays_within_budget() {
    for (name, source) in [
        ("growing-8", growing(512, 8)),
        ("saturating", growing(16, 40)),
        ("nested", nested(512)),
        ("nested-saturating", nested(20)),
    ] {
        assert_sound(name, &source, input(0));
    }
}

/// A step that may replace its list with one of unknown length, or append in
/// one arm and concatenate in the other, must fall back soundly.
const UNKNOWN: &str = "package cost.reach@1;
type State = { xs: List<U64, max=64>, src: List<U64, max=32>, i: U64 };
fn step(s: State) -> State {
  return if s.i == 2u64
    then { xs: listConcat(s.src, s.src), src: s.src, i: s.i + 1u64 }
    else if listLen(s.xs) < 64u64 && s.i < 100000u64
      then { xs: listAppend(s.xs, s.i), src: s.src, i: s.i + 1u64 }
      else s;
}
type In = { xs: List<U64, max=32> };
type Out = { n: U64 };
intent run(input: In) returns Out profile p.read basis none budget <= p.b {
  let initial: State = { xs: [], src: input.xs, i: 0u64 };
  let final = fold(initial, step, 12u64);
  return { n: listLen(final.xs) };
}";

#[test]
fn replaced_and_concatenated_lists_stay_within_budget() {
    assert_sound("unknown", UNKNOWN, input(32));
}

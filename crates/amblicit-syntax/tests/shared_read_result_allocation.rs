//! Shared-read schedule: prelude calls that build a new value charge its
//! allocation (#339).
//!
//! Oracles: `maxAllocatedBytes` is cumulative logical allocation (#328), so a
//! fold that builds a byte or Option value every round allocates at least that
//! value's size every round; and the reference evaluator meters every built
//! value at its actual size and must stay within the minimum budget.
#[path = "core_cost_schedules/reference.rs"]
mod reference;

use amblicit_syntax::{
    compile_to_core, lower_to_target_ir, parse_module, CompilerContext, CoreBudget,
    CoreCostSchedule, CoreModule, ResourceRef, TargetIrLoweringFacts, TargetLoweringStatus,
};
use reference::{Evaluator, V};
use std::collections::BTreeMap;

const CEILING: u64 = u64::MAX / 4;
const ROUNDS: u64 = 8;

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

fn minimums(source: &str) -> (u64, u64) {
    if let Err(errors) = compile(source, budget(CEILING, CEILING)) {
        panic!("fixture compiles: {errors:#?}");
    }
    (
        minimum(|v| compile(source, budget(v, CEILING)).is_ok()),
        minimum(|v| compile(source, budget(CEILING, v)).is_ok()),
    )
}

/// Each round concatenates two 256-byte values and slices the result.
const BYTES: &str = "package cost.alloc@1;
type State = { a: Bytes<max=256>, n: U64 };
fn step(s: State) -> State {
  let joined = s.a + s.a;
  let part = slice(joined, 0u64, len(joined));
  return { a: s.a, n: len(part) };
}
type In = { a: Bytes<max=256> };
type Out = { n: U64 };
intent run(input: In) returns Out profile p.read basis none budget <= p.b {
  let initial: State = { a: input.a, n: 0u64 };
  let final = fold(initial, step, ROUNDSu64);
  return { n: final.n };
}";

/// Each round builds a present Option of a 256-byte value and reads it back.
const OPTIONS: &str = "package cost.alloc@1;
type State = { a: Bytes<max=256>, kept: Option<Bytes<max=256>> };
fn step(s: State) -> State { return { a: s.a, kept: some(s.a) }; }
type In = { a: Bytes<max=256> };
type Out = { present: Bool };
intent run(input: In) returns Out profile p.read basis none budget <= p.b {
  let initial: State = { a: input.a, kept: none() };
  let final = fold(initial, step, ROUNDSu64);
  return { present: isSome(final.kept) };
}";

/// The source with its fold bound set to `rounds`.
fn rounds(source: &str, rounds: u64) -> String {
    source.replace("ROUNDSu64", &format!("{rounds}u64"))
}

/// Allocation the minimum budget adds for `ROUNDS` more fold rounds.
fn added_bytes_per_extra_rounds(source: &str) -> u64 {
    minimums(&rounds(source, 2 * ROUNDS)).1 - minimums(&rounds(source, ROUNDS)).1
}

#[test]
fn byte_results_built_each_round_are_charged_their_allocation() {
    // Each round, concat and slice each build a value of up to 512 bytes,
    // one cell (64 bytes) plus its payload.
    let added = added_bytes_per_extra_rounds(BYTES);
    assert!(added >= ROUNDS * 2 * (64 + 512), "added bytes {added}");
}

#[test]
fn option_results_built_each_round_are_charged_their_allocation() {
    // A present Option is one cell plus its 256-byte payload value.
    let added = added_bytes_per_extra_rounds(OPTIONS);
    assert!(added >= ROUNDS * (64 + 64 + 256), "added bytes {added}");
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

#[test]
fn native_target_refuses_one_byte_below_the_source_minimum() {
    for source in [rounds(BYTES, ROUNDS), rounds(OPTIONS, ROUNDS)] {
        let source = source.as_str();
        let (steps, bytes) = minimums(source);
        let core = compile(source, budget(steps, bytes)).expect("source minimum compiles");
        assert_eq!(
            lower_to_target_ir(&core, &facts()).status,
            TargetLoweringStatus::Lowered
        );
        let mut below = core.clone();
        below.intents.get_mut("run").unwrap().core_evaluation_budget = budget(steps, bytes - 1);
        assert_ne!(
            lower_to_target_ir(&below, &facts()).status,
            TargetLoweringStatus::Lowered
        );
    }
}

#[test]
fn built_byte_and_option_values_stay_within_budget() {
    for (name, source) in [
        ("bytes", rounds(BYTES, ROUNDS)),
        ("options", rounds(OPTIONS, ROUNDS)),
    ] {
        let source = source.as_str();
        let (steps, bytes) = minimums(source);
        let core = compile(source, budget(steps, bytes)).expect("accepted at its minimums");
        let mut evaluator = Evaluator {
            core: &core,
            meter: reference::Meter::default(),
            foreign: BTreeMap::new(),
        };
        let input = V::Rec(BTreeMap::from([("a".to_owned(), V::Bytes(vec![7; 256]))]));
        let outcome = evaluator.intent("run", input, Clone::clone);
        let metered = evaluator.meter;
        println!("{name}: static steps={steps} bytes={bytes}; metered {metered:?}; {outcome:?}");
        assert!(outcome.is_ok(), "{name}: {outcome:?}");
        assert!(
            metered.steps <= steps,
            "{name}: metered {} > {steps}",
            metered.steps
        );
        assert!(
            metered.bytes <= bytes,
            "{name}: metered {} > {bytes}",
            metered.bytes
        );
    }
}

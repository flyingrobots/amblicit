//! Selectable Core work schedules (#324).
//!
//! Oracles: the copying minimums were measured on main at 1a0178cb; the
//! shared-read rules come from the schedule's specification, and the
//! reference evaluator in `core_cost_schedules/reference.rs` meters them on
//! real inputs independently of the static meter.
#[path = "core_cost_schedules/reference.rs"]
mod reference;

use amblicit_syntax::{
    compile_to_core, lower_to_target_ir, parse_module, CompilerContext, CoreBudget,
    CoreCostSchedule, CoreModule, ResourceRef, TargetIrLoweringFacts, TargetLoweringFailureKind,
    TargetLoweringStatus,
};
use reference::{Evaluator, V};
use std::collections::BTreeMap;

const ROPE: &str = include_str!("../../../fixtures/lang/bounded-descent/avl-rope.amblicit");
const SHARED: &str = "core.cost.shared-reads/v1";
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
    schedule: CoreCostSchedule,
    budget: CoreBudget,
) -> Result<CoreModule, Vec<amblicit_syntax::CompilerError>> {
    compile_to_core(
        &parse_module(source).expect("fixture parses"),
        &CompilerContext::new()
            .with_core_cost_schedule(schedule)
            .with_operation_profile("rope.read", "continuum.profile.read-only/v1")
            .with_operation_profile("p.read", "continuum.profile.read-only/v1")
            .with_budget("rope.split", budget.clone())
            .with_budget("p.b", budget),
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

/// Smallest value in `0..=CEILING` that `accepts`, which must be monotone.
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
fn source_minimums(source: &str, schedule: CoreCostSchedule) -> (u64, u64) {
    if let Err(errors) = compile(source, schedule, budget(CEILING, CEILING)) {
        panic!("fixture compiles under a generous budget: {errors:#?}");
    }
    (
        minimum(|v| compile(source, schedule, budget(v, CEILING)).is_ok()),
        minimum(|v| compile(source, schedule, budget(CEILING, v)).is_ok()),
    )
}

fn native_minimums(core: &CoreModule, intent: &str) -> (u64, u64) {
    let lowered = |b: CoreBudget| {
        let mut core = core.clone();
        core.intents.get_mut(intent).unwrap().core_evaluation_budget = b;
        lower_to_target_ir(&core, &facts()).status == TargetLoweringStatus::Lowered
    };
    (
        minimum(|v| lowered(budget(v, CEILING))),
        minimum(|v| lowered(budget(CEILING, v))),
    )
}

#[test]
fn copying_schedule_keeps_the_rope_minimum_budgets() {
    let core = compile(ROPE, CoreCostSchedule::Copying, budget(CEILING, CEILING)).unwrap();
    assert!(!core.required_core_capabilities.iter().any(|c| c == SHARED));
    assert_eq!(
        source_minimums(ROPE, CoreCostSchedule::Copying),
        (647_943_859, 1_805_252_203)
    );
}

#[test]
fn shared_read_schedule_records_its_capability() {
    let core = compile(
        ROPE,
        CoreCostSchedule::SharedReads,
        budget(CEILING, CEILING),
    )
    .unwrap_or_else(|errors| panic!("{errors:#?}"));
    assert!(core.required_core_capabilities.iter().any(|c| c == SHARED));
    let report = lower_to_target_ir(&core, &facts());
    assert_eq!(report.status, TargetLoweringStatus::Lowered, "{report:#?}");
}

#[test]
fn shared_read_source_and_native_minimums_are_equal() {
    let source = source_minimums(ROPE, CoreCostSchedule::SharedReads);
    let core = compile(
        ROPE,
        CoreCostSchedule::SharedReads,
        budget(CEILING, CEILING),
    )
    .unwrap();
    assert_eq!(source, native_minimums(&core, "splitRope"));
}

#[test]
fn shared_read_schedule_cuts_the_rope_budget_by_two_orders() {
    let (steps, bytes) = source_minimums(ROPE, CoreCostSchedule::SharedReads);
    assert!(steps * 100 <= 647_943_859, "steps {steps}");
    assert!(bytes * 100 <= 1_805_252_203, "bytes {bytes}");
}

#[test]
fn target_without_the_capability_applies_the_copying_schedule() {
    let (steps, bytes) = source_minimums(ROPE, CoreCostSchedule::SharedReads);
    let mut core = compile(ROPE, CoreCostSchedule::SharedReads, budget(steps, bytes)).unwrap();
    assert_eq!(
        lower_to_target_ir(&core, &facts()).status,
        TargetLoweringStatus::Lowered
    );
    core.required_core_capabilities.retain(|c| c != SHARED);
    let refused = lower_to_target_ir(&core, &facts());
    assert_eq!(refused.status, TargetLoweringStatus::Unsupported);
    assert!(
        refused
            .failures
            .iter()
            .any(|f| f.kind == TargetLoweringFailureKind::InvalidCoreIdentity),
        "{:?}",
        refused.failures
    );
}

// ---- Soundness against the reference evaluator ----

fn int(value: u128) -> V {
    V::Int(value)
}

fn rec(fields: &[(&str, V)]) -> V {
    V::Rec(
        fields
            .iter()
            .map(|(k, v)| ((*k).to_owned(), v.clone()))
            .collect::<BTreeMap<_, _>>(),
    )
}

/// Compile at the minimum accepted budgets, evaluate, and require metered work
/// within both budgets. Returns (static steps, metered steps) for reporting.
fn assert_sound(name: &str, source: &str, intent: &str, input: V) {
    let (steps, bytes) = source_minimums(source, CoreCostSchedule::SharedReads);
    let core = compile(source, CoreCostSchedule::SharedReads, budget(steps, bytes))
        .unwrap_or_else(|errors| panic!("{name}: accepted at its minimums: {errors:#?}"));
    let mut evaluator = Evaluator {
        core: &core,
        meter: reference::Meter::default(),
    };
    let outcome = evaluator.intent(intent, input, Clone::clone);
    let metered = evaluator.meter;
    println!(
        "{name}: static steps={steps} bytes={bytes}; metered steps={} bytes={}; outcome={outcome:?}",
        metered.steps, metered.bytes
    );
    assert!(metered.steps > 0, "{name}: evaluator ran");
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

fn items(n: u128) -> V {
    V::List(
        (0..n)
            .map(|i| rec(&[("key", int(i)), ("weight", int(7))]))
            .collect(),
    )
}

const READS: &str = "package cost.adversarial@1;
type Item = { key: U64, weight: U64 };
type State = { items: List<Item, max=32>, cursor: U64, total: U64 };
fn weight(items: List<Item, max=32>, i: U64) -> U64 {
  return if i < listLen(items) then listAt(items, i).weight else 0u64;
}
fn step(s: State) -> State {
  return if s.cursor < 32u64 && s.total <= 1000000u64
      && weight(s.items, s.cursor) == weight(s.items, s.cursor) && weight(s.items, s.cursor) == 7u64
    then { items: s.items, cursor: s.cursor + 1u64, total: s.total + 1u64 }
    else s;
}
type In = { items: List<Item, max=32> };
type Out = { total: U64 };
intent run(input: In) returns Out profile p.read basis none budget <= p.b {
  let initial: State = { items: input.items, cursor: 0u64, total: 0u64 };
  let final = fold(initial, step, 40u64);
  return { total: final.total };
}";

#[test]
fn shared_reads_of_a_full_list_in_fold_state_stay_within_budget() {
    assert_sound("reads", READS, "run", rec(&[("items", items(32))]));
}

const FILL: &str = "package cost.adversarial@1;
type Item = { key: U64, weight: U64 };
type State = { source: List<Item, max=32>, acc: List<Item, max=32>, i: U64 };
fn step(s: State) -> State {
  return if s.acc == s.source then s
    else if s.i < listLen(s.source) && listLen(s.acc) < 32u64 && s.i < 32u64
      then { source: s.source, acc: listAppend(s.acc, listAt(s.source, s.i)), i: s.i + 1u64 }
      else s;
}
type In = { items: List<Item, max=32> };
type Out = { acc: List<Item, max=32> };
intent run(input: In) returns Out profile p.read basis none budget <= p.b {
  let initial: State = { source: input.items, acc: [], i: 0u64 };
  let final = fold(initial, step, 33u64);
  return { acc: final.acc };
}";

#[test]
fn full_scans_and_appends_to_capacity_stay_within_budget() {
    assert_sound("fill", FILL, "run", rec(&[("items", items(32))]));
}

const NESTED: &str = "package cost.adversarial@1;
type Item = { key: U64, weight: U64 };
type Inner = { items: List<Item, max=16>, n: U64 };
type Outer = { inner: Inner, rounds: U64 };
fn innerStep(s: Inner) -> Inner {
  return if listLen(s.items) < 16u64 && s.n < 1000u64
    then { items: listAppend(s.items, { key: s.n, weight: 7u64 }), n: s.n + 1u64 }
    else s;
}
fn outerStep(s: Outer) -> Outer {
  let empty: Inner = { items: [], n: 0u64 };
  let refilled = fold(empty, innerStep, 16u64);
  return if s.rounds < 100u64 then { inner: refilled, rounds: s.rounds + 1u64 } else s;
}
type In = { rounds: U64 };
type Out = { n: U64 };
intent run(input: In) returns Out profile p.read basis none budget <= p.b {
  let start: Inner = { items: [], n: 0u64 };
  let initial: Outer = { inner: start, rounds: 0u64 };
  let final = fold(initial, outerStep, 8u64);
  return { n: final.inner.n };
}";

#[test]
fn nested_folds_that_rebuild_lists_stay_within_budget() {
    assert_sound("nested", NESTED, "run", rec(&[("rounds", int(0))]));
}

const MATCHES: &str = "package cost.adversarial@1;
type Item = { key: U64, weight: U64 };
type Walk = variant { Going(List<Item, max=32>), Done(U64) };
fn step(w: Walk) -> Walk {
  return match w {
    Going(items) => if listLen(items) < 32u64 then Walk::Going(listAppend(items, { key: 1u64, weight: 7u64 }))
      else Walk::Done(listLen(items)),
    Done(n) => Walk::Done(n),
  };
}
type In = { items: List<Item, max=32> };
type Out = { n: U64 };
intent run(input: In) returns Out profile p.read basis none budget <= p.b {
  let final = fold(Walk::Going(input.items), step, 40u64);
  let n = match final { Going(items) => listLen(items), Done(total) => total, };
  return { n };
}";

#[test]
fn variant_state_with_payload_binders_stays_within_budget() {
    assert_sound("matches", MATCHES, "run", rec(&[("items", items(0))]));
}

/// A complete AVL tree over `leaves` leaves of `length` bytes each, laid out
/// leaves first; returns the arena and the root index.
fn arena(leaves: usize, length: u128) -> (V, u128) {
    let node = |length: u128, height: u128, left: u128, right: u128, leaf: bool| {
        rec(&[
            ("length", int(length)),
            ("height", int(height)),
            ("left", int(left)),
            ("right", int(right)),
            ("leaf", V::Bool(leaf)),
        ])
    };
    let absent = 4_294_967_295;
    let mut nodes: Vec<V> = (0..leaves)
        .map(|_| node(length, 1, absent, absent, true))
        .collect();
    let mut level: Vec<(u128, u128, u128)> = (0..leaves).map(|i| (i as u128, length, 1)).collect();
    while level.len() > 1 {
        let mut next = Vec::new();
        for pair in level.chunks(2) {
            let ((l, ll, lh), (r, rl, rh)) = (pair[0], pair[1]);
            let index = nodes.len() as u128;
            nodes.push(node(ll + rl, lh.max(rh) + 1, l, r, false));
            next.push((index, ll + rl, lh.max(rh) + 1));
        }
        level = next;
    }
    (V::List(nodes), level[0].0)
}

#[test]
fn rope_split_on_a_deep_tree_stays_within_budget() {
    // 16 leaves give height 5 in 31 nodes, leaving room for the fresh nodes
    // a split appends; the offset lands inside a middle leaf.
    // Offsets cover both ends, leaf edges and leaf interiors.
    let (nodes, root) = arena(16, 100);
    for offset in [0, 1, 99, 100, 750, 1501, 1599, 1600] {
        assert_sound(
            &format!("rope@{offset}"),
            ROPE,
            "splitRope",
            rec(&[
                ("arena", nodes.clone()),
                ("root", int(root)),
                ("offset", int(offset)),
            ]),
        );
    }
}

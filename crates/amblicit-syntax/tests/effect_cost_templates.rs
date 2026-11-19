//! Provider-published effect cost templates (#341).
//!
//! Oracle: a template's charge is added to every effect node bound to it, so
//! the minimum accepted budget grows by exactly the template's value for one
//! effect node, under both schedules; and native Target recomputes the same
//! charge from the facts it receives.
use amblicit_syntax::{
    admit_target_effect_costs, compile_to_core, decode_lawpack_adapter, decode_lawpack_bundle,
    digest_target_intrinsics_catalog, encode_canonical_cbor, lower_to_target_ir, parse_module,
    prepare_lawpack_compilation, CanonicalValue as C, CompilerContext, CompilerErrorKind,
    CoreBudget, CoreCostSchedule, CoreModule, ResourceRef, TargetEffectCostFact,
    TargetIrLoweringFacts, TargetLoweringStatus, TARGET_INTRINSICS_API_VERSION,
    TARGET_INTRINSICS_EFFECT_COST_API_VERSION,
};
use std::collections::BTreeMap;

const MANIFEST: &[u8] = include_bytes!("../../../fixtures/lawpack/hello-echo/manifest.cbor");
const EXPORTS: &[u8] = include_bytes!("../../../fixtures/lawpack/hello-echo/exports.cbor");
const ADAPTER: &[u8] = include_bytes!("../../../fixtures/lawpack/hello-echo/adapter.cbor");
const SOURCE: &str = include_str!("../../../fixtures/lawpack/hello-echo/create-greeting.amblicit");
const INTRINSIC: &str = "echo.dpo@1.anchored-node-attachment-create-if-absent";
const CATALOG: &str = "echo.dpo@1.intrinsics";
const CEILING: u64 = u64::MAX / 4;
/// The published parameter the test template reads.
const SCANS: u64 = 7;
/// The test template: steps `1000 + maxEdgeScans`, bytes `4096 + 3·maxEdgeScans`.
const STEPS: u64 = 1000 + SCANS;
const BYTES: u64 = 4096 + 3 * SCANS;

fn text(value: &str) -> C {
    C::Text(value.into())
}

fn map(entries: Vec<(&str, C)>) -> C {
    C::Map(entries.into_iter().map(|(k, v)| (text(k), v)).collect())
}

fn int(value: u64) -> C {
    C::Integer(i128::from(value))
}

fn constant(value: u64) -> C {
    map(vec![("const", int(value))])
}

fn scans() -> C {
    map(vec![(
        "measure",
        map(vec![("parameter", text("maxEdgeScans"))]),
    )])
}

fn catalog(version: &str) -> Vec<u8> {
    let rule = map(vec![
        (
            "steps",
            map(vec![("sum", C::Array(vec![constant(1000), scans()]))]),
        ),
        (
            "bytes",
            map(vec![(
                "sum",
                C::Array(vec![
                    constant(4096),
                    map(vec![("product", C::Array(vec![constant(3), scans()]))]),
                ]),
            )]),
        ),
    ]);
    let entry = map(vec![
        ("intrinsicClass", text("effect")),
        ("effectCostRule", rule),
    ]);
    encode_canonical_cbor(&map(vec![
        ("apiVersion", text(version)),
        ("intrinsics", map(vec![(INTRINSIC, entry)])),
        ("types", C::Map(Vec::new())),
    ]))
    .expect("canonical catalog")
}

/// Compiler context and Target facts for the Hello Echo effect program, with
/// the effect templates of a catalog of `version` when `with_templates`.
fn prepared(
    version: Option<&str>,
    parameters: &BTreeMap<String, u64>,
) -> (
    CompilerContext,
    TargetIrLoweringFacts,
    Vec<TargetEffectCostFact>,
) {
    let bundle = decode_lawpack_bundle(MANIFEST, EXPORTS).expect("Hello Echo lawpack");
    let adapter = decode_lawpack_adapter(&bundle, "echo.dpo@1", ADAPTER).expect("adapter");
    let module = parse_module(SOURCE).expect("source");
    let preparation = prepare_lawpack_compilation(&module, &bundle, &adapter).expect("facts");
    let facts = preparation.target_ir_facts().clone();
    let costs = match version {
        Some(version) => {
            let bytes = catalog(version);
            let pin = ResourceRef {
                coordinate: CATALOG.into(),
                digest: Some(digest_target_intrinsics_catalog(CATALOG, &bytes).expect("digest")),
            };
            admit_target_effect_costs(
                &facts.target_profile,
                &pin,
                &bytes,
                &facts.effect_lowerings,
                parameters,
            )
            .expect("templates admitted")
        }
        None => Vec::new(),
    };
    (
        preparation
            .compiler_context()
            .clone()
            .with_target_effect_costs(&costs),
        facts.with_target_effect_costs(&costs),
        costs,
    )
}

fn published() -> BTreeMap<String, u64> {
    BTreeMap::from([("maxEdgeScans".to_owned(), SCANS)])
}

fn compile(
    context: &CompilerContext,
    schedule: CoreCostSchedule,
    budget: CoreBudget,
) -> Result<CoreModule, Vec<amblicit_syntax::CompilerError>> {
    compile_to_core(
        &parse_module(SOURCE).expect("source"),
        &context
            .clone()
            .with_core_cost_schedule(schedule)
            .with_budget("hello.smallCreateBudget", budget),
    )
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

fn minimums(context: &CompilerContext, schedule: CoreCostSchedule) -> (u64, u64) {
    if let Err(errors) = compile(context, schedule, budget(CEILING, CEILING)) {
        panic!("compiles under a generous budget: {errors:#?}");
    }
    (
        minimum(|v| compile(context, schedule, budget(v, CEILING)).is_ok()),
        minimum(|v| compile(context, schedule, budget(CEILING, v)).is_ok()),
    )
}

#[test]
fn copying_schedule_adds_each_effect_template_to_the_minimum() {
    let (plain, _, _) = prepared(None, &published());
    let (charged, _, costs) = prepared(
        Some(TARGET_INTRINSICS_EFFECT_COST_API_VERSION),
        &published(),
    );
    assert_eq!(costs.len(), 1);
    let (steps, bytes) = minimums(&plain, CoreCostSchedule::Copying);
    assert_eq!(
        minimums(&charged, CoreCostSchedule::Copying),
        (steps + STEPS, bytes + BYTES)
    );
}

#[test]
fn shared_read_schedule_adds_each_effect_template_to_the_minimum() {
    let (plain, _, _) = prepared(None, &published());
    let (charged, _, _) = prepared(
        Some(TARGET_INTRINSICS_EFFECT_COST_API_VERSION),
        &published(),
    );
    let (steps, bytes) = minimums(&plain, CoreCostSchedule::SharedReads);
    assert_eq!(
        minimums(&charged, CoreCostSchedule::SharedReads),
        (steps + STEPS, bytes + BYTES)
    );
}

#[test]
fn native_target_admits_the_minimum_and_refuses_one_below() {
    let (charged, facts, _) = prepared(
        Some(TARGET_INTRINSICS_EFFECT_COST_API_VERSION),
        &published(),
    );
    let (steps, bytes) = minimums(&charged, CoreCostSchedule::SharedReads);
    let core = compile(
        &charged,
        CoreCostSchedule::SharedReads,
        budget(steps, bytes),
    )
    .expect("source minimum compiles");
    let report = lower_to_target_ir(&core, &facts);
    assert_eq!(report.status, TargetLoweringStatus::Lowered, "{report:#?}");
    for below in [budget(steps - 1, bytes), budget(steps, bytes - 1)] {
        let mut core = core.clone();
        core.intents
            .get_mut("createGreeting")
            .unwrap()
            .core_evaluation_budget = below;
        assert_ne!(
            lower_to_target_ir(&core, &facts).status,
            TargetLoweringStatus::Lowered
        );
    }
}

#[test]
fn a_v2_catalog_publishes_no_templates_and_changes_nothing() {
    let (plain, _, _) = prepared(None, &published());
    let (v2, _, costs) = prepared(Some(TARGET_INTRINSICS_API_VERSION), &published());
    assert!(costs.is_empty());
    for schedule in [CoreCostSchedule::Copying, CoreCostSchedule::SharedReads] {
        assert_eq!(minimums(&plain, schedule), minimums(&v2, schedule));
    }
}

#[test]
fn a_template_reading_an_unpublished_parameter_is_refused() {
    let (charged, _, _) = prepared(
        Some(TARGET_INTRINSICS_EFFECT_COST_API_VERSION),
        &BTreeMap::new(),
    );
    for schedule in [CoreCostSchedule::Copying, CoreCostSchedule::SharedReads] {
        let errors = compile(&charged, schedule, budget(CEILING, CEILING))
            .expect_err("an unestablished template refuses");
        assert!(
            errors
                .iter()
                .all(|e| e.kind == CompilerErrorKind::InvalidBound),
            "{errors:?}"
        );
    }
}

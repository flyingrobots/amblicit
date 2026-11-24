//! Provider-published effect cost templates (#341).
//!
//! Oracles:
//! - A template replaces the generic charge of the effect step it covers: with
//!   a zero template the step costs nothing beyond its input expression, and a
//!   template of value T raises the shared-read minimum by exactly T.
//! - Native Target recomputes the same charge from the facts it receives.
//! - Template parameters come from the effect's operation configuration at the
//!   paths the catalog declares; a `helper` parameter names a module helper
//!   that is priced by its own cost.
use amblicit_syntax::{
    admit_target_effect_costs, admit_target_pure_intrinsics, compile_to_core,
    decode_lawpack_adapter, decode_lawpack_bundle, digest_target_intrinsics_catalog,
    encode_canonical_cbor, lower_to_target_ir, parse_module, prepare_lawpack_compilation,
    CanonicalValue as C, CompilerContext, CompilerErrorKind, CoreBudget, CoreCostSchedule,
    CoreModule, ResourceRef, TargetEffectCostFact, TargetIrLoweringFacts, TargetLoweringStatus,
    TargetPureIntrinsicFailureKind, TARGET_INTRINSICS_API_VERSION,
    TARGET_INTRINSICS_EFFECT_COST_API_VERSION,
};

const MANIFEST: &[u8] = include_bytes!("../../../fixtures/lawpack/hello-echo/manifest.cbor");
const EXPORTS: &[u8] = include_bytes!("../../../fixtures/lawpack/hello-echo/exports.cbor");
const ADAPTER: &[u8] = include_bytes!("../../../fixtures/lawpack/hello-echo/adapter.cbor");
const SOURCE: &str = include_str!("../../../fixtures/lawpack/hello-echo/create-greeting.amblicit");
const INTRINSIC: &str = "echo.dpo@1.anchored-node-attachment-create-if-absent";
const CATALOG: &str = "echo.dpo@1.intrinsics";
const CEILING: u64 = u64::MAX / 4;
/// The published `maxEdgeScans` value.
const SCANS: u64 = 7;
/// The scan template: steps `1000 + maxEdgeScans`, bytes `4096 + 3·maxEdgeScans`.
const STEPS: u64 = 1000 + SCANS;
const BYTES: u64 = 4096 + 3 * SCANS;
const V3: &str = TARGET_INTRINSICS_EFFECT_COST_API_VERSION;

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

fn measure(inner: C) -> C {
    map(vec![("measure", inner)])
}

fn scans() -> C {
    measure(map(vec![("parameter", text("maxEdgeScans"))]))
}

fn scan_rule() -> C {
    map(vec![
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
    ])
}

fn zero_rule() -> C {
    map(vec![("steps", constant(0)), ("bytes", constant(0))])
}

/// A rule charging one call to the module helper the `selector` parameter names.
fn selector_rule() -> C {
    let half = |of: &str| {
        measure(map(vec![
            ("helper", map(vec![("parameter", text("selector"))])),
            ("of", text(of)),
        ]))
    };
    map(vec![("steps", half("steps")), ("bytes", half("bytes"))])
}

fn costliest_rule() -> C {
    let half = |of: &str| measure(map(vec![("costliestHelper", text(of))]));
    map(vec![("steps", half("steps")), ("bytes", half("bytes"))])
}

/// Echo's declarations: both parameters come from `graphReads`.
fn declarations() -> C {
    let declaration = |field: &str, kind: &str| {
        map(vec![
            ("path", C::Array(vec![text("graphReads"), text(field)])),
            ("kind", text(kind)),
        ])
    };
    map(vec![
        ("maxEdgeScans", declaration("maxEdgeScans", "uint")),
        ("selector", declaration("closureSelector", "helper")),
    ])
}

fn catalog(version: &str, rule: C) -> Vec<u8> {
    let entry = map(vec![
        ("intrinsicClass", text("effect")),
        ("effectCostRule", rule),
    ]);
    let mut fields = vec![
        ("apiVersion", text(version)),
        ("intrinsics", map(vec![(INTRINSIC, entry)])),
        ("types", C::Map(Vec::new())),
    ];
    if version == V3 {
        fields.push(("parameters", declarations()));
    }
    encode_canonical_cbor(&map(fields)).expect("canonical catalog")
}

/// An operation configuration publishing `maxEdgeScans` and, optionally, a selector.
fn configuration(selector: Option<&str>) -> Vec<u8> {
    let mut graph = vec![("maxEdgeScans", int(SCANS))];
    if let Some(selector) = selector {
        graph.push(("closureSelector", text(selector)));
    }
    encode_canonical_cbor(&map(vec![("graphReads", map(graph))])).expect("configuration")
}

struct Prepared {
    context: CompilerContext,
    facts: TargetIrLoweringFacts,
    costs: Vec<TargetEffectCostFact>,
}

fn admit(
    version: &str,
    rule: C,
    configuration: &[u8],
    facts: &TargetIrLoweringFacts,
) -> Result<Vec<TargetEffectCostFact>, Vec<amblicit_syntax::TargetPureIntrinsicFailure>> {
    let bytes = catalog(version, rule);
    let pin = ResourceRef {
        coordinate: CATALOG.into(),
        digest: Some(digest_target_intrinsics_catalog(CATALOG, &bytes).expect("digest")),
    };
    admit_target_effect_costs(
        &facts.target_profile,
        &pin,
        &bytes,
        &facts.effect_lowerings,
        configuration,
    )
}

/// The Hello Echo effect program's facts, with the effect templates of a
/// catalog of `version`, or none.
fn prepared(template: Option<(&str, C)>, selector: Option<&str>) -> Prepared {
    let bundle = decode_lawpack_bundle(MANIFEST, EXPORTS).expect("Hello Echo lawpack");
    let adapter = decode_lawpack_adapter(&bundle, "echo.dpo@1", ADAPTER).expect("adapter");
    let module = parse_module(SOURCE).expect("source");
    let preparation = prepare_lawpack_compilation(&module, &bundle, &adapter).expect("facts");
    let facts = preparation.target_ir_facts().clone();
    let costs = match template {
        Some((version, rule)) => {
            admit(version, rule, &configuration(selector), &facts).expect("templates admitted")
        }
        None => Vec::new(),
    };
    Prepared {
        context: preparation
            .compiler_context()
            .clone()
            .with_target_effect_costs(&costs),
        facts: facts.with_target_effect_costs(&costs),
        costs,
    }
}

fn budget(steps: u64, bytes: u64) -> CoreBudget {
    CoreBudget {
        max_steps: steps,
        max_allocated_bytes: bytes,
        max_output_bytes: CEILING,
    }
}

fn compile(
    context: &CompilerContext,
    source: &str,
    schedule: CoreCostSchedule,
    budget: CoreBudget,
) -> Result<CoreModule, Vec<amblicit_syntax::CompilerError>> {
    compile_to_core(
        &parse_module(source).expect("source"),
        &context
            .clone()
            .with_core_cost_schedule(schedule)
            .with_budget("hello.smallCreateBudget", budget),
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

fn minimums(context: &CompilerContext, source: &str, schedule: CoreCostSchedule) -> (u64, u64) {
    if let Err(errors) = compile(context, source, schedule, budget(CEILING, CEILING)) {
        panic!("compiles under a generous budget: {errors:#?}");
    }
    (
        minimum(|v| compile(context, source, schedule, budget(v, CEILING)).is_ok()),
        minimum(|v| compile(context, source, schedule, budget(CEILING, v)).is_ok()),
    )
}

const SHARED: CoreCostSchedule = CoreCostSchedule::SharedReads;

#[test]
fn a_template_replaces_the_generic_effect_charge() {
    let generic = minimums(&prepared(None, None).context, SOURCE, SHARED);
    let zero = minimums(
        &prepared(Some((V3, zero_rule())), None).context,
        SOURCE,
        SHARED,
    );
    assert!(
        zero.0 < generic.0 && zero.1 < generic.1,
        "{zero:?} vs {generic:?}"
    );
}

#[test]
fn the_shared_read_minimum_is_exactly_the_template_prediction() {
    let zero = minimums(
        &prepared(Some((V3, zero_rule())), None).context,
        SOURCE,
        SHARED,
    );
    let charged = minimums(
        &prepared(Some((V3, scan_rule())), None).context,
        SOURCE,
        SHARED,
    );
    assert_eq!(charged, (zero.0 + STEPS, zero.1 + BYTES));
}

#[test]
fn copying_schedule_charges_templates_on_top_of_its_own_work() {
    let plain = prepared(None, None);
    let charged = prepared(Some((V3, scan_rule())), None);
    let (steps, bytes) = minimums(&plain.context, SOURCE, CoreCostSchedule::Copying);
    assert_eq!(
        minimums(&charged.context, SOURCE, CoreCostSchedule::Copying),
        (steps + STEPS, bytes + BYTES)
    );
}

#[test]
fn native_target_admits_the_minimum_and_refuses_one_below() {
    let charged = prepared(Some((V3, scan_rule())), None);
    assert_eq!(charged.costs.len(), 1);
    let (steps, bytes) = minimums(&charged.context, SOURCE, SHARED);
    let core = compile(&charged.context, SOURCE, SHARED, budget(steps, bytes))
        .expect("source minimum compiles");
    let report = lower_to_target_ir(&core, &charged.facts);
    assert_eq!(report.status, TargetLoweringStatus::Lowered, "{report:#?}");
    for below in [budget(steps - 1, bytes), budget(steps, bytes - 1)] {
        let mut core = core.clone();
        core.intents
            .get_mut("createGreeting")
            .unwrap()
            .core_evaluation_budget = below;
        assert_ne!(
            lower_to_target_ir(&core, &charged.facts).status,
            TargetLoweringStatus::Lowered
        );
    }
}

#[test]
fn a_v2_catalog_publishes_no_templates_and_changes_nothing() {
    let plain = prepared(None, None);
    let v2 = prepared(Some((TARGET_INTRINSICS_API_VERSION, scan_rule())), None);
    assert_eq!(v2.costs, Vec::<TargetEffectCostFact>::new());
    for schedule in [CoreCostSchedule::Copying, SHARED] {
        assert_eq!(
            minimums(&plain.context, SOURCE, schedule),
            minimums(&v2.context, SOURCE, schedule)
        );
    }
}

#[test]
fn a_configuration_without_a_declared_parameter_is_refused_at_admission() {
    let facts = prepared(None, None).facts;
    let unpublished =
        encode_canonical_cbor(&map(vec![("graphReads", C::Map(Vec::new()))])).unwrap();
    let failures = admit(V3, scan_rule(), &unpublished, &facts).expect_err("must resolve");
    assert!(failures
        .iter()
        .all(|failure| failure.kind == TargetPureIntrinsicFailureKind::InvalidParameter));
}

#[test]
fn a_parameter_value_of_the_wrong_kind_is_refused_at_admission() {
    let facts = prepared(None, None).facts;
    let text_scans = encode_canonical_cbor(&map(vec![(
        "graphReads",
        map(vec![("maxEdgeScans", text("7"))]),
    )]))
    .unwrap();
    let failures = admit(V3, scan_rule(), &text_scans, &facts).expect_err("uint required");
    assert!(failures
        .iter()
        .all(|failure| failure.kind == TargetPureIntrinsicFailureKind::InvalidParameter));
    let numeric_selector = encode_canonical_cbor(&map(vec![(
        "graphReads",
        map(vec![("closureSelector", int(3))]),
    )]))
    .unwrap();
    let failures =
        admit(V3, selector_rule(), &numeric_selector, &facts).expect_err("helper name required");
    assert!(failures
        .iter()
        .all(|failure| failure.kind == TargetPureIntrinsicFailureKind::InvalidParameter));
}

#[test]
fn a_parameter_is_read_only_at_its_declared_path() {
    let facts = prepared(None, None).facts;
    let top_level = encode_canonical_cbor(&map(vec![("maxEdgeScans", int(SCANS))])).unwrap();
    let failures =
        admit(V3, scan_rule(), &top_level, &facts).expect_err("undeclared path is not read");
    assert!(
        failures
            .iter()
            .all(|failure| failure.kind == TargetPureIntrinsicFailureKind::InvalidParameter),
        "{failures:?}"
    );
}

/// The program plus `select`, which returns its argument, and `heavy`, which
/// builds an eight-item list.
fn with_helpers() -> String {
    format!(
        "{SOURCE}\nfn select(value: U64) -> U64 {{ return value; }}\nfn heavy(value: U64) -> List<U64, max=8> {{ return [value, value, value, value, value, value, value, value]; }}\n"
    )
}

#[test]
fn a_named_selector_is_charged_its_own_cost_not_the_costliest_helper() {
    let source = with_helpers();
    let select = minimums(
        &prepared(Some((V3, selector_rule())), Some("select")).context,
        &source,
        SHARED,
    );
    let heavy = minimums(
        &prepared(Some((V3, selector_rule())), Some("heavy")).context,
        &source,
        SHARED,
    );
    let costliest = minimums(
        &prepared(Some((V3, costliest_rule())), Some("select")).context,
        &source,
        SHARED,
    );
    // `heavy` is the costliest helper, so naming it equals the costliest rule;
    // naming the cheaper `select` charges strictly less.
    assert_eq!(heavy, costliest);
    assert!(
        select.0 < heavy.0 && select.1 < heavy.1,
        "{select:?} vs {heavy:?}"
    );
}

#[test]
fn a_selector_that_is_not_a_module_helper_is_refused() {
    let prepared = prepared(Some((V3, selector_rule())), Some("missing"));
    let errors = compile(
        &prepared.context,
        &with_helpers(),
        SHARED,
        budget(CEILING, CEILING),
    )
    .expect_err("an unresolved selector refuses");
    assert!(
        errors
            .iter()
            .all(|e| e.kind == CompilerErrorKind::InvalidBound),
        "{errors:?}"
    );
}

#[test]
fn the_copying_schedule_keeps_charging_the_costliest_helper_for_a_named_selector() {
    let source = with_helpers();
    let copying = CoreCostSchedule::Copying;
    let select = minimums(
        &prepared(Some((V3, selector_rule())), Some("select")).context,
        &source,
        copying,
    );
    let costliest = minimums(
        &prepared(Some((V3, costliest_rule())), Some("select")).context,
        &source,
        copying,
    );
    assert_eq!(select, costliest);
}

/// The application build admits pure intrinsics and then effect costs from
/// one catalog file; a `/v3` catalog that declares parameters must pass both.
#[test]
fn a_v3_catalog_with_parameters_passes_both_admissions_in_build_order() {
    let facts = prepared(None, None).facts;
    let bytes = catalog(V3, scan_rule());
    let pin = ResourceRef {
        coordinate: CATALOG.into(),
        digest: Some(digest_target_intrinsics_catalog(CATALOG, &bytes).expect("digest")),
    };
    admit_target_pure_intrinsics(&facts.target_profile, &pin, &bytes, &[])
        .expect("pure admission accepts a /v3 parameters map");
    let costs = admit_target_effect_costs(
        &facts.target_profile,
        &pin,
        &bytes,
        &facts.effect_lowerings,
        &configuration(None),
    )
    .expect("effect admission");
    assert_eq!(costs.len(), 1);
}

#[test]
fn a_v2_catalog_with_a_parameters_map_is_still_malformed() {
    let facts = prepared(None, None).facts;
    let entry = map(vec![("intrinsicClass", text("effect"))]);
    let bytes = encode_canonical_cbor(&map(vec![
        ("apiVersion", text(TARGET_INTRINSICS_API_VERSION)),
        ("intrinsics", map(vec![(INTRINSIC, entry)])),
        ("parameters", declarations()),
        ("types", C::Map(Vec::new())),
    ]))
    .unwrap();
    let pin = ResourceRef {
        coordinate: CATALOG.into(),
        digest: Some(digest_target_intrinsics_catalog(CATALOG, &bytes).expect("digest")),
    };
    let failures = admit_target_pure_intrinsics(&facts.target_profile, &pin, &bytes, &[])
        .expect_err("v2 refuses unknown keys");
    assert!(failures
        .iter()
        .all(|failure| failure.kind == TargetPureIntrinsicFailureKind::MalformedCatalog));
}

/// Echo builds one empty obstruction payload, `shape(record {})` = (1, 64),
/// when an effect step maps any obstruction arm. A template cannot see the
/// step's arms, so the replaced charge adds it.
#[test]
fn a_replaced_charge_keeps_the_empty_obstruction_payload() {
    // 37 steps and 4195 bytes are the rest of the program with a zero
    // template; (1, 64) is the obstruction payload and 1 the node step.
    let zero = minimums(
        &prepared(Some((V3, zero_rule())), None).context,
        SOURCE,
        SHARED,
    );
    assert_eq!(zero, (37 + 1 + 1, 4195 + 64));
}

#[test]
fn a_replaced_charge_keeps_the_effect_node_step_outside_the_template() {
    // Echo charges every effect-body node one step, the effect node included;
    // its templates do not contain that step. With the scan template the
    // shared minimum is the rest of the program, the payload, the template
    // and that one step, admitted exactly and refused one step below.
    let charged = prepared(Some((V3, scan_rule())), None);
    let minimum = minimums(&charged.context, SOURCE, SHARED);
    assert_eq!(minimum, (37 + 1 + STEPS + 1, 4195 + 64 + BYTES));
    let (steps, bytes) = minimum;
    assert!(compile(&charged.context, SOURCE, SHARED, budget(steps, bytes)).is_ok());
    assert!(compile(&charged.context, SOURCE, SHARED, budget(steps - 1, bytes)).is_err());
}

/// Hello Echo with one `let` before its effect, bound to a record whose
/// provider shape has 3 validations (the record and its two scalar fields).
fn with_let() -> String {
    SOURCE
        .replace(
            "intent createGreeting",
            "type Pair = { a: U64, b: Bool };\n\nintent createGreeting",
        )
        .replace(
            "  let receipt",
            "  let pair: Pair = { a: 1, b: true };\n  let receipt",
        )
}

/// What the `let` adds to the program's (steps, bytes) minimums.
fn let_delta(context: &CompilerContext, schedule: CoreCostSchedule) -> (u64, u64) {
    let plain = minimums(context, SOURCE, schedule);
    let bound = minimums(context, &with_let(), schedule);
    (bound.0 - plain.0, bound.1 - plain.1)
}

#[test]
fn a_let_in_an_effect_body_is_charged_its_type_validations_under_shared_reads() {
    // 11 steps and 450 bytes are the let's node step and its record
    // construction, as on main 9f10fb91; Echo's effect-body judge adds
    // validations(Pair) = 3.
    for template in [None, Some((V3, zero_rule())), Some((V3, scan_rule()))] {
        let context = prepared(template, None).context;
        assert_eq!(let_delta(&context, SHARED), (11 + 3, 450));
        let (steps, bytes) = minimums(&context, &with_let(), SHARED);
        assert!(compile(&context, &with_let(), SHARED, budget(steps, bytes)).is_ok());
        assert!(compile(&context, &with_let(), SHARED, budget(steps - 1, bytes)).is_err());
    }
}

#[test]
fn copying_minimums_are_unchanged_by_the_shared_read_effect_terms() {
    // Pinned on main 9f10fb91: the copying source check meters this
    // program's template alone, with or without the let; neither shared-read
    // term reaches it.
    let charged = prepared(Some((V3, scan_rule())), None).context;
    for source in [SOURCE.to_owned(), with_let()] {
        assert_eq!(
            minimums(&charged, &source, CoreCostSchedule::Copying),
            (STEPS, BYTES)
        );
    }
}

/// Echo's `/v3` catalog, copied byte for byte from echo `44098b15`
/// (`crates/echo-edict-provider-lowerer/resources/resource.target-intrinsics-3.cbor`).
/// That commit is on Echo's unmerged draft PR #1029, not on Echo's main; the
/// fixture must be re-vendored when #1029 merges.
const ECHO_V3: &[u8] =
    include_bytes!("../../../fixtures/echo-target-intrinsics-v3/resource.target-intrinsics-3.cbor");

#[test]
fn echo_v3_catalog_passes_both_admissions_in_build_order() {
    // Echo's catalog names `echo.dpo@2` types, so the target pin is that
    // profile; its digest form is taken from the Hello Echo fixture.
    let target = ResourceRef {
        coordinate: "echo.dpo@2".into(),
        ..prepared(None, None).facts.target_profile
    };
    let coordinate = "echo.dpo@2.intrinsics";
    let pin = ResourceRef {
        coordinate: coordinate.into(),
        digest: Some(digest_target_intrinsics_catalog(coordinate, ECHO_V3).expect("digest")),
    };
    admit_target_pure_intrinsics(&target, &pin, ECHO_V3, &[]).expect("pure admission");
    let lowerings: Vec<amblicit_syntax::TargetEffectLowering> = [
        "closure-read",
        "create-node",
        "ensure-edge",
        "ensure-node",
        "node-atom-read",
        "outgoing-edge-read",
        "replace-atom",
        "replace-edge",
    ]
    .iter()
    .map(|name| amblicit_syntax::TargetEffectLowering {
        effect: format!("echo.{name}"),
        target_intrinsic: format!("echo.dpo@2.{name}"),
        failure_mappings: std::collections::BTreeMap::new(),
    })
    .collect();
    let costs = admit_target_effect_costs(
        &target,
        &pin,
        ECHO_V3,
        &lowerings,
        &configuration(Some("select")),
    )
    .unwrap_or_else(|failures| panic!("effect admission refused: {failures:?}"));
    assert_eq!(costs.len(), lowerings.len());
}

/// A library whose only function is the costly `heavy`.
const LIBRARY: &str = "package library.heavy@1; fn heavy(value: U64) -> List<U64, max=8> { return [value, value, value, value, value, value, value, value]; }";

/// The program as a two-module source closure: the root (application) module
/// declares the cheap `select` and imports the library that declares `heavy`.
fn linked_root() -> String {
    let import = format!(
        "use module library.heavy@1 digest \"{}\" as lib;",
        amblicit_syntax::source_closure::source_module_digest(LIBRARY).expect("digest")
    );
    let (package, rest) = SOURCE.split_once('\n').expect("package line");
    format!("{package}\n{import}\n{rest}\nfn select(value: U64) -> U64 {{ return value; }}\n")
}

/// Linked-build minimums with the templates of `rule` configured for
/// `selector`, the way the application build passes them to a linked closure.
fn linked_minimums(
    rule: C,
    selector: &str,
    schedule: CoreCostSchedule,
) -> Result<(u64, u64), Vec<amblicit_syntax::TargetPureIntrinsicFailure>> {
    let root = linked_root();
    let closure = amblicit_syntax::source_closure::resolve_source_closure(
        &root,
        &[LIBRARY],
        amblicit_syntax::source_closure::SourceClosureLimits::default(),
    )
    .expect("closure");
    let linked = amblicit_syntax::source_linker::link_source_closure(&closure).expect("links");
    let base = prepared(None, None);
    let costs = admit(V3, rule, &configuration(Some(selector)), &base.facts).expect("admitted");
    let costs = linked.link_effect_costs(&costs)?;
    let context = base.context.with_target_effect_costs(&costs);
    let accepts = |steps: u64, bytes: u64| {
        linked
            .compile(
                &context
                    .clone()
                    .with_core_cost_schedule(schedule)
                    .with_budget("hello.smallCreateBudget", budget(steps, bytes)),
            )
            .map_err(|failures| {
                failures
                    .into_iter()
                    .map(|failure| failure.error.kind)
                    .collect::<Vec<_>>()
            })
    };
    if let Err(kinds) = accepts(CEILING, CEILING) {
        panic!("linked build compiles under a generous budget: {kinds:?}");
    }
    Ok((
        minimum(|v| accepts(v, CEILING).is_ok()),
        minimum(|v| accepts(CEILING, v).is_ok()),
    ))
}

#[test]
fn a_linked_build_charges_the_root_module_selector_under_shared_reads() {
    let linked = linked_minimums(selector_rule(), "select", SHARED).expect("select resolves");
    let costliest = linked_minimums(costliest_rule(), "select", SHARED).expect("resolves");
    // The single-module build of the same helpers charges the same selector.
    let single = minimums(
        &prepared(Some((V3, selector_rule())), Some("select")).context,
        &with_helpers(),
        SHARED,
    );
    assert_eq!(linked, single);
    assert!(
        linked.0 < costliest.0 && linked.1 < costliest.1,
        "{linked:?} vs {costliest:?}"
    );
}

#[test]
fn a_linked_build_keeps_charging_the_costliest_helper_under_copying() {
    let copying = CoreCostSchedule::Copying;
    let select = linked_minimums(selector_rule(), "select", copying).expect("select resolves");
    let costliest = linked_minimums(costliest_rule(), "select", copying).expect("resolves");
    assert_eq!(select, costliest);
}

#[test]
fn a_linked_selector_outside_the_root_module_is_refused() {
    for selector in ["heavy", "lib.heavy", "missing"] {
        let failures = linked_minimums(selector_rule(), selector, SHARED)
            .expect_err("only root-module functions resolve");
        assert!(
            failures
                .iter()
                .all(|failure| failure.kind == TargetPureIntrinsicFailureKind::InvalidParameter),
            "{selector}: {failures:?}"
        );
    }
}

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
    admit_target_effect_costs, admit_target_pure_intrinsics, author_lawpack, compile_to_core,
    decode_lawpack_adapter, decode_lawpack_bundle, digest_target_intrinsics_catalog,
    encode_canonical_cbor, lower_to_target_ir, parse_module, prepare_lawpack_compilation,
    CanonicalValue as C, CompilerContext, CompilerErrorKind, CoreBudget, CoreCostSchedule,
    CoreModule, LawpackArtifactKind, LawpackAuthoringDefinition, ResourceRef, TargetEffectCostFact,
    TargetIrLoweringFacts, TargetLoweringStatus, TargetPureIntrinsicFailureKind,
    TARGET_EFFECT_COST_MAX_DEPTH, TARGET_INTRINSICS_API_VERSION,
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

/// Hello Echo with a three-iteration loop before its effect whose body holds
/// the `Pair` let and a second effect step.
fn with_loop() -> String {
    SOURCE
        .replace(
            "intent createGreeting",
            "type Pair = { a: U64, b: Bool };\n\nintent createGreeting",
        )
        .replace(
            "  let receipt",
            "  let items: List<U64, max=3> = [1u64, 2u64, 3u64];\n  for i in items bounded 3 {\n    let pair: Pair = { a: 1, b: true };\n    let again: hello.GreetingReceipt = hello.createGreeting(input)\n      else { alreadyExists(existing) => hello.AlreadyExists };\n  }\n  let receipt",
        )
}

/// The loop program's shared-read minimum with the scan template on main
/// 9f10fb91, before either term.
const LOOP_ON_MAIN: (u64, u64) = (4131, 23101);

#[test]
fn a_loop_multiplies_the_effect_node_step_and_let_validations_by_its_bound() {
    // Per iteration the body's effect gains its node step (1) and the let its
    // validations(Pair) (3); the effect after the loop gains its node step,
    // and the `items` let its validations(List<U64, max=3>) = 3·1 + 1.
    // Each iteration no longer charges the binder copy or the body result.
    let context = prepared(Some((V3, scan_rule())), None).context;
    let minimum = minimums(&context, &with_loop(), SHARED);
    assert_eq!(
        minimum,
        (
            LOOP_ON_MAIN.0 + 3 * (1 + 3) + 1 + 4 - 3 * LOOP_ITERATION_RELEASED.0,
            LOOP_ON_MAIN.1 - 3 * LOOP_ITERATION_RELEASED.1,
        )
    );
    let (steps, bytes) = minimum;
    assert!(compile(&context, &with_loop(), SHARED, budget(steps, bytes)).is_ok());
    assert!(compile(&context, &with_loop(), SHARED, budget(steps - 1, bytes)).is_err());
}

/// A pure shared-read intent binding text, which the provider value shape
/// cannot express.
const PURE_TEXT: &str = "package cost.text@1;
type In = { name: String<max=8> };
type Out = { name: String<max=8> };
intent run(input: In) returns Out profile p.read basis none budget <= p.b {
  let name: String<max=8> = input.name;
  return { name: name };
}";

#[test]
fn a_text_let_refuses_in_a_v2_effect_program_but_not_in_a_pure_v2_intent() {
    // Echo has no text values and refuses such an effect program too.
    let effect = SOURCE.replace(
        "  let receipt",
        "  let note: String<max=256> = input.message;\n  let receipt",
    );
    for template in [None, Some((V3, scan_rule()))] {
        let context = prepared(template, None).context;
        let errors = compile(&context, &effect, SHARED, budget(CEILING, CEILING))
            .expect_err("text let in a v2 effect program is unestablished");
        assert!(
            errors
                .iter()
                .any(|e| e.kind == CompilerErrorKind::InvalidBound
                    && e.message.contains("shared-read Core work is unestablished")),
            "{errors:#?}"
        );
        assert!(compile(
            &context,
            &effect,
            CoreCostSchedule::Copying,
            budget(CEILING, CEILING)
        )
        .is_ok());
    }
    let pure = CompilerContext::new()
        .with_core_cost_schedule(SHARED)
        .with_operation_profile("p.read", "continuum.profile.read-only/v1")
        .with_budget("p.b", budget(CEILING, CEILING));
    if let Err(errors) = compile_to_core(&parse_module(PURE_TEXT).expect("source"), &pure) {
        panic!("a pure v2 intent's text let compiles: {errors:#?}");
    }
}

/// Hello Echo with one `attempt` before its effect: a helper that may fail,
/// its failure mapped to `hello.AlreadyExists` with an empty payload.
fn with_attempt() -> String {
    SOURCE
        .replace(
            "intent createGreeting",
            "failure Underflow;\n\nfn distance(start: U64, end: U64) -> U64 fails Underflow {\n  return if start <= end then end - start else fail Underflow;\n}\n\nintent createGreeting",
        )
        .replace(
            "  let receipt",
            "  let span: U64 = distance(1, 3) else hello.AlreadyExists;\n  let receipt",
        )
}

/// What warp-core's shared executor charges for an empty failure payload,
/// measured on Echo's typed-failure program (echo#1016): the record
/// expression's step and its one-cell shell, (2, 64), then encoding it,
/// `shape(record {})` = (1, 64).
const EMPTY_ARM: (u64, u64) = (3, 128);

/// The attempt program's shared-read minimums on main 65dc5362, which
/// charged the arm nothing: without a template, the zero template, the scan
/// template.
const ATTEMPT_ON_MAIN: [(u64, u64); 3] = [(134, 6531), (123, 4451), (1130, 8568)];
/// The looped attempt program's shared-read minimum on main 65dc5362 with
/// the scan template.
const ATTEMPT_LOOP_ON_MAIN: (u64, u64) = (1325, 9592);
/// The attempt program's copying minimum on main 65dc5362 with the scan
/// template.
const ATTEMPT_COPYING_ON_MAIN: (u64, u64) = (1161, 19_368);

/// A pure intent with an `attempt` whose arms carry an empty and a
/// one-field payload: no effect, so the effect-body arm term never applies.
const PURE_ATTEMPT: &str = r"package rope.example@1;
failure ClosureTooSmall;
failure ConflictingContent;
type Input = { value: U64, other: U64, };
type Output = { value: U64, };
fn checked(value: U64) -> U64 fails ClosureTooSmall {
  return if value < 10u64 then value else fail ClosureTooSmall;
}
fn both(value: U64, other: U64) -> U64 fails ConflictingContent, ClosureTooSmall {
  let first = expect checked(value);
  return if first == other then first else fail ConflictingContent;
}
intent evaluate(input: Input) returns Output profile p.read basis none budget <= p.b {
  let value = both(input.value, input.other) else {
    ClosureTooSmall => text.ClosureInsufficient,
    ConflictingContent => text.ContentIdentityMismatch({ observed: input.other }),
  };
  return { value };
}
";
/// Its shared-read minimum on main 65dc5362.
const PURE_ATTEMPT_ON_MAIN: (u64, u64) = (177, 788);

#[test]
fn a_pure_v2_intent_attempt_is_charged_as_on_main() {
    let pure = CompilerContext::new()
        .with_core_cost_schedule(SHARED)
        .with_operation_profile("p.read", "continuum.profile.read-only/v1")
        .with_operation_profile("text.replaceRange", "continuum.profile.read-only/v1");
    let admits = |steps: u64, bytes: u64| {
        compile_to_core(
            &parse_module(PURE_ATTEMPT).expect("source"),
            &pure.clone().with_budget("p.b", budget(steps, bytes)),
        )
        .is_ok()
    };
    assert_eq!(
        (
            minimum(|v| admits(v, CEILING)),
            minimum(|v| admits(CEILING, v))
        ),
        PURE_ATTEMPT_ON_MAIN
    );
}

#[test]
fn an_attempt_in_an_effect_body_is_charged_its_failure_arm_under_shared_reads() {
    // On main the arm was charged nothing; it now adds exactly what the
    // runtime charges for it. Success adds nothing: the shared executor never
    // validates an attempt's binding.
    for (template, on_main) in [
        (None, ATTEMPT_ON_MAIN[0]),
        (Some((V3, zero_rule())), ATTEMPT_ON_MAIN[1]),
        (Some((V3, scan_rule())), ATTEMPT_ON_MAIN[2]),
    ] {
        let context = prepared(template, None).context;
        let minimum = minimums(&context, &with_attempt(), SHARED);
        assert_eq!(minimum, (on_main.0 + EMPTY_ARM.0, on_main.1 + EMPTY_ARM.1));
        let (steps, bytes) = minimum;
        assert!(compile(&context, &with_attempt(), SHARED, budget(steps, bytes)).is_ok());
        assert!(compile(&context, &with_attempt(), SHARED, budget(steps - 1, bytes)).is_err());
        assert!(compile(&context, &with_attempt(), SHARED, budget(steps, bytes - 1)).is_err());
    }
}

/// Hello Echo with the attempt inside a three-iteration loop.
fn with_attempt_loop() -> String {
    with_attempt().replace(
        "  let span: U64 = distance(1, 3) else hello.AlreadyExists;\n",
        "  let items: List<U64, max=3> = [1u64, 2u64, 3u64];\n  for i in items bounded 3 {\n    let span: U64 = distance(1, i) else hello.AlreadyExists;\n  }\n",
    )
}

#[test]
fn a_loop_multiplies_the_attempt_arm_by_its_bound() {
    let context = prepared(Some((V3, scan_rule())), None).context;
    let looped = minimums(&context, &with_attempt_loop(), SHARED);
    assert_eq!(
        looped,
        (
            ATTEMPT_LOOP_ON_MAIN.0 + 3 * (EMPTY_ARM.0 - LOOP_ITERATION_RELEASED.0),
            ATTEMPT_LOOP_ON_MAIN.1 + 3 * (EMPTY_ARM.1 - LOOP_ITERATION_RELEASED.1)
        )
    );
    let (steps, bytes) = looped;
    assert!(compile(&context, &with_attempt_loop(), SHARED, budget(steps, bytes)).is_ok());
    assert!(compile(
        &context,
        &with_attempt_loop(),
        SHARED,
        budget(steps - 1, bytes)
    )
    .is_err());
}

/// Hello Echo with a `U64` local `n` and one `attempt` before its effect,
/// its failure mapped to `hello.AlreadyExists` with `payload` (`""` for an
/// empty payload, or `"({ at: n })"`).
fn with_payload_attempt(payload: &str) -> String {
    with_attempt().replace(
        "  let span: U64 = distance(1, 3) else hello.AlreadyExists;\n",
        &format!("  let n: U64 = 7u64;\n  let span: U64 = distance(1, 3) else hello.AlreadyExists{payload};\n"),
    )
}

/// Hello Echo with a `U64` local `n` and one `require` before its effect,
/// its failure `hello.AlreadyExists` with `payload`.
fn with_require(payload: &str) -> String {
    SOURCE.replace(
        "  let receipt",
        &format!("  let n: U64 = 7u64;\n  require n <= 9u64 else hello.AlreadyExists{payload};\n  let receipt"),
    )
}

/// Hello Echo with that `require` inside a three-iteration loop.
fn with_require_loop() -> String {
    SOURCE.replace(
        "  let receipt",
        "  let n: U64 = 7u64;\n  let items: List<U64, max=3> = [1u64, 2u64, 3u64];\n  for i in items bounded 3 {\n    require i <= n else hello.AlreadyExists;\n  }\n  let receipt",
    )
}

/// What warp-core's shared executor adds for one `U64` field `at` of a
/// failure payload (echo#1016, measured on Echo's typed-failure program with
/// `{ at: input.start }`): one keyed cell in the record's shell,
/// (1, 64 + len("at")), and, encoding it, the key and the integer,
/// (2, 64 + len("at") + 64). The field's own expression, a local here, is one
/// more step.
const AT_FIELD: (u64, u64) = (1 + 1 + 2, 66 + 130);

/// The payload-attempt program with an empty payload, with the scan template:
/// [`EMPTY_ARM`] already charged, as `an_attempt_in_an_effect_body_is_charged_its_failure_arm_under_shared_reads` pins it.
const PAYLOAD_ATTEMPT_EMPTY: (u64, u64) = (1137, 8760);

#[test]
fn an_attempt_arm_payload_field_is_charged_its_shell_cell_and_encoding() {
    let context = prepared(Some((V3, scan_rule())), None).context;
    assert_eq!(
        minimums(&context, &with_payload_attempt(""), SHARED),
        PAYLOAD_ATTEMPT_EMPTY
    );
    let source = with_payload_attempt("({ at: n })");
    let minimum = minimums(&context, &source, SHARED);
    assert_eq!(
        minimum,
        (
            PAYLOAD_ATTEMPT_EMPTY.0 + AT_FIELD.0,
            PAYLOAD_ATTEMPT_EMPTY.1 + AT_FIELD.1
        )
    );
    let (steps, bytes) = minimum;
    assert!(compile(&context, &source, SHARED, budget(steps, bytes)).is_ok());
    assert!(compile(&context, &source, SHARED, budget(steps - 1, bytes)).is_err());
    assert!(compile(&context, &source, SHARED, budget(steps, bytes - 1)).is_err());
}

/// The require programs' shared-read minimums with the scan template on
/// 66f6ca4d, which charged a `require` its predicate and payload fields
/// only: empty payload, `{ at: n }`, and the empty-payload loop.
const REQUIRE_BEFORE: [(u64, u64); 3] = [(1119, 8504), (1120, 8504), (1284, 9272)];
/// Their copying minimums, the same before and after.
const REQUIRE_COPYING: [(u64, u64); 3] = [(1007, 4117), (1007, 4117), (1010, 4117)];

#[test]
fn a_failing_require_in_an_effect_body_is_charged_its_payload_under_shared_reads() {
    // The runtime's failing path builds and encodes the payload as an
    // `attempt` arm does; its passing path adds nothing after the predicate,
    // so the failing path is the larger.
    let context = prepared(Some((V3, scan_rule())), None).context;
    for (source, before, added) in [
        (with_require(""), REQUIRE_BEFORE[0], EMPTY_ARM),
        (
            with_require("({ at: n })"),
            REQUIRE_BEFORE[1],
            // The field's own step was already charged.
            (EMPTY_ARM.0 + AT_FIELD.0 - 1, EMPTY_ARM.1 + AT_FIELD.1),
        ),
    ] {
        let minimum = minimums(&context, &source, SHARED);
        assert_eq!(minimum, (before.0 + added.0, before.1 + added.1));
        let (steps, bytes) = minimum;
        assert!(compile(&context, &source, SHARED, budget(steps, bytes)).is_ok());
        assert!(compile(&context, &source, SHARED, budget(steps - 1, bytes)).is_err());
        assert!(compile(&context, &source, SHARED, budget(steps, bytes - 1)).is_err());
    }
}

#[test]
fn a_loop_multiplies_the_require_payload_by_its_bound() {
    let context = prepared(Some((V3, scan_rule())), None).context;
    let looped = minimums(&context, &with_require_loop(), SHARED);
    assert_eq!(
        looped,
        (
            REQUIRE_BEFORE[2].0 + 3 * (EMPTY_ARM.0 - LOOP_ITERATION_RELEASED.0),
            REQUIRE_BEFORE[2].1 + 3 * (EMPTY_ARM.1 - LOOP_ITERATION_RELEASED.1)
        )
    );
    let (steps, bytes) = looped;
    assert!(compile(
        &context,
        &with_require_loop(),
        SHARED,
        budget(steps - 1, bytes)
    )
    .is_err());
}

#[test]
fn copying_require_minimums_are_unchanged() {
    let context = prepared(Some((V3, scan_rule())), None).context;
    let measured: Vec<_> = [
        with_require(""),
        with_require("({ at: n })"),
        with_require_loop(),
    ]
    .iter()
    .map(|source| minimums(&context, source, CoreCostSchedule::Copying))
    .collect();
    assert_eq!(measured, REQUIRE_COPYING);
}

/// A pure intent with a failing `require` and a one-field payload: no
/// effect, so the effect-body payload term never applies.
const PURE_REQUIRE: &str = r"package rope.require@1;
type Input = { value: U64, };
type Output = { value: U64, };
intent evaluate(input: Input) returns Output profile p.read basis none budget <= p.b {
  require input.value < 10u64 else text.ClosureInsufficient({ observed: input.value });
  return { value: input.value };
}
";
/// Its shared-read minimum on 66f6ca4d.
const PURE_REQUIRE_BEFORE: (u64, u64) = (90, 655);

#[test]
fn a_pure_v2_intent_require_is_charged_as_before() {
    let pure = CompilerContext::new()
        .with_core_cost_schedule(SHARED)
        .with_operation_profile("p.read", "continuum.profile.read-only/v1");
    let admits = |steps: u64, bytes: u64| {
        compile_to_core(
            &parse_module(PURE_REQUIRE).expect("source"),
            &pure.clone().with_budget("p.b", budget(steps, bytes)),
        )
        .is_ok()
    };
    assert_eq!(
        (
            minimum(|v| admits(v, CEILING)),
            minimum(|v| admits(CEILING, v))
        ),
        PURE_REQUIRE_BEFORE
    );
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
    assert_eq!(
        minimums(&charged, &with_attempt(), CoreCostSchedule::Copying),
        ATTEMPT_COPYING_ON_MAIN
    );
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

/// A payload field whose value is a record literal, `{ value: n }`: the
/// source checker accepts any expression as a payload field value.
const NESTED: &str = "({ at: { value: n } })";

/// What a record-literal field value costs to evaluate beyond a local, as
/// main 65dc5362 (before the effect-body arm term) already charged it: the
/// literal's step, its shell cell and its keyed cell.
const NESTED_WORK: (u64, u64) = (4, 197);

/// Encoding the field `at` as `{ value: U64 }` rather than `U64`: Echo's
/// lowerer sizes each payload field by the type it infers for the value
/// (`shape(Ty::Record(fields))`), so the nested record adds its keyed cell
/// `value` and the integer cell, (2, 64 + len("value") + 64).
const NESTED_ENCODING: (u64, u64) = (2, 133);

/// Hello Echo with that `require` and `payload` inside a three-iteration loop.
fn with_payload_require_loop(payload: &str) -> String {
    SOURCE.replace(
        "  let receipt",
        &format!("  let n: U64 = 7u64;\n  let items: List<U64, max=3> = [1u64, 2u64, 3u64];\n  for i in items bounded 3 {{\n    require i <= n else hello.AlreadyExists{payload};\n  }}\n  let receipt"),
    )
}

fn assert_exact_minimum(context: &CompilerContext, source: &str, expected: (u64, u64)) {
    let minimum = minimums(context, source, SHARED);
    assert_eq!(minimum, expected);
    let (steps, bytes) = minimum;
    assert!(compile(context, source, SHARED, budget(steps, bytes)).is_ok());
    for below in [budget(steps - 1, bytes), budget(steps, bytes - 1)] {
        let errors = compile(context, source, SHARED, below).expect_err("refused one below");
        let kinds: Vec<_> = errors.iter().map(|e| e.kind).collect();
        assert_eq!(kinds, [CompilerErrorKind::InvalidBound]);
    }
}

#[test]
fn a_record_payload_field_value_is_charged_its_inferred_shape_in_an_attempt_arm() {
    let context = prepared(Some((V3, scan_rule())), None).context;
    let scalar = minimums(&context, &with_payload_attempt("({ at: n })"), SHARED);
    assert_exact_minimum(
        &context,
        &with_payload_attempt(NESTED),
        (
            scalar.0 + NESTED_WORK.0 + NESTED_ENCODING.0,
            scalar.1 + NESTED_WORK.1 + NESTED_ENCODING.1,
        ),
    );
}

#[test]
fn a_record_payload_field_value_is_charged_its_inferred_shape_in_a_failing_require() {
    let context = prepared(Some((V3, scan_rule())), None).context;
    let scalar = minimums(&context, &with_require("({ at: n })"), SHARED);
    assert_exact_minimum(
        &context,
        &with_require(NESTED),
        (
            scalar.0 + NESTED_WORK.0 + NESTED_ENCODING.0,
            scalar.1 + NESTED_WORK.1 + NESTED_ENCODING.1,
        ),
    );
}

#[test]
fn a_loop_multiplies_a_record_payload_field_by_its_bound() {
    let context = prepared(Some((V3, scan_rule())), None).context;
    let scalar = minimums(&context, &with_payload_require_loop("({ at: n })"), SHARED);
    assert_exact_minimum(
        &context,
        &with_payload_require_loop(NESTED),
        (
            scalar.0 + 3 * (NESTED_WORK.0 + NESTED_ENCODING.0),
            scalar.1 + 3 * (NESTED_WORK.1 + NESTED_ENCODING.1),
        ),
    );
}

/// The record-payload programs' copying minimums on main 65dc5362, before
/// the effect-body arm term: attempt, require, looped require.
const NESTED_COPYING_BEFORE: [(u64, u64); 3] = [(1170, 19_693), (1007, 4117), (1010, 4117)];

#[test]
fn copying_minimums_of_record_payload_programs_are_unchanged() {
    let context = prepared(Some((V3, scan_rule())), None).context;
    let measured: Vec<_> = [
        with_payload_attempt(NESTED),
        with_require(NESTED),
        with_payload_require_loop(NESTED),
    ]
    .iter()
    .map(|source| minimums(&context, source, CoreCostSchedule::Copying))
    .collect();
    assert_eq!(measured, NESTED_COPYING_BEFORE);
}

/// What each iteration of a loop in an effect body was charged under the
/// shared-read schedule that warp-core's shared executor never charges on
/// any path: the binder's copy bytes (0, 64) (the item is shared, not
/// copied) and the loop body's `Null` result (2, 64) (a loop body has no
/// result to evaluate). Echo's effect-body judge charges neither.
const LOOP_ITERATION_RELEASED: (u64, u64) = (2, 64 + 64);

/// Echo's loop-scalar witness shape: a `require` with a `{ at: i }` payload
/// inside a loop bounded 3, before the effect.
fn with_scalar_require_loop() -> String {
    with_payload_require_loop("({ at: i })")
}

/// Two loops bounded 3, the inner one holding the `{ at: j }` require.
fn with_nested_require_loop() -> String {
    SOURCE.replace(
        "  let receipt",
        "  let n: U64 = 7u64;\n  let items: List<U64, max=3> = [1u64, 2u64, 3u64];\n  for i in items bounded 3 {\n    for j in items bounded 3 {\n      require j <= n else hello.AlreadyExists({ at: j });\n    }\n  }\n  let receipt",
    )
}

/// Shared-read minimums on main 615d2e59, before the loop term: the scalar
/// require loop, the nested require loop.
const LOOP_REQUIRE_ON_615D2E59: [(u64, u64); 2] = [(1305, 10_244), (1797, 13_340)];

#[test]
fn a_loop_iteration_in_an_effect_body_is_charged_no_binder_copy_or_body_result() {
    let context = prepared(Some((V3, scan_rule())), None).context;
    let scalar = LOOP_REQUIRE_ON_615D2E59[0];
    assert_exact_minimum(
        &context,
        &with_scalar_require_loop(),
        (
            scalar.0 - 3 * LOOP_ITERATION_RELEASED.0,
            scalar.1 - 3 * LOOP_ITERATION_RELEASED.1,
        ),
    );
}

#[test]
fn nested_loops_in_an_effect_body_release_the_term_for_every_iteration() {
    // Three outer iterations and nine inner ones.
    let context = prepared(Some((V3, scan_rule())), None).context;
    let nested = LOOP_REQUIRE_ON_615D2E59[1];
    assert_exact_minimum(
        &context,
        &with_nested_require_loop(),
        (
            nested.0 - 12 * LOOP_ITERATION_RELEASED.0,
            nested.1 - 12 * LOOP_ITERATION_RELEASED.1,
        ),
    );
}

/// The loop programs' copying minimums, the same before and after: the
/// scalar require loop and the nested require loop.
const LOOP_REQUIRE_COPYING: [(u64, u64); 2] = [(1010, 4117), (1019, 4117)];

#[test]
fn copying_minimums_of_loop_programs_are_unchanged() {
    let context = prepared(Some((V3, scan_rule())), None).context;
    let measured: Vec<_> = [with_scalar_require_loop(), with_nested_require_loop()]
        .iter()
        .map(|source| minimums(&context, source, CoreCostSchedule::Copying))
        .collect();
    assert_eq!(measured, LOOP_REQUIRE_COPYING);
}

/// A pure intent with a loop bounded 3: no effect, so the effect-body loop
/// term never applies.
const PURE_LOOP: &str = r"package rope.example@1;
failure ClosureTooSmall;
type Input = { items: List<U64, max=3>, limit: U64, };
type Output = { value: U64, };
intent evaluate(input: Input) returns Output profile p.read basis none budget <= p.b {
  for item in input.items bounded 3 {
    require item <= input.limit else text.ClosureInsufficient;
  }
  return { value: input.limit };
}
";
/// Its shared-read minimum on main 615d2e59.
const PURE_LOOP_ON_615D2E59: (u64, u64) = (250, 1300);

#[test]
fn a_pure_v2_intent_loop_is_charged_as_on_main() {
    let pure = CompilerContext::new()
        .with_core_cost_schedule(SHARED)
        .with_operation_profile("p.read", "continuum.profile.read-only/v1")
        .with_operation_profile("text.replaceRange", "continuum.profile.read-only/v1");
    let admits = |steps: u64, bytes: u64| {
        compile_to_core(
            &parse_module(PURE_LOOP).expect("source"),
            &pure.clone().with_budget("p.b", budget(steps, bytes)),
        )
        .is_ok()
    };
    assert_eq!(
        (
            minimum(|v| admits(v, CEILING)),
            minimum(|v| admits(CEILING, v))
        ),
        PURE_LOOP_ON_615D2E59
    );
}

/// `n` nested products whose first factor nests; `leaf` sits at level `n`.
fn nested_expression(n: usize, leaf: C) -> C {
    (0..n).fold(leaf, |inner, _| {
        map(vec![("product", C::Array(vec![inner, constant(1)]))])
    })
}

/// `n` levels: a `bytesMax` measure (level 1) whose type, at level 2, nests
/// records until a `word` at level `n`.
fn nested_type_measure(n: usize) -> C {
    let ty = (2..n).fold(map(vec![("word", C::Null)]), |inner, _| {
        map(vec![("record", map(vec![("f", inner)]))])
    });
    measure(map(vec![("bytesMax", ty)]))
}

/// Effect-cost admission of `bytes`, a `/v3` catalog for the lawpack's effect.
fn admit_catalog(bytes: &[u8]) -> Result<(), Vec<TargetPureIntrinsicFailureKind>> {
    let facts = prepared(None, None).facts;
    let pin = ResourceRef {
        coordinate: CATALOG.into(),
        digest: Some(digest_target_intrinsics_catalog(CATALOG, bytes).expect("digest")),
    };
    admit_target_effect_costs(
        &facts.target_profile,
        &pin,
        bytes,
        &facts.effect_lowerings,
        &configuration(None),
    )
    .map(|_| ())
    .map_err(|failures| failures.into_iter().map(|failure| failure.kind).collect())
}

/// Admission of a `/v3` catalog whose used effect's steps are `steps`.
fn admit_steps(steps: C) -> Result<(), Vec<TargetPureIntrinsicFailureKind>> {
    admit_catalog(&catalog(
        V3,
        map(vec![("steps", steps), ("bytes", constant(0))]),
    ))
}

/// Oracle: the published depth limit (`docs/abi/edict-target-profile.cddl`),
/// which the catalog schema applies at the same levels
/// (`current_pack_admits_cost_nesting_exactly_to_the_published_depth`).
#[test]
fn effect_cost_admission_reads_nesting_exactly_to_the_published_depth() {
    let depth = TARGET_EFFECT_COST_MAX_DEPTH;
    let cases = [
        (
            "expression at the limit",
            nested_expression(depth, constant(1)),
            true,
        ),
        (
            "expression past the limit",
            nested_expression(depth + 1, constant(1)),
            false,
        ),
        (
            "parameter measure at the limit",
            nested_expression(depth - 1, scans()),
            true,
        ),
        (
            "parameter measure past the limit",
            nested_expression(depth, scans()),
            false,
        ),
        ("type at the limit", nested_type_measure(depth), true),
        ("type past the limit", nested_type_measure(depth + 1), false),
    ];
    for (name, steps, admitted) in cases {
        let expected = if admitted {
            Ok(())
        } else {
            Err(vec![TargetPureIntrinsicFailureKind::InvalidCostRule])
        };
        assert_eq!(admit_steps(steps), expected, "{name}");
    }
}

/// The compiler reads only the rules of effects the program uses, so a rule
/// nested past the limit on an unused effect is admitted here. The catalog
/// schema applies the limit to every rule and refuses the same catalog
/// (`current_pack_refuses_an_unused_effect_nested_past_the_limit`).
#[test]
fn effect_cost_admission_ignores_an_unused_effect_nested_past_the_limit() {
    let used = map(vec![
        ("intrinsicClass", text("effect")),
        ("effectCostRule", zero_rule()),
    ]);
    let unused = map(vec![
        ("intrinsicClass", text("effect")),
        (
            "effectCostRule",
            map(vec![
                (
                    "steps",
                    nested_expression(TARGET_EFFECT_COST_MAX_DEPTH + 1, constant(1)),
                ),
                ("bytes", constant(0)),
            ]),
        ),
    ]);
    let bytes = encode_canonical_cbor(&map(vec![
        ("apiVersion", text(V3)),
        (
            "intrinsics",
            map(vec![
                (INTRINSIC, used),
                ("echo.dpo@1.unused-effect", unused),
            ]),
        ),
        ("types", C::Map(Vec::new())),
        ("parameters", declarations()),
    ]))
    .expect("canonical catalog");
    assert_eq!(admit_catalog(&bytes), Ok(()));
}

// Effect inputs written as expressions with no type coordinate of their own
// (#341, jedit#410). Hello Echo's input is all text, which the provider value
// shape has no measure for, so these use an authored lawpack whose effect
// input has a list of records, bytes and an integer.

const LIT_PIN: &str = "sha256:1111111111111111111111111111111111111111111111111111111111111111";

/// A lawpack exporting `put`, lowered to [`INTRINSIC`], whose input is
/// `PutInput = { items: List<Item, max=4>, n: U64, tag: Bytes<max=16> }` and
/// `Item = { count: U64, id: Bytes<exact=32> }`.
fn literal_definition() -> LawpackAuthoringDefinition {
    serde_json::from_value(serde_json::json!({
        "schema": "edict.lawpack-authoring/v1",
        "id": "graph.lit",
        "version": "1",
        "acceptedCoreAbi": ["edict.core/v1"],
        "dependencies": [],
        "exportsCoordinate": "graph.lit.exports/v1",
        "exports": {
            "types": [
                {"coordinate": "graph.lit@1.Done", "definition": "Record<>"},
                {"coordinate": "graph.lit@1.Item",
                 "definition": "Record<count:U64,id:Bytes<exact=32>>"},
                {"coordinate": "graph.lit@1.PutInput",
                 "definition": "Record<items:List<graph.lit@1.Item,max=4>,n:U64,tag:Bytes<max=16>>"}
            ],
            "constants": [],
            "pureFunctions": [],
            "effects": [{
                "coordinate": "graph.lit@1.put",
                "typeParameters": [],
                "inputType": "graph.lit@1.PutInput",
                "outputType": "graph.lit@1.Done",
                "executionClass": "runtime",
                "effectKindHint": "create",
                "footprintObligation": "graph.lit@1.footprint",
                "costObligation": "graph.lit@1.budget",
                "effectFailures": {},
                "guardSupport": true
            }],
            "obstructions": [],
            "operationProfiles": {
                "graph.lit@1.write": {
                    "opticTemplate": {
                        "opticKind": "affectReintegration",
                        "boundaryKind": "affect",
                        "supportPolicy": "graph.lit@1.support",
                        "lossDisposition": "graph.lit@1.lossless",
                        "basisTemplate": "graph.lit@1.basis",
                        "apertureRequirement": {
                            "kind": "abstractFootprintObligation",
                            "reference": "graph.lit@1.footprint"
                        }
                    },
                    "effectPredicate": "graph.lit@1.effects"
                }
            }
        },
        "targetAdapters": [{
            "coordinate": "graph.lit.echo-adapter/v1",
            "output": "adapter.cbor",
            "acceptedTargetProfile": {"id": "echo.dpo@1", "digest": LIT_PIN},
            "acceptedTargetIr": {"id": "echo.span-ir/v2", "digest": LIT_PIN},
            "operationProfiles": {
                "graph.lit@1.write": {
                    "core": "continuum.profile.graph-effects/v1",
                    "semanticEffects": ["graph.lit@1.put"],
                    "budgetObligation": "graph.lit@1.budget",
                    "targetConfiguration": {"local": "echo-config"}
                }
            },
            "effectImplementations": {
                "graph.lit@1.put": {
                    "targetIntrinsic": INTRINSIC,
                    "targetConfiguration": {"local": "echo-config"},
                    "writeClass": "create",
                    "footprintObligation": "graph.lit@1.footprint",
                    "costObligation": "graph.lit@1.budget",
                    "failureMappings": {}
                }
            },
            "budgets": {
                "graph.lit@1.budget": {
                    "maxSteps": 4096,
                    "maxAllocatedBytes": 65536,
                    "maxOutputBytes": 64
                }
            }
        }],
        "verifier": {"class": "declarative", "ruleset": {"local": "rules"}},
        "compatibility": {"id": "graph.lit.compatibility/v1", "digest": LIT_PIN},
        "conformanceFixtureCorpus": {"id": "graph.lit.fixtures/v1", "digest": LIT_PIN},
        "localResources": [{
            "name": "echo-config",
            "coordinate": "graph.lit.echo-config/v1",
            "output": "echo-config.cbor",
            "value": {"apiVersion": "graph.lit.echo-config/v1"}
        }, {
            "name": "rules",
            "coordinate": "graph.lit.verifier-rules/v1",
            "output": "verifier-rules.cbor",
            "value": {"opcodes": {"$edictBytes": "00"}}
        }]
    }))
    .expect("typed authoring definition")
}

/// The `put` lawpack's compiler context and Target facts with `rule` admitted.
fn literal_prepared(rule: C, selector: Option<&str>) -> (Prepared, String) {
    let authored = author_lawpack(&literal_definition(), &[]).expect("author the lawpack");
    let manifest = authored.artifact(LawpackArtifactKind::Manifest).unwrap();
    let exports = authored.artifact(LawpackArtifactKind::Exports).unwrap();
    let adapter = authored.artifact(LawpackArtifactKind::Adapter).unwrap();
    let bundle = decode_lawpack_bundle(manifest.bytes(), exports.bytes()).expect("bundle");
    let adapter = decode_lawpack_adapter(&bundle, "echo.dpo@1", adapter.bytes()).expect("adapter");
    let digest = manifest.digest().to_owned();
    // Preparation reads only the module's lawpack import.
    let header = literal_source(&digest, "", "fx.put(bound)");
    let module = parse_module(&header).expect("source");
    let preparation = prepare_lawpack_compilation(&module, &bundle, &adapter).expect("facts");
    let facts = preparation.target_ir_facts().clone();
    let costs = admit(V3, rule, &configuration(selector), &facts).expect("templates admitted");
    (
        Prepared {
            context: preparation
                .compiler_context()
                .clone()
                .with_target_effect_costs(&costs),
            facts: facts.with_target_effect_costs(&costs),
            costs,
        },
        digest,
    )
}

/// A `put` program: `lets` before the effect, whose input is `input_expr`.
fn literal_source(digest: &str, lets: &str, call: &str) -> String {
    format!(
        "package consumer.lit@1;\n\
         use lawpack graph.lit@1 digest \"{digest}\" as fx;\n\
         type Input = {{ id: Bytes<exact=32>, n: U64, tag: Bytes<max=16> }};\n\
         type Output = {{ id: Bytes<exact=32> }};\n\
         fn relay(value: fx.PutInput) -> fx.PutInput {{ return value; }}\n\
         intent write(input: Input) returns Output\n\
           profile fx.write\n\
           basis input.id\n\
           budget <= fx.budget\n\
         {{\n{lets}  let done: fx.Done = {call};\n  return {{ id: input.id }};\n}}\n"
    )
}

/// The effect input as a record literal built from the intent input.
const PUT_LITERAL: &str =
    "{ items: [{ count: input.n, id: input.id }], n: input.n, tag: input.tag }";

fn put_compile(
    context: &CompilerContext,
    source: &str,
    budget: CoreBudget,
) -> Result<CoreModule, Vec<amblicit_syntax::CompilerError>> {
    compile_to_core(
        &parse_module(source).expect("source"),
        &context
            .clone()
            .with_core_cost_schedule(SHARED)
            .with_budget("fx.budget", budget),
    )
}

fn put_minimums(context: &CompilerContext, source: &str) -> (u64, u64) {
    if let Err(errors) = put_compile(context, source, budget(CEILING, CEILING)) {
        panic!("compiles under a generous budget: {errors:#?}\n{source}");
    }
    (
        minimum(|v| put_compile(context, source, budget(v, CEILING)).is_ok()),
        minimum(|v| put_compile(context, source, budget(CEILING, v)).is_ok()),
    )
}

/// Asserts native Target lowers `source` at its source minimum and refuses one
/// step or byte below it; returns that minimum.
fn put_lowers_exactly_at_minimum(prepared: &Prepared, source: &str) -> (u64, u64) {
    let (steps, bytes) = put_minimums(&prepared.context, source);
    let core = put_compile(&prepared.context, source, budget(steps, bytes)).expect("minimum");
    let report = lower_to_target_ir(&core, &prepared.facts);
    assert_eq!(report.status, TargetLoweringStatus::Lowered, "{report:#?}");
    for below in [budget(steps - 1, bytes), budget(steps, bytes - 1)] {
        let mut core = core.clone();
        core.intents
            .get_mut("write")
            .unwrap()
            .core_evaluation_budget = below;
        assert_ne!(
            lower_to_target_ir(&core, &prepared.facts).status,
            TargetLoweringStatus::Lowered,
            "{source}"
        );
    }
    (steps, bytes)
}

fn input_path(fields: &[&str], item: u64, then: &[&str]) -> C {
    let names = |names: &[&str]| C::Array(names.iter().map(|name| text(name)).collect());
    let mut entries = vec![
        ("side", text("input")),
        ("fields", names(fields)),
        ("item", int(item)),
    ];
    if !then.is_empty() {
        entries.push(("then", names(then)));
    }
    map(vec![("path", map(entries))])
}

/// Every measure a template can take on the input type: `bytesMax` of a field
/// path, `listMax`, a `copy` through a list `item` and `then`, and the
/// input's `validations`.
fn input_measures_rule() -> C {
    let copy = |of: &str| {
        measure(map(vec![
            ("copy", input_path(&["items"], 1, &["id"])),
            ("of", text(of)),
        ]))
    };
    let tag = measure(map(vec![("bytesMax", input_path(&["tag"], 0, &[]))]));
    let items = measure(map(vec![("listMax", input_path(&["items"], 0, &[]))]));
    let checks = measure(map(vec![("validations", input_path(&[], 0, &[]))]));
    map(vec![
        (
            "steps",
            map(vec![(
                "sum",
                C::Array(vec![copy("steps"), items.clone(), checks]),
            )]),
        ),
        (
            "bytes",
            map(vec![("sum", C::Array(vec![copy("bytes"), tag, items]))]),
        ),
    ])
}

/// The input measures' values for `PutInput`, from the provider value shape:
/// copy(Bytes<exact=32>) = (1, 32 + 64); bytesMax(tag) = 16; listMax(items) =
/// 4; validations(PutInput) = 1 + (1 + 4·(1 + 1 + 1)) + 1 + 1 = 16.
const PUT_VALIDATIONS: u64 = 16;
const INPUT_MEASURES: (u64, u64) = (1 + 4 + PUT_VALIDATIONS, 32 + 64 + 16 + 4);

fn literal_rules() -> [(C, Option<&'static str>); 4] {
    [
        (input_measures_rule(), None),
        (scan_rule(), None),
        (selector_rule(), Some("relay")),
        (costliest_rule(), Some("relay")),
    ]
}

#[test]
fn an_effect_input_written_as_a_record_literal_is_costed_by_every_template_measure() {
    for (rule, selector) in literal_rules() {
        let (prepared, digest) = literal_prepared(rule, selector);
        let call = format!("fx.put({PUT_LITERAL})");
        put_lowers_exactly_at_minimum(&prepared, &literal_source(&digest, "", &call));
    }
}

#[test]
fn the_input_measures_charge_exactly_their_values_on_the_declared_input_type() {
    let (zero, digest) = literal_prepared(zero_rule(), None);
    let (measured, _) = literal_prepared(input_measures_rule(), None);
    let source = literal_source(&digest, "", &format!("fx.put({PUT_LITERAL})"));
    let base = put_minimums(&zero.context, &source);
    let charged = put_minimums(&measured.context, &source);
    assert_eq!((charged.0 - base.0, charged.1 - base.1), INPUT_MEASURES);
}

#[test]
fn a_literal_effect_input_costs_the_typed_let_form_minus_exactly_the_lets_own_terms() {
    // The documented v2 effect step is: the input expression's own cost (here
    // building the record), plus the template, plus the effect node's step,
    // plus the empty obstruction payload when arms are mapped (none here).
    // The typed-`let` form builds the same record in its `let`, whose charge is
    // the let node's step, the record (the same input expression), and the
    // effect-body validations of its declared type; the effect's input is then
    // a local read, one step and no bytes under v2. So the literal form is the
    // let form minus exactly: one let step, validations(PutInput) = 16 steps,
    // and the local read's one step. The template (measured on the same
    // declared type), the input record and the effect step are common to both.
    const LET_STEP: (u64, u64) = (1, 0);
    const LET_VALIDATIONS: (u64, u64) = (PUT_VALIDATIONS, 0);
    const LOCAL_READ: (u64, u64) = (1, 0);
    for (rule, selector) in literal_rules() {
        let (prepared, digest) = literal_prepared(rule, selector);
        let literal = put_lowers_exactly_at_minimum(
            &prepared,
            &literal_source(&digest, "", &format!("fx.put({PUT_LITERAL})")),
        );
        let bound = put_lowers_exactly_at_minimum(
            &prepared,
            &literal_source(
                &digest,
                &format!("  let bound: fx.PutInput = {PUT_LITERAL};\n"),
                "fx.put(bound)",
            ),
        );
        assert_eq!(
            literal,
            (
                bound.0 - LET_STEP.0 - LET_VALIDATIONS.0 - LOCAL_READ.0,
                bound.1 - LET_STEP.1 - LET_VALIDATIONS.1 - LOCAL_READ.1,
            ),
            "literal {literal:?} vs typed let {bound:?}"
        );
    }
}

#[test]
fn other_effect_input_forms_are_costed_and_lowered_at_their_minimum() {
    // A helper call has its own coordinate; a conditional of two literals has
    // none and is measured at the declared input type, as is a conditional
    // joining a literal with a typed local when the join names no declared
    // coordinate. (A field of
    // a record literal holding a list literal is refused earlier, by the
    // source checker, which gives a list literal no type without one expected.)
    let lets = format!("  let bound: fx.PutInput = {PUT_LITERAL};\n");
    let forms = [
        format!("fx.put(relay({PUT_LITERAL}))"),
        format!("fx.put(if input.n == 0u64 then {PUT_LITERAL} else {PUT_LITERAL})"),
        format!("fx.put(if input.n == 0u64 then {PUT_LITERAL} else bound)"),
    ];
    for (rule, selector) in literal_rules() {
        let (prepared, digest) = literal_prepared(rule, selector);
        for call in &forms {
            put_lowers_exactly_at_minimum(&prepared, &literal_source(&digest, &lets, call));
        }
    }
}

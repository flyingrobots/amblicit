//! Target-catalog pure intrinsics: pinned contracts, source calls through a
//! target import, bound-polymorphic arguments and independently recomputed
//! cost.
use edict_syntax::{
    admit_target_pure_intrinsics, compile_to_core, digest_target_intrinsics_catalog,
    encode_canonical_cbor, lower_to_target_ir, parse_module, CanonicalValue as V, CompilerContext,
    CompilerErrorKind, CoreBudget, CoreExpr, CoreImportKind, CoreModule, CoreNode,
    RequiredPureIntrinsic, ResourceRef, TargetIrLoweringFacts, TargetLoweringFailureKind,
    TargetLoweringStatus, TargetPureIntrinsicFact, TargetPureIntrinsicFailureKind,
};

const CATALOG: &str = "echo.dpo@1.intrinsics";
const CONTENT_ADDRESS: &str = "echo.dpo@1.contentAddress";
const CONTENT_EDGE_ID: &str = "echo.dpo@1.contentEdgeId";
const SLOT_EDGE_ID: &str = "echo.dpo@1.slotEdgeId";
const VERSION: &str = "echo.content-address/v1";
const ID: &str = "Bytes<exact=32>";
const ATOM_CEILING: &str = "Bytes<max=1048576>";
const EDGES: &str = "List<echo.dpo@1.OutgoingEdge,max=16>";
const EDGE: &str = "Record<edgeType:Bytes<exact=32>,to:Bytes<exact=32>>";

fn text(value: &str) -> V {
    V::Text(value.into())
}

fn map(entries: Vec<(&str, V)>) -> V {
    V::Map(
        entries
            .into_iter()
            .map(|(key, value)| (text(key), value))
            .collect(),
    )
}

fn texts(values: &[&str]) -> V {
    V::Array(values.iter().map(|value| text(value)).collect())
}

fn ints(values: &[i128]) -> V {
    V::Array(values.iter().map(|value| V::Integer(*value)).collect())
}

fn pure_entry(name: &str, arguments: &[&str], base: i128, per_argument: &[i128]) -> V {
    map(vec![
        ("intrinsicClass", text("pure")),
        ("typeParameters", V::Array(Vec::new())),
        ("argumentTypes", texts(arguments)),
        ("returnType", text(ID)),
        ("guardSupport", V::Bool(false)),
        ("footprintTemplate", text(&format!("{name}.footprint"))),
        ("costTemplate", text(&format!("{name}.cost"))),
        ("writeClass", text("none")),
        ("contractVersion", text(VERSION)),
        (
            "costRule",
            map(vec![
                ("base", V::Integer(base)),
                ("perArgumentMax", ints(per_argument)),
            ]),
        ),
    ])
}

fn effect_entry() -> V {
    map(vec![
        ("intrinsicClass", text("effect")),
        ("typeParameters", V::Array(Vec::new())),
        ("argumentTypes", texts(&["echo.dpo@1.replace.input"])),
        ("returnType", text("echo.dpo@1.replace.result")),
        ("effectKind", text("replace")),
        ("effectFailures", V::Map(Vec::new())),
        ("guardSupport", V::Bool(true)),
        ("footprintTemplate", text("echo.dpo@1.replace.footprint")),
        ("costTemplate", text("echo.dpo@1.replace.cost")),
        ("writeClass", text("replace")),
        ("canParticipateInAtomicGuard", V::Bool(true)),
    ])
}

/// The catalog document, as a mutable value so tests can corrupt one field.
fn catalog_value() -> V {
    map(vec![
        ("apiVersion", text("edict.target-profile.intrinsics/v2")),
        ("types", map(vec![("echo.dpo@1.OutgoingEdge", text(EDGE))])),
        (
            "intrinsics",
            map(vec![
                (
                    CONTENT_ADDRESS,
                    pure_entry(
                        CONTENT_ADDRESS,
                        &[ID, ID, ID, ATOM_CEILING, EDGES],
                        64,
                        &[0, 0, 0, 1, 2],
                    ),
                ),
                (
                    CONTENT_EDGE_ID,
                    pure_entry(CONTENT_EDGE_ID, &[ID, ID, ID, ID], 32, &[0, 0, 0, 0]),
                ),
                (
                    SLOT_EDGE_ID,
                    pure_entry(SLOT_EDGE_ID, &[ID, ID, ID], 32, &[0, 0, 0]),
                ),
                ("echo.dpo@1.replace", effect_entry()),
            ]),
        ),
    ])
}

fn field_mut<'a>(value: &'a mut V, key: &str) -> &'a mut V {
    let V::Map(entries) = value else {
        panic!("map")
    };
    &mut entries
        .iter_mut()
        .find(|(name, _)| name == &text(key))
        .unwrap_or_else(|| panic!("field {key}"))
        .1
}

fn target() -> ResourceRef {
    ResourceRef {
        coordinate: "echo.dpo@1".into(),
        digest: Some(format!("sha256:{}", "1".repeat(64))),
    }
}

/// The source import of the pinned target profile.
fn target_import() -> String {
    format!(
        "use target echo.dpo@1 digest \"{}\" as echo;",
        target().digest.unwrap()
    )
}

fn pinned(bytes: &[u8]) -> ResourceRef {
    ResourceRef {
        coordinate: CATALOG.into(),
        digest: Some(digest_target_intrinsics_catalog(CATALOG, bytes).expect("digest")),
    }
}

fn required() -> Vec<RequiredPureIntrinsic> {
    [CONTENT_ADDRESS, CONTENT_EDGE_ID, SLOT_EDGE_ID]
        .into_iter()
        .map(|coordinate| RequiredPureIntrinsic {
            coordinate: coordinate.into(),
            contract_version: VERSION.into(),
        })
        .collect()
}

fn admit_value(
    value: &V,
) -> Result<Vec<TargetPureIntrinsicFact>, Vec<TargetPureIntrinsicFailureKind>> {
    let bytes = encode_canonical_cbor(value).expect("canonical");
    admit_target_pure_intrinsics(&target(), &pinned(&bytes), &bytes, &required())
        .map_err(|failures| failures.into_iter().map(|failure| failure.kind).collect())
}

fn admitted() -> Vec<TargetPureIntrinsicFact> {
    admit_value(&catalog_value()).expect("admitted")
}

fn context(steps: u64) -> CompilerContext {
    let mut context = CompilerContext::new()
        .with_operation_profile("p.read", "continuum.profile.read-only/v1")
        .with_budget(
            "p.small",
            CoreBudget {
                max_steps: steps,
                max_allocated_bytes: u64::MAX,
                max_output_bytes: u64::MAX,
            },
        );
    for fact in admitted() {
        context = context.with_target_pure_intrinsic(fact);
    }
    context
}

fn facts() -> TargetIrLoweringFacts {
    TargetIrLoweringFacts {
        target_profile: target(),
        target_ir_domain: "echo.span-ir/v1".into(),
        operation_profiles: vec!["continuum.profile.read-only/v1".into()],
        obstruction_coordinates: Vec::new(),
        effect_lowerings: Vec::new(),
        effect_signatures: Vec::new(),
        pure_functions: Vec::new(),
    }
    .with_target_pure_intrinsics(&admitted())
}

/// One intent binding `node` to `expression`, with a configurable text bound.
fn source(text_type: &str, expression: &str) -> String {
    let import = target_import();
    format!(
        "package app.ids@1;
        {import}
        type Input = {{
          warp: Bytes<exact=32>, nodeType: Bytes<exact=32>, atomType: Bytes<exact=32>,
          text: {text_type}, anchor: Bytes<exact=32>, buffer: Bytes<exact=32>,
        }};
        type Output = {{ node: Bytes<exact=32>, }};
        intent ids(input: Input) returns Output
          profile p.read basis none budget <= p.small {{
          let node = {expression};
          return {{ node }};
        }}"
    )
}

const NODE: &str = "echo.contentAddress(input.warp, input.nodeType, input.atomType, input.text, [{ edgeType: input.anchor, to: input.buffer }])";

fn compile(text_type: &str, expression: &str, steps: u64) -> CoreModule {
    compile_to_core(
        &parse_module(&source(text_type, expression)).unwrap(),
        &context(steps),
    )
    .unwrap()
}

fn compile_errors(text: &str, steps: u64) -> Vec<CompilerErrorKind> {
    compile_to_core(&parse_module(text).unwrap(), &context(steps))
        .unwrap_err()
        .into_iter()
        .map(|error| error.kind)
        .collect()
}

fn first_call(core: &mut CoreModule) -> (&mut Vec<String>, &mut Vec<CoreExpr>, &mut String) {
    let CoreNode::Let {
        binding,
        value:
            CoreExpr::Call {
                callee,
                type_args,
                args,
            },
    } = &mut core.intents.get_mut("ids").unwrap().body.nodes[0]
    else {
        panic!("intrinsic binding")
    };
    let _ = binding;
    (type_args, args, callee)
}

fn refused(core: &CoreModule, facts: &TargetIrLoweringFacts, case: &str) {
    let report = lower_to_target_ir(core, facts);
    assert_eq!(report.status, TargetLoweringStatus::Unsupported, "{case}");
    assert!(report.artifact.is_none(), "{case}");
    assert!(
        report
            .failures
            .iter()
            .any(|f| f.kind == TargetLoweringFailureKind::InvalidCoreIdentity),
        "{case}: {:?}",
        report.failures
    );
}

// --- Admission ------------------------------------------------------------

#[test]
fn admission_returns_exactly_the_required_pinned_contracts() {
    let facts = admitted();
    let coordinates = facts
        .iter()
        .map(TargetPureIntrinsicFact::coordinate)
        .collect::<Vec<_>>();
    assert_eq!(
        coordinates,
        [CONTENT_ADDRESS, CONTENT_EDGE_ID, SLOT_EDGE_ID]
    );
    let node = &facts[0];
    assert_eq!(node.contract_version(), VERSION);
    assert_eq!(node.parameter_types(), [ID, ID, ID, ATOM_CEILING, EDGES]);
    assert_eq!(node.return_type(), ID);
    assert_eq!(node.cost_base(), 64);
    assert_eq!(node.cost_per_argument_max(), [0, 0, 0, 1, 2]);
    assert_eq!(node.target(), &target());
    assert!(node
        .provider_types()
        .contains_key("echo.dpo@1.OutgoingEdge"));
    assert!(
        facts[1].provider_types().is_empty(),
        "closures are per signature"
    );
}

#[test]
fn admission_refuses_malformed_target_pins() {
    // Target only lowers against a digest-locked profile, so admission must
    // not hand back facts pinned to a malformed one.
    let bytes = encode_canonical_cbor(&catalog_value()).unwrap();
    for digest in [
        "sha256:".to_owned(),
        format!("sha256:{}", "1".repeat(63)),
        format!("sha512:{}", "1".repeat(64)),
    ] {
        let malformed = ResourceRef {
            coordinate: "echo.dpo@1".into(),
            digest: Some(digest.clone()),
        };
        let failures =
            admit_target_pure_intrinsics(&malformed, &pinned(&bytes), &bytes, &required())
                .unwrap_err();
        assert_eq!(
            failures.iter().map(|f| f.kind).collect::<Vec<_>>(),
            [TargetPureIntrinsicFailureKind::UnpinnedResource],
            "{digest}"
        );
    }
}

#[test]
fn admission_fails_closed_on_pins() {
    let bytes = encode_canonical_cbor(&catalog_value()).unwrap();
    let wrong = ResourceRef {
        coordinate: CATALOG.into(),
        digest: Some(format!("sha256:{}", "2".repeat(64))),
    };
    let unpinned = ResourceRef {
        coordinate: CATALOG.into(),
        digest: None,
    };
    let unpinned_target = ResourceRef {
        coordinate: "echo.dpo@1".into(),
        digest: None,
    };
    for (target_ref, catalog_ref, kind) in [
        (
            target(),
            wrong,
            TargetPureIntrinsicFailureKind::DigestMismatch,
        ),
        (
            target(),
            unpinned,
            TargetPureIntrinsicFailureKind::UnpinnedResource,
        ),
        (
            unpinned_target,
            pinned(&bytes),
            TargetPureIntrinsicFailureKind::UnpinnedResource,
        ),
    ] {
        let failures = admit_target_pure_intrinsics(&target_ref, &catalog_ref, &bytes, &required())
            .unwrap_err();
        assert!(failures.iter().any(|f| f.kind == kind), "{failures:?}");
    }
    // A pin over different bytes never authenticates these bytes.
    let other_bytes = encode_canonical_cbor(&V::Map(vec![(text("x"), V::Null)])).unwrap();
    let failures =
        admit_target_pure_intrinsics(&target(), &pinned(&other_bytes), &bytes, &required())
            .unwrap_err();
    assert!(failures
        .iter()
        .any(|f| f.kind == TargetPureIntrinsicFailureKind::DigestMismatch));
    // Non-canonical bytes are refused before any digest comparison.
    let mut noncanonical = bytes.clone();
    noncanonical.push(0);
    let failures =
        admit_target_pure_intrinsics(&target(), &pinned(&bytes), &noncanonical, &required())
            .unwrap_err();
    assert!(failures
        .iter()
        .any(|f| f.kind == TargetPureIntrinsicFailureKind::NonCanonicalCatalog));
}

type Mutation = fn(&mut V);

/// Replace one field of one catalog entry.
fn set_entry(catalog: &mut V, coordinate: &str, key: &str, value: V) {
    *field_mut(field_mut(field_mut(catalog, "intrinsics"), coordinate), key) = value;
}

fn rule(base: i128, per_argument: &[i128]) -> V {
    map(vec![
        ("base", V::Integer(base)),
        ("perArgumentMax", ints(per_argument)),
    ])
}

/// One corruption of an otherwise valid catalog, and the refusal it must cause.
fn contract_mutations() -> [(&'static str, Mutation, TargetPureIntrinsicFailureKind); 12] {
    use TargetPureIntrinsicFailureKind as K;
    [
        (
            "missing intrinsic",
            |v| {
                let V::Map(entries) = field_mut(v, "intrinsics") else {
                    panic!()
                };
                entries.retain(|(key, _)| key != &text(SLOT_EDGE_ID));
            },
            K::MissingIntrinsic,
        ),
        (
            "wrong version",
            |v| {
                set_entry(
                    v,
                    CONTENT_ADDRESS,
                    "contractVersion",
                    text("echo.content-address/v2"),
                );
            },
            K::VersionMismatch,
        ),
        (
            "missing record type",
            |v| *field_mut(v, "types") = V::Map(Vec::new()),
            K::UnresolvedType,
        ),
        (
            "record type outside the target namespace",
            |v| {
                *field_mut(v, "types") = map(vec![
                    ("echo.dpo@1.OutgoingEdge", text(EDGE)),
                    ("other.Edge", text(EDGE)),
                ]);
            },
            K::OutsideNamespace,
        ),
        (
            "non-record provider type",
            |v| *field_mut(v, "types") = map(vec![("echo.dpo@1.OutgoingEdge", text(ID))]),
            K::InvalidProviderType,
        ),
        (
            "unknown argument type",
            |v| {
                let types = texts(&[ID, ID, ID, "echo.dpo@1.Missing"]);
                set_entry(v, CONTENT_EDGE_ID, "argumentTypes", types);
            },
            K::UnresolvedType,
        ),
        (
            "cost rule of the wrong arity",
            |v| set_entry(v, SLOT_EDGE_ID, "costRule", rule(1, &[0])),
            K::InvalidCostRule,
        ),
        (
            "cost coefficient on an unsized argument",
            |v| {
                set_entry(v, SLOT_EDGE_ID, "argumentTypes", texts(&[ID, ID, "U64"]));
                set_entry(v, SLOT_EDGE_ID, "costRule", rule(1, &[0, 0, 3]));
            },
            K::InvalidCostRule,
        ),
        (
            "negative cost",
            |v| set_entry(v, SLOT_EDGE_ID, "costRule", rule(-1, &[0, 0, 0])),
            K::InvalidCostRule,
        ),
        (
            "generic signature",
            |v| set_entry(v, SLOT_EDGE_ID, "typeParameters", texts(&["T"])),
            K::MalformedContract,
        ),
        (
            "unknown contract field",
            |v| {
                let V::Map(entries) = field_mut(field_mut(v, "intrinsics"), SLOT_EDGE_ID) else {
                    panic!()
                };
                entries.push((text("zz"), V::Null));
            },
            K::MalformedContract,
        ),
        (
            "v1 catalog without pure contracts",
            |v| *field_mut(v, "apiVersion") = text("edict.target-profile.intrinsics/v1"),
            K::MalformedCatalog,
        ),
    ]
}

/// Types nested at any depth, and inside provider records, must resolve.
fn nested_type_mutations() -> [(&'static str, Mutation, TargetPureIntrinsicFailureKind); 3] {
    use TargetPureIntrinsicFailureKind as K;
    [
        (
            "unresolved type nested in a map return",
            |v| {
                let nested = text("Map<U64,echo.dpo@1.Missing,max=1>");
                set_entry(v, SLOT_EDGE_ID, "returnType", nested);
            },
            K::UnresolvedType,
        ),
        (
            "foreign type nested in a map return",
            |v| {
                let nested = text("Map<U64,other.Missing,max=1>");
                set_entry(v, SLOT_EDGE_ID, "returnType", nested);
            },
            K::OutsideNamespace,
        ),
        (
            "provider record field nesting an unresolved type",
            |v| {
                *field_mut(v, "types") = map(vec![(
                    "echo.dpo@1.OutgoingEdge",
                    text("Record<edgeType:Bytes<exact=32>,to:List<other.Missing,max=1>>"),
                )]);
            },
            K::InvalidProviderType,
        ),
    ]
}

#[test]
fn admission_fails_closed_on_unknown_or_mismatched_contracts() {
    for (case, mutate, kind) in contract_mutations()
        .into_iter()
        .chain(nested_type_mutations())
    {
        let mut value = catalog_value();
        mutate(&mut value);
        let failures = admit_value(&value).expect_err(case);
        assert!(failures.contains(&kind), "{case}: {failures:?}");
    }
}

#[test]
fn an_effect_intrinsic_cannot_be_admitted_as_pure() {
    let bytes = encode_canonical_cbor(&catalog_value()).unwrap();
    let failures = admit_target_pure_intrinsics(
        &target(),
        &pinned(&bytes),
        &bytes,
        &[RequiredPureIntrinsic {
            coordinate: "echo.dpo@1.replace".into(),
            contract_version: VERSION.into(),
        }],
    )
    .unwrap_err();
    assert!(failures
        .iter()
        .any(|f| f.kind == TargetPureIntrinsicFailureKind::NotPure));
}

// --- Source calls ------------------------------------------------------------

#[test]
fn source_calls_lower_to_the_pinned_coordinate_and_target_accepts_them() {
    for (expression, callee, arity) in [
        (NODE, CONTENT_ADDRESS, 5),
        (
            "echo.contentEdgeId(input.warp, input.anchor, input.nodeType, input.buffer)",
            CONTENT_EDGE_ID,
            4,
        ),
        (
            "echo.slotEdgeId(input.warp, input.buffer, input.nodeType)",
            SLOT_EDGE_ID,
            3,
        ),
    ] {
        let mut core = compile("Bytes<max=4096>", expression, 1_000_000);
        assert!(core
            .imports
            .iter()
            .any(|import| import.kind == CoreImportKind::Target));
        let (type_args, args, actual) = first_call(&mut core);
        assert_eq!(actual, callee);
        assert_eq!(*type_args, Vec::<String>::new());
        assert_eq!(args.len(), arity);
        let report = lower_to_target_ir(&core, &facts());
        assert_eq!(report.status, TargetLoweringStatus::Lowered, "{report:?}");
    }
}

#[test]
fn narrower_operands_are_accepted_and_wider_ones_rejected() {
    // Bytes narrower than the ceiling and an empty edge list are accepted.
    compile(
        "Bytes<max=16>",
        "echo.contentAddress(input.warp, input.nodeType, input.atomType, input.text, [])",
        1_000_000,
    );
    // An atom wider than the contract ceiling is a type mismatch.
    let errors = compile_errors(&source("Bytes<max=2097152>", NODE), 10_000_000);
    assert!(
        errors.contains(&CompilerErrorKind::TypeMismatch),
        "{errors:?}"
    );
    // More edges than the authoring ceiling exceed the list bound.
    let edge = "{ edgeType: input.anchor, to: input.buffer }";
    let seventeen = vec![edge; 17].join(", ");
    let expression = format!(
        "echo.contentAddress(input.warp, input.nodeType, input.atomType, input.text, [{seventeen}])"
    );
    let errors = compile_errors(&source("Bytes<max=16>", &expression), 1_000_000);
    assert!(
        errors.contains(&CompilerErrorKind::InvalidBound),
        "{errors:?}"
    );
    // Swapped roles still have the record's exact field names.
    let errors = compile_errors(
        &source(
            "Bytes<max=16>",
            "echo.contentAddress(input.warp, input.nodeType, input.atomType, input.text, [{ edge: input.anchor, to: input.buffer }])",
        ),
        1_000_000,
    );
    assert_eq!(errors, [CompilerErrorKind::TypeMismatch]);
}

#[test]
fn calls_require_an_explicit_matching_target_import() {
    let pinned = target_import();
    for replacement in [
        String::new(),
        "use target echo.dpo@1 as echo;".to_owned(),
        format!(
            "use target echo.dpo@1 digest \"sha256:{}\" as echo;",
            "3".repeat(64)
        ),
        pinned.replace("echo.dpo@1", "echo.dpo@2"),
    ] {
        let text = source("Bytes<max=16>", NODE).replace(&pinned, &replacement);
        assert_eq!(
            compile_errors(&text, 1_000_000),
            [CompilerErrorKind::UnresolvedFunction],
            "{replacement}"
        );
    }
    let unknown = source(
        "Bytes<max=16>",
        "echo.contentAddres(input.warp, input.nodeType, input.atomType, input.text, [])",
    );
    assert_eq!(
        compile_errors(&unknown, 1_000_000),
        [CompilerErrorKind::UnresolvedFunction]
    );
    let generic = source(
        "Bytes<max=16>",
        "echo.slotEdgeId<U64>(input.warp, input.buffer, input.nodeType)",
    );
    assert_eq!(
        compile_errors(&generic, 1_000_000),
        [CompilerErrorKind::UnsupportedSourceShape]
    );
}

// --- Cost ----------------------------------------------------------------------

/// 64 + 1 * atomMax + 2 * edges, with one literal edge.
fn node_cost(atom_max: u64) -> u64 {
    64 + atom_max + 2
}

#[test]
fn source_charges_the_established_operand_bounds() {
    let small = node_cost(4096);
    compile("Bytes<max=4096>", NODE, small);
    let errors = compile_errors(&source("Bytes<max=4096>", NODE), small - 1);
    assert!(
        errors.contains(&CompilerErrorKind::InvalidBound),
        "{errors:?}"
    );
    // A wider operand is charged for its own wider bound.
    let errors = compile_errors(&source("Bytes<max=8192>", NODE), small);
    assert!(
        errors.contains(&CompilerErrorKind::InvalidBound),
        "{errors:?}"
    );
    compile("Bytes<max=8192>", NODE, node_cost(8192));
}

#[test]
fn target_recomputes_cost_and_refuses_a_forged_budget() {
    let mut core = compile("Bytes<max=4096>", NODE, node_cost(4096));
    let report = lower_to_target_ir(&core, &facts());
    assert_eq!(report.status, TargetLoweringStatus::Lowered, "{report:?}");
    core.intents
        .get_mut("ids")
        .unwrap()
        .core_evaluation_budget
        .max_steps = node_cost(4096) - 1;
    refused(
        &core,
        &facts(),
        "budget below the recomputed intrinsic cost",
    );
}

#[test]
fn target_bounds_a_conditional_list_by_its_larger_branch() {
    let conditional = "echo.contentAddress(input.warp, input.nodeType, input.atomType, input.text, if true then [] else [{ edgeType: input.anchor, to: input.buffer }])";
    // One edge in the larger branch costs the same as one literal edge.
    let mut core = compile("Bytes<max=4096>", conditional, node_cost(4096));
    let report = lower_to_target_ir(&core, &facts());
    assert_eq!(report.status, TargetLoweringStatus::Lowered, "{report:?}");
    core.intents
        .get_mut("ids")
        .unwrap()
        .core_evaluation_budget
        .max_steps = node_cost(4096) - 1;
    refused(&core, &facts(), "budget below the larger branch");
}

#[test]
fn source_functions_charge_intrinsic_calls() {
    let import = target_import();
    let text = format!(
        "package app.ids@1;
        {import}
        fn slot(warp: Bytes<exact=32>, from: Bytes<exact=32>, ty: Bytes<exact=32>) -> Bytes<exact=32> {{
          return echo.slotEdgeId(warp, from, ty);
        }}
        type Input = {{ warp: Bytes<exact=32>, from: Bytes<exact=32>, ty: Bytes<exact=32>, }};
        type Output = {{ edge: Bytes<exact=32>, }};
        intent ids(input: Input) returns Output
          profile p.read basis none budget <= p.small {{
          let edge = slot(input.warp, input.from, input.ty);
          return {{ edge }};
        }}"
    );
    // The helper's minimal budget exceeds that of the same helper returning
    // one argument (whose own work is no greater) by at least the intrinsic's
    // base cost, so the call is charged inside the helper.
    let identity = text.replace("echo.slotEdgeId(warp, from, ty)", "from");
    let charged = minimal_budget(&text);
    let uncharged = minimal_budget(&identity);
    assert!(charged >= uncharged + 32, "{charged} vs {uncharged}");
    let mut core = compile_to_core(&parse_module(&text).unwrap(), &context(1_000_000)).unwrap();
    let report = lower_to_target_ir(&core, &facts());
    assert_eq!(report.status, TargetLoweringStatus::Lowered, "{report:?}");
    // Target charges the helper's intrinsic call to the calling intent.
    core.intents
        .get_mut("ids")
        .unwrap()
        .core_evaluation_budget
        .max_steps = 31;
    refused(
        &core,
        &facts(),
        "helper intrinsic cost above the step budget",
    );
}

// --- Target authority ---------------------------------------------------------

#[test]
fn target_refuses_forged_intrinsic_calls() {
    for case in 0..6 {
        let mut core = compile("Bytes<max=4096>", NODE, 1_000_000);
        match case {
            0 => *first_call(&mut core).2 = "echo.dpo@1.contentAddres".into(),
            1 => first_call(&mut core).0.push("U64".into()),
            2 => {
                first_call(&mut core).1.pop();
            }
            3 => {
                // An argument the contract does not accept.
                first_call(&mut core).1[3] = CoreExpr::Const(edict_syntax::CoreValue::Int {
                    width: "U64".into(),
                    value: "1".into(),
                });
            }
            4 => core
                .imports
                .retain(|import| import.kind != CoreImportKind::Target),
            5 => {
                let CoreNode::Let { binding, .. } =
                    &mut core.intents.get_mut("ids").unwrap().body.nodes[0]
                else {
                    panic!()
                };
                binding.ty = "U64".into();
            }
            _ => unreachable!(),
        }
        refused(&core, &facts(), &format!("case {case}"));
    }
}

#[test]
fn target_refuses_calls_without_matching_pinned_facts() {
    let core = compile("Bytes<max=4096>", NODE, 1_000_000);
    let mut without = facts();
    without.pure_functions.clear();
    refused(&core, &without, "no intrinsic facts");
    let mut other_target = facts();
    other_target.target_profile.digest = Some(format!("sha256:{}", "4".repeat(64)));
    refused(
        &core,
        &other_target,
        "facts pinned to a different target profile",
    );
}

/// Smallest step budget under which `text` compiles.
fn minimal_budget(text: &str) -> u64 {
    let module = parse_module(text).unwrap();
    let (mut low, mut high) = (0_u64, 1_000_000_u64);
    assert!(compile_to_core(&module, &context(high)).is_ok());
    while low < high {
        let middle = low + (high - low) / 2;
        if compile_to_core(&module, &context(middle)).is_ok() {
            high = middle;
        } else {
            low = middle + 1;
        }
    }
    low
}

#[test]
fn helper_cost_metering_keeps_expression_depth_per_function() {
    // Each helper nests its callee in a conditional, so expression depth
    // would accumulate past Core's limit if it carried across calls. Core
    // bounds call height separately, so this valid chain must lower.
    let import = target_import();
    let links = (0..80).map(|index| {
        let next = index + 1;
        format!(
            "fn link{index}(warp: Bytes<exact=32>, from: Bytes<exact=32>, ty: Bytes<exact=32>) -> Bytes<exact=32> {{ return if true then link{next}(warp, from, ty) else from; }}\n"
        )
    });
    let helpers = std::iter::once("fn link80(warp: Bytes<exact=32>, from: Bytes<exact=32>, ty: Bytes<exact=32>) -> Bytes<exact=32> { return echo.slotEdgeId(warp, from, ty); }\n".to_owned())
        .chain(links)
        .collect::<String>();
    let text = format!(
        "package app.chain@1;
        {import}
        {helpers}
        type Input = {{ warp: Bytes<exact=32>, from: Bytes<exact=32>, ty: Bytes<exact=32>, }};
        type Output = {{ edge: Bytes<exact=32>, }};
        intent ids(input: Input) returns Output
          profile p.read basis none budget <= p.small {{
          let edge = link0(input.warp, input.from, input.ty);
          return {{ edge }};
        }}"
    );
    let core = compile_to_core(&parse_module(&text).unwrap(), &context(1_000_000)).unwrap();
    let report = lower_to_target_ir(&core, &facts());
    assert_eq!(report.status, TargetLoweringStatus::Lowered, "{report:?}");
}

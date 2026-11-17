//! The public source fixture for Echo's `ContentAddressV1` identity vectors.
//!
//! A provider runs the compiled fixture on the inputs in
//! `fixtures/content-address-v1/README.md` and must return the vendored Echo
//! vectors; Amblicit does not evaluate identity intrinsics. These tests check
//! the compiler half: Echo's catalog is admitted, and each fixture output
//! reaches its intrinsic with exactly that vector's inputs.
//!
//! Oracle: Echo's documented authoring interface for `echo.dpo@1`
//! (`docs/architecture/authored-operation-effects.md`, Identity intrinsics, at
//! Echo PR #972 head `ea9a42a5`): operand types, ceilings and cost floor.

use amblicit_provider_schema::{
    assemble_provider_contract_pack, ProviderContractPack, ProviderContractPackInput,
};
use amblicit_syntax::{
    admit_target_pure_intrinsics, canonical_target_profile_contract_resources, compile_to_core,
    digest_target_intrinsics_catalog, encode_canonical_cbor, lower_to_target_ir, parse_module,
    CanonicalValue as V, CompilerContext, CoreBudget, CoreExpr, CoreModule, CoreNode,
    RequiredPureIntrinsic, ResourceRef, TargetIrLoweringFacts, TargetLoweringStatus,
    TargetPureIntrinsicFact,
};

const CATALOG_CONTRACT: &str = "target-profile-intrinsics";
const CATALOG: &str = "echo.dpo@1.intrinsics";
const CONTENT_ADDRESS: &str = "echo.dpo@1.contentAddress";
const CONTENT_EDGE_ID: &str = "echo.dpo@1.contentEdgeId";
const SLOT_EDGE_ID: &str = "echo.dpo@1.slotEdgeId";
const VERSION: &str = "echo.content-address/v1";
const ID: &str = "Bytes<exact=32>";
const EDGE: &str = "Record<edgeType:Bytes<exact=32>,to:Bytes<exact=32>>";

const SOURCE: &str = include_str!("../../../fixtures/lang/intrinsics/content-address-v1.amblicit");

// --- Echo's catalog ABI ------------------------------------------------------

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

/// One pure contract with Echo's documented operand types and cost floor.
fn pure_entry(name: &str, arguments: &[&str], base: i128, per_argument: &[i128]) -> V {
    map(vec![
        ("intrinsicClass", text("pure")),
        ("typeParameters", V::Array(Vec::new())),
        (
            "argumentTypes",
            V::Array(arguments.iter().map(|argument| text(argument)).collect()),
        ),
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
                (
                    "perArgumentMax",
                    V::Array(per_argument.iter().copied().map(V::Integer).collect()),
                ),
            ]),
        ),
    ])
}

/// The three identity intrinsics with Echo's documented ABI: 32-byte ids, an
/// atom of at most 1,048,576 bytes, at most 16 outgoing edges, and costs of
/// 71 + 1 per atom byte + 67 per edge, 70 and 69 steps.
fn echo_catalog() -> V {
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
                        &[
                            ID,
                            ID,
                            ID,
                            "Bytes<max=1048576>",
                            "List<echo.dpo@1.OutgoingEdge,max=16>",
                        ],
                        71,
                        &[0, 0, 0, 1, 67],
                    ),
                ),
                (
                    CONTENT_EDGE_ID,
                    pure_entry(CONTENT_EDGE_ID, &[ID, ID, ID, ID], 70, &[0, 0, 0, 0]),
                ),
                (
                    SLOT_EDGE_ID,
                    pure_entry(SLOT_EDGE_ID, &[ID, ID, ID], 69, &[0, 0, 0]),
                ),
            ]),
        ),
    ])
}

fn target() -> ResourceRef {
    ResourceRef {
        coordinate: "echo.dpo@1".into(),
        digest: Some(format!("sha256:{}", "1".repeat(64))),
    }
}

fn admitted() -> Vec<TargetPureIntrinsicFact> {
    let bytes = encode_canonical_cbor(&echo_catalog()).expect("canonical catalog");
    let pinned = ResourceRef {
        coordinate: CATALOG.into(),
        digest: Some(digest_target_intrinsics_catalog(CATALOG, &bytes).expect("catalog digest")),
    };
    let required =
        [CONTENT_ADDRESS, CONTENT_EDGE_ID, SLOT_EDGE_ID].map(|coordinate| RequiredPureIntrinsic {
            coordinate: coordinate.into(),
            contract_version: VERSION.into(),
        });
    admit_target_pure_intrinsics(&target(), &pinned, &bytes, &required)
        .expect("the compiler admits Echo's catalog")
}

/// The current publication, assembled from the authoritative ABI sources.
fn current_pack() -> ProviderContractPack {
    assemble_provider_contract_pack(ProviderContractPackInput {
        common_cddl: include_bytes!("../../../docs/abi/edict-common.cddl"),
        core_cddl: include_bytes!("../../../docs/abi/edict-core.cddl"),
        lawpack_cddl: include_bytes!("../../../docs/abi/edict-lawpack.cddl"),
        lawpack_adapter_cddl: include_bytes!("../../../docs/abi/edict-lawpack-adapter.cddl"),
        target_profile_cddl: include_bytes!("../../../docs/abi/edict-target-profile.cddl"),
        authority_facts_cddl: include_bytes!("../../../docs/abi/edict-authority-facts.cddl"),
        result_projection_cddl: include_bytes!("../../../docs/abi/edict-result-projection.cddl"),
        target_ir_cddl: include_bytes!("../../../docs/abi/edict-target-ir.cddl"),
        source_closure_cddl: Some(include_bytes!(
            "../../../docs/abi/edict-source-closure.cddl"
        )),
        contract_resources: canonical_target_profile_contract_resources(),
    })
    .expect("pack assembles")
}

#[test]
fn echo_identity_catalog_is_valid_under_the_current_pack() {
    current_pack()
        .validate_contract(CATALOG_CONTRACT, &echo_catalog())
        .expect("the current pack admits Echo's identity catalog");
}

#[test]
fn compiler_admits_echo_identity_contracts_at_echos_cost_floor() {
    let contracts = admitted()
        .iter()
        .map(|fact| {
            (
                fact.coordinate().to_owned(),
                fact.parameter_types().to_vec(),
                fact.cost_base(),
                fact.cost_per_argument_max().to_vec(),
            )
        })
        .collect::<Vec<_>>();

    let atom = "Bytes<max=1048576>".to_owned();
    let edges = "List<echo.dpo@1.OutgoingEdge,max=16>".to_owned();
    let id = ID.to_owned();
    assert_eq!(
        contracts,
        [
            (
                CONTENT_ADDRESS.to_owned(),
                vec![id.clone(), id.clone(), id.clone(), atom, edges],
                71,
                vec![0, 0, 0, 1, 67],
            ),
            (
                CONTENT_EDGE_ID.to_owned(),
                vec![id.clone(); 4],
                70,
                vec![0; 4],
            ),
            (SLOT_EDGE_ID.to_owned(), vec![id; 3], 69, vec![0; 3]),
        ]
    );
}

// --- The public source fixture ----------------------------------------------

fn context() -> CompilerContext {
    let mut context = CompilerContext::new()
        .with_operation_profile("p.read", "continuum.profile.read-only/v1")
        .with_budget(
            "p.small",
            CoreBudget {
                max_steps: 65_536,
                max_allocated_bytes: 1_048_576,
                max_output_bytes: 65_536,
            },
        );
    for fact in admitted() {
        context = context.with_target_pure_intrinsic(fact);
    }
    context
}

fn compiled() -> CoreModule {
    compile_to_core(&parse_module(SOURCE).expect("fixture parses"), &context())
        .expect("fixture compiles to Core")
}

/// Core expressions as source-like text. The intent's input local prints as
/// `input`, and a local bound to a call prints as that call.
struct Render {
    input_type: String,
    calls: Vec<(String, String)>,
}

impl Render {
    fn expr(&self, expr: &CoreExpr) -> String {
        match expr {
            CoreExpr::Local { reference } if reference.ty == self.input_type => "input".into(),
            CoreExpr::Local { reference } => self
                .calls
                .iter()
                .find(|(local, _)| local == &reference.alpha_name)
                .map_or_else(
                    || panic!("unbound local {}", reference.alpha_name),
                    |(_, call)| call.clone(),
                ),
            CoreExpr::Field { base, field } => format!("{}.{field}", self.expr(base)),
            CoreExpr::List { values } => format!("[{}]", self.list(values)),
            CoreExpr::Record { fields } => self.record(fields.iter()),
            CoreExpr::OrderedRecord { fields } => self.record(fields.iter().map(|(k, v)| (k, v))),
            CoreExpr::Call {
                callee,
                type_args,
                args,
            } if type_args.is_empty() => format!("{callee}({})", self.list(args)),
            other => panic!("unexpected identity expression {other:?}"),
        }
    }

    fn list(&self, values: &[CoreExpr]) -> String {
        values
            .iter()
            .map(|value| self.expr(value))
            .collect::<Vec<_>>()
            .join(", ")
    }

    fn record<'a>(&self, fields: impl Iterator<Item = (&'a String, &'a CoreExpr)>) -> String {
        let fields = fields
            .map(|(name, value)| format!("{name}: {}", self.expr(value)))
            .collect::<Vec<_>>();
        format!("{{{}}}", fields.join(", "))
    }
}

/// Each output field of the compiled fixture, as the call that computes it.
fn outputs(core: &CoreModule) -> Vec<(String, String)> {
    let intent = &core.intents["ids"];
    let mut render = Render {
        input_type: intent.input.clone(),
        calls: Vec::new(),
    };
    for node in &intent.body.nodes {
        let CoreNode::Let { binding, value } = node else {
            panic!("unexpected fixture node {node:?}")
        };
        let call = render.expr(value);
        render.calls.push((binding.alpha_name.clone(), call));
    }
    let CoreExpr::Record { fields } = &intent.body.result else {
        panic!("fixture result is not a record")
    };
    fields
        .iter()
        .map(|(name, value)| (name.clone(), render.expr(value)))
        .collect()
}

#[test]
fn each_fixture_output_is_its_vectors_intrinsic_call_with_edges_as_written() {
    // Edges reach the provider as written; the identity law, not the
    // compiler, puts them in canonical order.
    let node = |edges: &str| {
        format!(
            "{CONTENT_ADDRESS}(input.warp, input.nodeType, input.atomType, input.atom, [{edges}])"
        )
    };
    let a_high = "{edgeType: input.edgeA, to: input.high}";
    let b_low = "{edgeType: input.edgeB, to: input.low}";
    let mut expected = vec![
        ("nodeZeroEdges", node("")),
        ("nodeOneEdge", node(a_high)),
        ("nodeTwoEdges", node(&format!("{b_low}, {a_high}"))),
        ("nodeTwoEdgesReordered", node(&format!("{a_high}, {b_low}"))),
        (
            "contentEdge",
            format!("{CONTENT_EDGE_ID}(input.warp, input.low, input.edgeA, input.high)"),
        ),
        (
            "slotEdge",
            format!("{SLOT_EDGE_ID}(input.warp, input.low, input.edgeA)"),
        ),
    ]
    .into_iter()
    .map(|(output, call)| (output.to_owned(), call))
    .collect::<Vec<_>>();
    expected.sort();

    assert_eq!(outputs(&compiled()), expected);
}

#[test]
fn fixture_lowers_to_target_ir_against_echos_catalog() {
    let facts = TargetIrLoweringFacts {
        target_profile: target(),
        target_ir_domain: "echo.span-ir/v1".into(),
        operation_profiles: vec!["continuum.profile.read-only/v1".into()],
        obstruction_coordinates: Vec::new(),
        effect_lowerings: Vec::new(),
        effect_signatures: Vec::new(),
        pure_functions: Vec::new(),
    }
    .with_target_pure_intrinsics(&admitted());

    let report = lower_to_target_ir(&compiled(), &facts);

    assert_eq!(report.status, TargetLoweringStatus::Lowered, "{report:?}");
    assert!(report.artifact.is_some(), "{report:?}");
}

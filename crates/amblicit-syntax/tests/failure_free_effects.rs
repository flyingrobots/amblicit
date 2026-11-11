//! Effects that declare no failures are callable from source without a map.
//!
//! Echo's five mutation intrinsics only emit a mutation intent; the runtime
//! gives them no obstruction path, and its catalog and runtime ABI require
//! their declared failure sets to be empty. A source program must be able to
//! bind each one directly, while an effect that declares any failure still
//! needs its failure map.
use amblicit_syntax::{
    author_lawpack, compile_to_core, decode_lawpack_adapter, decode_lawpack_bundle,
    lower_to_target_ir, parse_module, prepare_lawpack_compilation, CompilerError,
    CompilerErrorKind, CompilerStage, CoreModule, CoreNode, LawpackArtifactKind,
    LawpackAuthoredArtifactSet, LawpackAuthoringDefinition, TargetIrArtifact, TargetLoweringStatus,
};

const PIN: &str = "sha256:1111111111111111111111111111111111111111111111111111111111111111";
const ID: &str = "Bytes<exact=32>";
const ATOM: &str = "Bytes<max=64>";

/// Echo's five mutation intrinsics with the effect kind and ABI input fields
/// its runtime requires (`warp-core/src/edict_effects/mutations.rs`).
const MUTATIONS: [(&str, &str, &str, &[&str]); 5] = [
    (
        "ensureNode",
        "echo.dpo@1.ensure-node",
        "ensure",
        &["atomBytes", "atomType", "nodeId", "nodeType", "warpId"],
    ),
    (
        "createNode",
        "echo.dpo@1.create-node",
        "create",
        &["atomBytes", "atomType", "nodeId", "nodeType", "warpId"],
    ),
    (
        "replaceAtom",
        "echo.dpo@1.replace-atom",
        "replace",
        &[
            "atomBytes",
            "atomType",
            "expectedAtomDigest",
            "nodeId",
            "warpId",
        ],
    ),
    (
        "ensureEdge",
        "echo.dpo@1.ensure-edge",
        "ensure",
        &["edgeId", "edgeType", "fromId", "toId", "warpId"],
    ),
    (
        "replaceEdge",
        "echo.dpo@1.replace-edge",
        "replace",
        &["edgeId", "expectedToId", "toId", "warpId"],
    ),
];

fn input_type(fields: &[&str]) -> String {
    let fields = fields
        .iter()
        .map(|field| {
            let ty = if *field == "atomBytes" { ATOM } else { ID };
            format!("{field}:{ty}")
        })
        .collect::<Vec<_>>()
        .join(",");
    format!("Record<{fields}>")
}

/// A lawpack exporting one effect per mutation intrinsic, each with an empty
/// failure set, plus `readFact`, which declares a failure.
fn definition() -> LawpackAuthoringDefinition {
    let mut types = vec![
        serde_json::json!({"coordinate": "graph.rope@1.Done", "definition": "Record<>"}),
        serde_json::json!({"coordinate": "graph.rope@1.Address", "definition": ID}),
        serde_json::json!({"coordinate": "graph.rope@1.Fact", "definition": ATOM}),
    ];
    let mut effects = vec![serde_json::json!({
        "coordinate": "graph.rope@1.readFact",
        "typeParameters": [],
        "inputType": "graph.rope@1.Address",
        "outputType": "graph.rope@1.Fact",
        "executionClass": "runtime",
        "effectKindHint": "read",
        "footprintObligation": "graph.rope@1.footprint",
        "costObligation": "graph.rope@1.budget",
        "effectFailures": {
            "missing": {
                "authorityClass": "domainMappable",
                "payloadType": "graph.rope@1.Address"
            }
        },
        "guardSupport": true
    })];
    let mut implementations = serde_json::Map::new();
    implementations.insert(
        "graph.rope@1.readFact".to_owned(),
        serde_json::json!({
            "targetIntrinsic": "echo.dpo@1.node-atom-read",
            "targetConfiguration": {"local": "echo-config"},
            "writeClass": "read",
            "footprintObligation": "graph.rope@1.footprint",
            "costObligation": "graph.rope@1.budget",
            "failureMappings": {"missing": "graph.rope@1.Missing"}
        }),
    );
    let mut semantic_effects = vec!["graph.rope@1.readFact".to_owned()];
    for (name, intrinsic, kind, fields) in MUTATIONS {
        let effect = format!("graph.rope@1.{name}");
        let input = format!("graph.rope@1.{name}Input");
        types.push(serde_json::json!({"coordinate": input, "definition": input_type(fields)}));
        effects.push(serde_json::json!({
            "coordinate": effect,
            "typeParameters": [],
            "inputType": input,
            "outputType": "graph.rope@1.Done",
            "executionClass": "runtime",
            "effectKindHint": kind,
            "footprintObligation": "graph.rope@1.footprint",
            "costObligation": "graph.rope@1.budget",
            "effectFailures": {},
            "guardSupport": true
        }));
        implementations.insert(
            effect.clone(),
            serde_json::json!({
                "targetIntrinsic": intrinsic,
                "targetConfiguration": {"local": "echo-config"},
                "writeClass": kind,
                "footprintObligation": "graph.rope@1.footprint",
                "costObligation": "graph.rope@1.budget",
                "failureMappings": {}
            }),
        );
        semantic_effects.push(effect);
    }
    serde_json::from_value(serde_json::json!({
        "schema": "edict.lawpack-authoring/v1",
        "id": "graph.rope",
        "version": "1",
        "acceptedCoreAbi": ["edict.core/v1"],
        "dependencies": [],
        "exportsCoordinate": "graph.rope.exports/v1",
        "exports": {
            "types": types,
            "constants": [],
            "pureFunctions": [],
            "effects": effects,
            "obstructions": [{
                "coordinate": "graph.rope@1.Missing",
                "authorityClass": "domainMappable",
                "payloadSchema": "graph.rope@1.Address"
            }],
            "operationProfiles": {
                "graph.rope@1.write": {
                    "opticTemplate": {
                        "opticKind": "affectReintegration",
                        "boundaryKind": "affect",
                        "supportPolicy": "graph.rope@1.support",
                        "lossDisposition": "graph.rope@1.lossless",
                        "basisTemplate": "graph.rope@1.basis",
                        "apertureRequirement": {
                            "kind": "abstractFootprintObligation",
                            "reference": "graph.rope@1.footprint"
                        }
                    },
                    "effectPredicate": "graph.rope@1.effects"
                }
            }
        },
        "targetAdapters": [{
            "coordinate": "graph.rope.echo-adapter/v1",
            "output": "adapter.cbor",
            "acceptedTargetProfile": {"id": "echo.dpo@1", "digest": PIN},
            "acceptedTargetIr": {"id": "echo.span-ir/v2", "digest": PIN},
            "operationProfiles": {
                "graph.rope@1.write": {
                    "core": "continuum.profile.graph-effects/v1",
                    "semanticEffects": semantic_effects,
                    "budgetObligation": "graph.rope@1.budget",
                    "targetConfiguration": {"local": "echo-config"}
                }
            },
            "effectImplementations": implementations,
            "budgets": {
                "graph.rope@1.budget": {
                    "maxSteps": 4096,
                    "maxAllocatedBytes": 65536,
                    "maxOutputBytes": 64
                }
            }
        }],
        "verifier": {"class": "declarative", "ruleset": {"local": "rules"}},
        "compatibility": {"id": "graph.rope.compatibility/v1", "digest": PIN},
        "conformanceFixtureCorpus": {"id": "graph.rope.fixtures/v1", "digest": PIN},
        "localResources": [{
            "name": "echo-config",
            "coordinate": "graph.rope.echo-config/v1",
            "output": "echo-config.cbor",
            "value": {"apiVersion": "graph.rope.echo-config/v1"}
        }, {
            "name": "rules",
            "coordinate": "graph.rope.verifier-rules/v1",
            "output": "verifier-rules.cbor",
            "value": {"opcodes": {"$edictBytes": "00"}}
        }]
    }))
    .expect("typed authoring definition")
}

fn authored() -> LawpackAuthoredArtifactSet {
    author_lawpack(&definition(), &[]).expect("author the mutation lawpack")
}

fn record_for(fields: &[&str]) -> String {
    let entries = fields
        .iter()
        .map(|field| {
            let value = if *field == "atomBytes" {
                "input.bytes"
            } else {
                "input.id"
            };
            format!("{field}: {value}")
        })
        .collect::<Vec<_>>()
        .join(", ");
    format!("{{ {entries} }}")
}

/// One intent binding every mutation effect directly, with no failure map.
fn program(authored: &LawpackAuthoredArtifactSet, body: &str) -> String {
    let digest = authored
        .artifact(LawpackArtifactKind::Manifest)
        .expect("manifest")
        .digest();
    format!(
        "package consumer.rope@1;\n\
         use lawpack graph.rope@1 digest \"{digest}\" as fx;\n\
         type Input = {{ id: Bytes<exact=32>, bytes: Bytes<max=64>, }};\n\
         type Output = {{ id: Bytes<exact=32>, }};\n\
         intent write(input: Input) returns Output\n\
           profile fx.write\n\
           basis input.id\n\
           budget <= fx.budget\n\
         {{\n{body}  return {{ id: input.id }};\n}}\n"
    )
}

fn every_mutation_body() -> String {
    MUTATIONS
        .iter()
        .map(|(name, _, _, fields)| {
            format!(
                "  let {name}Done: fx.Done = fx.{name}({});\n",
                record_for(fields)
            )
        })
        .collect()
}

fn compile(
    authored: &LawpackAuthoredArtifactSet,
    source: &str,
) -> Result<(CoreModule, amblicit_syntax::TargetLoweringReport), Vec<CompilerError>> {
    let manifest = authored.artifact(LawpackArtifactKind::Manifest).unwrap();
    let exports = authored.artifact(LawpackArtifactKind::Exports).unwrap();
    let adapter = authored.artifact(LawpackArtifactKind::Adapter).unwrap();
    let bundle = decode_lawpack_bundle(manifest.bytes(), exports.bytes()).expect("exact bundle");
    let adapter = decode_lawpack_adapter(&bundle, "echo.dpo@1", adapter.bytes()).expect("adapter");
    let module = parse_module(source).expect("consumer parses");
    let preparation =
        prepare_lawpack_compilation(&module, &bundle, &adapter).expect("exact closure");
    let core = compile_to_core(&module, preparation.compiler_context())?;
    let report = lower_to_target_ir(&core, preparation.target_ir_facts());
    Ok((core, report))
}

fn lowered(report: amblicit_syntax::TargetLoweringReport) -> TargetIrArtifact {
    assert_eq!(
        report.status,
        TargetLoweringStatus::Lowered,
        "{:?}",
        report.failures
    );
    report.artifact.expect("lowered Target IR")
}

#[test]
fn each_echo_mutation_intrinsic_is_callable_without_a_failure_map() {
    let authored = authored();
    let source = program(&authored, &every_mutation_body());
    let (core, report) = compile(&authored, &source).expect("failure-free effects compile");

    let intent = &core.intents["write"];
    let effects = intent
        .body
        .nodes
        .iter()
        .filter_map(|node| match node {
            CoreNode::Effect {
                effect,
                obstruction_map,
                ..
            } => Some((effect.as_str(), obstruction_map.len())),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(
        effects,
        MUTATIONS
            .iter()
            .map(|(name, ..)| (
                match *name {
                    "ensureNode" => "fx.ensureNode",
                    "createNode" => "fx.createNode",
                    "replaceAtom" => "fx.replaceAtom",
                    "ensureEdge" => "fx.ensureEdge",
                    _ => "fx.replaceEdge",
                },
                0
            ))
            .collect::<Vec<_>>(),
        "one Core effect node per call, in source order, each with an empty obstruction map"
    );

    let artifact = lowered(report);
    let steps = &artifact.intents["write"].steps;
    assert_eq!(
        steps
            .iter()
            .map(|step| step.target_intrinsic.as_str())
            .collect::<Vec<_>>(),
        MUTATIONS
            .iter()
            .map(|(_, intrinsic, ..)| *intrinsic)
            .collect::<Vec<_>>()
    );
    for step in steps {
        assert!(
            step.obstruction_arms.is_empty(),
            "{}",
            step.target_intrinsic
        );
        assert!(
            step.obstruction_failures.is_empty(),
            "{}",
            step.target_intrinsic
        );
    }
}

#[test]
fn an_effect_that_can_fail_still_needs_its_failure_map() {
    let authored = authored();
    let bare = program(&authored, "  let fact: fx.Fact = fx.readFact(input.id);\n");
    let errors = compile(&authored, &bare).expect_err("a failing effect needs its map");
    assert_eq!(
        errors[0].kind,
        CompilerErrorKind::EffectWithoutFailureMapping
    );
    assert_eq!(errors[0].stage, CompilerStage::TypeCheck);
    assert!(bare[errors[0].span.start..errors[0].span.end].contains("fx.readFact"));
    assert!(errors[0].message.contains("missing"));

    let mapped = program(
        &authored,
        "  let fact: fx.Fact = fx.readFact(input.id)\n    else { missing(absent) => fx.Missing };\n",
    );
    compile(&authored, &mapped).expect("the mapped control compiles");
}

#[test]
fn a_failure_free_effect_inside_an_expression_is_still_refused() {
    let authored = authored();
    let (name, _, _, fields) = MUTATIONS[0];
    let call = format!("fx.{name}({})", record_for(fields));
    for body in [
        format!("  let done: fx.Done = if true then {call} else {call};\n"),
        format!("  let done: fx.Done = if input.id == input.id then {call} else {call};\n"),
    ] {
        let source = program(&authored, &body);
        let errors = compile(&authored, &source).expect_err("an effect is not a pure value");
        assert_eq!(
            errors[0].kind,
            CompilerErrorKind::EffectWithoutFailureMapping,
            "{body}: {errors:?}"
        );
        assert!(
            errors[0].message.contains("annotated `let`"),
            "{}",
            errors[0].message
        );
    }
}

#[test]
fn a_failure_free_effect_binding_requires_its_result_annotation() {
    let authored = authored();
    let (name, _, _, fields) = MUTATIONS[0];
    let source = program(
        &authored,
        &format!("  let done = fx.{name}({});\n", record_for(fields)),
    );
    let errors = compile(&authored, &source).expect_err("effect bindings are annotated");
    assert_eq!(
        errors[0].kind,
        CompilerErrorKind::UnsupportedSourceShape,
        "{errors:?}"
    );
    assert!(errors[0]
        .message
        .contains("explicit result type annotation"));
}

#[test]
fn an_effect_without_an_authenticated_signature_still_needs_a_map() {
    // A write-class fact carries no failure authority, so an empty failure
    // set cannot be inferred from it.
    let source = r#"package a.b@1;
use lawpack hello.echo@1 digest "sha256:1111111111111111111111111111111111111111111111111111111111111111" as hello;
type Input = { id: String<max=16>, };
type Receipt = { id: String<max=16>, };
type Output = { id: String<max=16>, };
intent t(input: Input) returns Output
  profile p.effectful
  basis none
  budget <= p.tiny {
  let receipt: Receipt = hello.known(input.id);
  return { id: input.id };
}"#;
    let context = amblicit_syntax::CompilerContext::new()
        .with_operation_profile("p.effectful", "test.effectful@1")
        .with_operation_profile_write_classes("p.effectful", [amblicit_syntax::WriteClass::Create])
        .with_budget(
            "p.tiny",
            amblicit_syntax::CoreBudget {
                max_steps: 100,
                max_allocated_bytes: 512,
                max_output_bytes: 512,
            },
        )
        .with_effect_write_class("hello.known", amblicit_syntax::WriteClass::Create);
    let module = parse_module(source).expect("legacy effect source parses");
    let errors =
        compile_to_core(&module, &context).expect_err("an unauthenticated effect is not admitted");
    assert_eq!(
        errors[0].kind,
        CompilerErrorKind::UnresolvedFunction,
        "{errors:?}"
    );
}

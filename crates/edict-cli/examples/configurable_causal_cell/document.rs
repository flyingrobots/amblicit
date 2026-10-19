//! Application-owned authoring JSON; Edict computes all local artifact digests.
use super::limits::{ByteBudgets, LimitError, Limits};
use serde_json::{json, Value};

pub fn document(limits: Limits) -> Result<Value, LimitError> {
    let budget = limits.budgets()?;
    Ok(json!({
        "schema": "edict.lawpack-build/v1",
        "outputDirectory": "vendor/cell",
        "dependencyBundles": [],
        "lawpack": {
            "schema": "edict.lawpack-authoring/v1",
            "id": "example.cell", "version": "1",
            "acceptedCoreAbi": ["edict.core/v1"], "dependencies": [],
            "exportsCoordinate": "example.cell.exports/v1",
            "exports": exports(limits),
            "targetAdapters": [adapter(budget)],
            "verifier": {"class": "declarative", "ruleset": {"local": "rules"}},
            "compatibility": {"local": "compatibility"},
            "conformanceFixtureCorpus": {"local": "fixtures"},
            "localResources": [
                {"name": "configuration", "coordinate": "echo.operation-lowering-configuration/v1",
                 "output": "configuration.cbor", "value": configuration(limits, budget)},
                {"name": "rules", "coordinate": "example.cell.rules/v1", "output": "rules.cbor",
                 "value": {"effect": "example.cell@1.createIfAbsent", "writeClass": "create"}},
                {"name": "compatibility", "coordinate": "example.cell.compatibility/v1", "output": "compatibility.cbor",
                 "value": {"core": "edict.core/v1", "target": "echo.span-ir/v1"}},
                {"name": "fixtures", "coordinate": "example.cell.fixtures/v1", "output": "fixtures.cbor",
                 "value": {"keyScalars": limits.key_scalars, "valueScalars": limits.value_scalars,
                           "replacementBytes": limits.replacement_bytes}}
            ]
        }
    }))
}

fn exports(limits: Limits) -> Value {
    let key = format!("String<max={},canonical=raw-utf8>", limits.key_scalars);
    let value = format!("String<max={},canonical=raw-utf8>", limits.value_scalars);
    json!({
        "types": [
            {"coordinate": "example.cell@1.CreateInput", "definition": format!("Record<basis:String<max=128,canonical=raw-utf8>,key:{key},value:{value}>")},
            {"coordinate": "example.cell@1.CreateReceipt", "definition": format!("Record<key:{key}>")},
            {"coordinate": "example.cell@1.ExistingValue", "definition": format!("Record<key:{key},value:{value}>")}
        ],
        "constants": [], "pureFunctions": [],
        "effects": [{
            "coordinate": "example.cell@1.createIfAbsent", "typeParameters": [],
            "inputType": "example.cell@1.CreateInput", "outputType": "example.cell@1.CreateReceipt",
            "executionClass": "runtime", "effectKindHint": "create",
            "footprintObligation": "example.cell@1.cellKeyFootprint", "costObligation": "example.cell@1.createBudget",
            "effectFailures": {"alreadyExists": {"authorityClass": "domainMappable", "payloadType": "example.cell@1.ExistingValue"}},
            "guardSupport": true
        }],
        "obstructions": [{"coordinate": "example.cell@1.AlreadyExists", "authorityClass": "domainMappable", "payloadSchema": "example.cell@1.ExistingValue"}],
        "operationProfiles": {"example.cell@1.createIfAbsent": {
            "opticTemplate": {"opticKind": "affectReintegration", "boundaryKind": "affect",
                "supportPolicy": "example.cell@1.directSupport", "lossDisposition": "example.cell@1.lossless",
                "apertureRequirement": {"kind": "abstractFootprintObligation", "reference": "example.cell@1.cellKeyFootprint"}},
            "effectPredicate": "example.cell@1.createIfAbsentEffect"
        }}
    })
}

fn adapter(budget: ByteBudgets) -> Value {
    json!({
        "coordinate": "example.cell.echo-adapter/v1", "output": "adapter.cbor",
        "acceptedTargetProfile": {"id": "echo.dpo@1", "digest": "sha256:2e2494121aecf5e6a2d920f5fb85408825d394765fad41484c416397c920fb04"},
        "acceptedTargetIr": {"id": "echo.span-ir/v1", "digest": "sha256:0057167e68f50c99dcce087b3e1cd677d17c5d1dc238bdb52d89469e1472fc2f"},
        "operationProfiles": {"example.cell@1.createIfAbsent": {
            "core": "continuum.profile.create/v1", "semanticEffects": ["example.cell@1.createIfAbsent"]
        }},
        "effectImplementations": {"example.cell@1.createIfAbsent": {
            "targetIntrinsic": "echo.dpo@1.anchored-node-attachment-create-if-absent",
            "targetConfiguration": {"local": "configuration"}, "writeClass": "create",
            "footprintObligation": "example.cell@1.cellKeyFootprint", "costObligation": "example.cell@1.createBudget",
            "failureMappings": {"alreadyExists": "echo.executable-operation/precondition-mismatch/v1"}
        }},
        "budgets": {"example.cell@1.createBudget": {"maxSteps": 16,
            "maxAllocatedBytes": budget.allocated, "maxOutputBytes": budget.output}}
    })
}

fn configuration(limits: Limits, budget: ByteBudgets) -> Value {
    json!({
        "apiVersion": "echo.operation-lowering-configuration/v1",
        "programKind": "anchored-node-attachment-create-if-absent/v1",
        "requiredNodeTypeProfile": "causal.cell.node/value/v1",
        "requiredAttachmentTypeProfile": "causal.cell.attachment/value/v1",
        "maxReplacementBytes": limits.replacement_bytes,
        "authorityProfile": "causal.cell.authority.application/v1",
        "budgetCeiling": {"steps": 16, "readBytes": 64, "writeBytes": budget.write},
        "invocationBinding": {"nodeKeyField": "key", "replacementField": "value",
            "nodeIdDerivation": "sha256-utf8/v1", "warpIdSource": "action-lane/v1"}
    })
}

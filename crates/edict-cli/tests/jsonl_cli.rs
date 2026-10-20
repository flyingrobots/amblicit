use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Output, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};

use edict_syntax::{
    compile_to_core, digest_core_module, digest_target_ir_artifact, lower_to_target_ir,
    CompilerContext, CoreBudget, ResourceRef, TargetEffectLowering, TargetIrLoweringFacts,
    WriteClass, ECHO_DPO_TARGET_PROFILE, ECHO_SPAN_IR_DOMAIN,
};
use serde_json::{json, Value};

const VALID_SOURCE: &str = r#"package examples.hello@1;

use lawpack hello.optics@1 digest "sha256:0000000000000000000000000000000000000000000000000000000000000000" as hello;

type HelloInput = {
  name: String<max=256>,
};

intent sayHello(input: HelloInput)
  returns HelloInput
  profile hello.readOnly
  basis none
  budget <= hello.tinyBudget
{
  return { name: input.name };
}
"#;

// The import contributes semantic-closure identity; the projection settings
// independently inject `target.replace` as a synthetic native effect.
const ECHO_SOURCE: &str = r#"package demo.echo@1;

use lawpack demo.write@1 digest "sha256:2222222222222222222222222222222222222222222222222222222222222222" as semantics;

type Input = { id: String<max=16>, basis: String<max=128>, };
type Receipt = { id: String<max=16>, };
type Output = { id: String<max=16>, };

intent replaceThing(input: Input)
  returns Output
  profile p.effectful
  basis input.basis
  budget <= p.tiny
{
  let receipt: Receipt = target.replace(input.id)
    else { rejected(reason) => domain.WriteRejected };
  return { id: input.id };
}
"#;

const EXTERNAL_REQUEST_SOURCE: &str = r#"package demo.external_request@1;

use capability workspace.snapshot.observe@1 digest "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa" as snapshot;

type Input = {
  payload: Bytes<max=1024>,
  scope: Bytes<max=32>,
  basis: Bytes<max=32>,
  maxSettlementBytes: U64,
  maxAttempts: U32,
};

intent observe(input: Input)
  returns ExternalActionRequest<Bytes<max=65536>>
  profile hello.readOnly
  basis input.basis
  budget <= hello.tinyBudget
{
  request pending: ExternalActionRequest<Bytes<max=65536>> =
    snapshot(input.payload)
    input schema workspace.snapshot.input@1 digest "sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
    settlement schema workspace.snapshot.settlement@1 digest "sha256:cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc"
    authority input.scope
    basis input.basis
    budget maxSettlementBytes input.maxSettlementBytes maxAttempts input.maxAttempts
    reconcile workspace.snapshot.reconcile@1 digest "sha256:dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd";
  return pending;
}
"#;

const ECHO_TARGET_PROFILE_DIGEST: &str =
    "sha256:1111111111111111111111111111111111111111111111111111111111111111";

static TEMP_COUNTER: AtomicUsize = AtomicUsize::new(0);

#[test]
fn build_accepts_application_request_without_compiler_input_records() {
    let root = temp_tree("missing-build-application");
    let application = root.join("missing-edict-application.json");
    let output = run_edict(&jsonl([json!({
        "schema": "edict.compiler.settings/v1",
        "type": "compilerSettings",
        "operation": "build",
        "application": application,
    })]));

    assert_eq!(output.status.code(), Some(2));
    assert_eq!(output.stdout, [] as [u8; 0]);
    let stderr = assert_jsonl_stream(&output.stderr, "stderr");
    let diagnostic = stderr
        .iter()
        .find(|line| line.get("type").and_then(Value::as_str) == Some("diagnostic"))
        .expect("build failure emits a diagnostic");
    assert_eq!(
        diagnostic.get("command").and_then(Value::as_str),
        Some("build")
    );
    assert_eq!(
        diagnostic.get("kind").and_then(Value::as_str),
        Some("ApplicationConfigReadFailed")
    );
    assert_status(&stderr, "error", 2);
    fs::remove_dir_all(root).expect("remove missing application test tree");
}

#[test]
fn build_rejects_compiler_input_records_instead_of_ignoring_them() {
    let root = temp_tree("build-input-record");
    let application = root.join("missing-edict-application.json");
    let output = run_edict(&jsonl([
        json!({
            "schema": "edict.compiler.settings/v1",
            "type": "compilerSettings",
            "operation": "build",
            "application": application,
        }),
        json!({
            "schema": "edict.compiler.input/v1",
            "type": "compilerInput",
            "kind": "source",
            "name": "ignored.edict",
            "source": VALID_SOURCE,
        }),
    ]));

    assert_eq!(output.status.code(), Some(2));
    assert_eq!(output.stdout, [] as [u8; 0]);
    let stderr = assert_jsonl_stream(&output.stderr, "stderr");
    let diagnostic = stderr
        .iter()
        .find(|line| line.get("type").and_then(Value::as_str) == Some("diagnostic"))
        .expect("build input rejection emits a diagnostic");
    assert_eq!(
        diagnostic.get("command").and_then(Value::as_str),
        Some("build")
    );
    assert_eq!(
        diagnostic.get("kind").and_then(Value::as_str),
        Some("InvalidInputRecord")
    );
    assert_status(&stderr, "error", 2);
    fs::remove_dir_all(root).expect("remove build input record test tree");
}

#[test]
fn build_rejects_unused_directory_extension_settings() {
    let root = temp_tree("build-directory-extensions");
    let application = root.join("missing-edict-application.json");
    let output = run_edict(&jsonl([json!({
        "schema": "edict.compiler.settings/v1",
        "type": "compilerSettings",
        "operation": "build",
        "application": application,
        "directoryExtensions": [".unused"],
    })]));

    assert_eq!(output.status.code(), Some(2));
    assert_eq!(output.stdout, [] as [u8; 0]);
    let stderr = assert_jsonl_stream(&output.stderr, "stderr");
    let diagnostic = stderr
        .iter()
        .find(|line| line.get("type").and_then(Value::as_str) == Some("diagnostic"))
        .expect("unused build setting emits a diagnostic");
    assert_eq!(
        diagnostic.get("command").and_then(Value::as_str),
        Some("build")
    );
    assert_eq!(
        diagnostic.get("kind").and_then(Value::as_str),
        Some("InvalidSettings")
    );
    assert_status(&stderr, "error", 2);
    fs::remove_dir_all(root).expect("remove directory extensions test tree");
}

#[test]
fn build_rejects_an_empty_lawpack_path_as_invalid_settings() {
    let output = run_edict(&jsonl([json!({
        "schema": "edict.compiler.settings/v1",
        "type": "compilerSettings",
        "operation": "build",
        "lawpack": "",
    })]));

    assert_eq!(output.status.code(), Some(2));
    assert_eq!(output.stdout, [] as [u8; 0]);
    let stderr = assert_jsonl_stream(&output.stderr, "stderr");
    let diagnostic = stderr
        .iter()
        .find(|line| line.get("type").and_then(Value::as_str) == Some("diagnostic"))
        .expect("empty lawpack path emits a diagnostic");
    assert_eq!(
        diagnostic.get("command").and_then(Value::as_str),
        Some("build")
    );
    assert_eq!(
        diagnostic.get("kind").and_then(Value::as_str),
        Some("InvalidSettings")
    );
    assert_status(&stderr, "error", 2);
}

#[test]
fn build_rejects_explicit_default_values_for_forbidden_settings() {
    let root = temp_tree("build-explicit-defaults");
    let application = root.join("missing-edict-application.json");

    for (field, value) in [
        ("emit", json!([])),
        ("followSymlinks", json!(false)),
        ("directoryExtensions", json!([".edict"])),
        ("checkOnly", json!(false)),
    ] {
        let mut settings = json!({
            "schema": "edict.compiler.settings/v1",
            "type": "compilerSettings",
            "operation": "build",
            "application": application,
        });
        settings[field] = value;
        let output = run_edict(&jsonl([settings]));

        assert_eq!(output.status.code(), Some(2), "field {field}");
        assert!(output.stdout.is_empty(), "field {field}");
        let stderr = assert_jsonl_stream(&output.stderr, "stderr");
        let diagnostic = stderr
            .iter()
            .find(|line| line.get("type").and_then(Value::as_str) == Some("diagnostic"))
            .expect("explicit forbidden build setting emits a diagnostic");
        assert_eq!(
            diagnostic.get("command").and_then(Value::as_str),
            Some("build"),
            "field {field}"
        );
        assert_eq!(
            diagnostic.get("kind").and_then(Value::as_str),
            Some("InvalidSettings"),
            "field {field}"
        );
        assert_status(&stderr, "error", 2);
    }

    fs::remove_dir_all(root).expect("remove explicit defaults test tree");
}

#[allow(
    clippy::too_many_lines,
    reason = "one external-consumer witness traces write, check, drift, and repair"
)]
#[test]
fn lawpack_build_writes_checks_repairs_and_is_cwd_independent() {
    let root = temp_tree("lawpack-external-consumer");
    let caller_one = temp_tree("lawpack-caller-one");
    let caller_two = temp_tree("lawpack-caller-two");
    let document_path = root.join("edict.lawpack.json");
    let document = json!({
        "schema": "edict.lawpack-build/v1",
        "outputDirectory": "vendor/example-text",
        "lawpack": {
            "schema": "edict.lawpack-authoring/v1",
            "id": "example.text",
            "version": "1",
            "acceptedCoreAbi": ["edict.core/v1"],
            "dependencies": [],
            "exportsCoordinate": "example.text.exports/v1",
            "exports": {
                "types": [{
                    "coordinate": "example.text@1.Key",
                    "definition": "String<max=64>"
                }],
                "constants": [],
                "pureFunctions": [],
                "effects": [],
                "obstructions": [],
                "operationProfiles": {}
            },
            "targetAdapters": [],
            "verifier": {
                "class": "declarative",
                "ruleset": {
                    "id": "example.text.verifier/v1",
                    "digest": "sha256:1111111111111111111111111111111111111111111111111111111111111111"
                }
            },
            "compatibility": {
                "id": "example.text.compatibility/v1",
                "digest": "sha256:2222222222222222222222222222222222222222222222222222222222222222"
            },
            "conformanceFixtureCorpus": {
                "id": "example.text.fixtures/v1",
                "digest": "sha256:3333333333333333333333333333333333333333333333333333333333333333"
            },
            "localResources": []
        },
        "dependencyBundles": []
    });
    fs::write(
        &document_path,
        serde_json::to_vec_pretty(&document).expect("encode lawpack build document"),
    )
    .expect("write lawpack build document");

    let write_request = jsonl([json!({
        "schema": "edict.compiler.settings/v1",
        "type": "compilerSettings",
        "operation": "build",
        "lawpack": document_path,
    })]);
    let first = run_edict_in_dir(&write_request, &caller_one);
    assert_eq!(first.status.code(), Some(0));
    assert_eq!(first.stderr, [] as [u8; 0]);
    let output = root.join("vendor/example-text");
    let manifest = fs::read(output.join("manifest.cbor")).expect("read first manifest");
    let exports = fs::read(output.join("exports.cbor")).expect("read first exports");

    let check_request = jsonl([json!({
        "schema": "edict.compiler.settings/v1",
        "type": "compilerSettings",
        "operation": "build",
        "lawpack": document_path,
        "checkOnly": true,
    })]);
    let check = run_edict_in_dir(&check_request, &caller_two);
    assert_eq!(check.status.code(), Some(0));
    assert_eq!(check.stderr, [] as [u8; 0]);
    assert_eq!(
        fs::read(output.join("manifest.cbor")).expect("manifest"),
        manifest
    );
    assert_eq!(
        fs::read(output.join("exports.cbor")).expect("exports"),
        exports
    );

    fs::write(output.join("exports.cbor"), b"drift").expect("inject output drift");
    let drift = run_edict_in_dir(&check_request, &caller_one);
    assert_eq!(drift.status.code(), Some(2));
    let diagnostics = assert_jsonl_stream(&drift.stderr, "stderr");
    assert_eq!(
        diagnostics[0].get("kind").and_then(Value::as_str),
        Some("LawpackOutputDrift")
    );
    assert_eq!(
        fs::read(output.join("exports.cbor")).expect("drift remains"),
        b"drift"
    );

    fs::write(output.join("stale.txt"), b"stale").expect("add stale owned output");
    let repaired = run_edict_in_dir(&write_request, &caller_two);
    assert_eq!(repaired.status.code(), Some(0));
    assert_eq!(
        fs::read(output.join("manifest.cbor")).expect("manifest"),
        manifest
    );
    assert_eq!(
        fs::read(output.join("exports.cbor")).expect("exports"),
        exports
    );
    assert!(!output.join("stale.txt").exists());

    fs::remove_dir_all(root).expect("remove external consumer tree");
    fs::remove_dir_all(caller_one).expect("remove first caller tree");
    fs::remove_dir_all(caller_two).expect("remove second caller tree");
}

#[test]
fn check_accepts_inline_source_jsonl_and_emits_jsonl_stdout() {
    let output = run_edict(&jsonl([
        compiler_settings(),
        json!({
            "schema": "edict.compiler.input/v1",
            "type": "compilerInput",
            "kind": "source",
            "name": "inline.edict",
            "source": VALID_SOURCE,
        }),
    ]));

    assert!(
        output.status.success(),
        "check should accept valid inline source"
    );
    assert_jsonl_stream(&output.stderr, "stderr");
    assert!(
        output.stderr.is_empty(),
        "successful check must not write stderr"
    );
    let stdout = assert_jsonl_stream(&output.stdout, "stdout");
    assert_eq!(check_result_count(&stdout), 1);
    assert_status(&stdout, "ok", 0);
}

#[test]
fn project_accepts_dirty_source_and_emits_syntax_core_target_ir_projection() {
    let output = run_edict(&jsonl([
        projection_settings(["syntax", "diagnostics", "core", "targetIr", "digests"]),
        json!({
            "schema": "edict.compiler.input/v1",
            "type": "compilerInput",
            "kind": "source",
            "name": "unsaved/demo.echo.edict",
            "source": ECHO_SOURCE,
        }),
    ]));

    let stdout = assert_successful_projection_output(&output);
    let (expected_core_digest, expected_target_digest) =
        expected_echo_projection_digests(ECHO_SOURCE);
    assert_syntax_projection(&stdout);
    assert_empty_projection_diagnostics(&stdout);
    assert_available_core_projection(&stdout, &expected_core_digest);
    assert_available_target_ir_projection(&stdout, &expected_target_digest);
}

#[test]
fn project_exposes_ordered_effect_binding_and_guard_without_hoisting() {
    let source = ECHO_SOURCE.replace(
        "  return { id: input.id };",
        "  let observed: String<max=16> = receipt.id;\n  require observed != \"\" else domain.WriteRejected;\n  return { id: observed };",
    );
    let mut settings = projection_settings(["core", "targetIr", "digests"]);
    settings["target"]["irDomain"] = json!("echo.span-ir/v2");
    let output = run_edict(&jsonl([
        settings,
        json!({
            "schema": "edict.compiler.input/v1", "type": "compilerInput",
            "kind": "source", "name": "ordered.edict", "source": source,
        }),
    ]));
    let stdout = assert_successful_projection_output(&output);
    let target = record_of_type(&stdout, "targetIr");
    assert_eq!(target["state"], "available");
    assert_eq!(target["domain"], "echo.span-ir/v2");
    let intent = &target["review"]["intents"]["replaceThing"];
    assert_eq!(
        intent["executionOrder"],
        json!([
            "replaceThing.step.0",
            "replaceThing.binding.0",
            "replaceThing.require.0",
        ])
    );
    assert_eq!(intent["pureBindings"][0]["id"], "replaceThing.binding.0");
    assert_eq!(intent["steps"][0]["id"], "replaceThing.step.0");
    assert_eq!(intent["requirements"][0]["id"], "replaceThing.require.0");
}

#[test]
fn project_exposes_external_requests_as_non_callable_review_data() {
    let output = run_edict(&jsonl([
        projection_settings(["diagnostics", "core", "targetIr"]),
        json!({
            "schema": "edict.compiler.input/v1",
            "type": "compilerInput",
            "kind": "source",
            "name": "unsaved/external-request.edict",
            "source": EXTERNAL_REQUEST_SOURCE,
        }),
    ]));

    let stdout = assert_successful_projection_output(&output);
    assert_empty_projection_diagnostics(&stdout);

    let core = record_of_type(&stdout, "core");
    assert_eq!(core.get("state").and_then(Value::as_str), Some("available"));
    assert_eq!(
        core.pointer("/review/intents/observe/body/nodes/0/kind")
            .and_then(Value::as_str),
        Some("externalActionRequest")
    );
    assert_eq!(
        core.pointer("/review/intents/observe/body/nodes/0/operation/coordinate")
            .and_then(Value::as_str),
        Some("workspace.snapshot.observe@1")
    );
    assert_external_request_review(
        core.pointer("/review/intents/observe/body/nodes/0")
            .expect("Core request review exists"),
    );

    let target = record_of_type(&stdout, "targetIr");
    assert_eq!(
        target.get("state").and_then(Value::as_str),
        Some("available")
    );
    assert_eq!(
        target
            .pointer("/review/semanticClosure/capabilities/0/coordinate")
            .and_then(Value::as_str),
        Some("workspace.snapshot.observe@1")
    );
    assert_eq!(
        target
            .pointer("/review/intents/observe/externalActionRequests/0/operation/coordinate")
            .and_then(Value::as_str),
        Some("workspace.snapshot.observe@1")
    );
    assert_external_request_review(
        target
            .pointer("/review/intents/observe/externalActionRequests/0")
            .expect("Target IR request review exists"),
    );
    assert_eq!(
        target
            .pointer("/review/intents/observe/steps")
            .and_then(Value::as_array)
            .map(Vec::len),
        Some(0)
    );
}

fn assert_external_request_review(request: &Value) {
    for (pointer, expected) in [
        ("/operation/coordinate", "workspace.snapshot.observe@1"),
        ("/inputType", "Bytes<max=1024>"),
        ("/settlementType", "Bytes<max=65536>"),
        ("/inputSchema/coordinate", "workspace.snapshot.input@1"),
        (
            "/settlementSchema/coordinate",
            "workspace.snapshot.settlement@1",
        ),
        ("/input/field", "payload"),
        ("/authorityScope/field", "scope"),
        ("/basis/field", "basis"),
        ("/budget/maxSettlementBytes/field", "maxSettlementBytes"),
        ("/budget/maxAttempts/field", "maxAttempts"),
        (
            "/reconciliationLaw/coordinate",
            "workspace.snapshot.reconcile@1",
        ),
        ("/state", "awaitingSettlement"),
        ("/settlementAdmission", "schemaRequired"),
    ] {
        assert_eq!(
            request.pointer(pointer).and_then(Value::as_str),
            Some(expected),
            "{pointer}"
        );
    }
}

#[test]
fn project_invalid_source_emits_diagnostics_without_process_failure() {
    let output = run_edict(&jsonl([
        projection_settings(["syntax", "diagnostics", "core"]),
        json!({
            "schema": "edict.compiler.input/v1",
            "type": "compilerInput",
            "kind": "source",
            "name": "unsaved/broken.edict",
            "source": "package demo.broken@1\n",
        }),
    ]));

    assert_eq!(
        output.status.code(),
        Some(0),
        "compiler diagnostics are projection data, not process failure"
    );
    assert!(
        output.stderr.is_empty(),
        "projection diagnostics should be emitted on stdout"
    );
    let stdout = assert_jsonl_stream(&output.stdout, "stdout");
    assert_status(&stdout, "ok", 0);

    let diagnostics = record_of_type(&stdout, "diagnostics");
    let items = diagnostics
        .get("diagnostics")
        .and_then(Value::as_array)
        .expect("diagnostics projection carries diagnostic items");
    assert!(
        items.iter().any(|item| {
            item.get("stage").and_then(Value::as_str) == Some("parse")
                && item.get("kind").and_then(Value::as_str) == Some("ExpectedToken")
        }),
        "parse failure should be reported as a stable projection diagnostic"
    );

    let core = record_of_type(&stdout, "core");
    assert_eq!(core.get("state").and_then(Value::as_str), Some("blocked"));
    assert_eq!(
        core.pointer("/reason/0/kind").and_then(Value::as_str),
        Some("ExpectedToken")
    );
}

#[test]
fn project_reserved_type_identity_emits_the_stable_compiler_kind() {
    let source = VALID_SOURCE.replace("HelloInput", "Unit");
    let output = run_edict(&jsonl([
        projection_settings(["diagnostics", "core"]),
        json!({
            "schema": "edict.compiler.input/v1",
            "type": "compilerInput",
            "kind": "source",
            "name": "unsaved/reserved.edict",
            "source": source,
        }),
    ]));

    assert_eq!(
        output.status.code(),
        Some(0),
        "compiler diagnostics are projection data, not process failure"
    );
    assert_eq!(output.stderr, [] as [u8; 0]);
    let stdout = assert_jsonl_stream(&output.stdout, "stdout");
    let diagnostics = record_of_type(&stdout, "diagnostics");
    let items = diagnostics
        .get("diagnostics")
        .and_then(Value::as_array)
        .expect("diagnostics projection carries compiler diagnostics");
    assert!(items.iter().any(|item| {
        item.get("stage").and_then(Value::as_str) == Some("typeCheck")
            && item.get("kind").and_then(Value::as_str) == Some("ReservedTypeIdentity")
    }));
    assert_eq!(
        record_of_type(&stdout, "core")
            .get("state")
            .and_then(Value::as_str),
        Some("blocked")
    );
}

#[test]
fn project_target_lowering_failure_is_structured_projection_data() {
    let output = run_edict(&jsonl([
        projection_settings(["diagnostics", "core", "targetIr"]),
        json!({
            "schema": "edict.compiler.input/v1",
            "type": "compilerInput",
            "kind": "source",
            "name": "unsaved/pure.edict",
            "source": VALID_SOURCE,
        }),
    ]));

    assert_eq!(
        output.status.code(),
        Some(0),
        "target lowering failure must not become a CLI transport failure"
    );
    assert!(
        output.stderr.is_empty(),
        "target lowering failure should be emitted on stdout"
    );
    let stdout = assert_jsonl_stream(&output.stdout, "stdout");
    assert_status(&stdout, "ok", 0);
    assert_eq!(
        record_of_type(&stdout, "core")
            .get("state")
            .and_then(Value::as_str),
        Some("available"),
        "Core should remain available when only Target IR lowering fails"
    );

    let target_ir = record_of_type(&stdout, "targetIr");
    assert_eq!(
        target_ir.get("state").and_then(Value::as_str),
        Some("failed")
    );
    assert_eq!(
        target_ir.pointer("/error/kind").and_then(Value::as_str),
        Some("lowering_error")
    );
    assert!(
        target_ir
            .pointer("/error/failures")
            .and_then(Value::as_array)
            .is_some_and(|failures| failures.iter().any(|failure| {
                failure.get("kind").and_then(Value::as_str) == Some("NoTargetSteps")
            })),
        "Target IR failure must preserve stable lowerer failure kinds"
    );
}

#[test]
fn project_syntax_only_does_not_run_authoritative_compiler_path() {
    let output = run_edict(&jsonl([
        json!({
            "schema": "edict.compiler.settings/v1",
            "type": "compilerSettings",
            "operation": "project",
            "emit": ["syntax"],
        }),
        json!({
            "schema": "edict.compiler.input/v1",
            "type": "compilerInput",
            "kind": "source",
            "name": "unsaved/incomplete.edict",
            "source": "package demo.incomplete@1\n",
        }),
    ]));

    assert_eq!(output.status.code(), Some(0));
    assert!(
        output.stderr.is_empty(),
        "syntax-only projection should not write stderr"
    );
    let stdout = assert_jsonl_stream(&output.stdout, "stdout");
    assert_eq!(
        stdout
            .iter()
            .filter_map(|line| line.get("type").and_then(Value::as_str))
            .collect::<Vec<_>>(),
        ["syntax", "status"]
    );
    assert_status(&stdout, "ok", 0);
    assert_status_counts(&stdout, 1, 0);
}

#[test]
fn project_syntax_only_lex_failure_emits_visible_diagnostics() {
    let output = run_edict(&jsonl([
        json!({
            "schema": "edict.compiler.settings/v1",
            "type": "compilerSettings",
            "operation": "project",
            "emit": ["syntax"],
        }),
        json!({
            "schema": "edict.compiler.input/v1",
            "type": "compilerInput",
            "kind": "source",
            "name": "unsaved/broken-lex.edict",
            "source": "package demo.broken@1;\nlet value = \"unterminated",
        }),
    ]));

    assert_eq!(output.status.code(), Some(0));
    assert!(
        output.stderr.is_empty(),
        "syntax lex failures should remain projection data on stdout"
    );
    let stdout = assert_jsonl_stream(&output.stdout, "stdout");
    assert_eq!(
        stdout
            .iter()
            .filter_map(|line| line.get("type").and_then(Value::as_str))
            .collect::<Vec<_>>(),
        ["diagnostics", "status"],
        "status errors must be backed by visible structured projection data"
    );
    let diagnostics = record_of_type(&stdout, "diagnostics");
    assert_eq!(
        diagnostics
            .pointer("/diagnostics/0/stage")
            .and_then(Value::as_str),
        Some("lex")
    );
    assert_eq!(
        diagnostics
            .pointer("/diagnostics/0/kind")
            .and_then(Value::as_str),
        Some("Lex")
    );
    assert_status_counts(&stdout, 1, 1);
}

#[test]
fn invalid_project_settings_report_project_command() {
    let output = run_edict(&jsonl([
        json!({
            "schema": "edict.compiler.settings/v1",
            "type": "compilerSettings",
            "operation": "project",
            "emit": ["targetIr"],
            "target": {
                "coordinate": "echo.dpo@1",
                "profileDigest": "sha256:AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
                "irDomain": "echo.span-ir/v1",
            },
        }),
        json!({
            "schema": "edict.compiler.input/v1",
            "type": "compilerInput",
            "kind": "source",
            "name": "unsaved/demo.echo.edict",
            "source": ECHO_SOURCE,
        }),
    ]));

    assert_eq!(output.status.code(), Some(2));
    assert!(
        output.stdout.is_empty(),
        "invalid project settings must not write stdout"
    );
    let stderr = assert_jsonl_stream(&output.stderr, "stderr");
    let diagnostic = stderr
        .iter()
        .find(|line| line.get("type").and_then(Value::as_str) == Some("diagnostic"))
        .expect("stderr must contain a CLI diagnostic");
    assert_eq!(
        diagnostic.get("command").and_then(Value::as_str),
        Some("project")
    );
    assert_eq!(
        diagnostic.get("kind").and_then(Value::as_str),
        Some("InvalidSettings")
    );
    let status = record_of_type(&stderr, "status");
    assert_eq!(
        status.get("command").and_then(Value::as_str),
        Some("project")
    );
    assert_status(&stderr, "error", 2);
}

#[test]
fn check_rejects_invalid_source_with_jsonl_stderr_only() {
    let output = run_edict(&jsonl([
        compiler_settings(),
        json!({
            "schema": "edict.compiler.input/v1",
            "type": "compilerInput",
            "kind": "source",
            "name": "broken.edict",
            "source": "package examples.broken@1\n",
        }),
    ]));

    assert_eq!(output.status.code(), Some(1));
    assert!(
        output.stdout.is_empty(),
        "failed compiler checks must not write stdout"
    );
    let stderr = assert_jsonl_stream(&output.stderr, "stderr");
    assert!(stderr.iter().any(|line| {
        line.get("type").and_then(Value::as_str) == Some("diagnostic")
            && line.get("stage").and_then(Value::as_str) == Some("parse")
            && line.get("kind").and_then(Value::as_str) == Some("ExpectedToken")
    }));
    assert_status(&stderr, "error", 1);
}

#[test]
fn non_jsonl_input_rejects_with_jsonl_cli_diagnostic() {
    let output = run_edict("not json\n");

    assert_eq!(output.status.code(), Some(2));
    assert!(
        output.stdout.is_empty(),
        "CLI input failures must not write stdout"
    );
    let stderr = assert_jsonl_stream(&output.stderr, "stderr");
    assert!(stderr.iter().any(|line| {
        line.get("type").and_then(Value::as_str) == Some("diagnostic")
            && line.get("stage").and_then(Value::as_str) == Some("cli")
            && line.get("kind").and_then(Value::as_str) == Some("InvalidJsonl")
    }));
    assert_status(&stderr, "error", 2);
}

#[test]
fn oversized_stdin_rejects_with_input_too_large_diagnostic() {
    let output = run_edict_with_env(&"x".repeat(65), &[(edict_cli::MAX_STDIN_BYTES_ENV, "64")]);

    assert_eq!(output.status.code(), Some(2));
    assert!(
        output.stdout.is_empty(),
        "oversized stdin must not write stdout"
    );
    let stderr = assert_jsonl_stream(&output.stderr, "stderr");
    let diagnostic = stderr
        .iter()
        .find(|line| line.get("kind").and_then(Value::as_str) == Some("InputTooLarge"))
        .expect("stderr must contain an InputTooLarge diagnostic");
    assert_eq!(diagnostic.get("stage").and_then(Value::as_str), Some("cli"));
    assert_eq!(
        diagnostic
            .get("input")
            .and_then(|input| input.get("kind"))
            .and_then(Value::as_str),
        Some("stdin")
    );
    let message = diagnostic
        .get("message")
        .and_then(Value::as_str)
        .unwrap_or_default();
    assert!(
        message.contains("64 bytes"),
        "InputTooLarge should report the configured byte limit"
    );
    assert_status(&stderr, "error", 2);
}

#[test]
fn input_root_rejects_path_outside_configured_root() {
    let allowed = temp_tree("allowed-root");
    let outside = temp_tree("outside-root");
    let outside_source = outside.join("outside.edict");
    fs::write(&outside_source, VALID_SOURCE).expect("write outside source");

    let output = run_edict(&jsonl([
        json!({
            "schema": "edict.compiler.settings/v1",
            "type": "compilerSettings",
            "operation": "check",
            "inputRoot": allowed,
        }),
        json!({
            "schema": "edict.compiler.input/v1",
            "type": "compilerInput",
            "kind": "path",
            "path": outside_source,
        }),
    ]));

    let _ = fs::remove_dir_all(&outside);
    let _ = fs::remove_dir_all(&allowed);

    assert_eq!(output.status.code(), Some(2));
    assert!(
        output.stdout.is_empty(),
        "root-confinement failures must not write stdout"
    );
    let stderr = assert_jsonl_stream(&output.stderr, "stderr");
    let diagnostic = stderr
        .iter()
        .find(|line| line.get("kind").and_then(Value::as_str) == Some("InputPathOutsideRoot"))
        .expect("stderr must contain an InputPathOutsideRoot diagnostic");
    assert_eq!(diagnostic.get("stage").and_then(Value::as_str), Some("cli"));
    assert_status(&stderr, "error", 2);
}

#[test]
fn input_root_null_rejects_as_invalid_settings() {
    let outside = temp_tree("null-input-root-outside");
    let outside_source = outside.join("outside.edict");
    fs::write(&outside_source, VALID_SOURCE).expect("write outside source");

    let output = run_edict(&jsonl([
        json!({
            "schema": "edict.compiler.settings/v1",
            "type": "compilerSettings",
            "operation": "check",
            "inputRoot": null,
        }),
        json!({
            "schema": "edict.compiler.input/v1",
            "type": "compilerInput",
            "kind": "path",
            "path": outside_source,
        }),
    ]));

    let _ = fs::remove_dir_all(&outside);

    assert_eq!(output.status.code(), Some(2));
    assert!(
        output.stdout.is_empty(),
        "settings failures must not write stdout"
    );
    let stderr = assert_jsonl_stream(&output.stderr, "stderr");
    let diagnostic = stderr
        .iter()
        .find(|line| line.get("kind").and_then(Value::as_str) == Some("InvalidSettings"))
        .expect("stderr must contain an InvalidSettings diagnostic");
    assert_eq!(diagnostic.get("stage").and_then(Value::as_str), Some("cli"));
    assert_status(&stderr, "error", 2);
}

#[test]
fn projection_object_settings_null_reject_as_invalid_settings() {
    for (operation, field) in [
        ("check", "compilerContext"),
        ("check", "target"),
        ("project", "compilerContext"),
        ("project", "target"),
    ] {
        let mut settings = json!({
            "schema": "edict.compiler.settings/v1",
            "type": "compilerSettings",
            "operation": operation,
        });
        if operation == "project" {
            settings["emit"] = json!(["syntax"]);
        }
        settings[field] = Value::Null;

        let output = run_edict(&jsonl([
            settings,
            json!({
                "schema": "edict.compiler.input/v1",
                "type": "compilerInput",
                "kind": "source",
                "name": "inline.edict",
                "source": VALID_SOURCE,
            }),
        ]));

        assert_eq!(
            output.status.code(),
            Some(2),
            "{operation} with null {field} must be rejected before source processing"
        );
        assert!(
            output.stdout.is_empty(),
            "settings failures must not write stdout"
        );
        let stderr = assert_jsonl_stream(&output.stderr, "stderr");
        let diagnostic = stderr
            .iter()
            .find(|line| line.get("kind").and_then(Value::as_str) == Some("InvalidSettings"))
            .expect("stderr must contain an InvalidSettings diagnostic");
        assert_eq!(diagnostic.get("stage").and_then(Value::as_str), Some("cli"));
        assert_eq!(
            diagnostic.get("command").and_then(Value::as_str),
            Some(operation)
        );
        assert_status(&stderr, "error", 2);
    }
}

#[test]
fn input_root_rejects_glob_outside_configured_root() {
    let allowed = temp_tree("allowed-glob-root");
    let outside = temp_tree("outside-glob-root");
    fs::write(outside.join("outside.edict"), VALID_SOURCE).expect("write outside source");

    let output = run_edict(&jsonl([
        json!({
            "schema": "edict.compiler.settings/v1",
            "type": "compilerSettings",
            "operation": "check",
            "inputRoot": allowed,
        }),
        json!({
            "schema": "edict.compiler.input/v1",
            "type": "compilerInput",
            "kind": "glob",
            "pattern": format!("{}/*.edict", outside.display()),
        }),
    ]));

    let _ = fs::remove_dir_all(&outside);
    let _ = fs::remove_dir_all(&allowed);

    assert_eq!(output.status.code(), Some(2));
    assert!(
        output.stdout.is_empty(),
        "root-confinement failures must not write stdout"
    );
    let stderr = assert_jsonl_stream(&output.stderr, "stderr");
    assert!(
        stderr
            .iter()
            .any(|line| line.get("kind").and_then(Value::as_str) == Some("InputPathOutsideRoot")),
        "stderr must contain an InputPathOutsideRoot diagnostic"
    );
    assert_status(&stderr, "error", 2);
}

#[test]
#[cfg(unix)]
fn input_root_glob_skips_dangling_symlink_matches() {
    use std::os::unix::fs::symlink;

    let root = temp_tree("glob-dangling-symlink");
    let source = root.join("valid.edict");
    let broken = root.join("broken.edict");
    fs::write(&source, VALID_SOURCE).expect("write valid source");
    symlink(root.join("missing.edict"), &broken).expect("create dangling symlink");

    let output = run_edict(&jsonl([
        json!({
            "schema": "edict.compiler.settings/v1",
            "type": "compilerSettings",
            "operation": "check",
            "inputRoot": root,
        }),
        json!({
            "schema": "edict.compiler.input/v1",
            "type": "compilerInput",
            "kind": "glob",
            "pattern": format!("{}/*.edict", root.display()),
        }),
    ]));

    let _ = fs::remove_dir_all(&root);

    assert!(
        output.status.success(),
        "dangling symlink glob matches should be skipped as non-files"
    );
    assert!(
        output.stderr.is_empty(),
        "successful check must not write stderr"
    );
    let stdout = assert_jsonl_stream(&output.stdout, "stdout");
    assert_eq!(check_result_count(&stdout), 1);
    assert_status(&stdout, "ok", 0);
}

#[test]
fn check_accepts_path_directory_path_list_glob_and_source_records() {
    let root = temp_tree("inputs");
    let nested = root.join("nested");
    fs::create_dir_all(&nested).expect("create nested fixture dir");
    let first = root.join("first.edict");
    let second = nested.join("second.edict");
    fs::write(&first, VALID_SOURCE).expect("write first source");
    fs::write(&second, VALID_SOURCE).expect("write second source");
    fs::write(root.join("ignored.txt"), VALID_SOURCE).expect("write ignored source");

    let output = run_edict(&jsonl([
        json!({
            "schema": "edict.compiler.settings/v1",
            "type": "compilerSettings",
            "operation": "check",
            "directoryExtensions": [".edict"],
            "followSymlinks": false,
        }),
        json!({
            "schema": "edict.compiler.input/v1",
            "type": "compilerInput",
            "kind": "path",
            "path": first,
        }),
        json!({
            "schema": "edict.compiler.input/v1",
            "type": "compilerInput",
            "kind": "pathList",
            "paths": [first, second],
        }),
        json!({
            "schema": "edict.compiler.input/v1",
            "type": "compilerInput",
            "kind": "directory",
            "path": root,
        }),
        json!({
            "schema": "edict.compiler.input/v1",
            "type": "compilerInput",
            "kind": "glob",
            "pattern": format!("{}/**/*.edict", root.display()),
        }),
        json!({
            "schema": "edict.compiler.input/v1",
            "type": "compilerInput",
            "kind": "source",
            "name": "inline.edict",
            "source": VALID_SOURCE,
        }),
    ]));

    let _ = fs::remove_dir_all(&root);

    assert!(
        output.status.success(),
        "all explicit input record kinds should check"
    );
    assert!(
        output.stderr.is_empty(),
        "successful check must not write stderr"
    );
    let stdout = assert_jsonl_stream(&output.stdout, "stdout");
    assert_eq!(check_result_count(&stdout), 8);
    assert_status(&stdout, "ok", 0);
}

#[test]
fn cli_schema_constants_match_checked_in_artifacts() {
    // Every schema identifier the binary emits or accepts must equal the
    // `properties.schema.const` of its checked-in JSON Schema artifact, so the
    // contract files cannot silently drift from the runtime constants.
    let schemas = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/schemas");
    let cases = [
        (
            edict_cli::COMPILER_SETTINGS_SCHEMA,
            "edict.compiler-settings.v1.schema.json",
        ),
        (
            edict_cli::COMPILER_INPUT_SCHEMA,
            "edict.compiler-input.v1.schema.json",
        ),
        (
            edict_cli::CHECK_RESULT_SCHEMA,
            "edict.cli-check-result.v1.schema.json",
        ),
        (
            edict_cli::PROJECTION_SYNTAX_SCHEMA,
            "edict.projection-syntax.v1.schema.json",
        ),
        (
            edict_cli::PROJECTION_DIAGNOSTICS_SCHEMA,
            "edict.projection-diagnostics.v1.schema.json",
        ),
        (
            edict_cli::PROJECTION_CORE_SCHEMA,
            "edict.projection-core.v1.schema.json",
        ),
        (
            edict_cli::PROJECTION_TARGET_IR_SCHEMA,
            "edict.projection-target-ir.v1.schema.json",
        ),
        (
            edict_cli::DIAGNOSTIC_SCHEMA,
            "edict.cli-diagnostic.v1.schema.json",
        ),
        (edict_cli::EVENT_SCHEMA, "edict.cli-event.v1.schema.json"),
        (edict_cli::INFO_SCHEMA, "edict.cli-info.v1.schema.json"),
    ];
    for (constant, file) in cases {
        let text = fs::read_to_string(schemas.join(file))
            .unwrap_or_else(|err| panic!("read schema artifact `{file}`: {err}"));
        let schema: Value = serde_json::from_str(&text)
            .unwrap_or_else(|err| panic!("parse schema artifact `{file}`: {err}"));
        let declared = schema["properties"]["schema"]["const"]
            .as_str()
            .unwrap_or_else(|| panic!("`{file}` missing `properties.schema.const`"));
        assert_eq!(
            declared, constant,
            "`{file}` const must match the runtime schema constant"
        );
    }
}

#[test]
fn version_flag_emits_info_record() {
    for flag in ["--version", "-V"] {
        let output = run_edict_args(&[flag]);
        assert_eq!(output.status.code(), Some(0), "{flag} should exit 0");
        assert!(output.stderr.is_empty(), "{flag} must not write stderr");
        let stdout = assert_jsonl_stream(&output.stdout, "stdout");
        assert_eq!(stdout.len(), 1, "{flag} emits exactly one record");
        assert_eq!(
            stdout[0].get("schema").and_then(Value::as_str),
            Some("edict.cli.info/v1")
        );
        assert_eq!(
            stdout[0].get("topic").and_then(Value::as_str),
            Some("version")
        );
        assert!(stdout[0].get("version").and_then(Value::as_str).is_some());
    }
}

#[test]
fn help_flag_emits_info_record() {
    for flag in ["--help", "-h"] {
        let output = run_edict_args(&[flag]);
        assert_eq!(output.status.code(), Some(0), "{flag} should exit 0");
        assert!(output.stderr.is_empty(), "{flag} must not write stderr");
        let stdout = assert_jsonl_stream(&output.stdout, "stdout");
        assert_eq!(stdout.len(), 1, "{flag} emits exactly one record");
        let record = &stdout[0];
        assert_eq!(record.get("topic").and_then(Value::as_str), Some("help"));
        assert!(record.get("usage").and_then(Value::as_str).is_some());
        // Pin the concrete public payload, not just field presence.
        assert_eq!(
            record.get("requestSchemas"),
            Some(&json!([
                "edict.compiler.settings/v1",
                "edict.compiler.input/v1"
            ])),
            "{flag} help must list the exact accepted request schemas"
        );
        let codes: Vec<i64> = record
            .get("exitCodes")
            .and_then(Value::as_array)
            .expect("help record carries exitCodes")
            .iter()
            .filter_map(|entry| entry.get("code").and_then(Value::as_i64))
            .collect();
        assert_eq!(
            codes,
            [0, 1, 2],
            "{flag} help must document exit codes 0, 1, 2 in order"
        );
        let exit_one = record
            .get("exitCodes")
            .and_then(Value::as_array)
            .expect("help record carries exitCodes")
            .iter()
            .find(|entry| entry.get("code").and_then(Value::as_i64) == Some(1))
            .expect("help record documents exit code 1");
        assert_eq!(
            exit_one.get("meaning").and_then(Value::as_str),
            Some("check operation compiler or validation diagnostics were produced"),
            "{flag} help must scope exit 1 to the check operation"
        );
        let exit_two = record
            .get("exitCodes")
            .and_then(Value::as_array)
            .expect("help record carries exitCodes")
            .iter()
            .find(|entry| entry.get("code").and_then(Value::as_i64) == Some(2))
            .expect("help record documents exit code 2");
        assert_eq!(
            exit_two.get("meaning").and_then(Value::as_str),
            Some("CLI input or usage was invalid, or a build failed"),
            "{flag} help must include build failures in exit 2"
        );
        assert_eq!(
            record.get("docs").and_then(Value::as_str),
            Some("docs/topics/cli/README.md"),
            "{flag} help must point at the CLI docs"
        );
    }
}

#[test]
fn unknown_argument_rejected_with_actionable_diagnostic() {
    let output = run_edict_args(&["--nope"]);
    assert_eq!(output.status.code(), Some(2));
    assert!(
        output.stdout.is_empty(),
        "rejected args must not write stdout"
    );
    let stderr = assert_jsonl_stream(&output.stderr, "stderr");
    let diagnostic = stderr
        .iter()
        .find(|line| line.get("kind").and_then(Value::as_str) == Some("InvalidArguments"))
        .expect("stderr must contain an InvalidArguments diagnostic");
    let message = diagnostic
        .get("message")
        .and_then(Value::as_str)
        .unwrap_or_default();
    assert!(
        message.contains("--help") && message.contains("docs/topics/cli/README.md"),
        "InvalidArguments must point at --help and the docs"
    );
    assert_status(&stderr, "error", 2);
}

fn run_edict_args(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_edict"))
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("run edict with args")
}

fn compiler_settings() -> Value {
    json!({
        "schema": "edict.compiler.settings/v1",
        "type": "compilerSettings",
        "operation": "check",
    })
}

fn projection_settings<const N: usize>(emit: [&str; N]) -> Value {
    let emit = emit.into_iter().collect::<Vec<_>>();
    json!({
        "schema": "edict.compiler.settings/v1",
        "type": "compilerSettings",
        "operation": "project",
        "emit": emit,
        "compilerContext": {
            "operationProfiles": [
                {
                    "source": "p.effectful",
                    "core": "continuum.profile.write/v1",
                    "allowedWriteClasses": ["replace"]
                },
                {
                    "source": "hello.readOnly",
                    "core": "continuum.profile.read-only/v1",
                    "allowedWriteClasses": ["read"]
                }
            ],
            "effectWriteClasses": [
                {
                    "effect": "target.replace",
                    "writeClass": "replace"
                }
            ],
            "budgets": [
                {
                    "source": "p.tiny",
                    "budget": {
                        "maxSteps": 8,
                        "maxAllocatedBytes": 1024,
                        "maxOutputBytes": 256
                    }
                },
                {
                    "source": "hello.tinyBudget",
                    "budget": {
                        "maxSteps": 64,
                        "maxAllocatedBytes": 4096,
                        "maxOutputBytes": 4096
                    }
                }
            ]
        },
        "target": {
            "coordinate": "echo.dpo@1",
            "profileDigest": ECHO_TARGET_PROFILE_DIGEST,
            "irDomain": "echo.span-ir/v1",
            "operationProfiles": [
                "continuum.profile.read-only/v1",
                "continuum.profile.write/v1"
            ],
            "obstructionCoordinates": ["rejected"],
            "effectLowerings": [
                {
                    "effect": "target.replace",
                    "targetIntrinsic": "echo.dpo@1.replace"
                }
            ]
        }
    })
}

fn jsonl<const N: usize>(records: [Value; N]) -> String {
    let mut out = String::new();
    for record in records {
        out.push_str(&serde_json::to_string(&record).expect("serialize input record"));
        out.push('\n');
    }
    out
}

fn run_edict(input: &str) -> Output {
    run_edict_with_env(input, &[])
}

fn run_edict_in_dir(input: &str, directory: &std::path::Path) -> Output {
    run_edict_configured(input, &[], Some(directory))
}

fn run_edict_with_env(input: &str, env: &[(&str, &str)]) -> Output {
    run_edict_configured(input, env, None)
}

fn run_edict_configured(
    input: &str,
    env: &[(&str, &str)],
    directory: Option<&std::path::Path>,
) -> Output {
    let bin = env!("CARGO_BIN_EXE_edict");
    let mut command = Command::new(bin);
    command
        .env_remove(edict_cli::MAX_STDIN_BYTES_ENV)
        .envs(env.iter().copied())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(directory) = directory {
        command.current_dir(directory);
    }
    let mut child = command.spawn().expect("spawn edict binary");
    child
        .stdin
        .as_mut()
        .expect("stdin pipe")
        .write_all(input.as_bytes())
        .expect("write jsonl stdin");
    child.wait_with_output().expect("collect output")
}

fn assert_jsonl_stream(bytes: &[u8], stream: &str) -> Vec<Value> {
    let text = std::str::from_utf8(bytes).unwrap_or_else(|_| panic!("{stream} must be utf-8"));
    if text.is_empty() {
        return Vec::new();
    }
    assert!(
        text.ends_with('\n'),
        "{stream} must end each JSONL record with a newline"
    );
    text.lines()
        .enumerate()
        .map(|(idx, line)| {
            assert!(
                !line.trim().is_empty(),
                "{stream} line {idx} must not be blank"
            );
            let value = serde_json::from_str::<Value>(line)
                .unwrap_or_else(|err| panic!("{stream} line {idx} must be JSON: {err}"));
            assert!(
                value.as_object().is_some(),
                "{stream} line {idx} must be a JSON object"
            );
            value
        })
        .collect()
}

fn check_result_count(lines: &[Value]) -> usize {
    lines
        .iter()
        .filter(|line| line.get("type").and_then(Value::as_str) == Some("checkResult"))
        .count()
}

fn assert_status(lines: &[Value], expected_status: &str, expected_exit: i32) {
    let status = lines
        .iter()
        .find(|line| line.get("type").and_then(Value::as_str) == Some("status"))
        .expect("stream must contain a status record");
    assert_eq!(
        status.get("schema").and_then(Value::as_str),
        Some("edict.cli.event/v1")
    );
    assert_eq!(
        status.get("status").and_then(Value::as_str),
        Some(expected_status)
    );
    assert_eq!(
        status.get("exitCode").and_then(Value::as_i64),
        Some(i64::from(expected_exit))
    );
}

fn assert_status_counts(lines: &[Value], expected_checked: i64, expected_errors: i64) {
    let status = lines
        .iter()
        .find(|line| line.get("type").and_then(Value::as_str) == Some("status"))
        .expect("stream must contain a status record");
    assert_eq!(
        status.get("checked").and_then(Value::as_i64),
        Some(expected_checked)
    );
    assert_eq!(
        status.get("errors").and_then(Value::as_i64),
        Some(expected_errors)
    );
}

fn assert_successful_projection_output(output: &Output) -> Vec<Value> {
    assert!(
        output.status.success(),
        "project should complete at process level for valid dirty source"
    );
    assert!(
        output.stderr.is_empty(),
        "successful projection must not write stderr"
    );
    let stdout = assert_jsonl_stream(&output.stdout, "stdout");
    assert_status(&stdout, "ok", 0);
    stdout
}

fn assert_syntax_projection(stdout: &[Value]) {
    let syntax = record_of_type(stdout, "syntax");
    assert_eq!(
        syntax.get("schema").and_then(Value::as_str),
        Some("edict.projection.syntax/v1")
    );
    assert_eq!(
        syntax
            .get("input")
            .and_then(|input| input.get("name"))
            .and_then(Value::as_str),
        Some("unsaved/demo.echo.edict")
    );
    let spans = syntax
        .get("spans")
        .and_then(Value::as_array)
        .expect("syntax projection carries spans");
    assert!(
        spans.iter().any(|span| {
            span.get("role").and_then(Value::as_str) == Some("keyword")
                && span.get("lexeme").and_then(Value::as_str) == Some("intent")
        }),
        "syntax spans must expose editor roles over dirty source text"
    );
}

fn assert_empty_projection_diagnostics(stdout: &[Value]) {
    let diagnostics = record_of_type(stdout, "diagnostics");
    assert_eq!(
        diagnostics.get("schema").and_then(Value::as_str),
        Some("edict.projection.diagnostics/v1")
    );
    assert_eq!(
        diagnostics
            .get("diagnostics")
            .and_then(Value::as_array)
            .map(Vec::len),
        Some(0)
    );
}

fn assert_available_core_projection(stdout: &[Value], expected_core_digest: &str) {
    let core = record_of_type(stdout, "core");
    assert_eq!(
        core.get("schema").and_then(Value::as_str),
        Some("edict.projection.core/v1")
    );
    assert_eq!(core.get("state").and_then(Value::as_str), Some("available"));
    assert_eq!(
        core.get("digest").and_then(Value::as_str),
        Some(expected_core_digest)
    );
    assert_eq!(
        core.pointer("/review/apiVersion").and_then(Value::as_str),
        Some("edict.core/v1")
    );
    assert!(
        core.pointer("/review/intents/replaceThing").is_some(),
        "Core review must include the lowered intent"
    );
    assert_eq!(
        core.pointer("/review/intents/replaceThing/basis/field")
            .and_then(Value::as_str),
        Some("basis"),
        "Core review must expose the hash-significant explicit basis"
    );
}

fn assert_available_target_ir_projection(stdout: &[Value], expected_target_digest: &str) {
    let target_ir = record_of_type(stdout, "targetIr");
    assert_eq!(
        target_ir.get("schema").and_then(Value::as_str),
        Some("edict.projection.target-ir/v1")
    );
    assert_eq!(
        target_ir.get("state").and_then(Value::as_str),
        Some("available")
    );
    assert_eq!(
        target_ir.get("domain").and_then(Value::as_str),
        Some("echo.span-ir/v1")
    );
    assert_eq!(
        target_ir
            .pointer("/target/coordinate")
            .and_then(Value::as_str),
        Some("echo.dpo@1")
    );
    assert_eq!(
        target_ir.get("digest").and_then(Value::as_str),
        Some(expected_target_digest)
    );
    assert_eq!(
        target_ir
            .pointer("/review/intents/replaceThing/steps/0/targetIntrinsic")
            .and_then(Value::as_str),
        Some("echo.dpo@1.replace")
    );
    assert_eq!(
        target_ir
            .pointer("/review/intents/replaceThing/basis/field")
            .and_then(Value::as_str),
        Some("basis"),
        "Target IR review must expose the hash-significant explicit basis"
    );
    assert_eq!(
        target_ir
            .pointer("/review/semanticClosure/sourceCore/coordinate")
            .and_then(Value::as_str),
        Some("demo.echo@1"),
        "Target IR review must expose the semantic closure source Core coordinate"
    );
    assert_eq!(
        target_ir
            .pointer("/review/semanticClosure/lawpacks/0/coordinate")
            .and_then(Value::as_str),
        Some("demo.write@1"),
        "Target IR review must expose the semantic closure lawpack coordinate"
    );
    assert_eq!(
        target_ir
            .pointer("/review/semanticClosure/lawpacks/0/digest")
            .and_then(Value::as_str),
        Some("sha256:2222222222222222222222222222222222222222222222222222222222222222"),
        "Target IR review must expose the semantic closure lawpack digest"
    );
}

fn record_of_type<'a>(lines: &'a [Value], record_type: &str) -> &'a Value {
    lines
        .iter()
        .find(|line| line.get("type").and_then(Value::as_str) == Some(record_type))
        .unwrap_or_else(|| panic!("stream must contain a `{record_type}` record"))
}

fn expected_echo_projection_digests(source: &str) -> (String, String) {
    let module = edict_syntax::parse_module(source).expect("source parses");
    let core = compile_to_core(&module, &projection_compiler_context()).expect("source compiles");
    let target_ir = lower_to_target_ir(&core, &projection_target_facts())
        .artifact
        .expect("source lowers to Echo Target IR");
    let core_digest = digest_core_module(&core)
        .expect("Core digest computes")
        .to_review_string();
    let target_digest = digest_target_ir_artifact(&target_ir)
        .expect("Target IR digest computes")
        .to_review_string();
    (core_digest, target_digest)
}

fn projection_compiler_context() -> CompilerContext {
    CompilerContext::new()
        .with_operation_profile("p.effectful", "continuum.profile.write/v1")
        .with_operation_profile_write_classes("p.effectful", [WriteClass::Replace])
        .with_operation_profile("hello.readOnly", "continuum.profile.read-only/v1")
        .with_operation_profile_write_classes("hello.readOnly", [WriteClass::Read])
        .with_effect_write_class("target.replace", WriteClass::Replace)
        .with_budget(
            "p.tiny",
            CoreBudget {
                max_steps: 8,
                max_allocated_bytes: 1024,
                max_output_bytes: 256,
            },
        )
        .with_budget(
            "hello.tinyBudget",
            CoreBudget {
                max_steps: 64,
                max_allocated_bytes: 4096,
                max_output_bytes: 4096,
            },
        )
}

fn projection_target_facts() -> TargetIrLoweringFacts {
    TargetIrLoweringFacts {
        target_profile: ResourceRef {
            coordinate: ECHO_DPO_TARGET_PROFILE.to_owned(),
            digest: Some(ECHO_TARGET_PROFILE_DIGEST.to_owned()),
        },
        target_ir_domain: ECHO_SPAN_IR_DOMAIN.to_owned(),
        operation_profiles: vec![
            "continuum.profile.read-only/v1".to_owned(),
            "continuum.profile.write/v1".to_owned(),
        ],
        obstruction_coordinates: vec!["rejected".to_owned()],
        effect_lowerings: vec![TargetEffectLowering {
            effect: "target.replace".to_owned(),
            target_intrinsic: "echo.dpo@1.replace".to_owned(),
            failure_mappings: std::collections::BTreeMap::new(),
        }],
        effect_signatures: Vec::new(),
        pure_functions: Vec::new(),
    }
}

fn temp_tree(name: &str) -> PathBuf {
    let counter = TEMP_COUNTER.fetch_add(1, Ordering::SeqCst);
    let path = std::env::temp_dir().join(format!(
        "edict-cli-jsonl-{}-{name}-{counter}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&path);
    fs::create_dir_all(&path).expect("create temp fixture dir");
    path
}

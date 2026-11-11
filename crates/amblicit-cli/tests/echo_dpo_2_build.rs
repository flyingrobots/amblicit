//! An application whose adapter accepts target profile `echo.dpo@2` lowers to
//! the ordered `echo.span-ir/v2` Target IR on the application build path
//! (issue #310).
//!
//! Amblicit carries no `echo.dpo@2` provider components, so a public build here
//! cannot publish an executable package. The in-process test drives the exact
//! functions the build calls up to provider invocation; the public-binary test
//! shows the build passes Target lowering and stops at the first provider
//! stage after it. Echo's provider package proves the full build.

#[path = "../examples/configurable_causal_cell/limits.rs"]
mod limits;

#[path = "../examples/configurable_causal_cell/document.rs"]
mod document;

#[path = "../examples/configurable_causal_cell/command.rs"]
mod command;

use std::fs;
use std::io::Write as _;
use std::path::Path;
use std::process::{Command, Output, Stdio};

use amblicit_syntax::{
    author_lawpack, compile_to_core, decode_canonical_cbor, decode_lawpack_adapter,
    decode_lawpack_bundle, digest_canonical_artifact, encode_canonical_cbor,
    encode_target_ir_artifact, lower_to_target_ir, parse_module, prepare_lawpack_compilation,
    CanonicalValue, LawpackArtifactKind, LawpackAuthoringDefinition, TargetLoweringStatus,
    TARGET_PROFILE_API_VERSION, TARGET_PROVIDER_ABI, TARGET_PROVIDER_MANIFEST_API_VERSION,
};
use limits::Limits;
use serde_json::{json, Value};

const ECHO_DPO_2: &str = "echo.dpo@2";
const ORDERED: &str = "echo.span-ir/v2";
const INTRINSIC: &str = "echo.dpo@2.anchored-node-attachment-create-if-absent";
const ECHO_DPO_1_PROFILE: &[u8] = include_bytes!(
    "../../../fixtures/providers/echo-target-profile/generated/primary/target-profile.echo-dpo.cbor"
);
const LIMITS: Limits = Limits {
    key_scalars: 64,
    value_scalars: 64,
    replacement_bytes: 256,
};

/// A distinct, canonical `echo.dpo@2` profile artifact. The build binds a
/// profile only by its digest, so these test bytes need not be Echo's.
fn echo_dpo_2_profile() -> (Vec<u8>, String) {
    fn retarget(value: &mut CanonicalValue) {
        match value {
            CanonicalValue::Text(text) => *text = text.replace("echo.dpo@1", ECHO_DPO_2),
            CanonicalValue::Array(values) => values.iter_mut().for_each(retarget),
            CanonicalValue::Map(entries) => entries.iter_mut().for_each(|(_, v)| retarget(v)),
            _ => {}
        }
    }
    let mut profile = decode_canonical_cbor(ECHO_DPO_1_PROFILE).expect("fixture profile");
    retarget(&mut profile);
    let bytes = encode_canonical_cbor(&profile).expect("encode echo.dpo@2 profile");
    let digest = digest_canonical_artifact(TARGET_PROFILE_API_VERSION, &bytes)
        .expect("canonical profile")
        .to_review_string();
    (bytes, digest)
}

/// The causal-cell example lawpack, retargeted to `echo.dpo@2`.
fn echo_dpo_2_lawpack_document(profile_digest: &str) -> Value {
    let mut document = document::document(LIMITS).expect("example document");
    let adapter = &mut document["lawpack"]["targetAdapters"][0];
    adapter["acceptedTargetProfile"] = json!({"id": ECHO_DPO_2, "digest": profile_digest});
    // Amblicit selects the contract by this id; it does not interpret the digest.
    adapter["acceptedTargetIr"] =
        json!({"id": ORDERED, "digest": format!("sha256:{}", "2f".repeat(32))});
    adapter["effectImplementations"]["example.cell@1.createIfAbsent"]["targetIntrinsic"] =
        json!(INTRINSIC);
    document
}

#[test]
fn echo_dpo_2_adapter_lowers_the_application_to_ordered_target_ir() {
    let (_, profile_digest) = echo_dpo_2_profile();
    let document = echo_dpo_2_lawpack_document(&profile_digest);
    let definition: LawpackAuthoringDefinition =
        serde_json::from_value(document["lawpack"].clone()).expect("typed authoring document");
    let authored = author_lawpack(&definition, &[]).expect("author echo.dpo@2 lawpack");
    let artifact = |kind| authored.artifact(kind).expect("authored artifact");
    let manifest = artifact(LawpackArtifactKind::Manifest);
    let bundle = decode_lawpack_bundle(
        manifest.bytes(),
        artifact(LawpackArtifactKind::Exports).bytes(),
    )
    .expect("authored bundle");
    let adapter = decode_lawpack_adapter(
        &bundle,
        ECHO_DPO_2,
        artifact(LawpackArtifactKind::Adapter).bytes(),
    )
    .expect("echo.dpo@2 adapter");
    let source =
        command::render(&["source", "64", "64", "256", manifest.digest()]).expect("example source");
    let module = parse_module(&source).expect("source parses");
    let prepared =
        prepare_lawpack_compilation(&module, &bundle, &adapter).expect("closure corroborates");
    let core = compile_to_core(&module, prepared.compiler_context()).expect("Core");

    let report = lower_to_target_ir(&core, prepared.target_ir_facts());

    assert_eq!(report.status, TargetLoweringStatus::Lowered, "{report:?}");
    let target_ir = report.artifact.expect("Target IR");
    assert_eq!(target_ir.domain, ORDERED);
    assert_eq!(target_ir.target_profile.coordinate, ECHO_DPO_2);
    assert_eq!(
        target_ir.target_profile.digest.as_deref(),
        Some(profile_digest.as_str())
    );
    let intent = &target_ir.intents["create"];
    assert_eq!(intent.steps.len(), 1);
    assert_eq!(intent.steps[0].target_intrinsic, INTRINSIC);
    assert!(intent.execution_order.is_some());
    encode_target_ir_artifact(&target_ir).expect("ordered Target IR encodes");
}

#[test]
fn public_build_of_an_echo_dpo_2_application_passes_target_lowering() {
    let directory = tempfile::Builder::new()
        .prefix("amblicit-echo-dpo-2-")
        .tempdir()
        .expect("owned temp directory");
    let root = directory.path();
    let (profile_bytes, profile_digest) = echo_dpo_2_profile();
    write_application(root, &profile_bytes, &profile_digest);

    let built = run_public_build(root, "application", "edict.application.json");

    assert_eq!(built.status.code(), Some(2), "{built:?}");
    assert!(built.stdout.is_empty(), "{built:?}");
    let kinds = std::str::from_utf8(&built.stderr)
        .expect("UTF-8 JSONL")
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).expect("JSONL record"))
        .filter(|record| record["schema"] == "edict.cli.diagnostic/v1")
        .map(|record| record["kind"].as_str().unwrap_or_default().to_owned())
        .collect::<Vec<_>>();
    // Before #310 this was `TargetLoweringFailed`. The first stage after
    // Target lowering reads the provider's artifact schema, which this package
    // declares but does not ship.
    assert_eq!(kinds, ["ProviderPackageReadFailed"], "{built:?}");
    assert!(!root.join(".build/application").exists());
}

fn write_application(root: &Path, profile_bytes: &[u8], profile_digest: &str) {
    fs::create_dir_all(root.join("src")).expect("source directory");
    fs::create_dir_all(root.join("provider/generated/primary")).expect("provider directory");
    fs::write(
        root.join("edict.lawpack.json"),
        serde_json::to_vec(&echo_dpo_2_lawpack_document(profile_digest)).expect("encode"),
    )
    .expect("write lawpack document");
    let authored = run_public_build(root, "lawpack", "edict.lawpack.json");
    assert_eq!(authored.status.code(), Some(0), "{authored:?}");
    let lawpack_digest =
        fs::read_to_string(root.join("vendor/cell/manifest.sha256")).expect("lawpack digest");
    fs::write(
        root.join("src/create.amblicit"),
        command::render(&["source", "64", "64", "256", lawpack_digest.trim()]).expect("source"),
    )
    .expect("write source");
    fs::write(
        root.join("provider/generated/primary/target-profile.echo-dpo-2.cbor"),
        profile_bytes,
    )
    .expect("write profile");
    let generated = json!({
        "kind": "generated",
        "semanticSource": {"coordinate": "echo.semantic-schema@1", "digest": format!("sha256:{}", "2".repeat(64))},
        "generator": {"coordinate": "echo-wesley-gen.provider-artifact-generator@1", "digest": format!("sha256:{}", "3".repeat(64))}
    });
    let provider = json!({
        "apiVersion": TARGET_PROVIDER_MANIFEST_API_VERSION,
        "providerAbi": TARGET_PROVIDER_ABI,
        "provider": {"coordinate": "echo.edict-provider@1", "digest": format!("sha256:{}", "1".repeat(64))},
        "artifacts": [
            {
                "role": "target-profile.echo-dpo-2",
                "artifactKind": "targetProfile",
                "resource": {"coordinate": ECHO_DPO_2, "digest": profile_digest},
                "source": generated.clone()
            },
            {
                "role": "schema.echo-provider-artifacts",
                "artifactKind": "artifactSchema",
                "resource": {"coordinate": "echo.provider-artifacts.cddl@1", "digest": format!("sha256:{}", "4".repeat(64))},
                "source": generated
            }
        ],
        "schemaBindings": [{
            "domain": "echo.generated-artifact/v1",
            "schemaRole": "schema.echo-provider-artifacts",
            "format": "selfContainedCddlV1",
            "rootRule": "generated-artifact"
        }]
    });
    fs::write(
        root.join("provider/provider-manifest.echo.json"),
        serde_json::to_vec_pretty(&provider).expect("encode provider manifest"),
    )
    .expect("write provider manifest");
    let mut application: Value =
        serde_json::from_str(&command::render(&["application"]).expect("application"))
            .expect("application JSON");
    application["target"]["profile"] = json!(ECHO_DPO_2);
    fs::write(
        root.join("edict.application.json"),
        serde_json::to_vec_pretty(&application).expect("encode application"),
    )
    .expect("write application");
}

fn run_public_build(root: &Path, key: &str, document: &str) -> Output {
    let request = json!({
        "schema": "edict.compiler.settings/v1", "type": "compilerSettings",
        "operation": "build", key: document,
    });
    let mut child = Command::new(env!("CARGO_BIN_EXE_amblicit"))
        .current_dir(root)
        .env_remove(amblicit_cli::MAX_STDIN_BYTES_ENV)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn public binary");
    writeln!(child.stdin.take().expect("stdin"), "{request}").expect("write request");
    child.wait_with_output().expect("public binary output")
}

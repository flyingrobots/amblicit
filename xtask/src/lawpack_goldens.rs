use std::fmt::Write as _;
use std::fs;
use std::path::Path;

use edict_syntax::{
    compile_to_core, decode_lawpack_adapter, decode_lawpack_bundle, digest_canonical_artifact,
    digest_core_module, digest_target_ir_artifact, encode_canonical_cbor, encode_core_module,
    encode_target_ir_artifact, lower_to_target_ir, parse_module, prepare_lawpack_compilation,
    CanonicalValue, TargetLoweringStatus, ValidatedLawpackAdapter, ValidatedLawpackBundle,
    EXTERNAL_ACTION_RESOURCE_API_VERSION, EXTERNAL_ACTION_RESOURCE_DIGEST_DOMAIN,
};
use sha2::{Digest, Sha256};

use crate::goldens::{check_golden_file_with_command, write_golden_file};

const DIGEST_FRAME: &str = "edict.digest/v1";
const EXPORTS_COORDINATE: &str = "hello.echo.exports/v1";
const FIXTURE_ROOT: &str = "fixtures/lawpack/hello-echo";
const MANIFEST_CBOR: &str = "fixtures/lawpack/hello-echo/manifest.cbor";
const MANIFEST_DIGEST: &str = "fixtures/lawpack/hello-echo/manifest.sha256";
const EXPORTS_CBOR: &str = "fixtures/lawpack/hello-echo/exports.cbor";
const EXPORTS_DIGEST: &str = "fixtures/lawpack/hello-echo/exports.sha256";
const ADAPTER_CBOR: &str = "fixtures/lawpack/hello-echo/adapter.cbor";
const ADAPTER_DIGEST: &str = "fixtures/lawpack/hello-echo/adapter.sha256";
const TARGET_CONFIGURATION_CBOR: &str =
    "fixtures/lawpack/hello-echo/echo-operation-configuration.cbor";
const TARGET_CONFIGURATION_DIGEST: &str =
    "fixtures/lawpack/hello-echo/echo-operation-configuration.sha256";
const CREATE_GREETING_SOURCE: &str = "fixtures/lawpack/hello-echo/create-greeting.edict";
const CREATE_GREETING_CORE_CBOR: &str = "fixtures/lawpack/hello-echo/create-greeting.core.cbor";
const CREATE_GREETING_CORE_DIGEST: &str = "fixtures/lawpack/hello-echo/create-greeting.core.sha256";
const CREATE_GREETING_TARGET_IR_CBOR: &str =
    "fixtures/lawpack/hello-echo/create-greeting.target-ir.cbor";
const CREATE_GREETING_TARGET_IR_DIGEST: &str =
    "fixtures/lawpack/hello-echo/create-greeting.target-ir.sha256";
const CREATE_GREETING_RESULT_PROJECTION_CBOR: &str =
    "fixtures/lawpack/hello-echo/create-greeting.result-projection.cbor";
const CREATE_GREETING_RESULT_PROJECTION_DIGEST: &str =
    "fixtures/lawpack/hello-echo/create-greeting.result-projection.sha256";
const ADAPTER_COORDINATE: &str = "hello.echo.echo-dpo-adapter/v1";
const TARGET_CONFIGURATION_COORDINATE: &str = "hello.echo.echo-operation-configuration/v1";
const ECHO_TARGET_PROFILE_DIGEST: [u8; 32] = [
    0x2e, 0x24, 0x94, 0x12, 0x1a, 0xec, 0xf5, 0xe6, 0xa2, 0xd9, 0x20, 0xf5, 0xfb, 0x85, 0x40, 0x88,
    0x25, 0xd3, 0x94, 0x76, 0x5f, 0xad, 0x41, 0x48, 0x4c, 0x41, 0x63, 0x97, 0xc9, 0x20, 0xfb, 0x04,
];
const ECHO_TARGET_IR_DIGEST: [u8; 32] = [
    0x00, 0x57, 0x16, 0x7e, 0x68, 0xf5, 0x0c, 0x99, 0xdc, 0xce, 0x08, 0x7b, 0x3e, 0x1c, 0xd6, 0x77,
    0xd1, 0x7c, 0x5d, 0x1d, 0xc2, 0x38, 0xbd, 0xb5, 0x2d, 0x89, 0x46, 0x9e, 0x14, 0x72, 0xfc, 0x2f,
];
const WRITE_COMMAND: &str = "cargo xtask lawpack-goldens --write";
const CAUSAL_CELL_FIXTURE_ROOT: &str = "fixtures/lawpack/causal-cell";
const CAUSAL_CELL_MANIFEST_CBOR: &str = "fixtures/lawpack/causal-cell/manifest.cbor";
const CAUSAL_CELL_MANIFEST_DIGEST: &str = "fixtures/lawpack/causal-cell/manifest.sha256";
const CAUSAL_CELL_EXPORTS_CBOR: &str = "fixtures/lawpack/causal-cell/exports.cbor";
const CAUSAL_CELL_EXPORTS_DIGEST: &str = "fixtures/lawpack/causal-cell/exports.sha256";
const CAUSAL_CELL_ADAPTER_CBOR: &str = "fixtures/lawpack/causal-cell/adapter.cbor";
const CAUSAL_CELL_ADAPTER_DIGEST: &str = "fixtures/lawpack/causal-cell/adapter.sha256";
const CAUSAL_CELL_CONFIGURATION_CBOR: &str =
    "fixtures/lawpack/causal-cell/echo-operation-configuration.cbor";
const CAUSAL_CELL_CONFIGURATION_DIGEST: &str =
    "fixtures/lawpack/causal-cell/echo-operation-configuration.sha256";
const CAUSAL_CELL_EXPORTS_COORDINATE: &str = "causal.cell.exports/v1";
const CAUSAL_CELL_ADAPTER_COORDINATE: &str = "causal.cell.echo-adapter/v1";
const CAUSAL_CELL_CONFIGURATION_COORDINATE: &str = "echo.operation-lowering-configuration/v1";
const WORKSPACE_SNAPSHOT_FIXTURE_ROOT: &str = "fixtures/lawpack/workspace-snapshot";
const WORKSPACE_SNAPSHOT_MANIFEST_CBOR: &str = "fixtures/lawpack/workspace-snapshot/manifest.cbor";
const WORKSPACE_SNAPSHOT_MANIFEST_DIGEST: &str =
    "fixtures/lawpack/workspace-snapshot/manifest.sha256";
const WORKSPACE_SNAPSHOT_EXPORTS_CBOR: &str = "fixtures/lawpack/workspace-snapshot/exports.cbor";
const WORKSPACE_SNAPSHOT_EXPORTS_DIGEST: &str =
    "fixtures/lawpack/workspace-snapshot/exports.sha256";
const WORKSPACE_SNAPSHOT_ADAPTER_CBOR: &str = "fixtures/lawpack/workspace-snapshot/adapter.cbor";
const WORKSPACE_SNAPSHOT_ADAPTER_DIGEST: &str =
    "fixtures/lawpack/workspace-snapshot/adapter.sha256";
const WORKSPACE_SNAPSHOT_CONFIGURATION_CBOR: &str =
    "fixtures/lawpack/workspace-snapshot/request-profile-configuration.cbor";
const WORKSPACE_SNAPSHOT_CONFIGURATION_DIGEST: &str =
    "fixtures/lawpack/workspace-snapshot/request-profile-configuration.sha256";
const WORKSPACE_SNAPSHOT_INPUT_SCHEMA_CBOR: &str =
    "fixtures/lawpack/workspace-snapshot/input-schema.cbor";
const WORKSPACE_SNAPSHOT_INPUT_SCHEMA_DIGEST: &str =
    "fixtures/lawpack/workspace-snapshot/input-schema.sha256";
const WORKSPACE_SNAPSHOT_SETTLEMENT_SCHEMA_CBOR: &str =
    "fixtures/lawpack/workspace-snapshot/settlement-schema.cbor";
const WORKSPACE_SNAPSHOT_SETTLEMENT_SCHEMA_DIGEST: &str =
    "fixtures/lawpack/workspace-snapshot/settlement-schema.sha256";
const WORKSPACE_SNAPSHOT_RECONCILIATION_LAW_CBOR: &str =
    "fixtures/lawpack/workspace-snapshot/reconciliation-law.cbor";
const WORKSPACE_SNAPSHOT_RECONCILIATION_LAW_DIGEST: &str =
    "fixtures/lawpack/workspace-snapshot/reconciliation-law.sha256";
const WORKSPACE_SNAPSHOT_SOURCE: &str =
    "fixtures/lawpack/workspace-snapshot/observe-workspace.edict";
const WORKSPACE_SNAPSHOT_CORE_CBOR: &str =
    "fixtures/lawpack/workspace-snapshot/observe-workspace.core.cbor";
const WORKSPACE_SNAPSHOT_CORE_DIGEST: &str =
    "fixtures/lawpack/workspace-snapshot/observe-workspace.core.sha256";
const WORKSPACE_SNAPSHOT_TARGET_IR_CBOR: &str =
    "fixtures/lawpack/workspace-snapshot/observe-workspace.target-ir.cbor";
const WORKSPACE_SNAPSHOT_TARGET_IR_DIGEST: &str =
    "fixtures/lawpack/workspace-snapshot/observe-workspace.target-ir.sha256";
const WORKSPACE_SNAPSHOT_EXPORTS_COORDINATE: &str = "workspace.snapshot.exports/v1";
const WORKSPACE_SNAPSHOT_ADAPTER_COORDINATE: &str = "workspace.snapshot.echo-adapter/v1";
const WORKSPACE_SNAPSHOT_CONFIGURATION_COORDINATE: &str = "workspace.snapshot.request-profile/v1";
const WORKSPACE_PATCH_FIXTURE_ROOT: &str = "fixtures/lawpack/workspace-patch";
const WORKSPACE_PATCH_MANIFEST_CBOR: &str = "fixtures/lawpack/workspace-patch/manifest.cbor";
const WORKSPACE_PATCH_MANIFEST_DIGEST: &str = "fixtures/lawpack/workspace-patch/manifest.sha256";
const WORKSPACE_PATCH_EXPORTS_CBOR: &str = "fixtures/lawpack/workspace-patch/exports.cbor";
const WORKSPACE_PATCH_EXPORTS_DIGEST: &str = "fixtures/lawpack/workspace-patch/exports.sha256";
const WORKSPACE_PATCH_ADAPTER_CBOR: &str = "fixtures/lawpack/workspace-patch/adapter.cbor";
const WORKSPACE_PATCH_ADAPTER_DIGEST: &str = "fixtures/lawpack/workspace-patch/adapter.sha256";
const WORKSPACE_PATCH_CONFIGURATION_CBOR: &str =
    "fixtures/lawpack/workspace-patch/request-profile-configuration.cbor";
const WORKSPACE_PATCH_CONFIGURATION_DIGEST: &str =
    "fixtures/lawpack/workspace-patch/request-profile-configuration.sha256";
const WORKSPACE_PATCH_INPUT_SCHEMA_CBOR: &str =
    "fixtures/lawpack/workspace-patch/input-schema.cbor";
const WORKSPACE_PATCH_INPUT_SCHEMA_DIGEST: &str =
    "fixtures/lawpack/workspace-patch/input-schema.sha256";
const WORKSPACE_PATCH_SETTLEMENT_SCHEMA_CBOR: &str =
    "fixtures/lawpack/workspace-patch/settlement-schema.cbor";
const WORKSPACE_PATCH_SETTLEMENT_SCHEMA_DIGEST: &str =
    "fixtures/lawpack/workspace-patch/settlement-schema.sha256";
const WORKSPACE_PATCH_RECONCILIATION_LAW_CBOR: &str =
    "fixtures/lawpack/workspace-patch/reconciliation-law.cbor";
const WORKSPACE_PATCH_RECONCILIATION_LAW_DIGEST: &str =
    "fixtures/lawpack/workspace-patch/reconciliation-law.sha256";
const WORKSPACE_PATCH_SOURCE: &str = "fixtures/lawpack/workspace-patch/apply-validated-patch.edict";
const WORKSPACE_PATCH_CORE_CBOR: &str =
    "fixtures/lawpack/workspace-patch/apply-validated-patch.core.cbor";
const WORKSPACE_PATCH_CORE_DIGEST: &str =
    "fixtures/lawpack/workspace-patch/apply-validated-patch.core.sha256";
const WORKSPACE_PATCH_TARGET_IR_CBOR: &str =
    "fixtures/lawpack/workspace-patch/apply-validated-patch.target-ir.cbor";
const WORKSPACE_PATCH_TARGET_IR_DIGEST: &str =
    "fixtures/lawpack/workspace-patch/apply-validated-patch.target-ir.sha256";
const WORKSPACE_PATCH_EXPORTS_COORDINATE: &str = "workspace.patch.exports/v1";
const WORKSPACE_PATCH_ADAPTER_COORDINATE: &str = "workspace.patch.echo-adapter/v1";
const WORKSPACE_PATCH_CONFIGURATION_COORDINATE: &str = "workspace.patch.request-profile/v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LawpackGoldenMode {
    Check,
    Write,
}

struct GeneratedExternalActionResource {
    coordinate: String,
    bytes: Vec<u8>,
    digest: String,
}

#[derive(Clone, Copy)]
struct InputSchemaResource<'a>(&'a GeneratedExternalActionResource);

#[derive(Clone, Copy)]
struct SettlementSchemaResource<'a>(&'a GeneratedExternalActionResource);

#[derive(Clone, Copy)]
struct ReconciliationLawResource<'a>(&'a GeneratedExternalActionResource);

struct CompiledExternalActionArtifacts {
    core_bytes: Vec<u8>,
    core_digest: String,
    target_ir_bytes: Vec<u8>,
    target_ir_digest: String,
}

pub(crate) fn lawpack_goldens(root: &Path, mode: LawpackGoldenMode) -> Result<(), String> {
    let artifacts = hello_echo_golden_artifacts(root)?
        .into_iter()
        .chain(causal_cell_golden_artifacts()?)
        .chain(workspace_snapshot_golden_artifacts()?)
        .chain(workspace_patch_golden_artifacts()?);
    for (path, bytes) in artifacts {
        match mode {
            LawpackGoldenMode::Check => {
                check_golden_file_with_command(root, path, &bytes, WRITE_COMMAND)?;
            }
            LawpackGoldenMode::Write => write_golden_file(&root.join(path), &bytes)?,
        }
    }

    println!(
        "lawpack-goldens: {FIXTURE_ROOT}, {CAUSAL_CELL_FIXTURE_ROOT}, {WORKSPACE_SNAPSHOT_FIXTURE_ROOT}, and {WORKSPACE_PATCH_FIXTURE_ROOT} {}",
        match mode {
            LawpackGoldenMode::Check => "checked",
            LawpackGoldenMode::Write => "written",
        }
    );
    Ok(())
}

fn workspace_snapshot_golden_artifacts() -> Result<Vec<(&'static str, Vec<u8>)>, String> {
    let input_schema = external_action_resource(
        "workspace.snapshot.input@1",
        "inputSchema",
        workspace_snapshot_input_schema(),
    )?;
    let settlement_schema = external_action_resource(
        "workspace.snapshot.settlement@1",
        "settlementSchema",
        workspace_snapshot_settlement_schema(),
    )?;
    let reconciliation_law = external_action_resource(
        "workspace.snapshot.reconcile@1",
        "reconciliationLaw",
        workspace_snapshot_reconciliation_law(),
    )?;
    let exports_value = workspace_snapshot_exports();
    let exports_bytes = encode_canonical_cbor(&exports_value)
        .map_err(|error| format!("encode workspace snapshot exports: {error}"))?;
    let exports_digest = digest_value(WORKSPACE_SNAPSHOT_EXPORTS_COORDINATE, &exports_value)?;

    let configuration_value = workspace_snapshot_target_configuration();
    let configuration_bytes = encode_canonical_cbor(&configuration_value)
        .map_err(|error| format!("encode workspace snapshot target configuration: {error}"))?;
    let configuration_digest = digest_value(
        WORKSPACE_SNAPSHOT_CONFIGURATION_COORDINATE,
        &configuration_value,
    )?;

    let adapter_value = workspace_snapshot_adapter(configuration_digest);
    let adapter_bytes = encode_canonical_cbor(&adapter_value)
        .map_err(|error| format!("encode workspace snapshot adapter: {error}"))?;
    let adapter_digest = digest_value(WORKSPACE_SNAPSHOT_ADAPTER_COORDINATE, &adapter_value)?;

    let manifest_value = workspace_snapshot_manifest(exports_digest, adapter_digest);
    let manifest_bytes = encode_canonical_cbor(&manifest_value)
        .map_err(|error| format!("encode workspace snapshot manifest: {error}"))?;
    let bundle = decode_lawpack_bundle(&manifest_bytes, &exports_bytes)
        .map_err(|failures| format!("validate workspace snapshot lawpack: {failures:?}"))?;
    let adapter = decode_lawpack_adapter(&bundle, "echo.dpo@1", &adapter_bytes)
        .map_err(|failures| format!("validate workspace snapshot adapter: {failures:?}"))?;

    let source = workspace_snapshot_application_source(
        &bundle.manifest_digest_review_string(),
        InputSchemaResource(&input_schema),
        SettlementSchemaResource(&settlement_schema),
        ReconciliationLawResource(&reconciliation_law),
    );
    let compiled =
        compile_external_action_application(&source, &bundle, &adapter, "workspace snapshot")?;

    Ok(vec![
        (WORKSPACE_SNAPSHOT_MANIFEST_CBOR, manifest_bytes),
        (
            WORKSPACE_SNAPSHOT_MANIFEST_DIGEST,
            format!("{}\n", bundle.manifest_digest_review_string()).into_bytes(),
        ),
        (WORKSPACE_SNAPSHOT_EXPORTS_CBOR, exports_bytes),
        (
            WORKSPACE_SNAPSHOT_EXPORTS_DIGEST,
            format!("{}\n", bundle.manifest().exports.digest_review_string()).into_bytes(),
        ),
        (WORKSPACE_SNAPSHOT_ADAPTER_CBOR, adapter_bytes),
        (
            WORKSPACE_SNAPSHOT_ADAPTER_DIGEST,
            format!("{}\n", sha256_review_string(&adapter_digest)).into_bytes(),
        ),
        (WORKSPACE_SNAPSHOT_CONFIGURATION_CBOR, configuration_bytes),
        (
            WORKSPACE_SNAPSHOT_CONFIGURATION_DIGEST,
            format!("{}\n", sha256_review_string(&configuration_digest)).into_bytes(),
        ),
        (WORKSPACE_SNAPSHOT_INPUT_SCHEMA_CBOR, input_schema.bytes),
        (
            WORKSPACE_SNAPSHOT_INPUT_SCHEMA_DIGEST,
            format!("{}\n", input_schema.digest).into_bytes(),
        ),
        (
            WORKSPACE_SNAPSHOT_SETTLEMENT_SCHEMA_CBOR,
            settlement_schema.bytes,
        ),
        (
            WORKSPACE_SNAPSHOT_SETTLEMENT_SCHEMA_DIGEST,
            format!("{}\n", settlement_schema.digest).into_bytes(),
        ),
        (
            WORKSPACE_SNAPSHOT_RECONCILIATION_LAW_CBOR,
            reconciliation_law.bytes,
        ),
        (
            WORKSPACE_SNAPSHOT_RECONCILIATION_LAW_DIGEST,
            format!("{}\n", reconciliation_law.digest).into_bytes(),
        ),
        (WORKSPACE_SNAPSHOT_SOURCE, source.into_bytes()),
        (WORKSPACE_SNAPSHOT_CORE_CBOR, compiled.core_bytes),
        (
            WORKSPACE_SNAPSHOT_CORE_DIGEST,
            format!("{}\n", compiled.core_digest).into_bytes(),
        ),
        (WORKSPACE_SNAPSHOT_TARGET_IR_CBOR, compiled.target_ir_bytes),
        (
            WORKSPACE_SNAPSHOT_TARGET_IR_DIGEST,
            format!("{}\n", compiled.target_ir_digest).into_bytes(),
        ),
    ])
}

fn workspace_snapshot_manifest(
    exports_digest: [u8; 32],
    adapter_digest: [u8; 32],
) -> CanonicalValue {
    map([
        ("apiVersion", text("edict.lawpack/v1")),
        ("id", text("workspace.snapshot")),
        ("version", text("1")),
        (
            "acceptedCoreAbi",
            CanonicalValue::Array(vec![text("edict.core/v1")]),
        ),
        ("dependencies", CanonicalValue::Array(Vec::new())),
        (
            "exports",
            resource_ref(WORKSPACE_SNAPSHOT_EXPORTS_COORDINATE, exports_digest),
        ),
        (
            "targetAdapters",
            CanonicalValue::Array(vec![map([
                (
                    "acceptedTargetProfile",
                    resource_ref("echo.dpo@1", ECHO_TARGET_PROFILE_DIGEST),
                ),
                (
                    "acceptedTargetIr",
                    resource_ref("echo.span-ir/v1", ECHO_TARGET_IR_DIGEST),
                ),
                (
                    "adapter",
                    resource_ref(WORKSPACE_SNAPSHOT_ADAPTER_COORDINATE, adapter_digest),
                ),
            ])]),
        ),
        (
            "verifier",
            map([
                ("class", text("declarative")),
                (
                    "ruleset",
                    resource_ref("workspace.snapshot.verifier-rules/v1", [0x84; 32]),
                ),
            ]),
        ),
        (
            "compatibility",
            resource_ref("workspace.snapshot.compatibility/v1", [0x85; 32]),
        ),
        (
            "conformanceFixtureCorpus",
            resource_ref("workspace.snapshot.fixtures/v1", [0x86; 32]),
        ),
    ])
}

fn workspace_snapshot_adapter(configuration_digest: [u8; 32]) -> CanonicalValue {
    map([
        ("apiVersion", text("edict.lawpack-adapter/v1")),
        ("class", text("declarative")),
        (
            "operationProfiles",
            map([(
                "workspace.snapshot@1.observeRequest",
                map([
                    ("core", text("continuum.profile.read-only/v1")),
                    ("semanticEffects", CanonicalValue::Array(Vec::new())),
                    (
                        "budgetObligation",
                        text("workspace.snapshot@1.tinyObservationBudget"),
                    ),
                    (
                        "targetConfiguration",
                        resource_ref(
                            WORKSPACE_SNAPSHOT_CONFIGURATION_COORDINATE,
                            configuration_digest,
                        ),
                    ),
                ]),
            )]),
        ),
        ("effectImplementations", CanonicalValue::Map(Vec::new())),
        (
            "budgets",
            map([(
                "workspace.snapshot@1.tinyObservationBudget",
                map([
                    ("maxSteps", CanonicalValue::Integer(512)),
                    ("maxAllocatedBytes", CanonicalValue::Integer(256 * 1024)),
                    ("maxOutputBytes", CanonicalValue::Integer(128 * 1024)),
                ]),
            )]),
        ),
    ])
}

fn workspace_snapshot_target_configuration() -> CanonicalValue {
    map([
        ("apiVersion", text("workspace.snapshot.request-profile/v1")),
        ("operation", text("workspace.snapshot.observe@1")),
        ("authorityClass", text("scoped")),
        ("basisClass", text("workspace-root")),
    ])
}

fn workspace_snapshot_exports() -> CanonicalValue {
    map([
        ("types", CanonicalValue::Array(Vec::new())),
        ("constants", CanonicalValue::Array(Vec::new())),
        ("pureFunctions", CanonicalValue::Array(Vec::new())),
        ("effects", CanonicalValue::Array(Vec::new())),
        ("obstructions", CanonicalValue::Array(Vec::new())),
        (
            "operationProfiles",
            map([(
                "workspace.snapshot@1.observeRequest",
                map([
                    (
                        "opticTemplate",
                        map([
                            ("opticKind", text("revelation")),
                            ("boundaryKind", text("projection")),
                            ("supportPolicy", text("workspace.snapshot@1.requestOnly")),
                            ("lossDisposition", text("workspace.snapshot@1.lossless")),
                            (
                                "apertureRequirement",
                                map([
                                    ("kind", text("abstractFootprintObligation")),
                                    ("ref", text("workspace.snapshot@1.authorityScope")),
                                ]),
                            ),
                        ]),
                    ),
                    (
                        "effectPredicate",
                        text("workspace.snapshot@1.externalObservation"),
                    ),
                ]),
            )]),
        ),
    ])
}

fn workspace_snapshot_application_source(
    manifest_digest: &str,
    input_schema: InputSchemaResource<'_>,
    settlement_schema: SettlementSchemaResource<'_>,
    reconciliation_law: ReconciliationLawResource<'_>,
) -> String {
    let input_schema_coordinate = &input_schema.0.coordinate;
    let input_schema_digest = &input_schema.0.digest;
    let settlement_schema_coordinate = &settlement_schema.0.coordinate;
    let settlement_schema_digest = &settlement_schema.0.digest;
    let reconciliation_law_coordinate = &reconciliation_law.0.coordinate;
    let reconciliation_law_digest = &reconciliation_law.0.digest;
    format!(
        r#"package examples.workspace_observer@1;

use lawpack workspace.snapshot@1 digest "{manifest_digest}" as workspace;
use capability workspace.snapshot.observe@1 digest "{manifest_digest}" as snapshot;

type ObserveInput = {{
  payload: Bytes<max=1024>,
  scope: Bytes<max=32>,
  basis: Bytes<max=32>,
  maxSettlementBytes: U64,
  maxAttempts: U32,
}};

intent observe(input: ObserveInput)
  returns ExternalActionRequest<Bytes<max=65536>>
  profile workspace.observeRequest
  basis input.basis
  budget <= workspace.tinyObservationBudget
{{
  request pending: ExternalActionRequest<Bytes<max=65536>> =
    snapshot(input.payload)
    input schema {input_schema_coordinate}
      digest "{input_schema_digest}"
    settlement schema {settlement_schema_coordinate}
      digest "{settlement_schema_digest}"
    authority input.scope
    basis input.basis
    budget
      maxSettlementBytes input.maxSettlementBytes
      maxAttempts input.maxAttempts
    reconcile {reconciliation_law_coordinate}
      digest "{reconciliation_law_digest}";
  return pending;
}}
"#
    )
}

fn workspace_patch_golden_artifacts() -> Result<Vec<(&'static str, Vec<u8>)>, String> {
    let input_schema = external_action_resource(
        "workspace.patch.input@1",
        "inputSchema",
        workspace_patch_input_schema(),
    )?;
    let settlement_schema = external_action_resource(
        "workspace.patch.settlement@1",
        "settlementSchema",
        workspace_patch_settlement_schema(),
    )?;
    let reconciliation_law = external_action_resource(
        "workspace.patch.reconcile@1",
        "reconciliationLaw",
        workspace_patch_reconciliation_law(),
    )?;
    let exports_value = workspace_patch_exports();
    let exports_bytes = encode_canonical_cbor(&exports_value)
        .map_err(|error| format!("encode workspace patch exports: {error}"))?;
    let exports_digest = digest_value(WORKSPACE_PATCH_EXPORTS_COORDINATE, &exports_value)?;

    let configuration_value = workspace_patch_target_configuration();
    let configuration_bytes = encode_canonical_cbor(&configuration_value)
        .map_err(|error| format!("encode workspace patch target configuration: {error}"))?;
    let configuration_digest = digest_value(
        WORKSPACE_PATCH_CONFIGURATION_COORDINATE,
        &configuration_value,
    )?;

    let adapter_value = workspace_patch_adapter(configuration_digest);
    let adapter_bytes = encode_canonical_cbor(&adapter_value)
        .map_err(|error| format!("encode workspace patch adapter: {error}"))?;
    let adapter_digest = digest_value(WORKSPACE_PATCH_ADAPTER_COORDINATE, &adapter_value)?;

    let manifest_value = workspace_patch_manifest(exports_digest, adapter_digest);
    let manifest_bytes = encode_canonical_cbor(&manifest_value)
        .map_err(|error| format!("encode workspace patch manifest: {error}"))?;
    let bundle = decode_lawpack_bundle(&manifest_bytes, &exports_bytes)
        .map_err(|failures| format!("validate workspace patch lawpack: {failures:?}"))?;
    let adapter = decode_lawpack_adapter(&bundle, "echo.dpo@1", &adapter_bytes)
        .map_err(|failures| format!("validate workspace patch adapter: {failures:?}"))?;

    let source = workspace_patch_application_source(
        &bundle.manifest_digest_review_string(),
        InputSchemaResource(&input_schema),
        SettlementSchemaResource(&settlement_schema),
        ReconciliationLawResource(&reconciliation_law),
    );
    let compiled =
        compile_external_action_application(&source, &bundle, &adapter, "workspace patch")?;

    Ok(vec![
        (WORKSPACE_PATCH_MANIFEST_CBOR, manifest_bytes),
        (
            WORKSPACE_PATCH_MANIFEST_DIGEST,
            format!("{}\n", bundle.manifest_digest_review_string()).into_bytes(),
        ),
        (WORKSPACE_PATCH_EXPORTS_CBOR, exports_bytes),
        (
            WORKSPACE_PATCH_EXPORTS_DIGEST,
            format!("{}\n", bundle.manifest().exports.digest_review_string()).into_bytes(),
        ),
        (WORKSPACE_PATCH_ADAPTER_CBOR, adapter_bytes),
        (
            WORKSPACE_PATCH_ADAPTER_DIGEST,
            format!("{}\n", sha256_review_string(&adapter_digest)).into_bytes(),
        ),
        (WORKSPACE_PATCH_CONFIGURATION_CBOR, configuration_bytes),
        (
            WORKSPACE_PATCH_CONFIGURATION_DIGEST,
            format!("{}\n", sha256_review_string(&configuration_digest)).into_bytes(),
        ),
        (WORKSPACE_PATCH_INPUT_SCHEMA_CBOR, input_schema.bytes),
        (
            WORKSPACE_PATCH_INPUT_SCHEMA_DIGEST,
            format!("{}\n", input_schema.digest).into_bytes(),
        ),
        (
            WORKSPACE_PATCH_SETTLEMENT_SCHEMA_CBOR,
            settlement_schema.bytes,
        ),
        (
            WORKSPACE_PATCH_SETTLEMENT_SCHEMA_DIGEST,
            format!("{}\n", settlement_schema.digest).into_bytes(),
        ),
        (
            WORKSPACE_PATCH_RECONCILIATION_LAW_CBOR,
            reconciliation_law.bytes,
        ),
        (
            WORKSPACE_PATCH_RECONCILIATION_LAW_DIGEST,
            format!("{}\n", reconciliation_law.digest).into_bytes(),
        ),
        (WORKSPACE_PATCH_SOURCE, source.into_bytes()),
        (WORKSPACE_PATCH_CORE_CBOR, compiled.core_bytes),
        (
            WORKSPACE_PATCH_CORE_DIGEST,
            format!("{}\n", compiled.core_digest).into_bytes(),
        ),
        (WORKSPACE_PATCH_TARGET_IR_CBOR, compiled.target_ir_bytes),
        (
            WORKSPACE_PATCH_TARGET_IR_DIGEST,
            format!("{}\n", compiled.target_ir_digest).into_bytes(),
        ),
    ])
}

fn workspace_patch_manifest(exports_digest: [u8; 32], adapter_digest: [u8; 32]) -> CanonicalValue {
    map([
        ("apiVersion", text("edict.lawpack/v1")),
        ("id", text("workspace.patch")),
        ("version", text("1")),
        (
            "acceptedCoreAbi",
            CanonicalValue::Array(vec![text("edict.core/v1")]),
        ),
        ("dependencies", CanonicalValue::Array(Vec::new())),
        (
            "exports",
            resource_ref(WORKSPACE_PATCH_EXPORTS_COORDINATE, exports_digest),
        ),
        (
            "targetAdapters",
            CanonicalValue::Array(vec![map([
                (
                    "acceptedTargetProfile",
                    resource_ref("echo.dpo@1", ECHO_TARGET_PROFILE_DIGEST),
                ),
                (
                    "acceptedTargetIr",
                    resource_ref("echo.span-ir/v1", ECHO_TARGET_IR_DIGEST),
                ),
                (
                    "adapter",
                    resource_ref(WORKSPACE_PATCH_ADAPTER_COORDINATE, adapter_digest),
                ),
            ])]),
        ),
        (
            "verifier",
            map([
                ("class", text("declarative")),
                (
                    "ruleset",
                    resource_ref("workspace.patch.verifier-rules/v1", [0x94; 32]),
                ),
            ]),
        ),
        (
            "compatibility",
            resource_ref("workspace.patch.compatibility/v1", [0x95; 32]),
        ),
        (
            "conformanceFixtureCorpus",
            resource_ref("workspace.patch.fixtures/v1", [0x96; 32]),
        ),
    ])
}

fn workspace_patch_adapter(configuration_digest: [u8; 32]) -> CanonicalValue {
    map([
        ("apiVersion", text("edict.lawpack-adapter/v1")),
        ("class", text("declarative")),
        (
            "operationProfiles",
            map([(
                "workspace.patch@1.applyValidatedRequest",
                map([
                    ("core", text("continuum.profile.request-only/v1")),
                    ("semanticEffects", CanonicalValue::Array(Vec::new())),
                    (
                        "budgetObligation",
                        text("workspace.patch@1.tinyPatchBudget"),
                    ),
                    (
                        "targetConfiguration",
                        resource_ref(
                            WORKSPACE_PATCH_CONFIGURATION_COORDINATE,
                            configuration_digest,
                        ),
                    ),
                ]),
            )]),
        ),
        ("effectImplementations", CanonicalValue::Map(Vec::new())),
        (
            "budgets",
            map([(
                "workspace.patch@1.tinyPatchBudget",
                map([
                    ("maxSteps", CanonicalValue::Integer(768)),
                    ("maxAllocatedBytes", CanonicalValue::Integer(512 * 1024)),
                    ("maxOutputBytes", CanonicalValue::Integer(128 * 1024)),
                ]),
            )]),
        ),
    ])
}

fn workspace_patch_target_configuration() -> CanonicalValue {
    map([
        ("apiVersion", text("workspace.patch.request-profile/v1")),
        ("operation", text("workspace.patch.applyValidated@1")),
        ("authorityClass", text("exact-writable-path-set")),
        ("basisClass", text("workspace-root")),
        ("patchClass", text("canonical-validated-patch")),
        ("forbiddenPathClass", text("ci-workflow")),
        ("postconditionClass", text("exact-resulting-workspace-root")),
        (
            "reconciliationClass",
            text("observe-postcondition-or-outcome-unknown"),
        ),
    ])
}

fn workspace_patch_exports() -> CanonicalValue {
    map([
        ("types", CanonicalValue::Array(Vec::new())),
        ("constants", CanonicalValue::Array(Vec::new())),
        ("pureFunctions", CanonicalValue::Array(Vec::new())),
        ("effects", CanonicalValue::Array(Vec::new())),
        ("obstructions", CanonicalValue::Array(Vec::new())),
        (
            "operationProfiles",
            map([(
                "workspace.patch@1.applyValidatedRequest",
                map([
                    (
                        "opticTemplate",
                        map([
                            ("opticKind", text("revelation")),
                            ("boundaryKind", text("projection")),
                            ("supportPolicy", text("workspace.patch@1.requestOnly")),
                            ("lossDisposition", text("workspace.patch@1.lossless")),
                            (
                                "apertureRequirement",
                                map([
                                    ("kind", text("abstractFootprintObligation")),
                                    ("ref", text("workspace.patch@1.writablePathPolicy")),
                                ]),
                            ),
                        ]),
                    ),
                    (
                        "effectPredicate",
                        text("workspace.patch@1.externalMutationRequest"),
                    ),
                ]),
            )]),
        ),
    ])
}

fn workspace_patch_application_source(
    manifest_digest: &str,
    input_schema: InputSchemaResource<'_>,
    settlement_schema: SettlementSchemaResource<'_>,
    reconciliation_law: ReconciliationLawResource<'_>,
) -> String {
    let input_schema_coordinate = &input_schema.0.coordinate;
    let input_schema_digest = &input_schema.0.digest;
    let settlement_schema_coordinate = &settlement_schema.0.coordinate;
    let settlement_schema_digest = &settlement_schema.0.digest;
    let reconciliation_law_coordinate = &reconciliation_law.0.coordinate;
    let reconciliation_law_digest = &reconciliation_law.0.digest;
    format!(
        r#"package examples.workspace_patcher@1;

use lawpack workspace.patch@1 digest "{manifest_digest}" as workspace;
use capability workspace.patch.applyValidated@1 digest "{manifest_digest}" as patch;

type ApplyPatchInput = {{
  patch: Bytes<max=65536>,
  authority: Bytes<max=32>,
  basis: Bytes<max=32>,
  maxSettlementBytes: U64,
  maxAttempts: U32,
}};

intent applyValidated(input: ApplyPatchInput)
  returns ExternalActionRequest<Bytes<max=65536>>
  profile workspace.applyValidatedRequest
  basis input.basis
  budget <= workspace.tinyPatchBudget
{{
  request pending: ExternalActionRequest<Bytes<max=65536>> =
    patch(input.patch)
    input schema {input_schema_coordinate}
      digest "{input_schema_digest}"
    settlement schema {settlement_schema_coordinate}
      digest "{settlement_schema_digest}"
    authority input.authority
    basis input.basis
    budget
      maxSettlementBytes input.maxSettlementBytes
      maxAttempts input.maxAttempts
    reconcile {reconciliation_law_coordinate}
      digest "{reconciliation_law_digest}";
  return pending;
}}
"#
    )
}

fn causal_cell_golden_artifacts() -> Result<Vec<(&'static str, Vec<u8>)>, String> {
    let exports_value = causal_cell_exports();
    let exports_bytes = encode_canonical_cbor(&exports_value)
        .map_err(|error| format!("encode causal.cell exports: {error}"))?;
    let exports_digest = digest_value(CAUSAL_CELL_EXPORTS_COORDINATE, &exports_value)?;

    let configuration_value = causal_cell_target_configuration();
    let configuration_bytes = encode_canonical_cbor(&configuration_value)
        .map_err(|error| format!("encode causal.cell target configuration: {error}"))?;
    let configuration_digest =
        digest_value(CAUSAL_CELL_CONFIGURATION_COORDINATE, &configuration_value)?;

    let adapter_value = causal_cell_adapter(configuration_digest);
    let adapter_bytes = encode_canonical_cbor(&adapter_value)
        .map_err(|error| format!("encode causal.cell adapter: {error}"))?;
    let adapter_digest = digest_value(CAUSAL_CELL_ADAPTER_COORDINATE, &adapter_value)?;

    let manifest_value = causal_cell_manifest(exports_digest, adapter_digest);
    let manifest_bytes = encode_canonical_cbor(&manifest_value)
        .map_err(|error| format!("encode causal.cell manifest: {error}"))?;
    let bundle = decode_lawpack_bundle(&manifest_bytes, &exports_bytes)
        .map_err(|failures| format!("validate causal.cell lawpack: {failures:?}"))?;
    let adapter = decode_lawpack_adapter(&bundle, "echo.dpo@1", &adapter_bytes)
        .map_err(|failures| format!("validate causal.cell adapter: {failures:?}"))?;

    let source = causal_cell_application_source(&bundle.manifest_digest_review_string());
    let module = parse_module(&source)
        .map_err(|error| format!("parse causal.cell application witness: {error:?}"))?;
    let preparation = prepare_lawpack_compilation(&module, &bundle, &adapter)
        .map_err(|failures| format!("prepare causal.cell application witness: {failures:?}"))?;
    let core = compile_to_core(&module, preparation.compiler_context())
        .map_err(|error| format!("compile causal.cell application witness: {error:?}"))?;
    let target_ir = lower_to_target_ir(&core, preparation.target_ir_facts());
    if target_ir.status != TargetLoweringStatus::Lowered || target_ir.artifact.is_none() {
        return Err(format!(
            "lower causal.cell application witness: expected artifact, got {:?}",
            target_ir.failures
        ));
    }

    Ok(vec![
        (CAUSAL_CELL_MANIFEST_CBOR, manifest_bytes),
        (
            CAUSAL_CELL_MANIFEST_DIGEST,
            format!("{}\n", bundle.manifest_digest_review_string()).into_bytes(),
        ),
        (CAUSAL_CELL_EXPORTS_CBOR, exports_bytes),
        (
            CAUSAL_CELL_EXPORTS_DIGEST,
            format!("{}\n", bundle.manifest().exports.digest_review_string()).into_bytes(),
        ),
        (CAUSAL_CELL_ADAPTER_CBOR, adapter_bytes),
        (
            CAUSAL_CELL_ADAPTER_DIGEST,
            format!("{}\n", sha256_review_string(&adapter_digest)).into_bytes(),
        ),
        (CAUSAL_CELL_CONFIGURATION_CBOR, configuration_bytes),
        (
            CAUSAL_CELL_CONFIGURATION_DIGEST,
            format!("{}\n", sha256_review_string(&configuration_digest)).into_bytes(),
        ),
    ])
}

fn causal_cell_manifest(exports_digest: [u8; 32], adapter_digest: [u8; 32]) -> CanonicalValue {
    map([
        ("apiVersion", text("edict.lawpack/v1")),
        ("id", text("causal.cell")),
        ("version", text("1")),
        (
            "acceptedCoreAbi",
            CanonicalValue::Array(vec![text("edict.core/v1")]),
        ),
        ("dependencies", CanonicalValue::Array(Vec::new())),
        (
            "exports",
            resource_ref(CAUSAL_CELL_EXPORTS_COORDINATE, exports_digest),
        ),
        (
            "targetAdapters",
            CanonicalValue::Array(vec![map([
                (
                    "acceptedTargetProfile",
                    resource_ref("echo.dpo@1", ECHO_TARGET_PROFILE_DIGEST),
                ),
                (
                    "acceptedTargetIr",
                    resource_ref("echo.span-ir/v1", ECHO_TARGET_IR_DIGEST),
                ),
                (
                    "adapter",
                    resource_ref(CAUSAL_CELL_ADAPTER_COORDINATE, adapter_digest),
                ),
            ])]),
        ),
        (
            "verifier",
            map([
                ("class", text("declarative")),
                (
                    "ruleset",
                    resource_ref("causal.cell.verifier-rules/v1", [0x74; 32]),
                ),
            ]),
        ),
        (
            "compatibility",
            resource_ref("causal.cell.compatibility/v1", [0x75; 32]),
        ),
        (
            "conformanceFixtureCorpus",
            resource_ref("causal.cell.fixtures/v1", [0x76; 32]),
        ),
    ])
}

fn causal_cell_adapter(configuration_digest: [u8; 32]) -> CanonicalValue {
    map([
        ("apiVersion", text("edict.lawpack-adapter/v1")),
        ("class", text("declarative")),
        (
            "operationProfiles",
            map([(
                "causal.cell@1.createIfAbsent",
                map([
                    ("core", text("continuum.profile.create/v1")),
                    (
                        "semanticEffects",
                        CanonicalValue::Array(vec![text("causal.cell@1.createIfAbsent")]),
                    ),
                ]),
            )]),
        ),
        (
            "effectImplementations",
            map([(
                "causal.cell@1.createIfAbsent",
                map([
                    (
                        "targetIntrinsic",
                        text("echo.dpo@1.anchored-node-attachment-create-if-absent"),
                    ),
                    (
                        "targetConfiguration",
                        resource_ref(CAUSAL_CELL_CONFIGURATION_COORDINATE, configuration_digest),
                    ),
                    ("writeClass", text("create")),
                    (
                        "footprintObligation",
                        text("causal.cell@1.cellKeyFootprint"),
                    ),
                    ("costObligation", text("causal.cell@1.smallCreateBudget")),
                    (
                        "failureMappings",
                        map([(
                            "alreadyExists",
                            text("echo.executable-operation/precondition-mismatch/v1"),
                        )]),
                    ),
                ]),
            )]),
        ),
        (
            "budgets",
            map([(
                "causal.cell@1.smallCreateBudget",
                map([
                    ("maxSteps", CanonicalValue::Integer(16)),
                    ("maxAllocatedBytes", CanonicalValue::Integer(2_048)),
                    ("maxOutputBytes", CanonicalValue::Integer(2_048)),
                ]),
            )]),
        ),
    ])
}

fn causal_cell_target_configuration() -> CanonicalValue {
    map([
        (
            "apiVersion",
            text("echo.operation-lowering-configuration/v1"),
        ),
        (
            "programKind",
            text("anchored-node-attachment-create-if-absent/v1"),
        ),
        ("requiredNodeTypeProfile", text("causal.cell.node/value/v1")),
        (
            "requiredAttachmentTypeProfile",
            text("causal.cell.attachment/value/v1"),
        ),
        ("maxReplacementBytes", CanonicalValue::Integer(256)),
        (
            "authorityProfile",
            text("causal.cell.authority.application/v1"),
        ),
        (
            "budgetCeiling",
            map([
                ("steps", CanonicalValue::Integer(16)),
                ("readBytes", CanonicalValue::Integer(64)),
                ("writeBytes", CanonicalValue::Integer(320)),
            ]),
        ),
        (
            "invocationBinding",
            map([
                ("nodeKeyField", text("key")),
                ("replacementField", text("value")),
                ("nodeIdDerivation", text("sha256-utf8/v1")),
                ("warpIdSource", text("action-lane/v1")),
            ]),
        ),
    ])
}

fn causal_cell_exports() -> CanonicalValue {
    map([
        (
            "types",
            CanonicalValue::Array(vec![
                map([
                    ("coordinate", text("causal.cell@1.CreateInput")),
                    (
                        "definition",
                        text(
                            "Record<basis:String<max=128,canonical=raw-utf8>,key:String<max=64,canonical=raw-utf8>,value:String<max=256,canonical=raw-utf8>>",
                        ),
                    ),
                ]),
                map([
                    ("coordinate", text("causal.cell@1.CreateReceipt")),
                    (
                        "definition",
                        text("Record<key:String<max=64,canonical=raw-utf8>>"),
                    ),
                ]),
                map([
                    ("coordinate", text("causal.cell@1.ExistingValue")),
                    (
                        "definition",
                        text(
                            "Record<key:String<max=64,canonical=raw-utf8>,value:String<max=256,canonical=raw-utf8>>",
                        ),
                    ),
                ]),
            ]),
        ),
        ("constants", CanonicalValue::Array(Vec::new())),
        ("pureFunctions", CanonicalValue::Array(Vec::new())),
        (
            "effects",
            CanonicalValue::Array(vec![map([
                ("coordinate", text("causal.cell@1.createIfAbsent")),
                ("typeParameters", CanonicalValue::Array(Vec::new())),
                ("inputType", text("causal.cell@1.CreateInput")),
                ("outputType", text("causal.cell@1.CreateReceipt")),
                ("executionClass", text("runtime")),
                ("effectKindHint", text("create")),
                (
                    "footprintObligation",
                    text("causal.cell@1.cellKeyFootprint"),
                ),
                ("costObligation", text("causal.cell@1.smallCreateBudget")),
                (
                    "effectFailures",
                    map([(
                        "alreadyExists",
                        map([
                            ("authorityClass", text("domainMappable")),
                            ("payloadType", text("causal.cell@1.ExistingValue")),
                        ]),
                    )]),
                ),
                ("guardSupport", CanonicalValue::Bool(true)),
            ])]),
        ),
        (
            "obstructions",
            CanonicalValue::Array(vec![map([
                ("coordinate", text("causal.cell@1.AlreadyExists")),
                ("authorityClass", text("domainMappable")),
                ("payloadSchema", text("causal.cell@1.ExistingValue")),
            ])]),
        ),
        (
            "operationProfiles",
            map([(
                "causal.cell@1.createIfAbsent",
                map([
                    (
                        "opticTemplate",
                        map([
                            ("opticKind", text("affectReintegration")),
                            ("boundaryKind", text("affect")),
                            ("supportPolicy", text("causal.cell@1.directSupport")),
                            ("lossDisposition", text("causal.cell@1.lossless")),
                            (
                                "apertureRequirement",
                                map([
                                    ("kind", text("abstractFootprintObligation")),
                                    ("ref", text("causal.cell@1.cellKeyFootprint")),
                                ]),
                            ),
                        ]),
                    ),
                    (
                        "effectPredicate",
                        text("causal.cell@1.createIfAbsentEffect"),
                    ),
                ]),
            )]),
        ),
    ])
}

fn causal_cell_application_source(manifest_digest: &str) -> String {
    format!(
        r#"package examples.hello_echo@1;

use lawpack causal.cell@1 digest "{manifest_digest}" as cell;

type GreetingCreated = {{
  key: String<max=64>,
  message: String<max=256>,
}};

intent createGreeting(input: cell.CreateInput) returns GreetingCreated
  profile cell.createIfAbsent
  basis input.basis
  budget <= cell.smallCreateBudget
{{
  let receipt: cell.CreateReceipt = cell.createIfAbsent(input)
    else {{ alreadyExists(existing) => cell.AlreadyExists }};
  return {{
    key: receipt.key,
    message: input.value,
  }};
}}
"#
    )
}

fn hello_echo_source_for_bundle(
    root: &Path,
    bundle: &ValidatedLawpackBundle,
) -> Result<String, String> {
    let checked_in_source = fs::read_to_string(root.join(CREATE_GREETING_SOURCE))
        .map_err(|error| format!("read {CREATE_GREETING_SOURCE}: {error}"))?;
    let prior_manifest_digest = fs::read_to_string(root.join(MANIFEST_DIGEST))
        .map_err(|error| format!("read {MANIFEST_DIGEST}: {error}"))?;
    let prior_manifest_digest = prior_manifest_digest.trim();
    if checked_in_source.matches(prior_manifest_digest).count() != 1 {
        return Err(format!(
            "{CREATE_GREETING_SOURCE}: expected exactly one import pinned to {prior_manifest_digest}"
        ));
    }
    Ok(checked_in_source.replace(
        prior_manifest_digest,
        &bundle.manifest_digest_review_string(),
    ))
}

fn hello_echo_golden_artifacts(root: &Path) -> Result<Vec<(&'static str, Vec<u8>)>, String> {
    let exports_value = hello_echo_exports();
    let exports_bytes = encode_canonical_cbor(&exports_value)
        .map_err(|error| format!("encode Hello Echo exports: {error}"))?;
    let exports_digest = digest_value(EXPORTS_COORDINATE, &exports_value)?;
    let target_configuration_value = hello_echo_target_configuration();
    let target_configuration_bytes = encode_canonical_cbor(&target_configuration_value)
        .map_err(|error| format!("encode Hello Echo target configuration: {error}"))?;
    let target_configuration_digest =
        digest_value(TARGET_CONFIGURATION_COORDINATE, &target_configuration_value)?;
    let adapter_value = hello_echo_adapter(target_configuration_digest);
    let adapter_bytes = encode_canonical_cbor(&adapter_value)
        .map_err(|error| format!("encode Hello Echo adapter: {error}"))?;
    let adapter_digest = digest_value(ADAPTER_COORDINATE, &adapter_value)?;
    let manifest_value = hello_echo_manifest(exports_digest, adapter_digest);
    let manifest_bytes = encode_canonical_cbor(&manifest_value)
        .map_err(|error| format!("encode Hello Echo manifest: {error}"))?;
    let bundle = decode_lawpack_bundle(&manifest_bytes, &exports_bytes)
        .map_err(|failures| format!("validate Hello Echo lawpack: {failures:?}"))?;
    let adapter = decode_lawpack_adapter(&bundle, "echo.dpo@1", &adapter_bytes)
        .map_err(|failures| format!("validate Hello Echo adapter: {failures:?}"))?;
    let source = hello_echo_source_for_bundle(root, &bundle)?;
    let module = parse_module(&source)
        .map_err(|error| format!("parse {CREATE_GREETING_SOURCE}: {error:?}"))?;
    let preparation = prepare_lawpack_compilation(&module, &bundle, &adapter)
        .map_err(|failures| format!("prepare Hello Echo compilation: {failures:?}"))?;
    let core = compile_to_core(&module, preparation.compiler_context())
        .map_err(|error| format!("compile Hello Echo Core: {error:?}"))?;
    let core_bytes =
        encode_core_module(&core).map_err(|error| format!("encode Hello Echo Core: {error}"))?;
    let core_digest = format!(
        "{}\n",
        digest_core_module(&core)
            .map_err(|error| format!("digest Hello Echo Core: {error}"))?
            .to_review_string()
    );
    let target_ir_report = lower_to_target_ir(&core, preparation.target_ir_facts());
    if target_ir_report.status != TargetLoweringStatus::Lowered {
        return Err(format!(
            "lower Hello Echo Target IR: expected lowered status, got {:?}",
            target_ir_report.status
        ));
    }
    let result_projection = target_ir_report
        .result_projections
        .get("createGreeting")
        .cloned()
        .ok_or_else(|| {
            "lower Hello Echo Target IR: lowered report omitted result projection".to_owned()
        })?;
    let target_ir = target_ir_report
        .artifact
        .ok_or_else(|| "lower Hello Echo Target IR: lowered report omitted artifact".to_owned())?;
    let target_ir_bytes = encode_target_ir_artifact(&target_ir)
        .map_err(|error| format!("encode Hello Echo Target IR: {error}"))?;
    let target_ir_digest = format!(
        "{}\n",
        digest_target_ir_artifact(&target_ir)
            .map_err(|error| format!("digest Hello Echo Target IR: {error}"))?
            .to_review_string()
    );
    let result_projection_digest = format!("{}\n", result_projection.digest().to_review_string());
    let manifest_digest = format!("{}\n", bundle.manifest_digest_review_string());
    let exports_digest = format!("{}\n", bundle.manifest().exports.digest_review_string());
    let adapter_digest = format!("{}\n", sha256_review_string(&adapter_digest));
    let target_configuration_digest =
        format!("{}\n", sha256_review_string(&target_configuration_digest));

    Ok(vec![
        (MANIFEST_CBOR, manifest_bytes),
        (MANIFEST_DIGEST, manifest_digest.into_bytes()),
        (EXPORTS_CBOR, exports_bytes),
        (EXPORTS_DIGEST, exports_digest.into_bytes()),
        (ADAPTER_CBOR, adapter_bytes),
        (ADAPTER_DIGEST, adapter_digest.into_bytes()),
        (TARGET_CONFIGURATION_CBOR, target_configuration_bytes),
        (
            TARGET_CONFIGURATION_DIGEST,
            target_configuration_digest.into_bytes(),
        ),
        (CREATE_GREETING_SOURCE, source.into_bytes()),
        (CREATE_GREETING_CORE_CBOR, core_bytes),
        (CREATE_GREETING_CORE_DIGEST, core_digest.into_bytes()),
        (CREATE_GREETING_TARGET_IR_CBOR, target_ir_bytes),
        (
            CREATE_GREETING_TARGET_IR_DIGEST,
            target_ir_digest.into_bytes(),
        ),
        (
            CREATE_GREETING_RESULT_PROJECTION_CBOR,
            result_projection.canonical_bytes().to_vec(),
        ),
        (
            CREATE_GREETING_RESULT_PROJECTION_DIGEST,
            result_projection_digest.into_bytes(),
        ),
    ])
}

fn hello_echo_manifest(exports_digest: [u8; 32], adapter_digest: [u8; 32]) -> CanonicalValue {
    map([
        ("apiVersion", text("edict.lawpack/v1")),
        ("id", text("hello.echo")),
        ("version", text("1")),
        (
            "acceptedCoreAbi",
            CanonicalValue::Array(vec![text("edict.core/v1")]),
        ),
        ("dependencies", CanonicalValue::Array(Vec::new())),
        ("exports", resource_ref(EXPORTS_COORDINATE, exports_digest)),
        (
            "targetAdapters",
            CanonicalValue::Array(vec![map([
                (
                    "acceptedTargetProfile",
                    resource_ref("echo.dpo@1", ECHO_TARGET_PROFILE_DIGEST),
                ),
                (
                    "acceptedTargetIr",
                    resource_ref("echo.span-ir/v1", ECHO_TARGET_IR_DIGEST),
                ),
                ("adapter", resource_ref(ADAPTER_COORDINATE, adapter_digest)),
            ])]),
        ),
        (
            "verifier",
            map([
                ("class", text("declarative")),
                (
                    "ruleset",
                    resource_ref("hello.echo.verifier-rules/v1", [0x44; 32]),
                ),
            ]),
        ),
        (
            "compatibility",
            resource_ref("hello.echo.compatibility/v1", [0x55; 32]),
        ),
        (
            "conformanceFixtureCorpus",
            resource_ref("hello.echo.fixtures/v1", [0x66; 32]),
        ),
    ])
}

fn hello_echo_adapter(target_configuration_digest: [u8; 32]) -> CanonicalValue {
    map([
        ("apiVersion", text("edict.lawpack-adapter/v1")),
        ("class", text("declarative")),
        (
            "operationProfiles",
            map([(
                "hello.echo@1.createGreeting",
                map([
                    ("core", text("continuum.profile.create/v1")),
                    (
                        "semanticEffects",
                        CanonicalValue::Array(vec![text("hello.echo@1.createGreeting")]),
                    ),
                ]),
            )]),
        ),
        (
            "effectImplementations",
            map([(
                "hello.echo@1.createGreeting",
                map([
                    (
                        "targetIntrinsic",
                        text("echo.dpo@1.anchored-node-attachment-create-if-absent"),
                    ),
                    (
                        "targetConfiguration",
                        resource_ref(TARGET_CONFIGURATION_COORDINATE, target_configuration_digest),
                    ),
                    ("writeClass", text("create")),
                    (
                        "footprintObligation",
                        text("hello.echo@1.greetingKeyFootprint"),
                    ),
                    ("costObligation", text("hello.echo@1.smallCreateBudget")),
                    (
                        "failureMappings",
                        map([(
                            "alreadyExists",
                            text("echo.executable-operation/precondition-mismatch/v1"),
                        )]),
                    ),
                ]),
            )]),
        ),
        (
            "budgets",
            map([(
                "hello.echo@1.smallCreateBudget",
                map([
                    ("maxSteps", CanonicalValue::Integer(16)),
                    ("maxAllocatedBytes", CanonicalValue::Integer(2_048)),
                    ("maxOutputBytes", CanonicalValue::Integer(2_048)),
                ]),
            )]),
        ),
    ])
}

fn hello_echo_target_configuration() -> CanonicalValue {
    map([
        (
            "apiVersion",
            text("echo.operation-lowering-configuration/v1"),
        ),
        (
            "programKind",
            text("anchored-node-attachment-create-if-absent/v1"),
        ),
        (
            "requiredNodeTypeProfile",
            text("hello.echo.node.greeting/v1"),
        ),
        (
            "requiredAttachmentTypeProfile",
            text("hello.echo.attachment.greeting-message/v1"),
        ),
        ("maxReplacementBytes", CanonicalValue::Integer(256)),
        (
            "authorityProfile",
            text("hello.echo.authority.local-demo/v1"),
        ),
        (
            "budgetCeiling",
            map([
                ("steps", CanonicalValue::Integer(16)),
                ("readBytes", CanonicalValue::Integer(64)),
                ("writeBytes", CanonicalValue::Integer(320)),
            ]),
        ),
        (
            "invocationBinding",
            map([
                ("nodeKeyField", text("key")),
                ("replacementField", text("message")),
                ("nodeIdDerivation", text("sha256-utf8/v1")),
                ("warpIdSource", text("action-lane/v1")),
            ]),
        ),
    ])
}

fn hello_echo_exports() -> CanonicalValue {
    map([
        (
            "types",
            CanonicalValue::Array(vec![
                map([
                    ("coordinate", text("hello.echo@1.CreateGreetingInput")),
                    (
                        "definition",
                        text(
                            "Record<basis:String<max=128,canonical=raw-utf8>,key:String<max=64,canonical=raw-utf8>,message:String<max=256,canonical=raw-utf8>>",
                        ),
                    ),
                ]),
                map([
                    ("coordinate", text("hello.echo@1.GreetingReceipt")),
                    (
                        "definition",
                        text("Record<key:String<max=64,canonical=raw-utf8>>"),
                    ),
                ]),
                map([
                    ("coordinate", text("hello.echo@1.ExistingGreeting")),
                    (
                        "definition",
                        text(
                            "Record<key:String<max=64,canonical=raw-utf8>,message:String<max=256,canonical=raw-utf8>>",
                        ),
                    ),
                ]),
            ]),
        ),
        ("constants", CanonicalValue::Array(Vec::new())),
        ("pureFunctions", CanonicalValue::Array(Vec::new())),
        (
            "effects",
            CanonicalValue::Array(vec![map([
                ("coordinate", text("hello.echo@1.createGreeting")),
                ("typeParameters", CanonicalValue::Array(Vec::new())),
                ("inputType", text("hello.echo@1.CreateGreetingInput")),
                ("outputType", text("hello.echo@1.GreetingReceipt")),
                ("executionClass", text("runtime")),
                ("effectKindHint", text("create")),
                (
                    "footprintObligation",
                    text("hello.echo@1.greetingKeyFootprint"),
                ),
                ("costObligation", text("hello.echo@1.smallCreateBudget")),
                (
                    "effectFailures",
                    map([(
                        "alreadyExists",
                        map([
                            ("authorityClass", text("domainMappable")),
                            ("payloadType", text("hello.echo@1.ExistingGreeting")),
                        ]),
                    )]),
                ),
                ("guardSupport", CanonicalValue::Bool(true)),
            ])]),
        ),
        (
            "obstructions",
            CanonicalValue::Array(vec![map([
                ("coordinate", text("hello.echo@1.AlreadyExists")),
                ("authorityClass", text("domainMappable")),
                ("payloadSchema", text("hello.echo@1.ExistingGreeting")),
            ])]),
        ),
        (
            "operationProfiles",
            map([(
                "hello.echo@1.createGreeting",
                map([
                    (
                        "opticTemplate",
                        map([
                            ("opticKind", text("affectReintegration")),
                            ("boundaryKind", text("affect")),
                            ("supportPolicy", text("hello.echo@1.directSupport")),
                            ("lossDisposition", text("hello.echo@1.lossless")),
                            (
                                "apertureRequirement",
                                map([
                                    ("kind", text("abstractFootprintObligation")),
                                    ("ref", text("hello.echo@1.greetingKeyFootprint")),
                                ]),
                            ),
                        ]),
                    ),
                    ("effectPredicate", text("hello.echo@1.createGreetingEffect")),
                ]),
            )]),
        ),
    ])
}

fn external_action_resource(
    coordinate: &str,
    kind: &str,
    definition: CanonicalValue,
) -> Result<GeneratedExternalActionResource, String> {
    let value = map([
        ("apiVersion", text(EXTERNAL_ACTION_RESOURCE_API_VERSION)),
        ("coordinate", text(coordinate)),
        ("kind", text(kind)),
        ("definition", definition),
    ]);
    let bytes = encode_canonical_cbor(&value)
        .map_err(|error| format!("encode external-action resource `{coordinate}`: {error}"))?;
    let digest = digest_canonical_artifact(EXTERNAL_ACTION_RESOURCE_DIGEST_DOMAIN, &bytes)
        .map_err(|error| format!("digest external-action resource `{coordinate}`: {error}"))?
        .to_review_string();
    Ok(GeneratedExternalActionResource {
        coordinate: coordinate.to_owned(),
        bytes,
        digest,
    })
}

fn compile_external_action_application(
    source: &str,
    bundle: &ValidatedLawpackBundle,
    adapter: &ValidatedLawpackAdapter,
    label: &str,
) -> Result<CompiledExternalActionArtifacts, String> {
    let module =
        parse_module(source).map_err(|error| format!("parse {label} application: {error:?}"))?;
    let preparation = prepare_lawpack_compilation(&module, bundle, adapter)
        .map_err(|failures| format!("prepare {label} application: {failures:?}"))?;
    let core = compile_to_core(&module, preparation.compiler_context())
        .map_err(|error| format!("compile {label} application: {error:?}"))?;
    let core_bytes =
        encode_core_module(&core).map_err(|error| format!("encode {label} Core: {error}"))?;
    let core_digest = digest_core_module(&core)
        .map_err(|error| format!("digest {label} Core: {error}"))?
        .to_review_string();
    let target_ir_report = lower_to_target_ir(&core, preparation.target_ir_facts());
    if target_ir_report.status != TargetLoweringStatus::Lowered {
        return Err(format!(
            "lower {label} Target IR: expected lowered status, got {:?}: {:?}",
            target_ir_report.status, target_ir_report.failures
        ));
    }
    let target_ir = target_ir_report
        .artifact
        .ok_or_else(|| format!("lower {label} Target IR: lowered report omitted artifact"))?;
    let request_count = target_ir
        .intents
        .values()
        .map(|intent| intent.external_action_requests.len())
        .sum::<usize>();
    if request_count != 1
        || target_ir
            .intents
            .values()
            .any(|intent| !intent.steps.is_empty())
    {
        return Err(format!(
            "{label} application must lower to one request and zero callable steps"
        ));
    }
    let target_ir_bytes = encode_target_ir_artifact(&target_ir)
        .map_err(|error| format!("encode {label} Target IR: {error}"))?;
    let target_ir_digest = digest_target_ir_artifact(&target_ir)
        .map_err(|error| format!("digest {label} Target IR: {error}"))?
        .to_review_string();
    Ok(CompiledExternalActionArtifacts {
        core_bytes,
        core_digest,
        target_ir_bytes,
        target_ir_digest,
    })
}

fn workspace_snapshot_input_schema() -> CanonicalValue {
    schema_definition(
        "boundedWorkspaceObservationInput",
        vec![
            schema_field(
                "kind",
                "literal:boundedWorkspaceObservationInput",
                "exact operation discriminator",
            ),
            schema_field(
                "paths",
                "array<canonical-relative-path>",
                "ordered exact read aperture",
            ),
        ],
    )
}

fn workspace_snapshot_settlement_schema() -> CanonicalValue {
    schema_definition(
        "boundedWorkspaceObservationSettlement",
        vec![
            schema_field(
                "kind",
                "literal:boundedWorkspaceObservationSettlement",
                "exact settlement discriminator",
            ),
            schema_field(
                "posture",
                "enum:succeeded|obstructed|outcomeUnknown",
                "terminal external-action posture",
            ),
            schema_field("basis", "bytes<exact=32>", "observed workspace root"),
            schema_field(
                "evidence",
                "bytes<exact=32>",
                "domain-separated observation evidence",
            ),
            schema_field(
                "files",
                "array<workspaceFile{path:text,bytes:bytes,digest:bytes32}>",
                "strictly ordered requested file observations",
            ),
            schema_field(
                "obstruction",
                "optional<text>",
                "typed obstruction or outcome-unknown code",
            ),
        ],
    )
}

fn workspace_snapshot_reconciliation_law() -> CanonicalValue {
    reconciliation_definition(
        "boundedWorkspaceObservationInput",
        "boundedWorkspaceObservationSettlement",
        &["basis", "evidence", "files", "obstruction"],
        "replay consumes the admitted settlement and performs no workspace read",
    )
}

fn workspace_patch_input_schema() -> CanonicalValue {
    schema_definition(
        "validatedWorkspacePatchInput",
        vec![
            schema_field(
                "kind",
                "literal:validatedWorkspacePatchInput",
                "exact operation discriminator",
            ),
            schema_field(
                "path",
                "canonical-relative-path",
                "single writable aperture",
            ),
            schema_field(
                "expectedContentDigest",
                "bytes<exact=32>",
                "basis-bound precondition",
            ),
            schema_field(
                "replacement",
                "bytes<max=65536>",
                "validated replacement bytes",
            ),
            schema_field(
                "replacementDigest",
                "bytes<exact=32>",
                "replacement identity",
            ),
        ],
    )
}

fn workspace_patch_settlement_schema() -> CanonicalValue {
    schema_definition(
        "validatedWorkspacePatchSettlement",
        vec![
            schema_field(
                "kind",
                "literal:validatedWorkspacePatchSettlement",
                "exact settlement discriminator",
            ),
            schema_field(
                "posture",
                "enum:succeeded|obstructed|outcomeUnknown",
                "terminal external-action posture",
            ),
            schema_field(
                "path",
                "optional<canonical-relative-path>",
                "settled aperture",
            ),
            schema_field(
                "requestBasis",
                "bytes<exact=32>",
                "admitted workspace basis",
            ),
            schema_field(
                "evidence",
                "bytes<exact=32>",
                "domain-separated settlement evidence",
            ),
            schema_field(
                "beforeContentDigest",
                "optional<bytes<exact=32>>",
                "observed pre-mutation content",
            ),
            schema_field(
                "afterContentDigest",
                "optional<bytes<exact=32>>",
                "observed postcondition content",
            ),
            schema_field(
                "resultingBasis",
                "optional<bytes<exact=32>>",
                "observed postcondition workspace root",
            ),
            schema_field(
                "obstruction",
                "optional<text>",
                "typed obstruction or outcome-unknown code",
            ),
        ],
    )
}

fn workspace_patch_reconciliation_law() -> CanonicalValue {
    reconciliation_definition(
        "validatedWorkspacePatchInput",
        "validatedWorkspacePatchSettlement",
        &[
            "path",
            "requestBasis",
            "evidence",
            "beforeContentDigest",
            "afterContentDigest",
            "resultingBasis",
            "obstruction",
        ],
        "replay consumes the admitted settlement and never reapplies the patch",
    )
}

fn schema_definition(root: &str, fields: Vec<CanonicalValue>) -> CanonicalValue {
    map([
        ("encoding", text("canonical-cbor")),
        ("root", text(root)),
        ("closed", CanonicalValue::Bool(true)),
        ("fields", CanonicalValue::Array(fields)),
    ])
}

fn schema_field(name: &str, field_type: &str, authority: &str) -> CanonicalValue {
    map([
        ("name", text(name)),
        ("type", text(field_type)),
        ("required", CanonicalValue::Bool(true)),
        ("authority", text(authority)),
    ])
}

fn reconciliation_definition(
    request_kind: &str,
    settlement_kind: &str,
    bindings: &[&str],
    replay_rule: &str,
) -> CanonicalValue {
    map([
        ("requestKind", text(request_kind)),
        ("settlementKind", text(settlement_kind)),
        (
            "terminalPostures",
            CanonicalValue::Array(vec![
                text("succeeded"),
                text("obstructed"),
                text("outcomeUnknown"),
            ]),
        ),
        (
            "requiredBindings",
            CanonicalValue::Array(bindings.iter().map(|binding| text(binding)).collect()),
        ),
        ("replayRule", text(replay_rule)),
    ])
}

fn digest_value(domain: &str, value: &CanonicalValue) -> Result<[u8; 32], String> {
    let framed = CanonicalValue::Array(vec![text(DIGEST_FRAME), text(domain), value.clone()]);
    let bytes = encode_canonical_cbor(&framed)
        .map_err(|error| format!("encode lawpack digest frame: {error}"))?;
    Ok(Sha256::digest(bytes).into())
}

fn sha256_review_string(digest: &[u8; 32]) -> String {
    let mut review = String::with_capacity(71);
    review.push_str("sha256:");
    for byte in digest {
        write!(&mut review, "{byte:02x}").expect("writing to a String cannot fail");
    }
    review
}

fn resource_ref(id: &str, digest: [u8; 32]) -> CanonicalValue {
    map([
        ("id", text(id)),
        (
            "digest",
            CanonicalValue::Array(vec![text("sha256"), CanonicalValue::Bytes(digest.to_vec())]),
        ),
    ])
}

fn map<const N: usize>(entries: [(&str, CanonicalValue); N]) -> CanonicalValue {
    CanonicalValue::Map(
        entries
            .into_iter()
            .map(|(key, value)| (text(key), value))
            .collect(),
    )
}

fn text(value: &str) -> CanonicalValue {
    CanonicalValue::Text(value.to_owned())
}

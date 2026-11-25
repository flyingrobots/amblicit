//! An application selects the shared-read Core cost schedule through its
//! `edict.application/v1` config (#330).
//!
//! The application calls an authored lawpack helper inside a bounded fold.
//! Amblicit carries no `echo.dpo@2` provider components, so, as in
//! `echo_dpo_2_build.rs`, the public build cannot publish a package: the
//! oracle is the first failing stage. Under the copying schedule the declared
//! budget is too small and compilation fails; under the shared-read schedule
//! compilation and Target lowering pass and the build stops at the provider.

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
    decode_canonical_cbor, digest_canonical_artifact, digest_target_intrinsics_catalog,
    encode_canonical_cbor, CanonicalValue, TARGET_PROFILE_API_VERSION, TARGET_PROVIDER_ABI,
    TARGET_PROVIDER_MANIFEST_API_VERSION,
};
use limits::Limits;
use serde_json::{json, Value};

const ECHO_DPO_2: &str = "echo.dpo@2";
const ECHO_DPO_1_PROFILE: &[u8] = include_bytes!(
    "../../../fixtures/providers/echo-target-profile/generated/primary/target-profile.echo-dpo.cbor"
);
const LIMITS: Limits = Limits {
    key_scalars: 64,
    value_scalars: 64,
    replacement_bytes: 256,
};
/// Echo's `/v3` catalog (`fixtures/echo-target-intrinsics-v3`), the base of
/// the catalog the retargeted profile pins.
const ECHO_V3_CATALOG: &[u8] =
    include_bytes!("../../../fixtures/echo-target-intrinsics-v3/resource.target-intrinsics-3.cbor");
const CATALOG_COORDINATE: &str = "echo.dpo.intrinsics/v3";
const CATALOG_PATH: &str = "generated/resources/resource.target-intrinsics-3.cbor";
/// The effect intrinsic the causal cell lowers to.
const CELL_EFFECT: &str = "echo.dpo@2.anchored-node-attachment-create-if-absent";
/// Declared operation steps: above the shared-read minimum, below the copying one.
const OPERATION_STEPS: u64 = 400_000;
/// Declared operation allocation, likewise between the two minimums.
const OPERATION_BYTES: u64 = 4_000_000;

/// How the application declares the catalog its profile pins.
#[derive(Clone, Copy)]
enum CatalogDeclaration {
    /// No `target.pureIntrinsics`.
    Undeclared,
    /// `target.pureIntrinsics` with the catalog location and no requirement.
    CatalogOnly,
    /// The catalog location and one pure intrinsic the program never calls,
    /// the form Echo's `effect_v2_admission` example uses.
    WithUncalledRequirement,
}

/// Echo's `/v3` catalog plus a template for the causal cell's effect, whose
/// steps are `effect_steps`, and its canonical bytes and digest.
fn catalog(effect_steps: u64) -> (Vec<u8>, Vec<u8>) {
    let mut document = decode_canonical_cbor(ECHO_V3_CATALOG).expect("vendored catalog");
    let CanonicalValue::Map(fields) = &mut document else {
        panic!("catalog map")
    };
    let (_, CanonicalValue::Map(intrinsics)) = fields
        .iter_mut()
        .find(|(key, _)| key == &CanonicalValue::Text("intrinsics".into()))
        .expect("intrinsics")
    else {
        panic!("intrinsics map")
    };
    let (_, create) = intrinsics
        .iter()
        .find(|(key, _)| key == &CanonicalValue::Text("echo.dpo@2.create-node".into()))
        .expect("create-node entry")
        .clone();
    let mut entry = create;
    let CanonicalValue::Map(entry_fields) = &mut entry else {
        panic!("entry map")
    };
    let constant = |value: u64| {
        CanonicalValue::Map(vec![(
            CanonicalValue::Text("const".into()),
            CanonicalValue::Integer(i128::from(value)),
        )])
    };
    for (key, value) in entry_fields.iter_mut() {
        if key == &CanonicalValue::Text("effectCostRule".into()) {
            *value = CanonicalValue::Map(vec![
                (CanonicalValue::Text("bytes".into()), constant(1)),
                (CanonicalValue::Text("steps".into()), constant(effect_steps)),
            ]);
        }
    }
    intrinsics.push((CanonicalValue::Text(CELL_EFFECT.into()), entry));
    let bytes = encode_canonical_cbor(&document).expect("encode catalog");
    let rendered =
        digest_target_intrinsics_catalog(CATALOG_COORDINATE, &bytes).expect("canonical catalog");
    let hex = rendered.strip_prefix("sha256:").expect("sha256 digest");
    let digest = (0..hex.len())
        .step_by(2)
        .map(|at| u8::from_str_radix(&hex[at..at + 2], 16).expect("hex digest"))
        .collect();
    (bytes, digest)
}

fn echo_dpo_2_profile(catalog_digest: &[u8]) -> (Vec<u8>, String) {
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
    let CanonicalValue::Map(fields) = &mut profile else {
        panic!("profile map")
    };
    let reference = fields
        .iter_mut()
        .find(|(key, _)| key == &CanonicalValue::Text("intrinsics".into()))
        .expect("profile intrinsics reference");
    reference.1 = CanonicalValue::Map(vec![
        (
            CanonicalValue::Text("id".into()),
            CanonicalValue::Text(CATALOG_COORDINATE.into()),
        ),
        (
            CanonicalValue::Text("digest".into()),
            CanonicalValue::Array(vec![
                CanonicalValue::Text("sha256".into()),
                CanonicalValue::Bytes(catalog_digest.to_vec()),
            ]),
        ),
    ]);
    let bytes = encode_canonical_cbor(&profile).expect("encode echo.dpo@2 profile");
    let digest = digest_canonical_artifact(TARGET_PROFILE_API_VERSION, &bytes)
        .expect("canonical profile")
        .to_review_string();
    (bytes, digest)
}

/// The causal-cell lawpack on `echo.dpo@2`, with an authored constant helper
/// and a larger operation budget.
fn lawpack_document(profile_digest: &str) -> Value {
    let mut document = document::document(LIMITS).expect("example document");
    document["lawpack"]["exports"]["pureFunctions"] = json!([{
        "source": "edict",
        "coordinate": "example.cell@1.weight",
        "returnType": "U64",
        "costTemplate": "example.cell@1.weightBudget",
        "determinismClass": "total",
        "body": {"params": [], "body": {
            "locals": [], "bindings": [],
            "result": {"kind": "const", "value": {"kind": "int", "width": "U64", "value": 7}}
        }}
    }]);
    let adapter = &mut document["lawpack"]["targetAdapters"][0];
    adapter["acceptedTargetProfile"] = json!({"id": ECHO_DPO_2, "digest": profile_digest});
    adapter["acceptedTargetIr"] =
        json!({"id": "echo.span-ir/v3", "digest": format!("sha256:{}", "2f".repeat(32))});
    adapter["effectImplementations"]["example.cell@1.createIfAbsent"]["targetIntrinsic"] =
        json!("echo.dpo@2.anchored-node-attachment-create-if-absent");
    adapter["budgets"]["example.cell@1.createBudget"]["maxSteps"] = json!(OPERATION_STEPS);
    adapter["budgets"]["example.cell@1.createBudget"]["maxAllocatedBytes"] = json!(OPERATION_BYTES);
    adapter["budgets"]["example.cell@1.createBudget"]["maxOutputBytes"] = json!(65_536);
    adapter["budgets"]["example.cell@1.weightBudget"] =
        json!({"maxSteps": 1, "maxAllocatedBytes": 0, "maxOutputBytes": 0});
    document
}

/// The example source, plus a fold whose state carries a 64-item list and
/// whose step calls the lawpack helper.
fn source(lawpack_digest: &str) -> String {
    let base = command::render(&["source", "64", "64", "256", lawpack_digest]).expect("source");
    let declarations = "type Item = { key: U64, weight: U64 };
type Tally = { items: List<Item, max=64>, i: U64 };
fn tallyStep(s: Tally) -> Tally {
  return if listLen(s.items) < 64u64 && s.i < 64u64
    then { items: listAppend(s.items, { key: s.i, weight: cell.weight() }), i: s.i + 1u64 }
    else s;
}

intent create";
    let tally = "{\n  let empty: Tally = { items: [], i: 0u64 };\n  let tally = fold(empty, tallyStep, 64u64);\n  let receipt";
    let with_declarations = base.replacen("intent create", declarations, 1);
    assert_ne!(with_declarations, base);
    let with_fold = with_declarations.replacen("{\n  let receipt", tally, 1);
    assert_ne!(with_fold, with_declarations);
    with_fold
}

fn write_application(
    root: &Path,
    schedule: Option<&str>,
    declaration: CatalogDeclaration,
    effect_steps: u64,
) {
    let (catalog_bytes, catalog_digest) = catalog(effect_steps);
    let (profile_bytes, profile_digest) = echo_dpo_2_profile(&catalog_digest);
    fs::create_dir_all(root.join("provider/generated/resources")).expect("catalog directory");
    fs::write(root.join("provider").join(CATALOG_PATH), catalog_bytes).expect("write catalog");
    fs::create_dir_all(root.join("src")).expect("source directory");
    fs::create_dir_all(root.join("provider/generated/primary")).expect("provider directory");
    fs::write(
        root.join("edict.lawpack.json"),
        serde_json::to_vec(&lawpack_document(&profile_digest)).expect("encode"),
    )
    .expect("write lawpack document");
    let authored = run_public_build(root, "lawpack", "edict.lawpack.json");
    assert_eq!(authored.status.code(), Some(0), "{authored:?}");
    let lawpack_digest =
        fs::read_to_string(root.join("vendor/cell/manifest.sha256")).expect("lawpack digest");
    fs::write(
        root.join("src/create.amblicit"),
        source(lawpack_digest.trim()),
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
            {"role": "target-profile.echo-dpo-2", "artifactKind": "targetProfile",
             "resource": {"coordinate": ECHO_DPO_2, "digest": profile_digest}, "source": generated.clone()},
            {"role": "schema.echo-provider-artifacts", "artifactKind": "artifactSchema",
             "resource": {"coordinate": "echo.provider-artifacts.cddl@1", "digest": format!("sha256:{}", "4".repeat(64))},
             "source": generated}
        ],
        "schemaBindings": [{"domain": "echo.generated-artifact/v1",
            "schemaRole": "schema.echo-provider-artifacts", "format": "selfContainedCddlV1",
            "rootRule": "generated-artifact"}]
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
    if let Some(schedule) = schedule {
        application["coreCostSchedule"] = json!(schedule);
    }
    match declaration {
        CatalogDeclaration::Undeclared => {}
        CatalogDeclaration::CatalogOnly => {
            application["target"]["pureIntrinsics"] = json!({"catalog": CATALOG_PATH});
        }
        CatalogDeclaration::WithUncalledRequirement => {
            application["target"]["pureIntrinsics"] = json!({
                "catalog": CATALOG_PATH,
                "required": [{"coordinate": "echo.dpo@2.slotEdgeId",
                              "contractVersion": "echo.content-address/v1"}]
            });
        }
    }
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

/// Diagnostic kinds of one application build under `schedule`.
fn first_failure(schedule: Option<&str>) -> Vec<String> {
    failures(schedule, CatalogDeclaration::CatalogOnly, 1)
        .into_iter()
        .map(|(kind, _)| kind)
        .collect()
}

/// Diagnostic (kind, message) pairs of one failed application build.
fn failures(
    schedule: Option<&str>,
    declaration: CatalogDeclaration,
    effect_steps: u64,
) -> Vec<(String, String)> {
    let directory = tempfile::Builder::new()
        .prefix("amblicit-cost-schedule-")
        .tempdir()
        .expect("owned temp directory");
    let root = directory.path();
    write_application(root, schedule, declaration, effect_steps);
    let built = run_public_build(root, "application", "edict.application.json");
    assert_eq!(built.status.code(), Some(2), "{built:?}");
    assert!(!root.join(".build/application").exists());
    std::str::from_utf8(&built.stderr)
        .expect("UTF-8 JSONL")
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).expect("JSONL record"))
        .filter(|record| record["schema"] == "edict.cli.diagnostic/v1")
        .map(|record| {
            (
                record["kind"].as_str().unwrap_or_default().to_owned(),
                record["message"].as_str().unwrap_or_default().to_owned(),
            )
        })
        .collect()
}

/// A template this large exceeds the declared budget on its own.
const OVER_BUDGET_TEMPLATE_STEPS: u64 = OPERATION_STEPS + 1;

#[test]
fn shared_read_build_charges_the_pinned_catalog_template_without_pure_requirements() {
    // Oracle: the template alone exceeds the budget, so a build that applies
    // it is refused at compilation; the generic charge passes compilation.
    let kinds = failures(
        Some("edict.core-cost/v2"),
        CatalogDeclaration::CatalogOnly,
        OVER_BUDGET_TEMPLATE_STEPS,
    );
    assert!(
        kinds.iter().any(|(kind, _)| kind == "InvalidBound"),
        "{kinds:?}"
    );
}

#[test]
fn shared_read_build_refuses_an_undeclared_pinned_catalog_instead_of_the_generic_charge() {
    let kinds = failures(
        Some("edict.core-cost/v2"),
        CatalogDeclaration::Undeclared,
        OVER_BUDGET_TEMPLATE_STEPS,
    );
    assert_eq!(kinds.len(), 1, "{kinds:?}");
    assert_eq!(kinds[0].0, "TargetIntrinsicCatalogUndeclared", "{kinds:?}");
    assert!(
        kinds[0].1.contains("target.pureIntrinsics.catalog"),
        "{kinds:?}"
    );
    assert!(kinds[0].1.contains(CATALOG_COORDINATE), "{kinds:?}");
}

#[test]
fn an_uncalled_pure_requirement_does_not_change_the_effect_charge() {
    // Echo's example names an uncalled intrinsic to reach the templates; the
    // catalog-only form must report the identical refusal and minimum.
    let schedule = Some("edict.core-cost/v2");
    let catalog_only = failures(
        schedule,
        CatalogDeclaration::CatalogOnly,
        OVER_BUDGET_TEMPLATE_STEPS,
    );
    let with_requirement = failures(
        schedule,
        CatalogDeclaration::WithUncalledRequirement,
        OVER_BUDGET_TEMPLATE_STEPS,
    );
    assert!(
        catalog_only.iter().any(|(kind, _)| kind == "InvalidBound"),
        "{catalog_only:?}"
    );
    assert_eq!(catalog_only, with_requirement);
}

#[test]
fn shared_read_selection_admits_a_helper_calling_fold_past_target_lowering() {
    assert_eq!(
        first_failure(Some("edict.core-cost/v2")),
        ["ProviderPackageReadFailed"]
    );
}

#[test]
fn the_same_application_exceeds_its_budget_under_the_copying_schedule() {
    let kinds = first_failure(None);
    assert_eq!(
        kinds.last().map(String::as_str),
        Some("ApplicationCompilationFailed"),
        "{kinds:?}"
    );
    assert!(kinds.iter().any(|kind| kind == "InvalidBound"), "{kinds:?}");
}

//! File-backed authority-facts loading behavior.
//!
//! These tests assert compiler behavior and stable load failure kinds. They do
//! not assert documentation text, repository layout, or implementation details.

use std::fs;
use std::path::{Path, PathBuf};

use edict_syntax::{
    compile_to_core, compiler_context_from_authority_facts,
    load_compiler_context_from_authority_fact_files, AuthorityFactSource, AuthorityFactSourceKind,
    AuthorityFactsDocument, AuthorityFactsLoadFailureKind, BudgetFact, CompilerErrorKind,
    CompilerStage, CoreBudget, EffectWriteClassFact, OperationProfileFact, WriteClass,
};

const BOUNDED_HELLO: &str = include_str!("../../../fixtures/lang/bounds/bounded-hello.edict");

#[test]
fn file_backed_authority_facts_compile_bounded_hello() {
    let dir = temp_case_dir("compile-bounded-hello");
    let target = write_json(
        &dir,
        "target-profile-facts.json",
        target_profile_facts("read"),
    );
    let lawpack = write_json(&dir, "lawpack-facts.json", lawpack_budget_facts());
    let context =
        load_compiler_context_from_authority_fact_files([target.as_path(), lawpack.as_path()])
            .expect("authority facts load");
    let module = edict_syntax::parse_module(BOUNDED_HELLO).expect("fixture parses");
    let core = compile_to_core(&module, &context).expect("loaded facts compile fixture");
    let intent = core.intents.get("sayHello").expect("compiled intent");

    assert_eq!(
        intent.required_operation_profile,
        "continuum.profile.read-only/v1"
    );
    assert_eq!(intent.core_evaluation_budget.max_steps, 64);
    assert_eq!(intent.core_evaluation_budget.max_allocated_bytes, 4096);
    assert_eq!(intent.core_evaluation_budget.max_output_bytes, 4096);
}

#[test]
fn file_backed_authority_facts_reject_write_effect_profile_mismatch() {
    let dir = temp_case_dir("profile-effect-mismatch");
    let target = write_json(
        &dir,
        "target-profile-facts.json",
        target_profile_facts("read"),
    );
    let lawpack = write_json(&dir, "lawpack-facts.json", lawpack_effect_facts("replace"));
    let context =
        load_compiler_context_from_authority_fact_files([target.as_path(), lawpack.as_path()])
            .expect("authority facts load");
    let module = edict_syntax::parse_module(
        "package a.b@1;\n\
         type Input = { id: String<max=16>, };\n\
         type Output = { id: String<max=16>, };\n\
         intent t(input: Input) returns Output\n\
           profile p.readOnly\n\
           basis none\n\
           budget <= p.tiny {\n\
           target.replace(input.id) else domain.WriteRejected;\n\
           return { id: input.id };\n\
         }",
    )
    .expect("source parses");

    let errors = compile_to_core(&module, &context)
        .expect_err("loaded write-class facts reject incompatible effect");

    assert!(errors
        .iter()
        .all(|err| err.stage == CompilerStage::TypeCheck));
    assert_eq!(
        errors
            .iter()
            .map(|err| err.kind)
            .collect::<Vec<CompilerErrorKind>>(),
        vec![CompilerErrorKind::ProfileEffectMismatch]
    );
}

#[test]
fn malformed_authority_facts_file_rejects_with_stable_kind() {
    let dir = temp_case_dir("malformed-json");
    let path = write_json(&dir, "bad.json", "{");
    let failures =
        load_compiler_context_from_authority_fact_files([path.as_path()]).expect_err("bad JSON");

    assert_eq!(
        failure_kinds(&failures),
        vec![AuthorityFactsLoadFailureKind::InvalidJson]
    );
}

#[test]
fn nondigest_authority_fact_source_rejects_with_stable_kind() {
    let dir = temp_case_dir("nondigest-source");
    let path = write_json(
        &dir,
        "floating-source.json",
        r#"{
          "apiVersion": "edict.authority-facts/v1",
          "source": {
            "kind": "targetProfile",
            "coordinate": "echo.dpo@1",
            "digest": null
          },
          "operationProfiles": [],
          "effectWriteClasses": [],
          "budgets": []
        }"#,
    );
    let failures = load_compiler_context_from_authority_fact_files([path.as_path()])
        .expect_err("floating source rejects");

    assert_eq!(
        failure_kinds(&failures),
        vec![AuthorityFactsLoadFailureKind::NonDigestLockedSource]
    );
}

#[test]
fn omitted_authority_fact_source_coordinate_rejects_with_stable_kind() {
    let dir = temp_case_dir("omitted-source-coordinate");
    let path = write_json(
        &dir,
        "omitted-source-coordinate.json",
        r#"{
          "apiVersion": "edict.authority-facts/v1",
          "source": {
            "kind": "targetProfile",
            "digest": "sha256:1111111111111111111111111111111111111111111111111111111111111111"
          },
          "operationProfiles": [],
          "effectWriteClasses": [],
          "budgets": []
        }"#,
    );
    let failures = load_compiler_context_from_authority_fact_files([path.as_path()])
        .expect_err("omitted source coordinate rejects");

    assert_eq!(
        failure_kinds(&failures),
        vec![AuthorityFactsLoadFailureKind::MissingCoordinate]
    );
    assert_eq!(failures[0].field, "source.coordinate");
}

#[test]
fn conflicting_file_backed_authority_facts_reject_before_context() {
    let dir = temp_case_dir("conflicting-facts");
    let read = write_json(&dir, "read-effect.json", lawpack_effect_facts("read"));
    let replace = write_json(&dir, "replace-effect.json", lawpack_effect_facts("replace"));
    let failures =
        load_compiler_context_from_authority_fact_files([read.as_path(), replace.as_path()])
            .expect_err("conflicting facts reject");

    assert_eq!(
        failure_kinds(&failures),
        vec![AuthorityFactsLoadFailureKind::ConflictingFact]
    );
}

#[test]
fn duplicate_fact_coordinates_in_one_json_document_reject_before_context() {
    let dir = temp_case_dir("duplicate-json-facts");
    let path = write_json(
        &dir,
        "duplicate-facts.json",
        r#"{
          "apiVersion": "edict.authority-facts/v1",
          "source": {
            "kind": "lawpack",
            "coordinate": "hello.lawpack@1",
            "digest": "sha256:2222222222222222222222222222222222222222222222222222222222222222"
          },
          "operationProfiles": [],
          "effectWriteClasses": [
            { "effect": "target.replace", "writeClass": "replace" },
            { "effect": "target.replace", "writeClass": "replace" }
          ],
          "budgets": []
        }"#,
    );

    let failures = load_compiler_context_from_authority_fact_files([path.as_path()])
        .expect_err("duplicate JSON fact coordinates reject");

    assert_eq!(
        failure_kinds(&failures),
        vec![AuthorityFactsLoadFailureKind::DuplicateFact]
    );
}

#[test]
fn duplicate_fact_coordinates_in_direct_document_reject_before_context() {
    let duplicate = EffectWriteClassFact {
        effect: "target.replace".to_owned(),
        write_class: WriteClass::Replace,
    };
    let document = AuthorityFactsDocument {
        api_version: "edict.authority-facts/v1".to_owned(),
        source: AuthorityFactSource {
            kind: AuthorityFactSourceKind::Lawpack,
            coordinate: "hello.lawpack@1".to_owned(),
            digest: format!("sha256:{}", "2".repeat(64)),
        },
        operation_profiles: Vec::new(),
        effect_write_classes: vec![duplicate.clone(), duplicate],
        budgets: Vec::new(),
    };

    let failures = compiler_context_from_authority_facts(&[document])
        .expect_err("duplicate direct fact coordinates reject");

    assert_eq!(
        failure_kinds(&failures),
        vec![AuthorityFactsLoadFailureKind::DuplicateFact]
    );
}

#[test]
fn mixed_authority_source_digests_reject_before_context() {
    let dir = temp_case_dir("mixed-source-digests");
    let first = write_json(
        &dir,
        "target-profile-a.json",
        target_profile_facts_with_digest(
            "read",
            "sha256:1111111111111111111111111111111111111111111111111111111111111111",
        ),
    );
    let second = write_json(
        &dir,
        "target-profile-b.json",
        target_profile_facts_with_digest(
            "read",
            "sha256:4444444444444444444444444444444444444444444444444444444444444444",
        ),
    );
    let failures =
        load_compiler_context_from_authority_fact_files([first.as_path(), second.as_path()])
            .expect_err("mixed source digests reject");

    assert_eq!(
        failure_kinds(&failures),
        vec![AuthorityFactsLoadFailureKind::ConflictingFact]
    );
}

#[test]
fn invalid_loaded_profile_coordinates_reject_with_stable_kind() {
    let dir = temp_case_dir("invalid-profile-coordinate");
    let path = write_json(
        &dir,
        "invalid-profile-coordinate.json",
        r#"{
          "apiVersion": "edict.authority-facts/v1",
          "source": {
            "kind": "targetProfile",
            "coordinate": "echo.dpo@1",
            "digest": "sha256:1111111111111111111111111111111111111111111111111111111111111111"
          },
          "operationProfiles": [
            {
              "source": "hello.readOnly",
              "core": "continuum profile read-only/v1",
              "allowedWriteClasses": ["read"]
            }
          ],
          "effectWriteClasses": [],
          "budgets": []
        }"#,
    );
    let failures = load_compiler_context_from_authority_fact_files([path.as_path()])
        .expect_err("invalid profile coordinate rejects");

    assert_eq!(
        failure_kinds(&failures),
        vec![AuthorityFactsLoadFailureKind::InvalidCoordinate]
    );
    assert_eq!(failures[0].field, "operationProfiles.core");
    assert_eq!(failures[0].coordinate, "continuum profile read-only/v1");
}

#[test]
fn abi_custom_write_class_loads_and_prefixed_custom_rejects() {
    let dir = temp_case_dir("custom-write-class");
    let custom = write_json(&dir, "custom.json", target_profile_facts("custom"));
    load_compiler_context_from_authority_fact_files([custom.as_path()])
        .expect("ABI custom write class loads");

    let prefixed = write_json(
        &dir,
        "prefixed-custom.json",
        target_profile_facts("custom:tenant-specific"),
    );
    let failures = load_compiler_context_from_authority_fact_files([prefixed.as_path()])
        .expect_err("prefixed custom write class rejects");

    assert_failure_kind_present(&failures, AuthorityFactsLoadFailureKind::InvalidWriteClass);
}

#[test]
fn non_abi_write_class_casing_rejects_with_stable_kind() {
    let dir = temp_case_dir("mixed-case-write-class");
    let path = write_json(&dir, "mixed-case.json", target_profile_facts("READ"));
    let failures = load_compiler_context_from_authority_fact_files([path.as_path()])
        .expect_err("mixed-case write class rejects");

    assert_failure_kind_present(&failures, AuthorityFactsLoadFailureKind::InvalidWriteClass);

    let custom = write_json(
        &dir,
        "mixed-case-custom.json",
        target_profile_facts("Custom"),
    );
    let failures = load_compiler_context_from_authority_fact_files([custom.as_path()])
        .expect_err("mixed-case custom write class rejects");

    assert_failure_kind_present(&failures, AuthorityFactsLoadFailureKind::InvalidWriteClass);
}

#[test]
fn direct_authority_fact_documents_validate_before_context_merge() {
    let document = AuthorityFactsDocument {
        api_version: "edict.authority-facts/v0".to_owned(),
        source: AuthorityFactSource {
            kind: AuthorityFactSourceKind::TargetProfile,
            coordinate: "echo profile".to_owned(),
            digest: "sha256:not-a-review-digest".to_owned(),
        },
        operation_profiles: vec![OperationProfileFact {
            source: "hello readOnly".to_owned(),
            core: "continuum profile read-only/v1".to_owned(),
            allowed_write_classes: vec![WriteClass::Custom("tenant".to_owned())],
        }],
        effect_write_classes: vec![EffectWriteClassFact {
            effect: "target replace".to_owned(),
            write_class: WriteClass::Custom("tenant".to_owned()),
        }],
        budgets: vec![BudgetFact {
            source: "p tiny".to_owned(),
            budget: CoreBudget {
                max_steps: 1,
                max_allocated_bytes: 1,
                max_output_bytes: 1,
            },
        }],
    };

    let failures = compiler_context_from_authority_facts(&[document])
        .expect_err("directly constructed invalid facts reject");
    let observed = failure_field_set(&failures);

    for expected in [
        (
            AuthorityFactsLoadFailureKind::InvalidApiVersion,
            "apiVersion",
        ),
        (
            AuthorityFactsLoadFailureKind::InvalidCoordinate,
            "source.coordinate",
        ),
        (
            AuthorityFactsLoadFailureKind::NonDigestLockedSource,
            "source.digest",
        ),
        (
            AuthorityFactsLoadFailureKind::InvalidCoordinate,
            "operationProfiles.source",
        ),
        (
            AuthorityFactsLoadFailureKind::InvalidCoordinate,
            "operationProfiles.core",
        ),
        (
            AuthorityFactsLoadFailureKind::InvalidWriteClass,
            "operationProfiles.allowedWriteClasses",
        ),
        (
            AuthorityFactsLoadFailureKind::InvalidCoordinate,
            "effectWriteClasses.effect",
        ),
        (
            AuthorityFactsLoadFailureKind::InvalidWriteClass,
            "effectWriteClasses.writeClass",
        ),
        (
            AuthorityFactsLoadFailureKind::InvalidCoordinate,
            "budgets.source",
        ),
    ] {
        assert!(
            observed.contains(&(expected.0, expected.1.to_owned())),
            "missing failure for {expected:?}; observed {observed:?}"
        );
    }
}

#[test]
fn loader_failure_kinds_cover_public_branches() {
    let dir = temp_case_dir("loader-public-branches");

    let missing = dir.join("missing.json");
    let failures = load_compiler_context_from_authority_fact_files([missing.as_path()])
        .expect_err("missing file rejects");
    assert_eq!(
        failure_kinds(&failures),
        vec![AuthorityFactsLoadFailureKind::Io]
    );

    let invalid_api = write_json(
        &dir,
        "invalid-api.json",
        r#"{
          "apiVersion": "edict.authority-facts/v0",
          "source": {
            "kind": "targetProfile",
            "coordinate": "echo.dpo@1",
            "digest": "sha256:1111111111111111111111111111111111111111111111111111111111111111"
          },
          "operationProfiles": [],
          "effectWriteClasses": [],
          "budgets": []
        }"#,
    );
    let failures = load_compiler_context_from_authority_fact_files([invalid_api.as_path()])
        .expect_err("unsupported API version rejects");
    assert_eq!(
        failure_kinds(&failures),
        vec![AuthorityFactsLoadFailureKind::InvalidApiVersion]
    );

    let invalid_source_kind = write_json(
        &dir,
        "invalid-source-kind.json",
        r#"{
          "apiVersion": "edict.authority-facts/v1",
          "source": {
            "kind": "registry",
            "coordinate": "echo.dpo@1",
            "digest": "sha256:1111111111111111111111111111111111111111111111111111111111111111"
          },
          "operationProfiles": [],
          "effectWriteClasses": [],
          "budgets": []
        }"#,
    );
    let failures = load_compiler_context_from_authority_fact_files([invalid_source_kind.as_path()])
        .expect_err("unsupported source kind rejects");
    assert_eq!(
        failure_kinds(&failures),
        vec![AuthorityFactsLoadFailureKind::InvalidSourceKind]
    );
}

fn temp_case_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "edict-authority-facts-{name}-{}",
        std::process::id()
    ));
    if dir.exists() {
        fs::remove_dir_all(&dir).expect("remove stale temp authority-facts directory");
    }
    fs::create_dir_all(&dir).expect("create temp authority-facts directory");
    dir
}

fn write_json(dir: &Path, name: &str, contents: impl AsRef<str>) -> PathBuf {
    let path = dir.join(name);
    fs::write(&path, contents.as_ref()).expect("write authority-facts JSON");
    path
}

fn target_profile_facts(allowed_write_class: &str) -> String {
    target_profile_facts_with_digest(
        allowed_write_class,
        "sha256:1111111111111111111111111111111111111111111111111111111111111111",
    )
}

fn target_profile_facts_with_digest(allowed_write_class: &str, digest: &str) -> String {
    target_profile_facts_with_digest_and_core(
        allowed_write_class,
        digest,
        "continuum.profile.read-only/v1",
    )
}

fn target_profile_facts_with_digest_and_core(
    allowed_write_class: &str,
    digest: &str,
    core_profile: &str,
) -> String {
    format!(
        r#"{{
          "apiVersion": "edict.authority-facts/v1",
          "source": {{
            "kind": "targetProfile",
            "coordinate": "echo.dpo@1",
            "digest": "{digest}"
          }},
          "operationProfiles": [
            {{
              "source": "hello.readOnly",
              "core": "{core_profile}",
              "allowedWriteClasses": ["{allowed_write_class}"]
            }},
            {{
              "source": "p.readOnly",
              "core": "{core_profile}",
              "allowedWriteClasses": ["{allowed_write_class}"]
            }}
          ],
          "effectWriteClasses": [],
          "budgets": []
        }}"#
    )
}

fn lawpack_budget_facts() -> &'static str {
    r#"{
      "apiVersion": "edict.authority-facts/v1",
      "source": {
        "kind": "lawpack",
        "coordinate": "hello.optics@1",
        "digest": "sha256:2222222222222222222222222222222222222222222222222222222222222222"
      },
      "operationProfiles": [],
      "effectWriteClasses": [],
      "budgets": [
        {
          "source": "hello.tinyBudget",
          "maxSteps": 64,
          "maxAllocatedBytes": 4096,
          "maxOutputBytes": 4096
        }
      ]
    }"#
}

fn lawpack_effect_facts(write_class: &str) -> String {
    format!(
        r#"{{
          "apiVersion": "edict.authority-facts/v1",
          "source": {{
            "kind": "lawpack",
            "coordinate": "hello.optics@1",
            "digest": "sha256:3333333333333333333333333333333333333333333333333333333333333333"
          }},
          "operationProfiles": [],
          "effectWriteClasses": [
            {{
              "effect": "target.replace",
              "writeClass": "{write_class}"
            }}
          ],
          "budgets": [
            {{
              "source": "p.tiny",
              "maxSteps": 1,
              "maxAllocatedBytes": 1,
              "maxOutputBytes": 1
            }}
          ]
        }}"#
    )
}

fn failure_kinds(
    failures: &[edict_syntax::AuthorityFactsLoadFailure],
) -> Vec<AuthorityFactsLoadFailureKind> {
    failures.iter().map(|failure| failure.kind).collect()
}

fn assert_failure_kind_present(
    failures: &[edict_syntax::AuthorityFactsLoadFailure],
    expected: AuthorityFactsLoadFailureKind,
) {
    let observed = failure_kinds(failures);
    assert!(
        observed.contains(&expected),
        "expected {expected:?} in {observed:?}"
    );
}

fn failure_field_set(
    failures: &[edict_syntax::AuthorityFactsLoadFailure],
) -> Vec<(AuthorityFactsLoadFailureKind, String)> {
    failures
        .iter()
        .map(|failure| (failure.kind, failure.field.clone()))
        .collect()
}

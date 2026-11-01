//! Public application admission; this fixture does not claim provider execution.
use super::*;
use amblicit_syntax::{
    decode_canonical_cbor, digest_canonical_artifact, digest_target_intrinsics_catalog,
    encode_canonical_cbor, CanonicalValue as C,
};

fn cbor(value: Value) -> C {
    match value {
        Value::Null => C::Null,
        Value::Bool(v) => C::Bool(v),
        Value::Number(v) => C::Integer(i128::from(v.as_i64().expect("integer fixture"))),
        Value::String(v) => C::Text(v),
        Value::Array(v) => C::Array(v.into_iter().map(cbor).collect()),
        Value::Object(v) => C::Map(v.into_iter().map(|(k, v)| (C::Text(k), cbor(v))).collect()),
    }
}

fn replace(value: &mut C, old: &C, new: &C) {
    if value == old {
        *value = new.clone();
        return;
    }
    match value {
        C::Array(values) => {
            for v in values {
                replace(v, old, new);
            }
        }
        C::Map(values) => {
            for (_, v) in values {
                replace(v, old, new);
            }
        }
        _ => {}
    }
}

fn digest(domain: &str, bytes: &[u8]) -> C {
    C::Array(vec![
        C::Text("sha256".into()),
        C::Bytes(
            digest_canonical_artifact(domain, bytes)
                .expect("canonical fixture")
                .bytes()
                .to_vec(),
        ),
    ])
}
fn rendered(digest: &C) -> String {
    let C::Array(parts) = digest else {
        panic!("digest")
    };
    let C::Bytes(bytes) = &parts[1] else {
        panic!("digest bytes")
    };
    let mut rendered = String::from("sha256:");
    for byte in bytes {
        write!(&mut rendered, "{byte:02x}").expect("write digest");
    }
    rendered
}
fn load(path: &Path) -> C {
    decode_canonical_cbor(&fs::read(path).expect("fixture bytes")).expect("canonical fixture")
}
fn save(path: &Path, value: &C) -> Vec<u8> {
    let bytes = encode_canonical_cbor(value).expect("encode fixture");
    fs::write(path, &bytes).expect("write fixture");
    bytes
}

fn fixture(root: &Path) -> PathBuf {
    fixture_with_catalog(root, |_| {})
}

fn fixture_with_catalog(root: &Path, change: impl FnOnce(&mut Value)) -> PathBuf {
    fixture_with_profile(root, change, |_| {})
}

fn fixture_with_profile(
    root: &Path,
    change: impl FnOnce(&mut Value),
    change_profile: impl FnOnce(&mut C),
) -> PathBuf {
    let config = write_external_action_application(root);
    let mut catalog = json!({"apiVersion":"edict.target-profile.intrinsics/v2", "intrinsics": {
        "echo.dpo@1.identity": {"intrinsicClass":"pure", "typeParameters":[],
            "argumentTypes":["Bytes<max=32>"], "returnType":"Bytes<max=32>",
            "guardSupport":false, "footprintTemplate":"empty", "costTemplate":"identity",
            "writeClass":"none", "contractVersion":"test.identity/v1",
            "costRule":{"base":1,"perArgumentMax":[1]}}
    }});
    change(&mut catalog);
    let catalog = cbor(catalog);
    let bytes = save(&root.join("provider/intrinsics.cbor"), &catalog);
    let pin =
        digest_target_intrinsics_catalog("echo.dpo.intrinsics/v1", &bytes).expect("catalog digest");
    let profile_path = root.join("provider/generated/primary/target-profile.echo-dpo.cbor");
    let old_profile = digest(
        "edict.target-profile/v1",
        &fs::read(&profile_path).expect("profile"),
    );
    let mut profile = load(&profile_path);
    let C::Map(fields) = &mut profile else {
        panic!("profile map")
    };
    let (_, reference) = fields
        .iter_mut()
        .find(|(k, _)| k == &C::Text("intrinsics".into()))
        .expect("catalog ref");
    // Resource digests use the catalog coordinate as their canonical domain.
    *reference = C::Map(vec![
        (
            C::Text("id".into()),
            C::Text("echo.dpo.intrinsics/v1".into()),
        ),
        (
            C::Text("digest".into()),
            digest("echo.dpo.intrinsics/v1", &bytes),
        ),
    ]);
    assert_eq!(rendered(&digest("echo.dpo.intrinsics/v1", &bytes)), pin);
    change_profile(&mut profile);
    let new_profile = digest("edict.target-profile/v1", &save(&profile_path, &profile));
    let adapter_path = root.join("vendor/workspace-snapshot/adapter.cbor");
    let old_adapter = digest(
        "edict.lawpack-adapter/v1",
        &fs::read(&adapter_path).expect("adapter"),
    );
    let mut adapter = load(&adapter_path);
    replace(&mut adapter, &old_profile, &new_profile);
    let new_adapter = digest("edict.lawpack-adapter/v1", &save(&adapter_path, &adapter));
    let manifest_path = root.join("vendor/workspace-snapshot/manifest.cbor");
    let old_manifest = digest(
        "edict.lawpack/v1",
        &fs::read(&manifest_path).expect("manifest"),
    );
    let mut manifest = load(&manifest_path);
    replace(&mut manifest, &old_adapter, &new_adapter);
    replace(&mut manifest, &old_profile, &new_profile);
    let new_manifest = digest("edict.lawpack/v1", &save(&manifest_path, &manifest));
    let source_path = root.join("src/observe-workspace.amblicit");
    let source = fs::read_to_string(&source_path)
        .expect("source")
        .replace(&rendered(&old_manifest), &rendered(&new_manifest))
        .replace(
            "type ObserveInput",
            &format!(
                "use target echo.dpo@1 digest \"{}\" as echo;\n\ntype ObserveInput",
                rendered(&new_profile)
            ),
        )
        .replace(
            "  request pending:",
            "  let authority = echo.identity(input.scope);\n  request pending:",
        )
        .replace("authority input.scope", "authority authority");
    fs::write(source_path, source).expect("source with intrinsic");
    let provider_path = root.join("provider/provider-manifest.echo.json");
    let mut provider: Value = serde_json::from_slice(&fs::read(&provider_path).expect("provider"))
        .expect("provider JSON");
    provider["artifacts"][0]["resource"]["digest"] = json!(rendered(&new_profile));
    fs::write(
        provider_path,
        serde_json::to_vec(&provider).expect("provider JSON"),
    )
    .expect("provider");
    let mut application: Value =
        serde_json::from_slice(&fs::read(&config).expect("application")).expect("application JSON");
    application["target"]["pureIntrinsics"] = json!({"catalog":"intrinsics.cbor","required":[{
        "coordinate":"echo.dpo@1.identity","contractVersion":"test.identity/v1"}]});
    fs::write(
        &config,
        serde_json::to_vec(&application).expect("application JSON"),
    )
    .expect("application");
    config
}

#[test]
fn public_application_admits_profile_pinned_intrinsic_into_core_and_target() {
    let root = temp_tree("intrinsic-admission");
    let config = fixture(&root);
    let output = build(&config);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    for name in ["core.cbor", "target-ir.cbor"] {
        let artifact = load(&root.join(".build/application").join(name));
        assert!(
            contains_identity_call(&artifact),
            "{name} must retain the actual call"
        );
    }
    fs::remove_dir_all(root).expect("cleanup");
}

fn contains_identity_call(value: &C) -> bool {
    match value {
        C::Map(fields) => {
            let field = |name: &str| {
                fields
                    .iter()
                    .find(|(k, _)| k == &C::Text(name.into()))
                    .map(|(_, v)| v)
            };
            (field("kind") == Some(&C::Text("call".into()))
                && field("callee") == Some(&C::Text("echo.dpo@1.identity".into())))
                || fields.iter().any(|(_, v)| contains_identity_call(v))
        }
        C::Array(values) => values.iter().any(contains_identity_call),
        _ => false,
    }
}

fn edit_config(path: &Path, edit: impl FnOnce(&mut Value)) {
    let mut value = serde_json::from_slice(&fs::read(path).expect("config")).expect("config JSON");
    edit(&mut value);
    fs::write(path, serde_json::to_vec(&value).expect("JSON")).expect("save config");
}

fn refusal(root: &Path, config: &Path, expected: &str) {
    let output_dir = root.join(".build/application");
    fs::create_dir_all(&output_dir).expect("previous output");
    fs::write(output_dir.join("retained.bin"), b"previous").expect("retained output");
    let output = build(config);
    assert_eq!(output.status.code(), Some(2));
    let stream = records(&output.stderr);
    assert!(
        stream.iter().any(|r| r["kind"] == expected),
        "expected {expected}: {stream:?}"
    );
    assert!(output.stdout.is_empty());
    assert_eq!(
        fs::read(output_dir.join("retained.bin")).expect("preserved"),
        b"previous"
    );
    assert_eq!(fs::read_dir(output_dir).expect("outputs").count(), 1);
    fs::remove_dir_all(root).expect("cleanup");
}

#[test]
fn public_application_refuses_invalid_intrinsic_requirements_without_publication() {
    for (case, expected) in [
        ("version", "TargetIntrinsicVersionMismatch"),
        ("missing", "TargetIntrinsicMissingIntrinsic"),
        ("duplicate", "TargetIntrinsicDuplicateRequirement"),
        ("empty", "InvalidApplicationConfig"),
        ("too-many", "InvalidApplicationConfig"),
        ("escape", "InvalidApplicationConfig"),
        ("self-pin", "InvalidApplicationConfig"),
    ] {
        let root = temp_tree(case);
        let config = fixture(&root);
        edit_config(&config, |v| {
            let declaration = &mut v["target"]["pureIntrinsics"];
            match case {
                "version" => declaration["required"][0]["contractVersion"] = json!("wrong/v1"),
                "missing" => declaration["required"][0]["coordinate"] = json!("echo.dpo@1.missing"),
                "duplicate" => {
                    declaration["required"] =
                        json!([declaration["required"][0], declaration["required"][0]]);
                }
                "empty" => declaration["required"] = json!([]),
                "too-many" => {
                    declaration["required"] = json!(vec![declaration["required"][0].clone(); 65]);
                }
                "escape" => declaration["catalog"] = json!("../intrinsics.cbor"),
                "self-pin" => declaration["digest"] = json!("sha256:untrusted"),
                _ => panic!("case"),
            }
        });
        refusal(&root, &config, expected);
    }
}

#[test]
fn public_application_refuses_invalid_catalogs_without_publication() {
    for (case, expected) in [
        ("v1", "TargetIntrinsicMalformedCatalog"),
        ("effect", "TargetIntrinsicNotPure"),
        ("cost", "TargetIntrinsicInvalidCostRule"),
        ("shape", "TargetIntrinsicMalformedContract"),
    ] {
        let root = temp_tree(case);
        let config = fixture_with_catalog(&root, |v| match case {
            "v1" => v["apiVersion"] = json!("edict.target-profile.intrinsics/v1"),
            "effect" => v["intrinsics"]["echo.dpo@1.identity"]["intrinsicClass"] = json!("effect"),
            "cost" => v["intrinsics"]["echo.dpo@1.identity"]["costRule"]["base"] = json!(-1),
            "shape" => {
                v["intrinsics"]["echo.dpo@1.identity"]
                    .as_object_mut()
                    .expect("contract")
                    .remove("returnType");
            }
            _ => panic!("case"),
        });
        refusal(&root, &config, expected);
    }
}

#[test]
fn public_application_rejects_catalog_substitution_and_foreign_source_target() {
    for (case, expected) in [
        ("substitution", "TargetIntrinsicDigestMismatch"),
        ("noncanonical", "TargetIntrinsicNonCanonicalCatalog"),
        ("missing-file", "TargetIntrinsicCatalogReadFailed"),
        ("foreign-target", "UnresolvedFunction"),
    ] {
        let root = temp_tree(case);
        let config = fixture(&root);
        let catalog = root.join("provider/intrinsics.cbor");
        match case {
            "substitution" => {
                save(&catalog, &C::Bool(false));
            }
            "noncanonical" => fs::write(catalog, [0xff]).expect("bad bytes"),
            "missing-file" => fs::remove_file(catalog).expect("remove catalog"),
            "foreign-target" => {
                let path = root.join("src/observe-workspace.amblicit");
                let text = fs::read_to_string(&path).expect("source");
                let line = text
                    .lines()
                    .find(|s| s.starts_with("use target"))
                    .expect("target import");
                fs::write(
                    path,
                    text.replace(
                        line,
                        &format!(
                            "use target echo.dpo@1 digest \"sha256:{}\" as echo;",
                            "1".repeat(64)
                        ),
                    ),
                )
                .expect("foreign pin");
            }
            _ => panic!("case"),
        }
        refusal(&root, &config, expected);
    }
}

#[test]
fn public_application_requires_well_formed_profile_owned_catalog_reference() {
    for case in ["missing", "short-digest", "extra-field"] {
        let root = temp_tree(case);
        let config = fixture_with_profile(
            &root,
            |_| {},
            |profile| {
                let C::Map(fields) = profile else {
                    panic!("profile")
                };
                if case == "missing" {
                    fields.retain(|(k, _)| k != &C::Text("intrinsics".into()));
                    return;
                }
                let (_, C::Map(reference)) = fields
                    .iter_mut()
                    .find(|(k, _)| k == &C::Text("intrinsics".into()))
                    .expect("intrinsics")
                else {
                    panic!("reference")
                };
                if case == "extra-field" {
                    reference.push((C::Text("selfPin".into()), C::Bool(true)));
                } else {
                    let (_, value) = reference
                        .iter_mut()
                        .find(|(k, _)| k == &C::Text("digest".into()))
                        .expect("digest");
                    *value = C::Array(vec![C::Text("sha256".into()), C::Bytes(vec![0; 31])]);
                }
            },
        );
        refusal(&root, &config, "InvalidTargetIntrinsicReference");
    }
}

#[test]
fn public_application_bounds_catalog_bytes() {
    let root = temp_tree("oversized-catalog");
    let config = fixture(&root);
    fs::write(root.join("provider/intrinsics.cbor"), vec![0; 1_048_577])
        .expect("oversized fixture");
    refusal(&root, &config, "ApplicationArtifactTooLarge");
}

#[cfg(unix)]
#[test]
fn public_application_rejects_catalog_symlink_outside_provider() {
    let root = temp_tree("catalog-symlink");
    let config = fixture(&root);
    let outside = root.join("outside-provider.cbor");
    fs::copy(root.join("provider/intrinsics.cbor"), &outside).expect("outside copy");
    std::os::unix::fs::symlink(outside, root.join("provider/escaped.cbor")).expect("symlink");
    edit_config(&config, |v| {
        v["target"]["pureIntrinsics"]["catalog"] = json!("escaped.cbor");
    });
    refusal(&root, &config, "ApplicationPathEscape");
}

#[test]
fn public_application_omission_grants_no_intrinsic_facts() {
    let root = temp_tree("omitted-intrinsics");
    let config = fixture(&root);
    edit_config(&config, |v| {
        v["target"]
            .as_object_mut()
            .expect("target")
            .remove("pureIntrinsics");
    });
    fs::remove_file(root.join("provider/intrinsics.cbor")).expect("no catalog to load");
    refusal(&root, &config, "UnresolvedFunction");
}

#[test]
fn public_application_charges_intrinsic_operand_bound_against_budget() {
    let root = temp_tree("intrinsic-cost");
    // scope is Bytes<max=32>; 32*32 exceeds the fixture's 512-step budget.
    // The ordinary successful fixture uses a coefficient of 1.
    let config = fixture_with_catalog(&root, |v| {
        v["intrinsics"]["echo.dpo@1.identity"]["costRule"]["perArgumentMax"] = json!([32]);
    });
    refusal(&root, &config, "InvalidBound");
}

#[test]
fn executable_application_intrinsics_still_require_provider_artifacts() {
    let root = temp_tree("executable-intrinsic");
    let config = fixture(&root);
    edit_config(&config, |v| {
        v["buildKind"] = json!("executableOperation");
        v["externalActionResources"] = json!([]);
    });
    let path = root.join("src/observe-workspace.amblicit");
    let source = fs::read_to_string(&path).expect("source");
    let imports = source
        .lines()
        .filter(|line| line.starts_with("use lawpack") || line.starts_with("use target"))
        .collect::<Vec<_>>()
        .join("\n");
    fs::write(path,format!("package examples.workspace_observer@1;\n{imports}\nintent observe(input: Bytes<max=32>) returns Bytes<max=32> profile workspace.observeRequest basis none budget <= workspace.tinyObservationBudget {{ let value=echo.identity(input); return value; }}")).expect("pure intent");
    // Compiler and result projection succeed, but the fixture has no provider schema.
    // The executable route must not publish compiler artifacts as an accepted package.
    refusal(&root, &config, "ProviderPackageReadFailed");
}

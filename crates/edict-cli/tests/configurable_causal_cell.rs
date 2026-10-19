#[path = "../examples/configurable_causal_cell/limits.rs"]
mod limits;

use edict_syntax::{encode_canonical_cbor, CanonicalValue};
use limits::{LimitError, Limits};

#[test]
fn configurable_cell_budgets_match_maximum_unicode_encoding() {
    for (key_scalars, value_scalars, replacement_bytes) in [
        (64, 64, 256),
        (64, 1024, 4096),
        (1, 5, 20),
        (6, 6, 24),
        (64, 16384, 65536),
    ] {
        let limits = Limits {
            key_scalars,
            value_scalars,
            replacement_bytes,
        };
        let budget = limits.budgets().expect("consistent finite parameters");
        let value = CanonicalValue::Map(vec![
            (
                CanonicalValue::Text("key".into()),
                CanonicalValue::Text("😀".repeat(usize::try_from(key_scalars).unwrap())),
            ),
            (
                CanonicalValue::Text("value".into()),
                CanonicalValue::Text("😀".repeat(usize::try_from(value_scalars).unwrap())),
            ),
        ]);
        let encoded = encode_canonical_cbor(&value).unwrap();
        assert_eq!(budget.output, u64::try_from(encoded.len()).unwrap());
        assert_eq!(budget.write, replacement_bytes + 64);
        let input = CanonicalValue::Map(vec![
            (
                CanonicalValue::Text("basis".into()),
                CanonicalValue::Text("😀".repeat(128)),
            ),
            (
                CanonicalValue::Text("key".into()),
                CanonicalValue::Text("😀".repeat(usize::try_from(key_scalars).unwrap())),
            ),
            (
                CanonicalValue::Text("value".into()),
                CanonicalValue::Text("😀".repeat(usize::try_from(value_scalars).unwrap())),
            ),
        ]);
        let input_len = u64::try_from(encode_canonical_cbor(&input).unwrap().len()).unwrap();
        assert_eq!(
            budget.allocated,
            input_len + budget.output + replacement_bytes
        );
        assert_eq!(limits.budgets(), Ok(budget), "calculation is deterministic");
    }
}

#[test]
fn configurable_cell_limits_reject_inconsistent_and_overflowing_parameters() {
    for limits in [
        Limits {
            key_scalars: 0,
            value_scalars: 64,
            replacement_bytes: 256,
        },
        Limits {
            key_scalars: 64,
            value_scalars: 0,
            replacement_bytes: 256,
        },
        Limits {
            key_scalars: 64,
            value_scalars: 64,
            replacement_bytes: 0,
        },
    ] {
        assert_eq!(limits.budgets(), Err(LimitError::Zero));
    }
    for (value_scalars, replacement_bytes) in [(256, 256), (65, 256), (64, 255)] {
        assert_eq!(
            Limits {
                key_scalars: 64,
                value_scalars,
                replacement_bytes
            }
            .budgets(),
            Err(LimitError::ScalarBytesExceedCap),
            "{value_scalars} scalars against {replacement_bytes} bytes"
        );
    }
    for limits in [
        Limits {
            key_scalars: u64::MAX,
            value_scalars: 1,
            replacement_bytes: 4,
        },
        Limits {
            key_scalars: 1,
            value_scalars: u64::MAX,
            replacement_bytes: u64::MAX,
        },
        Limits {
            key_scalars: 1,
            value_scalars: 1,
            replacement_bytes: u64::MAX,
        },
    ] {
        assert_eq!(limits.budgets(), Err(LimitError::Overflow));
    }
}

#[path = "../examples/configurable_causal_cell/document.rs"]
mod document;

#[test]
fn configurable_cell_documents_author_repeatable_valid_closures() {
    use edict_syntax::{
        author_lawpack, decode_lawpack_adapter, decode_lawpack_bundle, LawpackArtifactKind,
        LawpackAuthoringDefinition,
    };
    let mut identities = Vec::new();
    for (value_scalars, replacement_bytes) in [(64, 256), (1024, 4096)] {
        let limits = Limits {
            key_scalars: 64,
            value_scalars,
            replacement_bytes,
        };
        let doc = document::document(limits).expect("generate authoring JSON");
        assert_eq!(doc, document::document(limits).unwrap());
        let definition: LawpackAuthoringDefinition =
            serde_json::from_value(doc["lawpack"].clone()).expect("public typed authoring schema");
        let artifacts = author_lawpack(&definition, &[]).expect("public authoring succeeds");
        assert_eq!(artifacts, author_lawpack(&definition, &[]).unwrap());
        let manifest = artifacts.artifact(LawpackArtifactKind::Manifest).unwrap();
        let exports = artifacts.artifact(LawpackArtifactKind::Exports).unwrap();
        let bundle = decode_lawpack_bundle(manifest.bytes(), exports.bytes()).unwrap();
        let adapter = artifacts.artifact(LawpackArtifactKind::Adapter).unwrap();
        decode_lawpack_adapter(&bundle, "echo.dpo@1", adapter.bytes())
            .expect("exact authored adapter");
        assert_eq!(bundle.exports().types[0].definition, format!("Record<basis:String<max=128,canonical=raw-utf8>,key:String<max=64,canonical=raw-utf8>,value:String<max={value_scalars},canonical=raw-utf8>>"));
        assert_eq!(
            doc["lawpack"]["localResources"][0]["value"]["maxReplacementBytes"],
            replacement_bytes
        );
        assert_eq!(
            doc["lawpack"]["targetAdapters"][0]["budgets"]["example.cell@1.createBudget"]
                ["maxOutputBytes"],
            limits.budgets().unwrap().output
        );
        identities.push(manifest.digest().to_owned());
    }
    assert_ne!(
        identities[0], identities[1],
        "changed semantic bounds change identity"
    );
}

#[test]
fn configurable_cell_public_authoring_matches_in_memory_artifacts() {
    use edict_syntax::{author_lawpack, LawpackAuthoringDefinition};
    for (value_scalars, replacement_bytes) in [(64, 256), (1024, 4096)] {
        let doc = document::document(Limits {
            key_scalars: 64,
            value_scalars,
            replacement_bytes,
        })
        .unwrap();
        let definition: LawpackAuthoringDefinition =
            serde_json::from_value(doc["lawpack"].clone()).unwrap();
        let expected = author_lawpack(&definition, &[]).unwrap();
        for copy in 0..2 {
            let directory = test_directory(&format!("edict-configurable-{value_scalars}-{copy}-"));
            let root = directory.path().to_path_buf();
            std::fs::write(
                root.join("edict.lawpack.json"),
                serde_json::to_vec(&doc).unwrap(),
            )
            .unwrap();
            let output = run_public_build(&root, "lawpack", "edict.lawpack.json");
            assert_eq!(output.status.code(), Some(0), "public build: {output:?}");
            assert!(output.stderr.is_empty(), "{output:?}");
            for artifact in expected.artifacts() {
                assert_eq!(
                    std::fs::read(root.join("vendor/cell").join(artifact.path())).unwrap(),
                    artifact.bytes(),
                    "exact public artifact {}",
                    artifact.path()
                );
            }
        }
    }
}

#[path = "../examples/configurable_causal_cell/command.rs"]
mod command;

#[test]
fn configurable_cell_command_renders_compilable_source() {
    use edict_syntax::{
        author_lawpack, compile_to_core, decode_lawpack_adapter, decode_lawpack_bundle,
        parse_module, prepare_lawpack_compilation, LawpackArtifactKind, LawpackAuthoringDefinition,
    };
    for (scalars, bytes) in [("64", "256"), ("1024", "4096")] {
        let json = command::render(&["lawpack", "64", scalars, bytes]).unwrap();
        let document: serde_json::Value = serde_json::from_str(&json).unwrap();
        let definition: LawpackAuthoringDefinition =
            serde_json::from_value(document["lawpack"].clone()).unwrap();
        let authored = author_lawpack(&definition, &[]).unwrap();
        let manifest = authored.artifact(LawpackArtifactKind::Manifest).unwrap();
        let source = command::render(&["source", "64", scalars, bytes, manifest.digest()]).unwrap();
        let module = parse_module(&source).expect("generated source parses");
        let bundle = decode_lawpack_bundle(
            manifest.bytes(),
            authored
                .artifact(LawpackArtifactKind::Exports)
                .unwrap()
                .bytes(),
        )
        .unwrap();
        let adapter = decode_lawpack_adapter(
            &bundle,
            "echo.dpo@1",
            authored
                .artifact(LawpackArtifactKind::Adapter)
                .unwrap()
                .bytes(),
        )
        .unwrap();
        let prepared = prepare_lawpack_compilation(&module, &bundle, &adapter).unwrap();
        compile_to_core(&module, prepared.compiler_context())
            .expect("generated source fits its authored budget");
    }
    let app: serde_json::Value =
        serde_json::from_str(&command::render(&["application"]).unwrap()).unwrap();
    assert_eq!(app["coordinate"], "examples.configurable_cell@1");
    assert_eq!(app["target"]["providerPackage"], "provider");
}

#[test]
fn configurable_cell_command_refuses_invalid_arguments() {
    use command::CommandError;
    assert_eq!(command::render(&[]), Err(CommandError::Usage));
    assert_eq!(command::render(&["unknown"]), Err(CommandError::Usage));
    assert_eq!(
        command::render(&["application", "extra"]),
        Err(CommandError::Usage)
    );
    assert_eq!(
        command::render(&["lawpack", "no", "64", "256"]),
        Err(CommandError::InvalidNumber)
    );
    assert_eq!(
        command::render(&["lawpack", "64", "256", "256"]),
        Err(CommandError::Limits(LimitError::ScalarBytesExceedCap))
    );
    for digest in ["sha256:0", "sha256:ZZ", "\"; let injected = true;"] {
        assert_eq!(
            command::render(&["source", "64", "64", "256", digest]),
            Err(CommandError::InvalidDigest)
        );
    }
}

#[ignore = "requires the pinned real Echo package in EDICT_F06_PROVIDER"]
#[test]
fn configurable_cell_real_provider_accepts_both_variants() {
    use std::fs;
    let provider = std::path::PathBuf::from(
        std::env::var_os("EDICT_F06_PROVIDER").expect("explicit pinned provider package"),
    );
    let manifest: serde_json::Value =
        serde_json::from_slice(&fs::read(provider.join("provider-manifest.echo.json")).unwrap())
            .unwrap();
    assert_eq!(
        manifest["provider"]["digest"],
        "sha256:21e5267310b6b4c06bf24a814d76790cbd58a4a9dcc33205dddd6fc1937594dd"
    );
    let mut prior = std::collections::BTreeMap::new();
    for (scalars, bytes, copy) in [
        ("64", "256", 0),
        ("64", "256", 1),
        ("1024", "4096", 0),
        ("1024", "4096", 1),
    ] {
        let directory = test_directory(&format!("edict-real-cell-{bytes}-{copy}-"));
        let root = directory.path().to_path_buf();
        fs::create_dir(root.join("src")).unwrap();
        copy_provider_tree(&provider, &root.join("provider"));
        let document = command::render(&["lawpack", "64", scalars, bytes]).unwrap();
        fs::write(root.join("edict.lawpack.json"), document).unwrap();
        let authored = run_public_build(&root, "lawpack", "edict.lawpack.json");
        assert_eq!(authored.status.code(), Some(0), "{authored:?}");
        let digest = fs::read_to_string(root.join("vendor/cell/manifest.sha256")).unwrap();
        fs::write(
            root.join("src/create.edict"),
            command::render(&["source", "64", scalars, bytes, digest.trim()]).unwrap(),
        )
        .unwrap();
        fs::write(
            root.join("edict.application.json"),
            command::render(&["application"]).unwrap(),
        )
        .unwrap();
        let built = run_public_build(&root, "application", "edict.application.json");
        assert_eq!(built.status.code(), Some(0), "{built:?}");
        let package =
            fs::read(root.join(".build/application/executable-operation-package.cbor")).unwrap();
        let report = fs::read(root.join(".build/application/verification-report.cbor")).unwrap();
        assert!(!package.is_empty() && !report.is_empty());
        if let Some((old_package, old_report)) =
            prior.insert(bytes, (package.clone(), report.clone()))
        {
            assert_eq!(
                old_package, package,
                "package is independent of application directory"
            );
            assert_eq!(
                old_report, report,
                "verification is independent of application directory"
            );
        }
        check_invalid_configurations(&root, scalars, bytes, &package, &report);
        check_independent_byte_cap(&root, scalars, bytes, &package, &report);
        check_invalid_closures(&root, scalars, bytes, &digest, &package, &report);
    }
}

fn run_public_build(root: &std::path::Path, key: &str, document: &str) -> std::process::Output {
    use std::io::Write;
    use std::process::{Command, Stdio};
    let request = serde_json::json!({"schema": "edict.compiler.settings/v1", "type": "compilerSettings", "operation": "build", key: document});
    let mut child = Command::new(env!("CARGO_BIN_EXE_edict"))
        .current_dir(root)
        .env_remove(edict_cli::MAX_STDIN_BYTES_ENV)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    writeln!(child.stdin.take().unwrap(), "{request}").unwrap();
    child.wait_with_output().unwrap()
}

fn copy_provider_tree(source: &std::path::Path, target: &std::path::Path) {
    std::fs::create_dir(target).unwrap();
    for entry in std::fs::read_dir(source).unwrap() {
        let entry = entry.unwrap();
        let destination = target.join(entry.file_name());
        let kind = entry.file_type().unwrap();
        if kind.is_dir() {
            copy_provider_tree(&entry.path(), &destination);
        } else {
            assert!(
                kind.is_file(),
                "provider fixture contains only files/directories"
            );
            std::fs::copy(entry.path(), destination).unwrap();
        }
    }
}

fn assert_public_failure(output: &std::process::Output, kind: &str) {
    assert_eq!(output.status.code(), Some(2), "expected {kind}: {output:?}");
    assert!(output.stdout.is_empty(), "{output:?}");
    let records: Vec<serde_json::Value> = std::str::from_utf8(&output.stderr)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert!(
        records
            .iter()
            .any(|record| record["schema"] == "edict.cli.diagnostic/v1" && record["kind"] == kind),
        "missing {kind}: {records:?}"
    );
}

fn check_invalid_closures(
    root: &std::path::Path,
    scalars: &str,
    bytes: &str,
    digest: &str,
    package: &[u8],
    report: &[u8],
) {
    use std::fs;
    let mut smaller: serde_json::Value =
        serde_json::from_slice(&fs::read(root.join("edict.lawpack.json")).unwrap()).unwrap();
    smaller["lawpack"]["targetAdapters"][0]["budgets"]["example.cell@1.createBudget"]
        ["maxOutputBytes"] = 1.into();
    fs::write(
        root.join("edict.lawpack.json"),
        serde_json::to_vec(&smaller).unwrap(),
    )
    .unwrap();
    let authored = run_public_build(root, "lawpack", "edict.lawpack.json");
    assert_eq!(authored.status.code(), Some(0), "{authored:?}");
    assert_public_failure(
        &run_public_build(root, "application", "edict.application.json"),
        "InvalidApplicationClosure",
    );
    let new_digest = fs::read_to_string(root.join("vendor/cell/manifest.sha256")).unwrap();
    assert_ne!(digest, new_digest);
    fs::write(
        root.join("src/create.edict"),
        command::render(&["source", "64", scalars, bytes, new_digest.trim()]).unwrap(),
    )
    .unwrap();
    assert_public_failure(
        &run_public_build(root, "application", "edict.application.json"),
        "InvalidBound",
    );
    assert_eq!(
        package,
        fs::read(root.join(".build/application/executable-operation-package.cbor")).unwrap()
    );
    assert_eq!(
        report,
        fs::read(root.join(".build/application/verification-report.cbor")).unwrap()
    );
}

// Characterize existing provider refusal behavior through public authoring and
// application build. Re-authoring and repinning bypass stale-digest rejection,
// so these assertions exercise schema admission and the pinned provider,
// rather than stopping at a stale manifest pin.
fn check_invalid_configurations(
    root: &std::path::Path,
    scalars: &str,
    bytes: &str,
    package: &[u8],
    report: &[u8],
) {
    use std::fs;
    let original = fs::read(root.join("edict.lawpack.json")).unwrap();
    let source = fs::read(root.join("src/create.edict")).unwrap();
    for case in ["zero-cap", "aliased-fields"] {
        let mut document: serde_json::Value = serde_json::from_slice(&original).unwrap();
        let configuration = &mut document["lawpack"]["localResources"][0]["value"];
        match case {
            "zero-cap" => configuration["maxReplacementBytes"] = 0.into(),
            "aliased-fields" => {
                configuration["invocationBinding"]["replacementField"] = "key".into();
            }
            _ => unreachable!(),
        }
        fs::write(
            root.join("edict.lawpack.json"),
            serde_json::to_vec(&document).unwrap(),
        )
        .unwrap();
        let authored = run_public_build(root, "lawpack", "edict.lawpack.json");
        assert_eq!(authored.status.code(), Some(0), "{case}: {authored:?}");
        let digest = fs::read_to_string(root.join("vendor/cell/manifest.sha256")).unwrap();
        fs::write(
            root.join("src/create.edict"),
            command::render(&["source", "64", scalars, bytes, digest.trim()]).unwrap(),
        )
        .unwrap();
        assert_public_failure(
            &run_public_build(root, "application", "edict.application.json"),
            if case == "zero-cap" {
                "InvalidProviderInvocation"
            } else {
                "ProviderLowererRefused"
            },
        );
        assert_eq!(
            package,
            fs::read(root.join(".build/application/executable-operation-package.cbor")).unwrap()
        );
        assert_eq!(
            report,
            fs::read(root.join(".build/application/verification-report.cbor")).unwrap()
        );
    }
    fs::write(root.join("edict.lawpack.json"), original).unwrap();
    fs::write(root.join("src/create.edict"), source).unwrap();
    let restored = run_public_build(root, "lawpack", "edict.lawpack.json");
    assert_eq!(restored.status.code(), Some(0), "{restored:?}");
}

// The v1 provider admits independently declared byte caps. This witness makes
// that limitation explicit instead of assigning the helper's consistency
// guarantee to the lowerer or verifier.
fn check_independent_byte_cap(
    root: &std::path::Path,
    scalars: &str,
    bytes: &str,
    package: &[u8],
    report: &[u8],
) {
    use std::fs;
    let original = fs::read(root.join("edict.lawpack.json")).unwrap();
    let source = fs::read(root.join("src/create.edict")).unwrap();
    let mut document: serde_json::Value = serde_json::from_slice(&original).unwrap();
    document["lawpack"]["localResources"][0]["value"]["maxReplacementBytes"] = 1.into();
    fs::write(
        root.join("edict.lawpack.json"),
        serde_json::to_vec(&document).unwrap(),
    )
    .unwrap();
    let authored = run_public_build(root, "lawpack", "edict.lawpack.json");
    assert_eq!(authored.status.code(), Some(0), "{authored:?}");
    let digest = fs::read_to_string(root.join("vendor/cell/manifest.sha256")).unwrap();
    fs::write(
        root.join("src/create.edict"),
        command::render(&["source", "64", scalars, bytes, digest.trim()]).unwrap(),
    )
    .unwrap();
    let built = run_public_build(root, "application", "edict.application.json");
    assert_eq!(
        built.status.code(),
        Some(0),
        "independent byte cap: {built:?}"
    );
    let changed =
        fs::read(root.join(".build/application/executable-operation-package.cbor")).unwrap();
    assert_ne!(
        changed, package,
        "new configuration changes executable identity"
    );
    let decoded = edict_syntax::decode_canonical_cbor(&changed).unwrap();
    let CanonicalValue::Bytes(program) = map_field(&decoded, "program") else {
        panic!("package must contain canonical program bytes");
    };
    let program = edict_syntax::decode_canonical_cbor(program).unwrap();
    assert_eq!(
        map_field(&program, "max_replacement_bytes"),
        &CanonicalValue::Integer(1)
    );
    fs::write(root.join("edict.lawpack.json"), original).unwrap();
    fs::write(root.join("src/create.edict"), source).unwrap();
    let restored = run_public_build(root, "lawpack", "edict.lawpack.json");
    assert_eq!(restored.status.code(), Some(0), "{restored:?}");
    let restored = run_public_build(root, "application", "edict.application.json");
    assert_eq!(restored.status.code(), Some(0), "{restored:?}");
    assert_eq!(
        package,
        fs::read(root.join(".build/application/executable-operation-package.cbor")).unwrap()
    );
    assert_eq!(
        report,
        fs::read(root.join(".build/application/verification-report.cbor")).unwrap()
    );
}

fn map_field<'a>(value: &'a CanonicalValue, key: &str) -> &'a CanonicalValue {
    let CanonicalValue::Map(fields) = value else {
        panic!("expected canonical map");
    };
    fields
        .iter()
        .find_map(|(name, value)| (name == &CanonicalValue::Text(key.into())).then_some(value))
        .expect("required program field")
}

fn test_directory(prefix: &str) -> tempfile::TempDir {
    tempfile::Builder::new()
        .prefix(prefix)
        .tempdir()
        .expect("create owned test workspace")
}

#[test]
fn configurable_cell_test_directories_cleanup_on_unwind() {
    let directory = test_directory("edict-cell-unwind");
    let path = directory.path().to_path_buf();
    let peer = test_directory("edict-cell-unwind");
    assert_ne!(path, peer.path());
    std::fs::write(peer.path().join("artifact"), b"peer").unwrap();
    std::fs::write(path.join("artifact"), b"owned temporary artifact").unwrap();
    let unwind = std::panic::catch_unwind(move || {
        let _owned = directory;
        panic!("simulated assertion failure");
    });
    assert!(unwind.is_err());
    let leaked = path.exists();
    if leaked {
        std::fs::remove_dir_all(&path).unwrap();
    }
    assert!(!leaked, "owned test tree must be removed during unwinding");
    assert_eq!(
        std::fs::read(peer.path().join("artifact")).unwrap(),
        b"peer"
    );
    let peer_path = peer.path().to_path_buf();
    drop(peer);
    assert!(
        !peer_path.exists(),
        "normal drop also removes owned workspace"
    );
}

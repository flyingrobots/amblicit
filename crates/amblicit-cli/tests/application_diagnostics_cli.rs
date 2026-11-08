//! Application build failures preserve each source diagnostic through the public binary.
use std::fmt::Write as _;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};

use amblicit_syntax::{TARGET_PROVIDER_ABI, TARGET_PROVIDER_MANIFEST_API_VERSION};
use serde_json::{json, Value};

static SEQUENCE: AtomicUsize = AtomicUsize::new(0);

fn test_ok<T, E: std::fmt::Debug>(result: Result<T, E>, context: &str) -> T {
    result.unwrap_or_else(|error| panic!("{context}: {error:?}"))
}

fn resource_json(coordinate: &str, digest: char) -> Value {
    json!({"coordinate":coordinate,"digest":format!("sha256:{}", digest.to_string().repeat(64))})
}

fn temp_tree(label: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!(
        "edict-build-diag-{label}-{}-{}",
        std::process::id(),
        SEQUENCE.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&root).expect("create owned fixture root");
    root
}

fn run(request: &Value) -> std::process::Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_amblicit"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn public Edict binary");
    writeln!(child.stdin.take().expect("stdin pipe"), "{request}").expect("write request");
    child
        .wait_with_output()
        .expect("collect public binary output")
}

fn build(config: &Path) -> std::process::Output {
    run(
        &json!({"schema":"edict.compiler.settings/v1","type":"compilerSettings","operation":"build","application":config}),
    )
}

fn records(bytes: &[u8]) -> Vec<Value> {
    std::str::from_utf8(bytes)
        .expect("UTF8 JSONL")
        .lines()
        .map(|line| serde_json::from_str(line).expect("structured JSONL record"))
        .collect()
}

fn validate_stream(stream: &[Value]) {
    let diagnostic: Value = serde_json::from_str(include_str!(
        "../../../docs/schemas/edict.cli-diagnostic.v1.schema.json"
    ))
    .expect("diagnostic schema");
    let event: Value = serde_json::from_str(include_str!(
        "../../../docs/schemas/edict.cli-event.v1.schema.json"
    ))
    .expect("event schema");
    let diagnostic =
        jsonschema::validator_for(&diagnostic).expect("compile self-contained diagnostic schema");
    let event = jsonschema::validator_for(&event).expect("compile self-contained event schema");
    for record in stream {
        let validator = match record["type"].as_str() {
            Some("diagnostic") => &diagnostic,
            Some("status") => &event,
            other => panic!("unexpected build/error stream record: {other:?}"),
        };
        validator.validate(record).unwrap_or_else(|error| {
            panic!("record violates authoritative schema: {error}; {record}")
        });
    }
}

fn summary<'a>(stream: &'a [Value], kind: &str) -> &'a Value {
    stream
        .iter()
        .find(|record| record["kind"] == kind)
        .unwrap_or_else(|| panic!("expected {kind} phase; actual records {stream:?}"))
}

fn assert_location(record: &Value, path: &Path, source: &str, start: usize) {
    assert_eq!(record["command"], "build");
    assert_eq!(record["span"]["start"], start);
    let end = usize::try_from(record["span"]["end"].as_u64().expect("byte end"))
        .expect("span fits this platform");
    assert!(end >= start && end <= source.len());
    assert_eq!(
        record["sourceLocation"]["path"],
        fs::canonicalize(path)
            .expect("canonical source path")
            .display()
            .to_string()
    );
    let before = &source[..start];
    let line = before.bytes().filter(|byte| *byte == b'\n').count() + 1;
    let column = before
        .rsplit('\n')
        .next()
        .expect("current line")
        .chars()
        .count()
        + 1;
    assert_eq!(record["sourceLocation"]["line"], line);
    assert_eq!(record["sourceLocation"]["column"], column);
    assert!(
        record.get("line").is_none(),
        "source rows must not overload request-line coordinates"
    );
}

#[test]
fn application_build_emits_compiler_records_and_preserves_output_on_failure() {
    let root = temp_tree("compiler");
    let config = write_external_action_application(&root);
    let path = root.join("src/observe-workspace.amblicit");
    let original = fs::read_to_string(&path).expect("read valid source");
    assert_eq!(original.matches("  return pending;").count(), 1);
    let source = original.replace("  return pending;", "  let marker = \"🦀\"; let wrong: U64 = true;\n  let missing = unknownValue;\n  return pending;").replace('\n', "\r\n");
    fs::write(&path, &source).expect("write invalid compiler source");
    let output_dir = root.join(".build/application");
    fs::create_dir_all(&output_dir).expect("prepare previous output");
    fs::write(output_dir.join("retained.bin"), b"previous output").expect("previous bytes");
    let output = build(&config);
    assert_eq!(output.status.code(), Some(2));
    assert_eq!(output.stdout, [] as [u8; 0]);
    let stream = records(&output.stderr);
    let diagnostic_summary = summary(&stream, "ApplicationCompilationFailed");
    let compiler: Vec<_> = stream
        .iter()
        .filter(|record| record["stage"] == "typeCheck")
        .collect();
    assert_eq!(
        compiler.len(),
        2,
        "each compiler error needs its own record: {stream:?}"
    );
    for (kind, anchor) in [
        ("TypeMismatch", "let wrong"),
        ("UnresolvedType", "unknownValue"),
    ] {
        let record = compiler
            .iter()
            .find(|record| record["kind"] == kind)
            .expect("stable compiler kind");
        assert_location(
            record,
            &path,
            &source,
            source.find(anchor).expect("error anchor"),
        );
    }
    assert!(!diagnostic_summary["message"]
        .as_str()
        .expect("summary message")
        .contains("[CompilerError"));
    assert_eq!(stream.last().expect("terminal record")["errors"], 3);
    assert_eq!(stream.last().expect("terminal record")["checked"], 0);
    assert_eq!(stream.last().expect("terminal record")["exitCode"], 2);
    validate_stream(&stream);
    assert_eq!(
        fs::read(output_dir.join("retained.bin")).expect("retained output"),
        b"previous output"
    );
    assert_eq!(
        fs::read_dir(&output_dir)
            .expect("owned output listing")
            .count(),
        1
    );
    fs::remove_dir_all(root).expect("clean owned tree");
}

#[test]
fn application_build_emits_parser_eof_location_without_publishing() {
    let root = temp_tree("parser-eof");
    let config = write_external_action_application(&root);
    let path = root.join("src/observe-workspace.amblicit");
    let source = "// 🦀\r\npackage examples.broken@1;\r\nintent broken(";
    fs::write(&path, source).expect("write EOF source");
    let output = build(&config);
    assert_eq!(output.status.code(), Some(2));
    assert_eq!(output.stdout, [] as [u8; 0]);
    let stream = records(&output.stderr);
    summary(&stream, "InvalidApplicationSource");
    let parsed = amblicit_syntax::parse_module(source).expect_err("EOF fixture has a parser error");
    assert_eq!(parsed.span.start, source.len());
    assert_eq!(parsed.span.end, source.len());
    let record = stream
        .iter()
        .find(|record| record["stage"] == "parse")
        .expect("individual parser record");
    assert_eq!(record["kind"], parsed.kind.code());
    assert_location(record, &path, source, source.len());
    assert_eq!(stream.last().expect("terminal status")["errors"], 2);
    validate_stream(&stream);
    assert!(!root.join(".build").exists());
    fs::remove_dir_all(root).expect("clean owned tree");
}

#[test]
fn diagnostic_schemas_preserve_check_and_project_streams() {
    for operation in ["check", "project"] {
        let request = if operation == "check" {
            format!(
                "{}\n{}",
                json!({"schema":"edict.compiler.settings/v1","type":"compilerSettings","operation":operation}),
                json!({"schema":"edict.compiler.input/v1","type":"compilerInput","kind":"source","name":"bad","source":"not source"})
            )
        } else {
            json!({"schema":"edict.compiler.settings/v1","type":"compilerSettings","operation":operation}).to_string()
        };
        let mut child = Command::new(env!("CARGO_BIN_EXE_amblicit"))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("spawn binary");
        writeln!(child.stdin.take().expect("stdin"), "{request}").expect("write request");
        let output = child
            .wait_with_output()
            .expect("collect compatibility control");
        assert!(!output.status.success());
        validate_stream(&records(&output.stderr));
    }
}

fn write_application_fixture_resources(root: &Path) {
    let source_directory = root.join("src");
    let lawpack_directory = root.join("vendor/workspace-snapshot");
    let provider_directory = root.join("provider");
    let provider_generated = provider_directory.join("generated/primary");
    for directory in [&source_directory, &lawpack_directory, &provider_generated] {
        test_ok(
            fs::create_dir_all(directory),
            "create application fixture tree",
        );
    }

    for (path, bytes) in [
        (
            source_directory.join("observe-workspace.amblicit"),
            include_bytes!(
                "../../../fixtures/lawpack/workspace-snapshot/observe-workspace.amblicit"
            )
            .as_slice(),
        ),
        (
            lawpack_directory.join("manifest.cbor"),
            include_bytes!("../../../fixtures/lawpack/workspace-snapshot/manifest.cbor")
                .as_slice(),
        ),
        (
            lawpack_directory.join("exports.cbor"),
            include_bytes!("../../../fixtures/lawpack/workspace-snapshot/exports.cbor")
                .as_slice(),
        ),
        (
            lawpack_directory.join("adapter.cbor"),
            include_bytes!("../../../fixtures/lawpack/workspace-snapshot/adapter.cbor")
                .as_slice(),
        ),
        (
            lawpack_directory.join("request-profile-configuration.cbor"),
            include_bytes!(
                "../../../fixtures/lawpack/workspace-snapshot/request-profile-configuration.cbor"
            )
            .as_slice(),
        ),
        (
            lawpack_directory.join("input-schema.cbor"),
            include_bytes!("../../../fixtures/lawpack/workspace-snapshot/input-schema.cbor")
                .as_slice(),
        ),
        (
            lawpack_directory.join("settlement-schema.cbor"),
            include_bytes!(
                "../../../fixtures/lawpack/workspace-snapshot/settlement-schema.cbor"
            )
            .as_slice(),
        ),
        (
            lawpack_directory.join("reconciliation-law.cbor"),
            include_bytes!(
                "../../../fixtures/lawpack/workspace-snapshot/reconciliation-law.cbor"
            )
            .as_slice(),
        ),
        (
            provider_generated.join("target-profile.echo-dpo.cbor"),
            include_bytes!(
                "../../../fixtures/providers/echo-target-profile/generated/primary/target-profile.echo-dpo.cbor"
            )
            .as_slice(),
        ),
    ] {
        test_ok(fs::write(path, bytes), "write application fixture");
    }
}

fn application_fixture_provider_manifest() -> Value {
    serde_json::json!({
        "apiVersion": TARGET_PROVIDER_MANIFEST_API_VERSION,
        "providerAbi": TARGET_PROVIDER_ABI,
        "provider": resource_json("echo.edict-provider@1", '1'),
        "artifacts": [
            {
                "role": "target-profile.echo-dpo",
                "artifactKind": "targetProfile",
                "resource": {
                    "coordinate": "echo.dpo@1",
                    "digest": "sha256:2e2494121aecf5e6a2d920f5fb85408825d394765fad41484c416397c920fb04"
                },
                "source": {
                    "kind": "generated",
                    "semanticSource": resource_json("echo.semantic-schema@1", '2'),
                    "generator": resource_json("echo-wesley-gen.provider-artifact-generator@1", '3')
                }
            },
            {
                "role": "schema.echo-provider-artifacts",
                "artifactKind": "artifactSchema",
                "resource": resource_json("echo.provider-artifacts.cddl@1", '4'),
                "source": {
                    "kind": "generated",
                    "semanticSource": resource_json("echo.semantic-schema@1", '2'),
                    "generator": resource_json("echo-wesley-gen.provider-artifact-generator@1", '3')
                }
            }
        ],
        "schemaBindings": [{
            "domain": "echo.generated-artifact/v1",
            "schemaRole": "schema.echo-provider-artifacts",
            "format": "selfContainedCddlV1",
            "rootRule": "generated-artifact"
        }]
    })
}

fn write_external_action_application(root: &Path) -> PathBuf {
    write_application_fixture_resources(root);
    let provider_directory = root.join("provider");
    let provider_manifest = application_fixture_provider_manifest();
    test_ok(
        fs::write(
            provider_directory.join("provider-manifest.echo.json"),
            test_ok(
                serde_json::to_vec_pretty(&provider_manifest),
                "encode provider manifest",
            ),
        ),
        "write provider manifest",
    );

    let application = serde_json::json!({
        "schema": "edict.application/v1",
        "buildKind": "externalAction",
        "coordinate": "examples.workspace_observer@1",
        "sources": ["src/observe-workspace.amblicit"],
        "lawpacks": [{
            "manifest": "vendor/workspace-snapshot/manifest.cbor",
            "exports": "vendor/workspace-snapshot/exports.cbor",
            "adapter": "vendor/workspace-snapshot/adapter.cbor",
            "targetConfiguration": "vendor/workspace-snapshot/request-profile-configuration.cbor"
        }],
        "externalActionResources": [
            {"artifact": "vendor/workspace-snapshot/input-schema.cbor"},
            {"artifact": "vendor/workspace-snapshot/settlement-schema.cbor"},
            {"artifact": "vendor/workspace-snapshot/reconciliation-law.cbor"}
        ],
        "target": {
            "profile": "echo.dpo@1",
            "providerPackage": "provider"
        },
        "outputDirectory": ".build/application"
    });
    let config_path = root.join("edict.application.json");
    test_ok(
        fs::write(
            &config_path,
            test_ok(
                serde_json::to_vec_pretty(&application),
                "encode application manifest",
            ),
        ),
        "write application manifest",
    );
    config_path
}

#[test]
fn application_signature_mismatch_has_structured_field_context() {
    let root = temp_tree("signature");
    let config = write_external_action_application(&root);
    let vendor = root.join("vendor/hello-echo");
    fs::create_dir_all(&vendor).expect("create exact lawpack fixture");
    for (name, bytes) in [
        (
            "manifest.cbor",
            include_bytes!("../../../fixtures/lawpack/hello-echo/manifest.cbor").as_slice(),
        ),
        (
            "exports.cbor",
            include_bytes!("../../../fixtures/lawpack/hello-echo/exports.cbor").as_slice(),
        ),
        (
            "adapter.cbor",
            include_bytes!("../../../fixtures/lawpack/hello-echo/adapter.cbor").as_slice(),
        ),
        (
            "configuration.cbor",
            include_bytes!(
                "../../../fixtures/lawpack/hello-echo/echo-operation-configuration.cbor"
            )
            .as_slice(),
        ),
    ] {
        fs::write(vendor.join(name), bytes).expect("write exact lawpack resource");
    }
    let mut application: Value =
        serde_json::from_slice(&fs::read(&config).expect("read config")).expect("config JSON");
    application
        .as_object_mut()
        .expect("config object")
        .remove("buildKind");
    application["coordinate"] = json!("examples.hello_echo@1");
    application["lawpacks"] = json!([{"manifest":"vendor/hello-echo/manifest.cbor","exports":"vendor/hello-echo/exports.cbor","adapter":"vendor/hello-echo/adapter.cbor","targetConfiguration":"vendor/hello-echo/configuration.cbor"}]);
    application["externalActionResources"] = json!([]);
    fs::write(
        &config,
        serde_json::to_vec(&application).expect("encode fixture config"),
    )
    .expect("write config");
    let declaration = "type WideInput = { basis: String<max=128>, key: String<max=65>, message: String<max=256>, };\n\n";
    let source = include_str!("../../../fixtures/lawpack/hello-echo/create-greeting.amblicit")
        .replace(
            "type GreetingCreated",
            &format!("{declaration}type GreetingCreated"),
        )
        .replace("input: hello.CreateGreetingInput", "input: WideInput");
    fs::write(root.join("src/observe-workspace.amblicit"), source).expect("write mismatch source");
    let output = build(&config);
    assert_eq!(output.status.code(), Some(2));
    let stream = records(&output.stderr);
    summary(&stream, "ApplicationCompilationFailed");
    let cause = stream
        .iter()
        .find(|r| {
            r["kind"] == "TypeMismatch"
                && r["message"]
                    .as_str()
                    .is_some_and(|m| m.contains("input.key"))
        })
        .expect("field mismatch cause");
    assert_eq!(cause["signatureMismatch"]["position"], "input");
    assert_eq!(
        cause["signatureMismatch"]["path"],
        json!([{"kind":"field","name":"key"}])
    );
    assert_eq!(
        cause["signatureMismatch"]["expectedType"],
        "String<max=64,canonical=raw-utf8>"
    );
    assert_eq!(
        cause["signatureMismatch"]["actualType"],
        "String<max=65,canonical=raw-utf8>"
    );
    validate_stream(&stream);
    assert!(!root.join(".build/application").exists());
    fs::remove_dir_all(root).expect("remove owned fixture");
}

fn write_module_application(root: &Path, libraries: &[(&str, &str, &str)]) -> PathBuf {
    let config = write_external_action_application(root);
    let source_path = root.join("src/observe-workspace.amblicit");
    let mut source = fs::read_to_string(&source_path).expect("root source");
    let mut imports = String::new();
    let mut sources = vec![json!("src/observe-workspace.amblicit")];
    for (coordinate, name, library) in libraries {
        let pin =
            amblicit_syntax::source_closure::source_module_digest(library).expect("source digest");
        writeln!(
            imports,
            "use module {coordinate} digest \"{pin}\" as {name};"
        )
        .expect("format import");
        let path = format!("src/{name}.amblicit");
        fs::write(root.join(&path), library).expect("library source");
        sources.push(json!(path));
    }
    source = source.replacen(
        "package examples.workspace_observer@1;",
        &format!("package examples.workspace_observer@1;\n{imports}"),
        1,
    );
    fs::write(&source_path, source).expect("importing root source");
    let mut manifest: Value =
        serde_json::from_slice(&fs::read(&config).expect("config")).expect("JSON");
    manifest["sources"] = json!(sources);
    fs::write(
        &config,
        serde_json::to_vec(&manifest).expect("encode config"),
    )
    .expect("module config");
    config
}

#[test]
fn application_build_emits_original_library_locations_across_modules() {
    let root = temp_tree("module-compiler");
    let first = "package library.first@1;\r\n// 🦀\r\nfn broken() -> U64 { return true; }";
    let second = "package library.second@1;\nfn broken() -> U64 { return false; }";
    // Manifest dependency order is deliberately the reverse of package order.
    let config = write_module_application(
        &root,
        &[
            ("library.second@1", "second", second),
            ("library.first@1", "first", first),
        ],
    );
    let output = build(&config);
    assert_eq!(output.status.code(), Some(2));
    let stream = records(&output.stderr);
    summary(&stream, "ApplicationCompilationFailed");
    for (name, source, anchor) in [
        ("first", first, "return true"),
        ("second", second, "return false"),
    ] {
        let path = root.join(format!("src/{name}.amblicit"));
        let canonical = fs::canonicalize(&path)
            .expect("canonical path")
            .display()
            .to_string();
        let cause = stream
            .iter()
            .find(|record| {
                record["kind"] == "TypeMismatch" && record["sourceLocation"]["path"] == canonical
            })
            .expect("original-library mismatch");
        assert_location(
            cause,
            &path,
            source,
            source.find(anchor).expect("literal anchor"),
        );
    }
    let causes = stream
        .iter()
        .filter(|record| record.get("sourceLocation").is_some())
        .count();
    assert_eq!(
        stream.last().expect("terminal record")["errors"],
        causes + 1
    );
    validate_stream(&stream);
    assert!(!root.join(".build/application").exists());
    fs::remove_dir_all(root).expect("remove fixture");
}

#[test]
fn application_build_reports_dependency_parse_and_link_failures_at_original_paths() {
    for (label, library, summary_kind, stage, kind, anchor) in [
        (
            "parse",
            "package library.bad@1;\nfn broken() -> U64 { return ; }",
            "InvalidApplicationSource",
            "parse",
            "ExpectedToken",
            "; }",
        ),
        (
            "link",
            "package library.bad@1;\nfn broken() -> U64 { return missing(); }",
            "ApplicationSourceLinkingFailed",
            "resolve",
            "UnresolvedName",
            "missing",
        ),
    ] {
        let root = temp_tree(label);
        let config = write_module_application(&root, &[("library.bad@1", "bad", library)]);
        let output = build(&config);
        assert_eq!(output.status.code(), Some(2));
        let stream = records(&output.stderr);
        summary(&stream, summary_kind);
        let cause = stream
            .iter()
            .find(|record| record["stage"] == stage && record["kind"] == kind)
            .unwrap_or_else(|| panic!("expected {stage}/{kind}: {stream:?}"));
        assert_location(
            cause,
            &root.join("src/bad.amblicit"),
            library,
            library.find(anchor).expect("diagnostic anchor"),
        );
        assert_eq!(stream.last().expect("terminal record")["errors"], 2);
        validate_stream(&stream);
        assert!(!root.join(".build/application").exists());
        fs::remove_dir_all(root).expect("remove fixture");
    }
}

#[test]
fn application_module_surface_failures_preserve_original_locations() {
    for in_root in [false, true] {
        let root = temp_tree("module-surface");
        let library = "package library.bad@1;\r\n// 🦀\r\nfn repeated() -> U64 { return 1; }\r\nfn repeated() -> U64 { return 2; }";
        let config = write_module_application(&root, &[("library.bad@1", "bad", library)]);
        let path = if in_root {
            root.join("src/observe-workspace.amblicit")
        } else {
            root.join("src/bad.amblicit")
        };
        if in_root {
            let mut source = fs::read_to_string(&path).expect("root source");
            source.push_str(
                "\nfn repeated() -> U64 { return 1; }\nfn repeated() -> U64 { return 2; }",
            );
            fs::write(&path, source).expect("invalid root source");
            // Keep the dependency valid so both cases test their own file.
            let valid = "package library.bad@1; fn valid() -> U64 { return 1; }";
            let pin =
                amblicit_syntax::source_closure::source_module_digest(valid).expect("valid pin");
            let old =
                amblicit_syntax::source_closure::source_module_digest(library).expect("old pin");
            fs::write(root.join("src/bad.amblicit"), valid).expect("valid dependency");
            let source = fs::read_to_string(&path)
                .expect("root source")
                .replace(&old.to_string(), &pin.to_string());
            fs::write(&path, source).expect("pin dependency");
        }
        let source = fs::read_to_string(&path).expect("original source");
        let module = amblicit_syntax::parse_module(&source).expect("parsable surface error");
        let errors = amblicit_syntax::validate_surface(&module).expect_err("duplicate declaration");
        let output = build(&config);
        assert_eq!(output.status.code(), Some(2));
        let stream = records(&output.stderr);
        summary(&stream, "ApplicationCompilationFailed");
        for error in errors {
            let record = stream
                .iter()
                .find(|r| {
                    r["kind"] == "SurfaceValidation" && r["span"]["start"] == error.span.start
                })
                .unwrap_or_else(|| panic!("missing surface diagnostic: {stream:?}"));
            assert_eq!(record["message"], error.message);
            assert_location(record, &path, &source, error.span.start);
        }
        validate_stream(&stream);
        assert!(!root.join(".build/application").exists());
        fs::remove_dir_all(root).expect("remove fixture");
    }
}

#[test]
fn application_module_build_prepares_shared_lawpack_aliases() {
    for substituted in [false, true] {
        let root = temp_tree("module-lawpack");
        let digest = if substituted {
            format!("sha256:{}", "0".repeat(64))
        } else {
            include_str!("../../../fixtures/lawpack/workspace-snapshot/manifest.sha256")
                .trim()
                .to_owned()
        };
        let library = format!("package library.shared@1; use lawpack workspace.snapshot@1 digest \"{digest}\" as local; type Value = U64;");
        let config = write_module_application(&root, &[("library.shared@1", "shared", &library)]);
        let output = build(&config);
        if substituted {
            assert_eq!(output.status.code(), Some(2));
            let stream = records(&output.stderr);
            summary(&stream, "InvalidApplicationClosure");
            validate_stream(&stream);
            assert!(!root.join(".build/application").exists());
        } else {
            assert_eq!(
                output.status.code(),
                Some(0),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            let core =
                fs::read(root.join(".build/application/core.cbor")).expect("public compiled Core");
            let target = fs::read(root.join(".build/application/target-ir.cbor"))
                .expect("public lowered Target IR");
            amblicit_syntax::decode_canonical_cbor(&core).expect("canonical Core");
            amblicit_syntax::decode_canonical_cbor(&target).expect("canonical Target IR");
        }
        fs::remove_dir_all(root).expect("remove fixture");
    }
}

#[path = "support/application_intrinsics.rs"]
mod application_intrinsics;

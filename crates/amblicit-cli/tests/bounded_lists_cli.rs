//! Public source-to-Core/Target projection; executable provider acceptance is separate.
use serde_json::{json, Value};
use std::io::Write;
use std::process::{Command, Stdio};

const SOURCE: &str = include_str!("../../../fixtures/lang/functions/bounded-worklist.amblicit");

fn project(source: &str) -> Vec<Value> {
    let settings = json!({
        "schema": "edict.compiler.settings/v1", "type": "compilerSettings", "operation": "project",
        "emit": ["diagnostics", "core", "targetIr"],
        "compilerContext": {
            "operationProfiles": [{"source": "p.read", "core": "continuum.profile.read-only/v1"}],
            "budgets": [{"source": "p.large", "budget": {"maxSteps": 1_000_000, "maxAllocatedBytes": 1_000_000, "maxOutputBytes": 1_000_000}}]
        },
        "target": {"coordinate": "echo.dpo@1", "profileDigest": format!("sha256:{}", "1".repeat(64)), "irDomain": "echo.span-ir/v1", "operationProfiles": ["continuum.profile.read-only/v1"], "obstructionCoordinates": [], "effectLowerings": []}
    });
    let input = json!({"schema":"edict.compiler.input/v1", "type":"compilerInput", "kind":"source", "name":"bounded-worklist.amblicit", "source":source});
    let mut child = Command::new(env!("CARGO_BIN_EXE_amblicit"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(format!("{settings}\n{input}\n").as_bytes())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(output.stderr, [] as [u8; 0]);
    String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

#[test]
fn public_compiler_projects_bounded_worklists_repeatably() {
    let first = project(SOURCE);
    let second = project(SOURCE);
    for kind in ["core", "targetIr"] {
        let artifact =
            |records: &[Value]| records.iter().find(|r| r["type"] == kind).unwrap().clone();
        let a = artifact(&first);
        let b = artifact(&second);
        assert_eq!(a["state"], "available", "{first:?}");
        assert_eq!(a["digest"], b["digest"]);
        assert!(a["digest"].as_str().unwrap().starts_with("sha256:"));
    }
    let core = first.iter().find(|r| r["type"] == "core").unwrap();
    assert!(core["review"]["requiredCoreCapabilities"]
        .as_array()
        .unwrap()
        .contains(&json!("core.list/v1")));
    assert_eq!(
        first.iter().find(|r| r["type"] == "status").unwrap()["errors"],
        0
    );
}

#[test]
fn public_compiler_reports_unsafe_list_access_and_small_result_bounds() {
    for (source, kind) in [
        (
            SOURCE.replace("index < listLen(xs)", "index <= listLen(xs)"),
            "UnsupportedSourceShape",
        ),
        (
            SOURCE.replace("let joined =", "let joined: List<Item,max=2> ="),
            "TypeMismatch",
        ),
        (
            SOURCE.replace("max=2>, right", "max=18446744073709551615>, right"),
            "InvalidBound",
        ),
    ] {
        let records = project(&source);
        let diagnostics = records.iter().find(|r| r["type"] == "diagnostics").unwrap()
            ["diagnostics"]
            .as_array()
            .unwrap();
        assert!(diagnostics.iter().any(|d| d["kind"] == kind), "{records:?}");
        assert_ne!(
            records.iter().find(|r| r["type"] == "core").unwrap()["state"],
            "available"
        );
    }
}

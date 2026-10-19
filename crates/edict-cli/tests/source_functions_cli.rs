//! Real public JSONL projection of source-authored functions.
use serde_json::{json, Value};
use std::io::Write;
use std::process::{Command, Stdio};

#[test]
fn public_project_compiles_jim_source_functions_to_authenticated_core_and_target() {
    let source = include_str!("../../../fixtures/lang/functions/range-assembly.edict");
    let settings = json!({
        "schema": "edict.compiler.settings/v1", "type": "compilerSettings",
        "operation": "project", "emit": ["diagnostics", "core", "targetIr"],
        "compilerContext": {
            "operationProfiles": [{"source": "text.replaceRange", "core": "continuum.profile.read-only/v1"}],
            "budgets": [{"source": "text.replaceRangeBudget", "budget": {
                "maxSteps": 4096, "maxAllocatedBytes": 65536, "maxOutputBytes": 4096
            }}]
        },
        "target": {
            "coordinate": "echo.dpo@1",
            "profileDigest": format!("sha256:{}", "1".repeat(64)),
            "irDomain": "echo.span-ir/v1",
            "operationProfiles": ["continuum.profile.read-only/v1"],
            "obstructionCoordinates": [], "effectLowerings": []
        }
    });
    let input = json!({"schema": "edict.compiler.input/v1", "type": "compilerInput",
        "kind": "source", "name": "RangeAssembly.edict", "source": source});
    let mut child = Command::new(env!("CARGO_BIN_EXE_edict"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("public compiler starts");
    let request = format!("{settings}\n{input}\n");
    child
        .stdin
        .take()
        .unwrap()
        .write_all(request.as_bytes())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(output.stderr, Vec::<u8>::new());
    let records: Vec<Value> = String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    for kind in ["core", "targetIr"] {
        let record = records
            .iter()
            .find(|record| record["type"] == kind)
            .unwrap();
        assert_eq!(record["state"], "available", "{record}");
        assert!(record["digest"].as_str().unwrap().starts_with("sha256:"));
    }
    let core = records
        .iter()
        .find(|record| record["type"] == "core")
        .unwrap();
    assert!(
        core.pointer("/review/functions/assembleFragments")
            .is_some(),
        "source-owned body is part of the public review"
    );
}

#[test]
fn public_project_deep_expression_returns_a_diagnostic_without_aborting() {
    let nested = format!("{}value{}", "expression(".repeat(129), ")".repeat(129));
    let source = format!(
        "package functions.example@1;\n\
         fn expression(value: U64) -> U64 {{ return value; }}\n\
         fn nested(value: U64) -> U64 {{ return {nested}; }}\n\
         type Input = {{ value: U64, other: U64, }};\n\
         type Output = {{ value: U64, }};\n\
         intent evaluate(input: Input) returns Output profile p.read basis none budget <= p.small {{\n\
         return {{ value: input.value }};\n}}"
    );
    let settings = json!({
        "schema": "edict.compiler.settings/v1", "type": "compilerSettings",
        "operation": "project", "emit": ["diagnostics"],
        "compilerContext": {
            "operationProfiles": [{"source": "p.read", "core": "continuum.profile.read-only/v1"}],
            "budgets": [{"source": "p.small", "budget": {
                "maxSteps": 4096, "maxAllocatedBytes": u64::MAX, "maxOutputBytes": u64::MAX
            }}]
        }
    });
    let input = json!({"schema": "edict.compiler.input/v1", "type": "compilerInput",
        "kind": "source", "name": "DeepExpression.edict", "source": source});
    // The compiler runs as its normal process, without the unit witness's larger test stack.
    let mut child = Command::new(env!("CARGO_BIN_EXE_edict"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("public compiler starts");
    child
        .stdin
        .take()
        .unwrap()
        .write_all(format!("{settings}\n{input}\n").as_bytes())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert_eq!(
        output.status.code(),
        Some(0),
        "stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(output.stderr, Vec::<u8>::new());
    let records: Vec<Value> = String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    let projection = records
        .iter()
        .find(|record| record["type"] == "diagnostics")
        .expect("successful project emits the diagnostic inspection");
    let diagnostics = projection["diagnostics"].as_array().unwrap();
    assert_eq!(diagnostics.len(), 1, "{records:?}");
    assert_eq!(diagnostics[0]["kind"], "InvalidBound", "{records:?}");
    let status = records
        .iter()
        .find(|record| record["type"] == "status")
        .unwrap();
    assert_eq!(status["errors"], 1, "{records:?}");
}

//! The public Target review preserves mapped failure semantics without adding
//! a failure field to ordinary bindings. Oracle: the authored source below.
use serde_json::{json, Value};
use std::io::Write;
use std::process::{Command, Stdio};

const SOURCE: &str = r"package review.example@1;
failure Stop;
type Input = { value: U64 };
type Output = { value: U64 };
fn checked(value: U64) -> U64 fails Stop {
  return if value < 10u64 then value else fail Stop;
}
intent evaluate(input: Input) returns Output profile p.read basis none budget <= p.small {
  let value = checked(input.value) else { Stop => text.Stopped({ code: 42u64 }), };
  let ordinary = value;
  return { value: ordinary };
}
";

fn project(domain: &str) -> Vec<Value> {
    project_source(domain, SOURCE)
}

fn project_source(domain: &str, source: &str) -> Vec<Value> {
    let settings = json!({
        "schema": "edict.compiler.settings/v1", "type": "compilerSettings",
        "operation": "project", "emit": ["diagnostics", "core", "targetIr"],
        "compilerContext": {
            "operationProfiles": [{"source": "p.read", "core": "continuum.profile.read-only/v1"}],
            "budgets": [{"source": "p.small", "budget": {
                "maxSteps": 100_000, "maxAllocatedBytes": 1_048_576, "maxOutputBytes": 4096
            }}]
        },
        "target": {
            "coordinate": "echo.dpo@1",
            "profileDigest": format!("sha256:{}", "1".repeat(64)),
            "irDomain": domain,
            "operationProfiles": ["continuum.profile.read-only/v1"],
            "obstructionCoordinates": [], "effectLowerings": []
        }
    });
    let input = json!({
        "schema": "edict.compiler.input/v1", "type": "compilerInput",
        "kind": "source", "name": "TypedFailureReview.edict", "source": source,
    });
    let mut child = Command::new(env!("CARGO_BIN_EXE_amblicit"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("public compiler starts");
    child
        .stdin
        .take()
        .expect("compiler stdin")
        .write_all(format!("{settings}\n{input}\n").as_bytes())
        .expect("JSONL request");
    let output = child.wait_with_output().expect("compiler exits");
    assert!(
        output.status.success(),
        "{domain}: stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stderr.is_empty(), "{domain}: unexpected stderr");
    let records: Vec<Value> = String::from_utf8(output.stdout)
        .expect("UTF-8 output")
        .lines()
        .map(|line| serde_json::from_str(line).expect("JSONL record"))
        .collect();
    for kind in ["core", "targetIr"] {
        let record = records
            .iter()
            .find(|record| record["type"] == kind)
            .expect("projection");
        assert_eq!(record["state"], "available", "{domain}: {record}");
    }
    records
}

fn record<'a>(records: &'a [Value], kind: &str) -> &'a Value {
    records
        .iter()
        .find(|record| record["type"] == kind)
        .expect("projection")
}

#[test]
fn target_review_displays_mapped_failure_coordinate_reason_and_payload() {
    let expected = json!({"review.example@1.Stop": {
        "reasonKind": "text.Stopped",
        "payload": {"code": {"kind": "const", "value": {
            "kind": "int", "width": "U64", "value": "42"
        }}}
    }});
    for domain in ["echo.span-ir/v2", "echo.span-ir/v3"] {
        let records = project(domain);
        assert_eq!(
            record(&records, "core")["review"]["intents"]["evaluate"]["body"]["nodes"][0]
                ["failureMap"],
            expected,
            "{domain}: Core control"
        );
        assert_eq!(
            record(&records, "targetIr")["review"]["intents"]["evaluate"]["pureBindings"][0]
                ["failureArms"],
            expected,
            "{domain}: Target review must expose the binding's mapped failures"
        );
    }
}

#[test]
fn target_review_omits_failure_arms_for_an_ordinary_binding() {
    for domain in ["echo.span-ir/v2", "echo.span-ir/v3"] {
        let records = project(domain);
        let bindings = record(&records, "targetIr")["review"]["intents"]["evaluate"]
            ["pureBindings"]
            .as_array()
            .expect("pure bindings");
        assert_eq!(bindings.len(), 2, "{domain}: mapped and ordinary binding");
        assert_eq!(bindings[1]["id"], "evaluate.binding.1");
        assert!(
            bindings[1].get("failureArms").is_none(),
            "{domain}: ordinary binding retains its existing review shape"
        );
    }
}

#[test]
fn public_review_preserves_ordered_record_entry_sequence() {
    let source = format!(
        "{SOURCE}\n{}",
        r"
failure First;
failure Second;
type Pair = { z: U64, a: U64 };
fn ordered() -> Pair fails First, Second { return { z: fail First, a: fail Second }; }
fn total() -> Pair { return { z: 1u64, a: 2u64 }; }
"
    );
    for domain in ["echo.span-ir/v2", "echo.span-ir/v3"] {
        let records = project_source(domain, &source);
        let functions = &record(&records, "core")["review"]["functions"];
        assert_eq!(
            functions["ordered"]["body"]["result"],
            json!({
                "kind": "orderedRecord",
                "fields": [
                    {"name":"z", "value":{"kind":"fail", "failure":"review.example@1.First"}},
                    {"name":"a", "value":{"kind":"fail", "failure":"review.example@1.Second"}},
                ],
            }),
            "{domain}: review retains source evaluation order"
        );
        assert_eq!(functions["total"]["body"]["result"]["kind"], "record");
        assert!(
            functions["total"]["body"]["result"]["fields"].is_object(),
            "{domain}: total record retains its existing review shape"
        );
    }
}

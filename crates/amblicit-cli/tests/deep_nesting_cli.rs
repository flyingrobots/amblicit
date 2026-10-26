//! Process-boundary witnesses for source nesting (amblicit#277).
//!
//! Each case runs the normal `amblicit` binary on its default main-thread
//! stack. A stack overflow aborts the child, which fails the assertion here
//! instead of the test harness.

use std::io::Write;
use std::process::{Command, Output, Stdio};

use amblicit_syntax::MAX_SOURCE_NESTING_DEPTH;
use serde_json::{json, Value};

const VALID_SOURCE: &str = r#"package examples.hello@1;

use lawpack hello.optics@1 digest "sha256:0000000000000000000000000000000000000000000000000000000000000000" as hello;

type HelloInput = {
  name: String<max=256>,
};

intent sayHello(input: HelloInput)
  returns HelloInput
  profile hello.readOnly
  basis none
  budget <= hello.tinyBudget
{
  return { name: input.name };
}
"#;

/// An intent whose body is `body`, around the hello fixture's header.
fn intent_with_body(body: &str) -> String {
    format!(
        "package examples.deep@1;\n\n\
         use lawpack hello.optics@1 digest \"sha256:{zeros}\" as hello;\n\n\
         type HelloInput = {{\n  name: String<max=256>,\n}};\n\n\
         intent sayHello(input: HelloInput)\n  returns HelloInput\n  profile hello.readOnly\n  \
         basis none\n  budget <= hello.tinyBudget\n{{\n{body}\n  return {{ name: input.name }};\n}}\n",
        zeros = "0".repeat(64)
    )
}

fn let_expr(expr: &str) -> String {
    intent_with_body(&format!("  let x = {expr};"))
}

/// Every source shape whose nesting the parser either recurses into or builds
/// into a deeper syntax tree without recursing.
fn shapes(n: usize) -> Vec<(&'static str, String)> {
    vec![
        (
            "parentheses",
            let_expr(&format!("{}true{}", "(".repeat(n), ")".repeat(n))),
        ),
        ("not", let_expr(&format!("{}true", "!".repeat(n)))),
        ("negation", let_expr(&format!("{}1", "-".repeat(n)))),
        (
            "calls",
            let_expr(&format!("{}true{}", "f(".repeat(n), ")".repeat(n))),
        ),
        (
            "lists",
            let_expr(&format!("{}true{}", "[".repeat(n), "]".repeat(n))),
        ),
        (
            "records",
            let_expr(&format!("{}true{}", "{ a: ".repeat(n), " }".repeat(n))),
        ),
        (
            "ternaries",
            let_expr(&format!("{}true", "if true then true else ".repeat(n))),
        ),
        (
            "binary chain",
            let_expr(&format!("true{}", " || true".repeat(n))),
        ),
        (
            "field chain",
            let_expr(&format!("input{}", ".name".repeat(n))),
        ),
        ("call chain", let_expr(&format!("f{}", "()".repeat(n)))),
        (
            "if statements",
            intent_with_body(&format!("{}{}", "if true {\n".repeat(n), "}\n".repeat(n))),
        ),
        (
            "else-if chain",
            intent_with_body(&format!("if true {{}}{}", " else if true {}".repeat(n))),
        ),
        (
            "types",
            intent_with_body(&format!(
                "  let x: {}String<max=1>{} = input.name;",
                "Option<".repeat(n),
                ">".repeat(n)
            )),
        ),
    ]
}

fn check(sources: &[(&str, &str)]) -> Output {
    let mut request = json!({
        "schema": "edict.compiler.settings/v1",
        "type": "compilerSettings",
        "operation": "check",
    })
    .to_string();
    for (name, source) in sources {
        request.push('\n');
        request.push_str(
            &json!({
                "schema": "edict.compiler.input/v1",
                "type": "compilerInput",
                "kind": "source",
                "name": name,
                "source": source,
            })
            .to_string(),
        );
    }
    request.push('\n');
    let mut child = Command::new(env!("CARGO_BIN_EXE_amblicit"))
        .env_remove(amblicit_cli::MAX_STDIN_BYTES_ENV)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn amblicit binary");
    child
        .stdin
        .take()
        .expect("stdin pipe")
        .write_all(request.as_bytes())
        .expect("write jsonl stdin");
    child.wait_with_output().expect("collect output")
}

fn records(bytes: &[u8]) -> Vec<Value> {
    String::from_utf8(bytes.to_vec())
        .expect("UTF-8 JSONL")
        .lines()
        .map(|line| serde_json::from_str(line).expect("JSONL record"))
        .collect()
}

fn describe(output: &Output) -> String {
    format!(
        "status={:?} stdout={} stderr={}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

#[test]
fn nesting_beyond_the_limit_is_a_parse_diagnostic_not_an_abort() {
    let mut failures = Vec::new();
    for (shape, source) in shapes(20_000) {
        assert!(source.len() < 1024 * 1024, "{shape} fits the source limit");
        let output = check(&[("deep.amblicit", &source)]);
        let refused = output.status.code() == Some(1) && {
            let stderr = records(&output.stderr);
            let diagnostic = stderr.iter().find(|record| record["type"] == "diagnostic");
            diagnostic.is_some_and(|d| d["stage"] == "parse" && d["kind"] == "NestingTooDeep")
                && stderr
                    .iter()
                    .any(|record| record["type"] == "status" && record["exitCode"] == 1)
        };
        if !refused {
            failures.push(format!("{shape}: {}", describe(&output)));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn nesting_at_the_limit_completes_without_aborting() {
    // Recursive shapes also spend a few levels on the statement and the
    // enclosing `let` expression, so they sit just under the limit.
    for (shape, source) in shapes(MAX_SOURCE_NESTING_DEPTH - 4) {
        let output = check(&[("deep.amblicit", &source)]);
        assert!(
            matches!(output.status.code(), Some(0 | 1)),
            "{shape}: {}",
            describe(&output)
        );
        let stderr = records(&output.stderr);
        assert!(
            stderr
                .iter()
                .all(|record| record["kind"] != "NestingTooDeep"),
            "{shape} within the limit must not be refused for nesting: {}",
            describe(&output)
        );
    }
}

#[test]
fn a_deep_input_does_not_lose_the_rest_of_the_request() {
    // A failed check withholds every `checkResult` and reports diagnostics with
    // a checked count. Before the limit, the abort lost that whole response.
    let deep = let_expr(&format!("{}true{}", "(".repeat(20_000), ")".repeat(20_000)));
    let output = check(&[("hello.amblicit", VALID_SOURCE), ("deep.amblicit", &deep)]);
    assert_eq!(output.status.code(), Some(1), "{}", describe(&output));
    assert!(output.stdout.is_empty(), "{}", describe(&output));
    let stderr = records(&output.stderr);
    let diagnostics: Vec<&Value> = stderr
        .iter()
        .filter(|record| record["type"] == "diagnostic")
        .collect();
    assert_eq!(diagnostics.len(), 1, "{}", describe(&output));
    assert_eq!(diagnostics[0]["input"]["name"], "deep.amblicit");
    assert_eq!(diagnostics[0]["kind"], "NestingTooDeep");
    let status = stderr
        .iter()
        .find(|record| record["type"] == "status")
        .expect("status record");
    assert_eq!(
        status["checked"], 1,
        "the valid input is still checked: {status}"
    );
    assert_eq!(status["errors"], 1, "{status}");
}

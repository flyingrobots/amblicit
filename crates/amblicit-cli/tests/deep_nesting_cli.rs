//! Process-boundary witnesses for source nesting (amblicit#277).
//!
//! Each case runs the normal `amblicit` binary as a child process. A stack
//! overflow aborts the child, which fails the assertion here instead of the
//! test harness.

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

/// Thirteen source shapes covering each guarded chokepoint (`expr`, `stmt`,
/// `type_ref`), each directly self-recursive production (`unary`, `else if`)
/// and each loop-built chain (operators, fields, calls); see the nesting-limit
/// rule in `crates/amblicit-syntax/src/parser.rs`.
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

fn request(settings: &Value, sources: &[(&str, &str)]) -> String {
    let mut request = settings.to_string();
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
    request
}

fn check_settings() -> Value {
    json!({
        "schema": "edict.compiler.settings/v1",
        "type": "compilerSettings",
        "operation": "check",
    })
}

/// `project` with the compiler context CLI-TP-040 uses, so the compiler, Core
/// and canonical stages run on the source.
fn project_settings() -> Value {
    json!({
        "schema": "edict.compiler.settings/v1",
        "type": "compilerSettings",
        "operation": "project",
        "emit": ["diagnostics"],
        "compilerContext": {
            "operationProfiles": [{"source": "p.read", "core": "continuum.profile.read-only/v1"}],
            "budgets": [{"source": "p.small", "budget": {
                "maxSteps": 4096, "maxAllocatedBytes": u64::MAX, "maxOutputBytes": u64::MAX
            }}]
        }
    })
}

/// Run `command` on `request`. The stdin cap override is removed so a
/// developer's environment cannot shrink the cap below these 20,000-token
/// sources.
fn run_request(command: &mut Command, request: &str) -> Output {
    let mut child = command
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

fn check(sources: &[(&str, &str)]) -> Output {
    run_request(
        &mut Command::new(env!("CARGO_BIN_EXE_amblicit")),
        &request(&check_settings(), sources),
    )
}

fn project(sources: &[(&str, &str)]) -> Output {
    run_request(
        &mut Command::new(env!("CARGO_BIN_EXE_amblicit")),
        &request(&project_settings(), sources),
    )
}

/// The main-thread stack a POSIX shell gives the binary when `ulimit -s` is
/// set to 1 MiB: the smallest default main-thread stack among the platforms
/// the binary is built for.
#[cfg(unix)]
const SMALL_MAIN_THREAD_STACK_KIB: usize = 1024;

/// Run `check` in a child whose main thread has a 1 MiB stack.
#[cfg(unix)]
fn check_on_small_main_thread(sources: &[(&str, &str)]) -> Output {
    let mut command = Command::new("sh");
    command
        .arg("-c")
        .arg(format!(
            "ulimit -s {SMALL_MAIN_THREAD_STACK_KIB} && exec \"$0\"",
        ))
        .arg(env!("CARGO_BIN_EXE_amblicit"));
    run_request(&mut command, &request(&check_settings(), sources))
}

/// A module whose source function `nested` holds `body`, as CLI-TP-040 shapes
/// it, so `project` type-checks and lowers the expression.
fn function_with_body(body: &str) -> String {
    format!(
        "package functions.deep@1;\n\
         fn expression(value: U64) -> U64 {{ return value; }}\n\
         fn nested(value: U64) -> U64 {{ return {body}; }}\n\
         type Input = {{ value: U64, other: U64, }};\n\
         type Output = {{ value: U64, }};\n\
         intent evaluate(input: Input) returns Output profile p.read basis none budget <= p.small {{\n\
         return {{ value: input.value }};\n}}"
    )
}

/// Node-building shapes inside a source function body.
fn function_shapes(n: usize) -> Vec<(&'static str, String)> {
    vec![
        (
            "calls",
            function_with_body(&format!(
                "{}value{}",
                "expression(".repeat(n),
                ")".repeat(n)
            )),
        ),
        (
            "parentheses",
            function_with_body(&format!("{}value{}", "(".repeat(n), ")".repeat(n))),
        ),
        (
            "lists",
            function_with_body(&format!("{}value{}", "[".repeat(n), "]".repeat(n))),
        ),
        (
            "records",
            function_with_body(&format!("{}value{}", "{ value: ".repeat(n), " }".repeat(n))),
        ),
        (
            "negation",
            function_with_body(&format!("{}value", "-".repeat(n))),
        ),
        (
            "binary chain",
            function_with_body(&format!("value{}", " + value".repeat(n))),
        ),
        (
            "field chain",
            function_with_body(&format!("value{}", ".value".repeat(n))),
        ),
    ]
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
fn nesting_just_under_the_limit_completes_without_aborting() {
    // Recursive shapes also spend a few levels on the statement and the
    // enclosing `let` expression, so they sit just under the limit. Every
    // shape checks clean: `check` stops after parsing and surface validation.
    for (shape, source) in shapes(MAX_SOURCE_NESTING_DEPTH - 4) {
        let output = check(&[("deep.amblicit", &source)]);
        assert_eq!(
            output.status.code(),
            Some(0),
            "{shape}: {}",
            describe(&output)
        );
        assert!(output.stderr.is_empty(), "{shape}: {}", describe(&output));
    }
}

#[test]
fn project_just_under_the_limit_completes_without_aborting() {
    // `check` stops after parsing and surface validation. `project` runs the
    // compiler, Core integrity and canonical stages on the tree the parser
    // accepted, so this is the witness that those stages survive the limit.
    for (shape, source) in function_shapes(MAX_SOURCE_NESTING_DEPTH - 4) {
        let output = project(&[("deep.amblicit", &source)]);
        assert_eq!(
            output.status.code(),
            Some(0),
            "{shape}: {}",
            describe(&output)
        );
        let stdout = records(&output.stdout);
        let status = stdout
            .iter()
            .find(|record| record["type"] == "status")
            .unwrap_or_else(|| panic!("{shape}: status record: {}", describe(&output)));
        assert_eq!(status["checked"], 1, "{shape}: {status}");
        let refused_for_nesting = stdout
            .iter()
            .filter(|record| record["type"] == "diagnostics")
            .flat_map(|record| {
                record["diagnostics"]
                    .as_array()
                    .cloned()
                    .unwrap_or_default()
            })
            .any(|diagnostic| diagnostic["kind"] == "NestingTooDeep");
        assert!(
            !refused_for_nesting,
            "{shape} within the limit must not be refused for nesting: {}",
            describe(&output)
        );
    }
}

#[cfg(unix)]
#[test]
fn the_limit_holds_on_a_small_main_thread_stack() {
    // The binary must not depend on the platform's default main-thread stack:
    // Windows gives the main thread 1 MiB, far less than the parser needs to
    // reach the limit in a debug build.
    let source = let_expr(&format!(
        "{}true{}",
        "(".repeat(MAX_SOURCE_NESTING_DEPTH - 4),
        ")".repeat(MAX_SOURCE_NESTING_DEPTH - 4)
    ));
    let output = check_on_small_main_thread(&[("deep.amblicit", &source)]);
    assert_eq!(output.status.code(), Some(0), "{}", describe(&output));
    let deep = let_expr(&format!("{}true{}", "(".repeat(20_000), ")".repeat(20_000)));
    let output = check_on_small_main_thread(&[("deep.amblicit", &deep)]);
    assert_eq!(output.status.code(), Some(1), "{}", describe(&output));
    let stderr = records(&output.stderr);
    assert!(
        stderr
            .iter()
            .any(|record| record["type"] == "diagnostic" && record["kind"] == "NestingTooDeep"),
        "{}",
        describe(&output)
    );
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

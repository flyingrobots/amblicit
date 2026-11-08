//! Process-boundary witnesses for source nesting (amblicit#277).
//!
//! Each case runs the normal `amblicit` binary as a child process. A stack
//! overflow aborts the child, which fails the assertion here instead of the
//! test harness.

use std::io::Write;
use std::process::{Command, Output, Stdio};

use amblicit_syntax::{MAX_CANONICAL_NESTING_DEPTH, MAX_SOURCE_NESTING_DEPTH};
use serde_json::{json, Value};

/// The deepest nesting every shape in this file is accepted at. Parentheses,
/// `!`, `-`, calls, lists, records, types and `else if` are accepted at
/// `MAX - 2` and refused one level deeper; the chains, ternaries and nested
/// `if` are accepted one level deeper still. The exact boundary of every shape
/// is pinned by `every_shape_has_the_documented_boundary` in
/// `crates/amblicit-syntax/tests/parse_nesting.rs`.
const DEEPEST_COMMON_NESTING: usize = MAX_SOURCE_NESTING_DEPTH - 2;

/// The deepest nested-call source function whose Core module the canonical
/// encoder accepts, measured (see `project_encodes_the_deepest_calls_the_pipeline_accepts`,
/// which pins it from both sides): each Core call costs two canonical
/// nesting levels, and the module around the function costs the rest of
/// `MAX_CANONICAL_NESTING_DEPTH`. Between this depth and the compiler's own
/// bound of 128 nested calls, `core` and `digests` emission fails as a
/// `CoreDigest` CLI failure (amblicit#302).
const DEEPEST_ENCODABLE_CALL_NESTING: usize = (MAX_CANONICAL_NESTING_DEPTH - 6) / 2;

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

/// `project` with the compiler context CLI-TP-040 uses, so `type_check` and,
/// when it passes, `lower_core` with Core integrity run on the source; `emit`
/// decides whether the Core module is also canonically encoded.
fn project_settings(emit: &[&str]) -> Value {
    json!({
        "schema": "edict.compiler.settings/v1",
        "type": "compilerSettings",
        "operation": "project",
        "emit": emit,
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

fn project(emit: &[&str], sources: &[(&str, &str)]) -> Output {
    run_request(
        &mut Command::new(env!("CARGO_BIN_EXE_amblicit")),
        &request(&project_settings(emit), sources),
    )
}

/// The `diagnostics` entries of a `project` run's stdout records.
fn project_diagnostics(stdout: &[Value]) -> Vec<Value> {
    stdout
        .iter()
        .filter(|record| record["type"] == "diagnostics")
        .flat_map(|record| {
            record["diagnostics"]
                .as_array()
                .cloned()
                .unwrap_or_default()
        })
        .collect()
}

fn status_record<'a>(records: &'a [Value], output: &Output) -> &'a Value {
    records
        .iter()
        .find(|record| record["type"] == "status")
        .unwrap_or_else(|| panic!("status record: {}", describe(output)))
}

/// Split a projection's stdout into its parsed records and the raw `core`
/// line. The `core` record embeds the Core review tree, which at the depths
/// this file projects nests past `serde_json`'s default recursion limit when
/// parsed back here; the CLI's writer has no such limit, so the line is kept
/// whole and read for its `state` and `digest` fields.
fn projection_records(stdout: &[u8]) -> (Vec<Value>, Option<String>) {
    let mut records = Vec::new();
    let mut core = None;
    for line in String::from_utf8(stdout.to_vec())
        .expect("UTF-8 JSONL")
        .lines()
    {
        if line.contains(r#""type":"core""#) {
            core = Some(line.to_owned());
        } else {
            records.push(serde_json::from_str(line).expect("JSONL record"));
        }
    }
    (records, core)
}

/// Byte offset of `nested`'s body in [`function_with_body`].
fn function_body_offset(source: &str) -> usize {
    let marker = "fn nested(value: U64) -> U64 { return ";
    source.find(marker).expect("nested function") + marker.len()
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

/// Shapes inside a source function body, with the compiler diagnostic each
/// one gets at any depth above zero (`None` for a shape that compiles).
/// Parentheses build no node, so the compiler sees a leaf; lists, records and
/// negation are refused at the root; calls, the binary chain and the field
/// chain are walked (see `type_check_walks_a_tree_at_the_deepest_common_depth`).
/// The kinds are today's reasons for refusing each shape in a `U64` function
/// (a list or record is not a `U64`; `-` applies only to a literal; source
/// `U64` addition is unsupported; `value` has no field `value`; nested calls
/// pass the compiler's depth bound), not facts about nesting: if one of those
/// language rules changes, update the kind here.
fn function_shapes(n: usize) -> Vec<(&'static str, String, Option<&'static str>)> {
    vec![
        (
            "calls",
            function_with_body(&format!(
                "{}value{}",
                "expression(".repeat(n),
                ")".repeat(n)
            )),
            Some("InvalidBound"),
        ),
        (
            "parentheses",
            function_with_body(&format!("{}value{}", "(".repeat(n), ")".repeat(n))),
            None,
        ),
        (
            "lists",
            function_with_body(&format!("{}value{}", "[".repeat(n), "]".repeat(n))),
            Some("TypeMismatch"),
        ),
        (
            "records",
            function_with_body(&format!("{}value{}", "{ value: ".repeat(n), " }".repeat(n))),
            Some("TypeMismatch"),
        ),
        (
            "negation",
            function_with_body(&format!("{}value", "-".repeat(n))),
            Some("UnsupportedSourceShape"),
        ),
        (
            "binary chain",
            function_with_body(&format!("value{}", " + value".repeat(n))),
            Some("UnsupportedSourceShape"),
        ),
        (
            "field chain",
            function_with_body(&format!("value{}", ".value".repeat(n))),
            Some("TypeMismatch"),
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
            let diagnostics: Vec<&Value> = stderr
                .iter()
                .filter(|record| record["type"] == "diagnostic")
                .collect();
            diagnostics.len() == 1
                && diagnostics[0]["stage"] == "parse"
                && diagnostics[0]["kind"] == "NestingTooDeep"
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
fn nesting_at_the_deepest_common_depth_checks_clean() {
    // Every shape checks clean: `check` stops after parsing and surface
    // validation, and `DEEPEST_COMMON_NESTING` is within every shape's bound.
    for (shape, source) in shapes(DEEPEST_COMMON_NESTING) {
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
fn project_at_the_deepest_common_depth_does_not_abort() {
    // `check` stops after parsing. `project` hands the accepted tree to the
    // compiler, which refuses every node-building source-function body deeper
    // than 128 (its own graph depth bound) inside `type_check`, so Core
    // integrity and canonical encoding never see these trees. This witness proves that
    // the compiler reaches its own diagnostic for each shape, in a debug
    // build, without aborting; `project` exits 0 whatever the compiler says,
    // so the diagnostic kind and error count are what is asserted.
    for (shape, source, expected) in function_shapes(DEEPEST_COMMON_NESTING) {
        let output = project(&["diagnostics"], &[("deep.amblicit", &source)]);
        assert_eq!(
            output.status.code(),
            Some(0),
            "{shape}: {}",
            describe(&output)
        );
        let stdout = records(&output.stdout);
        let status = status_record(&stdout, &output);
        let diagnostics = project_diagnostics(&stdout);
        let kinds: Vec<&Value> = diagnostics.iter().map(|d| &d["kind"]).collect();
        if let Some(kind) = expected {
            assert_eq!(status["errors"], 1, "{shape}: {status}");
            assert_eq!(kinds, [kind], "{shape}: {}", describe(&output));
        } else {
            assert_eq!(status["errors"], 0, "{shape}: {status}");
            assert!(kinds.is_empty(), "{shape}: {}", describe(&output));
        }
    }
}

#[test]
fn type_check_walks_a_tree_at_the_deepest_common_depth() {
    // A root refusal and a deep walk both end in one diagnostic; the span
    // tells them apart. These left-spine shapes are refused at their
    // innermost leaf, so the span covers that leaf alone, which the checker
    // can only reach by walking every level above it.
    let n = DEEPEST_COMMON_NESTING;
    let leaves = [
        (
            "binary chain",
            function_with_body(&format!("value{}", " + value".repeat(n))),
            "value + value",
        ),
        (
            "field chain",
            function_with_body(&format!("value{}", ".value".repeat(n))),
            "value.value",
        ),
    ];
    for (shape, source, leaf) in leaves {
        let output = project(&["diagnostics"], &[("deep.amblicit", &source)]);
        assert_eq!(
            output.status.code(),
            Some(0),
            "{shape}: {}",
            describe(&output)
        );
        let diagnostics = project_diagnostics(&records(&output.stdout));
        assert_eq!(diagnostics.len(), 1, "{shape}: {}", describe(&output));
        let span = &diagnostics[0]["span"];
        let start = function_body_offset(&source);
        assert_eq!(span["start"], start, "{shape}: {}", diagnostics[0]);
        assert_eq!(
            span["end"],
            start + leaf.len(),
            "{shape}: {}",
            diagnostics[0]
        );
    }
    // Nested calls are refused by the compiler's own depth bound, which it
    // reaches only by walking past 128 levels.
    let source = function_with_body(&format!(
        "{}value{}",
        "expression(".repeat(n),
        ")".repeat(n)
    ));
    let output = project(&["diagnostics"], &[("deep.amblicit", &source)]);
    let diagnostics = project_diagnostics(&records(&output.stdout));
    assert_eq!(diagnostics.len(), 1, "{}", describe(&output));
    assert_eq!(diagnostics[0]["kind"], "InvalidBound", "{}", diagnostics[0]);
}

#[test]
fn project_encodes_the_deepest_calls_the_pipeline_accepts() {
    // The deepest node-building tree that reaches Core integrity and canonical
    // encoding is bounded by those stages, not by the parser: `type_check` and
    // `lower_core` accept 128 nested calls, and canonical encoding accepts 61
    // (amblicit#302 tracks the gap between them).
    let calls = |n: usize| {
        function_with_body(&format!(
            "{}value{}",
            "expression(".repeat(n),
            ")".repeat(n)
        ))
    };
    let output = project(
        &["diagnostics", "core", "digests"],
        &[("deep.amblicit", &calls(DEEPEST_ENCODABLE_CALL_NESTING))],
    );
    assert_eq!(output.status.code(), Some(0), "{}", describe(&output));
    let (stdout, core) = projection_records(&output.stdout);
    assert_eq!(
        status_record(&stdout, &output)["errors"],
        0,
        "{}",
        describe(&output)
    );
    let core = core.unwrap_or_else(|| panic!("core record: {}", describe(&output)));
    // The review tree carries no `state` or `digest` key of its own, so each
    // occurs once, as the record's field.
    assert_eq!(core.matches(r#""state":"#).count(), 1, "{core}");
    assert!(core.contains(r#""state":"available""#), "{core}");
    assert_eq!(core.matches(r#""digest":"#).count(), 1, "{core}");
    assert!(core.contains(r#""digest":"sha256:"#), "{core}");

    let output = project(
        &["diagnostics", "core", "digests"],
        &[("deep.amblicit", &calls(DEEPEST_ENCODABLE_CALL_NESTING + 1))],
    );
    assert_eq!(output.status.code(), Some(2), "{}", describe(&output));
    let stderr = records(&output.stderr);
    assert!(
        stderr
            .iter()
            .any(|record| record["kind"] == "CoreDigest"
                && record["message"] == "NestingLimitExceeded"),
        "one level deeper fails canonical encoding (amblicit#302): {}",
        describe(&output)
    );

    let output = project(&["diagnostics"], &[("deep.amblicit", &calls(128))]);
    assert_eq!(output.status.code(), Some(0), "{}", describe(&output));
    let stdout = records(&output.stdout);
    assert_eq!(
        status_record(&stdout, &output)["errors"],
        0,
        "{}",
        describe(&output)
    );
}

#[cfg(unix)]
#[test]
fn the_limit_holds_on_a_small_main_thread_stack() {
    // The binary must not depend on the platform's default main-thread stack:
    // Windows gives the main thread 1 MiB, far less than the parser needs to
    // reach the limit in a debug build.
    let source = let_expr(&format!(
        "{}true{}",
        "(".repeat(DEEPEST_COMMON_NESTING),
        ")".repeat(DEEPEST_COMMON_NESTING)
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

//! Source nesting limit (amblicit#277): the parser refuses nesting past
//! [`MAX_SOURCE_NESTING_DEPTH`] with a stable diagnostic instead of
//! exhausting the stack.

use amblicit_syntax::{parse_module, ParseError, ParseErrorKind, MAX_SOURCE_NESTING_DEPTH};

/// Debug builds spend far more stack per nesting level than release builds;
/// give the witness a stack that covers the limit in either profile.
const WITNESS_STACK_BYTES: usize = 64 * 1024 * 1024;

fn parse_on_witness_stack(source: String) -> Result<(), ParseError> {
    std::thread::Builder::new()
        .stack_size(WITNESS_STACK_BYTES)
        .spawn(move || parse_module(&source).map(drop))
        .expect("spawn witness thread")
        .join()
        .expect("parser does not panic")
}

/// A named source builder for a shape of the given size.
type SourceShape = (&'static str, fn(usize) -> String);

fn intent_with_body(body: &str) -> String {
    format!(
        "package examples.deep@1;\n\
         type Input = {{ ok: Bool, }};\n\
         intent run(input: Input) returns Input profile p.read basis none budget <= p.small {{\n\
         {body}\n  return {{ ok: input.ok }};\n}}\n"
    )
}

fn let_expr(expr: &str) -> String {
    intent_with_body(&format!("  let x = {expr};"))
}

fn assert_nesting_refused(shape: &str, source: String) {
    let error = parse_on_witness_stack(source).expect_err(shape);
    assert_eq!(
        error.kind,
        ParseErrorKind::NestingTooDeep,
        "{shape}: {error}"
    );
    assert_eq!(error.kind.code(), "NestingTooDeep");
}

fn assert_parses(shape: &str, source: String) {
    if let Err(error) = parse_on_witness_stack(source) {
        panic!("{shape} within the limit must parse: {error}");
    }
}

#[test]
fn chains_built_without_recursion_have_an_exact_height_boundary() {
    // `k` operators build a tree `k` tall, and the enclosing `let` statement
    // is one more level, so `MAX - 1` operators is the deepest accepted chain.
    // Height, unlike depth, is not spent by the `let` right-hand side: `expr`
    // adds height only for a ternary.
    let n = MAX_SOURCE_NESTING_DEPTH - 1;
    let shapes: [SourceShape; 3] = [
        ("binary chain", |k| {
            let_expr(&format!("true{}", " || true".repeat(k)))
        }),
        ("field chain", |k| {
            let_expr(&format!("input{}", ".ok".repeat(k)))
        }),
        ("call chain", |k| let_expr(&format!("f{}", "()".repeat(k)))),
    ];
    for (shape, source) in shapes {
        assert_parses(shape, source(n));
        assert_nesting_refused(shape, source(n + 1));
    }
}

#[test]
fn chain_height_includes_nested_chains() {
    // Each parenthesised level holds a chain of `width` operators whose first
    // operand is the next level, so the tree is `levels * width` tall while
    // the parser recurses only `levels` deep.
    let (levels, width) = (16, 32);
    let mut expr = "true".to_owned();
    for _ in 0..levels {
        expr = format!("({expr}){}", " || true".repeat(width));
    }
    assert!(levels * width > MAX_SOURCE_NESTING_DEPTH);
    assert_nesting_refused("nested chains", let_expr(&expr));
}

#[test]
fn recursion_past_the_limit_is_refused() {
    let n = MAX_SOURCE_NESTING_DEPTH + 1;
    let cases = [
        (
            "parentheses",
            let_expr(&format!("{}true{}", "(".repeat(n), ")".repeat(n))),
        ),
        ("not", let_expr(&format!("{}true", "!".repeat(n)))),
        (
            "lists",
            let_expr(&format!("{}true{}", "[".repeat(n), "]".repeat(n))),
        ),
        (
            "records",
            let_expr(&format!("{}true{}", "{ a: ".repeat(n), " }".repeat(n))),
        ),
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
                "  let x: {}Bool{} = input.ok;",
                "Option<".repeat(n),
                ">".repeat(n)
            )),
        ),
        (
            "type-call type arguments",
            // Type arguments are parsed speculatively; the parentheses spend
            // the depth so that the refusal happens inside that speculation.
            let_expr(&format!(
                "{}f<{}Bool{}>(true){}",
                "(".repeat(n - 64),
                "Option<".repeat(64),
                ">".repeat(64),
                ")".repeat(n - 64)
            )),
        ),
        (
            "match scrutinees",
            let_expr(&format!(
                "{}input.ok{}",
                "match ".repeat(n),
                " { Yes => true }".repeat(n)
            )),
        ),
        (
            "match arm bodies",
            let_expr(&format!(
                "{}true{}",
                "match input.ok { Yes => ".repeat(n),
                " }".repeat(n)
            )),
        ),
        (
            "variant payloads",
            let_expr(&format!("{}true{}", "Kind::Yes(".repeat(n), ")".repeat(n))),
        ),
        (
            "for bodies",
            intent_with_body(&format!(
                "{}{}",
                "for item in input.ok bounded p.limit {\n".repeat(n),
                "}\n".repeat(n)
            )),
        ),
        ("yield blocks", intent_with_body(&nested_yield_blocks(n))),
    ];
    for (shape, source) in cases {
        assert_nesting_refused(shape, source);
    }
}

/// `n` effect-yield blocks, each holding the next as a statement.
fn nested_yield_blocks(n: usize) -> String {
    let mut body = "let x0 = true;".to_owned();
    for _ in 0..n {
        body = format!("let x = if true {{ {body} yield true; }} else {{ yield true; }};");
    }
    body
}

#[test]
fn recursive_shapes_have_an_exact_depth_boundary() {
    // Depth, unlike height (see the chain boundary test above), is spent by
    // the enclosing statement and by the `let` right-hand side, one level
    // each, so a source's own nesting is accepted up to `MAX - 2` levels and
    // refused at `MAX - 1`.
    let n = MAX_SOURCE_NESTING_DEPTH - 2;
    let shapes: [SourceShape; 3] = [
        ("parentheses", |k| {
            let_expr(&format!("{}true{}", "(".repeat(k), ")".repeat(k)))
        }),
        ("not", |k| let_expr(&format!("{}true", "!".repeat(k)))),
        // `<` starts a speculative type-call parse one level deeper. At the
        // boundary that speculation is refused on entry, which must read as
        // "not a type call" so the relational reading, which spends no level,
        // still parses. This leaf drives the speculation to its last branch:
        // `input.ok` parses as a type path and the `>` is consumed before the
        // adjacent-`(` test fails.
        ("relational leaf", |k| {
            let_expr(&format!(
                "{}input.ok < input.ok > input.ok{}",
                "(".repeat(k),
                ")".repeat(k)
            ))
        }),
    ];
    for (shape, source) in shapes {
        assert_parses(shape, source(n));
        assert_nesting_refused(shape, source(n + 1));
    }
}

#[test]
fn ordinary_nesting_still_parses() {
    let n = MAX_SOURCE_NESTING_DEPTH / 2;
    assert_parses(
        "parentheses",
        let_expr(&format!("{}true{}", "(".repeat(n), ")".repeat(n))),
    );
    assert_parses(
        "calls",
        let_expr(&format!("{}true{}", "f(".repeat(n), ")".repeat(n))),
    );
    assert_parses(
        "if statements",
        intent_with_body(&format!("{}{}", "if true {\n".repeat(n), "}\n".repeat(n))),
    );
}

#[test]
fn a_huge_chain_is_refused_on_a_default_test_thread() {
    // Half a mebibyte of `|| true` would make a tree far taller than any
    // recursive later stage could walk; the parser stops at the limit. The
    // asserted kind is the proof of refusal. Running on the default test
    // thread is strong evidence, not proof, that the tree was never built:
    // dropping 65,536 boxed nodes would most likely overflow that thread.
    let source = let_expr(&format!("true{}", " || true".repeat(65_536)));
    let error = parse_module(&source).expect_err("huge chain");
    assert_eq!(error.kind, ParseErrorKind::NestingTooDeep);
}

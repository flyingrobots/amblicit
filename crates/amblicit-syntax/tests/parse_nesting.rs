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
        ("negation", let_expr(&format!("{}1", "-".repeat(n)))),
        (
            "calls",
            let_expr(&format!("{}true{}", "f(".repeat(n), ")".repeat(n))),
        ),
        (
            "ternaries",
            let_expr(&format!("{}true", "if true then true else ".repeat(n))),
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

/// One row of the documented boundary table: a shape, its source at a
/// given nesting, and the deepest nesting it is accepted at.
type BoundaryRow = (&'static str, fn(usize) -> String, usize);

/// The rows of the boundary table whose context spends two levels.
fn depth_bounded_rows() -> Vec<BoundaryRow> {
    let depth_row = MAX_SOURCE_NESTING_DEPTH - 2;
    vec![
        (
            "parentheses",
            |k| let_expr(&format!("{}true{}", "(".repeat(k), ")".repeat(k))),
            depth_row,
        ),
        (
            "not",
            |k| let_expr(&format!("{}true", "!".repeat(k))),
            depth_row,
        ),
        (
            "negation",
            |k| let_expr(&format!("{}1", "-".repeat(k))),
            depth_row,
        ),
        (
            "calls",
            |k| let_expr(&format!("{}true{}", "f(".repeat(k), ")".repeat(k))),
            depth_row,
        ),
        (
            "lists",
            |k| let_expr(&format!("{}true{}", "[".repeat(k), "]".repeat(k))),
            depth_row,
        ),
        (
            "records",
            |k| let_expr(&format!("{}true{}", "{ a: ".repeat(k), " }".repeat(k))),
            depth_row,
        ),
        (
            "match scrutinees",
            |k| {
                let_expr(&format!(
                    "{}input.ok{}",
                    "match ".repeat(k),
                    " { Yes => true }".repeat(k)
                ))
            },
            depth_row,
        ),
        (
            "match arm bodies",
            |k| {
                let_expr(&format!(
                    "{}true{}",
                    "match input.ok { Yes => ".repeat(k),
                    " }".repeat(k)
                ))
            },
            depth_row,
        ),
        (
            "variant payloads",
            |k| let_expr(&format!("{}true{}", "Kind::Yes(".repeat(k), ")".repeat(k))),
            depth_row,
        ),
        (
            "types",
            |k| {
                intent_with_body(&format!(
                    "  let x: {}Bool{} = input.ok;",
                    "Option<".repeat(k),
                    ">".repeat(k)
                ))
            },
            depth_row,
        ),
        (
            "else-if chain",
            |k| intent_with_body(&format!("if true {{}}{}", " else if true {}".repeat(k))),
            depth_row,
        ),
    ]
}

/// The rows of the boundary table whose leaf or form spends its own levels.
fn rows_with_their_own_overhead() -> Vec<BoundaryRow> {
    let depth_row = MAX_SOURCE_NESTING_DEPTH - 2;
    vec![
        // `<` starts a speculative type-call parse one level deeper. At the
        // boundary that speculation is refused for nesting, and because the
        // tokens are not type-call arguments, the relational reading, which
        // spends no level, still parses.
        (
            "relational leaf",
            |k| {
                let_expr(&format!(
                    "{}input.ok < input.ok > input.ok{}",
                    "(".repeat(k),
                    ")".repeat(k)
                ))
            },
            depth_row,
        ),
        // The leaf type of the type arguments spends one more level.
        (
            "type-call arguments",
            |k| {
                let_expr(&format!(
                    "f<{}Bool{}>(true)",
                    "Option<".repeat(k),
                    ">".repeat(k)
                ))
            },
            MAX_SOURCE_NESTING_DEPTH - 3,
        ),
        // Each effect `if` spends two levels: its `let` statement and the
        // statement inside its yield block.
        (
            "yield blocks",
            |k| intent_with_body(&nested_yield_blocks(k)),
            (MAX_SOURCE_NESTING_DEPTH - 2) / 2,
        ),
    ]
}

/// The rows of the boundary table whose context spends one level: chains
/// bounded by height, and statements and ternaries that nest without the
/// `let` expression's level.
fn height_bounded_rows() -> Vec<BoundaryRow> {
    let height_row = MAX_SOURCE_NESTING_DEPTH - 1;
    vec![
        (
            "binary chain",
            |k| let_expr(&format!("true{}", " || true".repeat(k))),
            height_row,
        ),
        (
            "field chain",
            |k| let_expr(&format!("input{}", ".ok".repeat(k))),
            height_row,
        ),
        (
            "call chain",
            |k| let_expr(&format!("f{}", "()".repeat(k))),
            height_row,
        ),
        (
            "ternaries",
            |k| let_expr(&format!("{}true", "if true then true else ".repeat(k))),
            height_row,
        ),
        (
            "if statements",
            |k| intent_with_body(&format!("{}{}", "if true {\n".repeat(k), "}\n".repeat(k))),
            height_row,
        ),
        (
            "for bodies",
            |k| {
                intent_with_body(&format!(
                    "{}{}",
                    "for item in input.ok bounded p.limit {\n".repeat(k),
                    "}\n".repeat(k)
                ))
            },
            height_row,
        ),
    ]
}

#[test]
fn every_shape_has_the_documented_boundary() {
    // The table in `MAX_SOURCE_NESTING_DEPTH`'s doc and the syntax README. The
    // first row spends two levels outside its own nesting (the statement and
    // the `let` expression, or for types the leaf type, for `else if` the
    // condition), so it stops at `MAX - 2`. The second row spends one: the
    // chains are bounded by height, which only the statement adds above them,
    // and ternaries and nested `if` nest without the `let` expression's level.
    let rows = depth_bounded_rows()
        .into_iter()
        .chain(rows_with_their_own_overhead())
        .chain(height_bounded_rows());
    for (shape, source, deepest) in rows {
        assert_parses(shape, source(deepest));
        assert_nesting_refused(shape, source(deepest + 1));
    }
}

/// A node-building production wrapped around an operator chain: the opening
/// and closing text of one level.
type Wrapper = (&'static str, &'static str, &'static str);

#[test]
fn height_composes_through_every_node_building_production() {
    // `levels` levels of a production around a chain of `width` operators
    // build a tree `levels + width` tall (plus one for the statement), while
    // the parser recurses only `levels + 2` deep. So the height bound, not the
    // depth bound, decides, and only if each level is counted by its
    // `composite`: a production that forgets it under-counts by one level per
    // nesting and accepts a chain one operator too long. Parentheses build no
    // node and add no height.
    let levels = 64;
    let wrappers: [Wrapper; 8] = [
        ("lists", "[", "]"),
        ("records", "{ a: ", " }"),
        ("call arguments", "f(", ")"),
        ("not", "!(", ")"),
        ("negation", "-(", ")"),
        ("match arm bodies", "match input.ok { Yes => ", " }"),
        ("variant payloads", "Kind::Yes(", ")"),
        ("ternaries", "if true then (", ") else true"),
    ];
    let source = |open: &str, close: &str, width: usize| {
        let_expr(&format!(
            "{}true{}{}",
            open.repeat(levels),
            " || true".repeat(width),
            close.repeat(levels)
        ))
    };
    let deepest = MAX_SOURCE_NESTING_DEPTH - 1 - levels;
    for (shape, open, close) in wrappers {
        assert_parses(shape, source(open, close, deepest));
        assert_nesting_refused(shape, source(open, close, deepest + 1));
    }
    let deepest_in_parentheses = MAX_SOURCE_NESTING_DEPTH - 1;
    assert_parses("parentheses", source("(", ")", deepest_in_parentheses));
    assert_nesting_refused("parentheses", source("(", ")", deepest_in_parentheses + 1));
}

#[test]
fn deep_type_arguments_keep_their_kind_wherever_they_sit() {
    // The relational reading of type-call arguments stops early at a `,` or
    // at the `=` of a refinement, so whether the speculation's nesting refusal
    // stands must be decided by the shape of the tokens, not by where the
    // relational reading later fails.
    let deep_option = |k: usize| format!("{}Bool{}", "Option<".repeat(k), ">".repeat(k));
    // A deep type argument after a shallow one, at the top level.
    assert_nesting_refused(
        "deep second type argument",
        let_expr(&format!(
            "f<Bool, {}>(true)",
            deep_option(MAX_SOURCE_NESTING_DEPTH)
        )),
    );
    // A shallow-looking type call in a call argument, at the boundary.
    let in_call = |n: usize| {
        let_expr(&format!(
            "{}g(f<Bool, Option<Bool>>(true)){}",
            "(".repeat(n),
            ")".repeat(n)
        ))
    };
    assert_parses(
        "type call in a call argument",
        in_call(MAX_SOURCE_NESTING_DEPTH - 5),
    );
    assert_nesting_refused(
        "type call in a call argument",
        in_call(MAX_SOURCE_NESTING_DEPTH - 4),
    );
    // Commas inside the type arguments, at the boundary where the innermost
    // expression runs at the depth limit's last admitted level.
    for call in ["f<A, B<Bool>>(true)", "f<List<Bool, max=1>>(true)"] {
        let wrapped = |n: usize| let_expr(&format!("{}{call}{}", "(".repeat(n), ")".repeat(n)));
        assert_parses(call, wrapped(MAX_SOURCE_NESTING_DEPTH - 4));
        assert_nesting_refused(call, wrapped(MAX_SOURCE_NESTING_DEPTH - 3));
    }
    // A refinement's `=` inside the type arguments.
    let refined = |n: usize| {
        let_expr(&format!(
            "{}f<Option<String<max=1>>>(true){}",
            "(".repeat(n),
            ")".repeat(n)
        ))
    };
    assert_parses(
        "refined type argument",
        refined(MAX_SOURCE_NESTING_DEPTH - 4),
    );
    assert_nesting_refused(
        "refined type argument",
        refined(MAX_SOURCE_NESTING_DEPTH - 3),
    );
}

#[test]
fn balanced_comparisons_are_not_mistaken_for_type_calls() {
    // These comparisons balance their angle brackets before an adjacent `(`,
    // so they look like type calls by bracket count alone. Their speculative
    // type reading is refused for nesting, but `b < c > d` is not a type
    // argument (a nested `<...>` must be followed by `,` or `>`), so they are
    // read as comparisons and parse wherever that reading fits.
    let n = MAX_SOURCE_NESTING_DEPTH - 3;
    assert_parses(
        "balanced comparison at the limit",
        let_expr(&format!(
            "{}a < b < c > d >(e){}",
            "(".repeat(n),
            ")".repeat(n)
        )),
    );
    let names = |prefix: &str, count: usize, separator: &str| {
        (1..=count)
            .map(|i| format!("{prefix}{i}"))
            .collect::<Vec<_>>()
            .join(separator)
    };
    let deep_balanced = format!("a<{}>{}>(e)", names("b", 57, "<"), names("c", 56, ">"));
    assert_parses(
        "deeply balanced comparison",
        let_expr(&format!(
            "{}{deep_balanced}{}",
            "(".repeat(198),
            ")".repeat(198)
        )),
    );
}

#[test]
fn many_comparisons_at_the_limit_parse() {
    // Every argument's `<` here is refused on entry, and every one of those
    // refusals asks whether the tokens from that `<` form type-call
    // arguments. The answers are computed once per parse, so a long argument
    // list costs linear time rather than one rescan of the rest per argument.
    let arguments = vec!["input < input"; 20_000].join(", ");
    let n = MAX_SOURCE_NESTING_DEPTH - 3;
    assert_parses(
        "comparisons in call arguments at the limit",
        let_expr(&format!("{}g({arguments}){}", "(".repeat(n), ")".repeat(n))),
    );
}

#[test]
fn a_malformed_deep_comparison_reports_its_syntax_error() {
    // The speculative type reading of this comparison is refused for nesting,
    // but its tokens are not type-call arguments, so the source is read as a
    // comparison and its real syntax error is reported.
    let n = MAX_SOURCE_NESTING_DEPTH - 3;
    let source = let_expr(&format!(
        "{}input.ok < input.ok < input.ok + {}",
        "(".repeat(n),
        ")".repeat(n)
    ));
    let error = parse_on_witness_stack(source).expect_err("malformed comparison");
    assert_eq!(error.kind, ParseErrorKind::ExpectedToken, "{error}");
}

#[test]
fn a_type_call_at_the_limit_is_still_refused() {
    // At 254 parentheses, where a relational leaf still parses, the type
    // call's speculative parse is refused on entry, and because its tokens
    // have the shape of type-call arguments the refusal stands. One
    // parenthesis fewer, the type arguments fit and the call parses.
    let n = MAX_SOURCE_NESTING_DEPTH - 2;
    for call in ["f<Bool>(true)", "f<Bool>()"] {
        assert_parses(
            call,
            let_expr(&format!("{}{call}{}", "(".repeat(n - 1), ")".repeat(n - 1))),
        );
        assert_nesting_refused(
            call,
            let_expr(&format!("{}{call}{}", "(".repeat(n), ")".repeat(n))),
        );
    }
}

#[test]
fn comparison_chains_parse_wherever_their_relational_reading_fits() {
    // Each `<` of `a < b < c`, read speculatively as type arguments, costs a
    // depth level that the relational reading does not. The comparison must
    // parse at every depth where the relational reading fits, including just
    // below the boundary, where the speculation is refused inside its type
    // arguments rather than on entry.
    let leaf = "input.ok < input.ok < input.ok";
    let wrapped = |n: usize| let_expr(&format!("{}{leaf}{}", "(".repeat(n), ")".repeat(n)));
    for n in MAX_SOURCE_NESTING_DEPTH - 5..=MAX_SOURCE_NESTING_DEPTH - 2 {
        assert_parses("comparison chain in parentheses", wrapped(n));
    }
    assert_nesting_refused(
        "comparison chain in parentheses",
        wrapped(MAX_SOURCE_NESTING_DEPTH - 1),
    );
    // A long comparison chain is bounded by height, like any operator chain,
    // although its speculative type reading would nest one level per `<`. Its
    // operands are bare names: a field access such as `input.ok` is itself
    // one level of height.
    let chain = |k: usize| let_expr(&format!("input{}", " < input".repeat(k)));
    assert_parses("comparison chain", chain(MAX_SOURCE_NESTING_DEPTH - 1));
    assert_nesting_refused("comparison chain", chain(MAX_SOURCE_NESTING_DEPTH));
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

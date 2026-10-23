//! Conformance: keywords are reserved as bare identifiers but remain legal as
//! member names after `.` (SPEC Edict Language v1 §1510-1511).

mod common;
use amblicit_syntax::{parse_module, ParseErrorKind};
use common::body;

#[test]
fn keywords_are_rejected_as_bare_values() {
    for kw in [
        "then", "else", "yield", "for", "in", "bounded", "return", "let", "where", "require",
        "assert", "intent", "type", "enum",
    ] {
        let src = body(&format!("  let x = {kw};\n  return {{ x }};"));
        let err = parse_module(&src).expect_err("keyword must reject as bare value");
        assert_eq!(err.kind, ParseErrorKind::ReservedKeyword, "keyword `{kw}`");
    }
}

#[test]
fn keywords_are_rejected_as_let_binder_names() {
    // A binder is a bare identifier; `let match = …;` must not bind a keyword.
    for kw in ["match", "for", "return", "yield"] {
        let src = body(&format!("  let {kw} = input.a;\n  return {{ x }};"));
        let err = parse_module(&src).expect_err("keyword must reject as let binder");
        assert_eq!(err.kind, ParseErrorKind::ReservedKeyword, "keyword `{kw}`");
    }
}

#[test]
fn ternary_is_not_a_bare_operand() {
    // Per the grammar the ternary sits at the top of `expr`, so it is only an
    // operand when parenthesized; a bare `a + if …` must reject (and no longer
    // silently swallow `if` as an identifier).
    let bare = body("  let x = a + if c then b else d;\n  return { x };");
    let err = parse_module(&bare).expect_err("bare ternary operand must reject");
    assert_eq!(err.kind, ParseErrorKind::ReservedKeyword);

    let parenthesized = body("  let x = a + (if c then b else d);\n  return { x };");
    parse_module(&parenthesized).expect("parenthesized ternary is a valid operand");
}

#[test]
fn keywords_are_legal_as_member_names() {
    // SPEC §1511: `ref.ensure(value)` and `history.event.record(value)` are
    // legal even though `record` is a keyword — keywords may follow `.`.
    let src = body(
        "  let a = ref.ensure(input.value);\n\
         \x20 let b = history.event.record(input.value);\n\
         \x20 return { a, b };",
    );
    parse_module(&src).expect("keyword member names parse");
}

#[test]
fn prelude_constructors_are_not_reserved() {
    // `none`/`some`/`len`/`hash` are prelude values, not keywords; they must
    // remain usable as bare identifiers and call targets.
    let src = body(
        "  let n = none<shape.T>();\n\
         \x20 let s = some(input.x);\n\
         \x20 let k = len(input.bytes);\n\
         \x20 return { n, s, k };",
    );
    parse_module(&src).expect("prelude constructors parse as ordinary identifiers");
}

//! Negative parse fixtures: source the parser must reject.
//!
//! Phase 1 scope is syntactic rejection only. Semantic rejections (naked
//! unbounded `String`, clause requiredness, `migration`/`projection` as
//! *reserved* words, read-only proofs, etc.) belong to the semantic-validation
//! layer and are tracked separately.

use amblicit_syntax::{parse_module, ParseErrorKind};

fn rejects(src: &str, kind: ParseErrorKind) {
    let err = parse_module(src).expect_err("expected a parse error");
    assert_eq!(err.kind, kind, "wrong error for source:\n{src}");
}

#[test]
fn reserved_future_decls_are_rejected_at_top_level() {
    // `migration` / `projection` are reserved for future syntax; as v1 top-level
    // declarations they are not `type`/`intent` and must reject.
    rejects(
        "package a.b@1;\nmigration M = {};",
        ParseErrorKind::ExpectedToken,
    );
    rejects(
        "package a.b@1;\nprojection P = {};",
        ParseErrorKind::ExpectedToken,
    );
}

#[test]
fn missing_package_is_rejected() {
    rejects(
        "type T = { x: Bytes<max=1>, };",
        ParseErrorKind::ExpectedKeyword,
    );
}

#[test]
fn bytes_rejects_canonical_policy() {
    // Bytes are measured/hashed raw; only String may pin a canonicalization
    // policy (EDICT-LANG-BYTES-NOCANON-001). `Bytes<max=8, canonical=nfc>` must
    // not parse: after the max bound the grammar expects `>`, not `,`.
    rejects(
        "package a.b@1;\ntype T = { b: Bytes<max=8, canonical=nfc>, };",
        ParseErrorKind::ExpectedToken,
    );
}

#[test]
fn unterminated_string_is_rejected() {
    rejects(
        "package a.b@1;\nuse lawpack x.y@1 digest \"sha256:dead as z;",
        ParseErrorKind::Lex,
    );
}

//! Conformance: the bounded `hello` example parses to the expected AST shape.
//!
//! Fixture: `fixtures/lang/bounds/bounded-hello.edict` (positive fixture for
//! `EDICT-LANG-BOUNDS-001`). This is the first real-world parse target.

use amblicit_syntax::ast::{
    BoundRef, Decl, Expr, ImportKind, IntentClause, RecordEntry, ScalarRefine, Stmt, TypeExpr,
    TypeRef,
};
use amblicit_syntax::{parse_module, ParseErrorKind};

const HELLO: &str = include_str!("../../../fixtures/lang/bounds/bounded-hello.edict");

#[test]
fn bounded_hello_parses() {
    let module = parse_module(HELLO).expect("bounded-hello must parse");

    // package examples.hello@1;
    assert_eq!(module.package.path, vec!["examples", "hello"]);
    assert_eq!(module.package.version, "1");

    // use lawpack hello.optics@1 digest "sha256:..." as hello;
    assert_eq!(module.imports.len(), 1);
    let imp = &module.imports[0];
    assert_eq!(imp.kind, ImportKind::Lawpack);
    assert_eq!(imp.alias, "hello");
    assert_eq!(imp.package.as_ref().unwrap().path, vec!["hello", "optics"]);
    assert!(imp.digest.as_ref().unwrap().starts_with("sha256:"));

    // two type decls
    assert_eq!(module.decls.len(), 3);
    let Decl::Type(hello_input) = &module.decls[0] else {
        panic!("decl 0 is a type")
    };
    assert_eq!(hello_input.name, "HelloInput");
    let TypeExpr::Record(fields) = &hello_input.body else {
        panic!("HelloInput is a record")
    };
    assert_eq!(fields.len(), 1);
    assert_eq!(fields[0].name, "name");
    assert_eq!(
        fields[0].ty,
        TypeRef::StringTy(Some(ScalarRefine {
            max: BoundRef::Int {
                value: 256,
                suffix: None,
            },
            canonical: None
        }))
    );

    let Decl::Type(hello_reading) = &module.decls[1] else {
        panic!("decl 1 is a type")
    };
    assert_eq!(hello_reading.name, "HelloReading");

    // intent sayHello(...) ...
    let Decl::Intent(intent) = &module.decls[2] else {
        panic!("decl 2 is an intent")
    };
    assert_eq!(intent.name, "sayHello");
    assert_eq!(intent.params.len(), 1);
    assert_eq!(intent.params[0].name, "input");

    // clauses: profile, basis none, budget, where
    assert!(matches!(&intent.clauses[0], IntentClause::Profile(p) if p == &["hello", "readOnly"]));
    assert!(matches!(&intent.clauses[1], IntentClause::Basis(None)));
    assert!(matches!(&intent.clauses[2], IntentClause::Budget(b) if b == &["hello", "tinyBudget"]));
    assert!(matches!(&intent.clauses[3], IntentClause::Where(w) if w.len() == 1));

    // body: let message = "hello, " + input.name;  return { message };
    assert_eq!(intent.body.stmts.len(), 2);
    let Stmt::Let { name, value, .. } = &intent.body.stmts[0] else {
        panic!("stmt 0 is let")
    };
    assert_eq!(name, "message");
    assert!(
        matches!(value, Expr::Binary { .. }),
        "concat is a binary expr"
    );

    let Stmt::Return { value, .. } = &intent.body.stmts[1] else {
        panic!("stmt 1 is return")
    };
    let Expr::Record { entries, .. } = value else {
        panic!("return value is a record")
    };
    assert_eq!(entries.len(), 1);
    assert!(matches!(&entries[0], RecordEntry::Shorthand { name, .. } if name == "message"));
}

#[test]
fn missing_semicolon_is_a_parse_error() {
    let bad = "package examples.hello@1\n";
    let err = parse_module(bad).expect_err("missing package semicolon must reject");
    assert_eq!(err.kind, ParseErrorKind::ExpectedToken);
    assert!(
        err.message.contains("Semi"),
        "diagnostic should identify the missing semicolon: {err}"
    );
}

#[test]
fn multi_part_package_version() {
    let m = parse_module("package a.b@1.2.3-beta;").expect("multi-part version parses");
    assert_eq!(m.package.version, "1.2.3-beta");
}

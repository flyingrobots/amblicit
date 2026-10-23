//! Conformance: the statically bounded `for` loop parses, and a loop without
//! its mandatory `bounded` cardinality is rejected.
//!
//! Grammar: `for-stmt = "for" ident "in" expr "bounded" bound-ref block`
//! (SPEC Edict Language v1; every loop carries a provable maximum count).

mod common;
use amblicit_syntax::ast::{BoundRef, Stmt};
use amblicit_syntax::parse_module;
use common::{body, parse_ok};

fn first_stmt(src: &str) -> Stmt {
    parse_ok(src)
        .decls
        .into_iter()
        .next()
        .and_then(|d| match d {
            amblicit_syntax::ast::Decl::Intent(i) => i.body.stmts.into_iter().next(),
            _ => panic!("decl 0 is an intent"),
        })
        .expect("a statement")
}

#[test]
fn for_with_integer_bound_parses() {
    let src = body("  for item in input.items bounded 100 {\n    assert item.ok;\n  }");
    let Stmt::For {
        var, bound, body, ..
    } = first_stmt(&src)
    else {
        panic!("stmt 0 is a for");
    };
    assert_eq!(var, "item");
    assert_eq!(
        bound,
        BoundRef::Int {
            value: 100,
            suffix: None,
        }
    );
    assert_eq!(body.stmts.len(), 1, "one assert in the loop body");
}

#[test]
fn for_with_coordinate_bound_parses() {
    // The bound may be a digest-locked coordinate rather than a literal.
    let src = body("  for x in input.xs bounded rope.maxLeaves {\n    guarantee x.valid;\n  }");
    let Stmt::For { bound, .. } = first_stmt(&src) else {
        panic!("stmt 0 is a for");
    };
    assert_eq!(
        bound,
        BoundRef::Coord(vec!["rope".into(), "maxLeaves".into()])
    );
}

#[test]
fn for_without_bounded_is_rejected() {
    // An unbounded loop has no provable cardinality and must not parse.
    let src = body("  for item in input.items {\n    assert item.ok;\n  }");
    assert!(
        parse_module(&src).is_err(),
        "a `for` without `bounded` must reject"
    );
}

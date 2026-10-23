//! Conformance: the `if` family parses — pure ternary (`if … then … else …`),
//! the effectful branch-yield (`if pred { … yield a; } else { … yield b; }`,
//! legal only as a `let` rhs), and the statement-level `if`/`else if`/`else`.
//!
//! Grammar: `if-expr`, `effect-branch-expr`, `if-stmt` (SPEC Edict Language v1).

mod common;
use amblicit_syntax::ast::{ElseClause, Expr, Stmt};
use amblicit_syntax::parse_module;
use common::{body, intent_of};

const BLOB: &str = include_str!("../../../fixtures/lang/effects/conditional-blob.amblicit");

#[test]
fn pure_ternary_parses_as_let_value() {
    let src = body("  let x = if input.n == 0 then input.lo else input.hi;\n  return { x };");
    let m = parse_module(&src).expect("ternary parses");
    let intent = intent_of(&m);
    let Stmt::Let { value, els, .. } = &intent.body.stmts[0] else {
        panic!("stmt 0 is a let");
    };
    assert!(els.is_none(), "a ternary let has no effect-else handler");
    assert!(matches!(value, Expr::If { .. }), "rhs is a pure ternary");
}

#[test]
fn ternary_is_usable_in_nested_expression_position() {
    // The ternary sits at the top of `expr`, so it nests inside a call arg.
    let src = body("  let x = f(if input.a then input.b else input.c);\n  return { x };");
    let m = parse_module(&src).expect("nested ternary parses");
    let intent = intent_of(&m);
    let Stmt::Let {
        value: Expr::Call { args, .. },
        ..
    } = &intent.body.stmts[0]
    else {
        panic!("rhs is a call");
    };
    assert!(
        matches!(args[0], Expr::If { .. }),
        "the call arg is a ternary"
    );
}

#[test]
fn branch_yield_parses_only_as_let_rhs() {
    let src = body(
        "  let blob = if len(b) == 0 {\n\
         \x20   yield none<shape.T>();\n\
         \x20 } else {\n\
         \x20   let r = echo.ref<shape.T>(id);\n\
         \x20   yield some(r);\n\
         \x20 };\n\
         \x20 return { blob };",
    );
    let m = parse_module(&src).expect("branch-yield parses");
    let intent = intent_of(&m);
    let Stmt::Let {
        value:
            Expr::IfYield {
                then_block,
                else_block,
                ..
            },
        ..
    } = &intent.body.stmts[0]
    else {
        panic!("rhs is a branch-yield");
    };
    assert_eq!(then_block.stmts.len(), 0, "then branch is just a yield");
    assert_eq!(
        else_block.stmts.len(),
        1,
        "else branch has one let before yield"
    );
    assert!(
        matches!(*then_block.value, Expr::Call { .. }),
        "then yields a call"
    );
    assert!(
        matches!(*else_block.value, Expr::Call { .. }),
        "else yields a call"
    );
}

#[test]
fn branch_yield_without_yield_is_rejected() {
    // Each branch must end with `yield expr;`.
    let src = body("  let blob = if cond { let x = a; } else { yield b; };\n  return { blob };");
    assert!(
        parse_module(&src).is_err(),
        "a branch with no yield must reject"
    );
}

#[test]
fn if_statement_with_else_if_chain_parses() {
    let src = body(
        "  if input.a == input.b {\n\
         \x20   return { x: input.a };\n\
         \x20 } else if input.a == input.c {\n\
         \x20   return { x: input.c };\n\
         \x20 } else {\n\
         \x20   return { x: input.b };\n\
         \x20 }",
    );
    let m = parse_module(&src).expect("if/else-if/else parses");
    let intent = intent_of(&m);
    let Stmt::If { els, .. } = &intent.body.stmts[0] else {
        panic!("stmt 0 is an if");
    };
    // first else is a chained `else if` → ElseClause::If wrapping another Stmt::If
    let Some(first_else) = els else {
        panic!("there is an else arm")
    };
    let ElseClause::If(inner) = first_else.as_ref() else {
        panic!("first else is a chained `else if`");
    };
    let Stmt::If { els: inner_els, .. } = inner.as_ref() else {
        panic!("the chained arm is itself an if");
    };
    assert!(
        matches!(inner_els.as_deref(), Some(ElseClause::Block(_))),
        "the final else is a plain block",
    );
}

#[test]
fn conditional_blob_fixture_parses() {
    let m = parse_module(BLOB).expect("conditional-blob fixture parses");
    let intent = intent_of(&m);
    // body: let initialBytes; let initialBlob = <branch-yield>; return
    assert_eq!(intent.body.stmts.len(), 3);
    let Stmt::Let {
        value: Expr::IfYield { else_block, .. },
        ..
    } = &intent.body.stmts[1]
    else {
        panic!("stmt 1 is a branch-yield let");
    };
    // else branch: let blobRef; let blob = ensure(...) else ...; yield some(blob)
    assert_eq!(else_block.stmts.len(), 2);
}

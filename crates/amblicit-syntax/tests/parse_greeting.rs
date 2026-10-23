//! Conformance: the target-backed `readGreeting` example parses.
//!
//! Exercises calls (`echo.ref<shape.Greeting>(...)`, `.read()`), an effect-`else`
//! obstruction clause, multiple imports (shape + lawpack + target), and explicit
//! record fields. Fixture: `fixtures/lang/effects/read-greeting.edict`.

use amblicit_syntax::ast::{
    Decl, Expr, ImportKind, IntentClause, ObstructionHandler, RecordEntry, Stmt,
};
use amblicit_syntax::parse_module;

const GREETING: &str = include_str!("../../../fixtures/lang/effects/read-greeting.edict");

#[test]
fn read_greeting_parses() {
    let m = parse_module(GREETING).expect("read-greeting must parse");

    // three imports: shape, lawpack, target
    assert_eq!(m.imports.len(), 3);
    assert_eq!(m.imports[0].kind, ImportKind::Shape);
    assert_eq!(
        m.imports[0].shape_path.as_deref(),
        Some("schemas/greeting.graphql")
    );
    assert!(m.imports[0].digest.is_none());
    assert_eq!(m.imports[1].kind, ImportKind::Lawpack);
    assert!(m.imports[1].digest.is_some());
    assert_eq!(m.imports[2].kind, ImportKind::Target);
    assert_eq!(m.imports[2].alias, "echo");

    let Decl::Intent(intent) = &m.decls[0] else {
        panic!("decl 0 is an intent")
    };
    assert_eq!(intent.name, "readGreeting");
    assert!(matches!(&intent.clauses[1], IntentClause::Basis(Some(_))));

    // body: let (call w/ type-args), let (call w/ effect-else), return record(2)
    assert_eq!(intent.body.stmts.len(), 3);

    let Stmt::Let { value, els, .. } = &intent.body.stmts[0] else {
        panic!("stmt 0 is let")
    };
    assert!(els.is_none());
    let Expr::Call {
        type_args, args, ..
    } = value
    else {
        panic!("rhs is a call")
    };
    assert_eq!(
        type_args.len(),
        1,
        "echo.ref<shape.Greeting> has one type arg"
    );
    assert_eq!(args.len(), 1);

    let Stmt::Let { els, .. } = &intent.body.stmts[1] else {
        panic!("stmt 1 is let")
    };
    assert!(
        matches!(els, Some(ObstructionHandler::Single(_))),
        "read() else <obstruction>"
    );

    let Stmt::Return { value, .. } = &intent.body.stmts[2] else {
        panic!("stmt 2 is return")
    };
    let Expr::Record { entries, .. } = value else {
        panic!("return is a record")
    };
    assert_eq!(entries.len(), 2);
    assert!(matches!(&entries[0], RecordEntry::Field { name, .. } if name == "greetingId"));
}

#[test]
fn generic_call_vs_comparison_disambiguation() {
    // `a < b` is comparison; `f<T>(x)` is a type-call. Both must parse, in the
    // same body, without the `<` heuristic misfiring.
    let src = "package a.b@1;\n\
        intent t(input: shape.In) returns shape.Out basis none budget <= p.b {\n\
          let cmp = input.lo < input.hi;\n\
          let made = echo.ref<shape.T>(input.id);\n\
          return { cmp, made };\n\
        }";
    let m = parse_module(src).expect("both forms parse");
    let Decl::Intent(intent) = &m.decls[0] else {
        panic!("intent")
    };
    let Stmt::Let { value: cmp, .. } = &intent.body.stmts[0] else {
        panic!("let cmp")
    };
    assert!(matches!(cmp, Expr::Binary { .. }), "`<` here is comparison");
    let Stmt::Let { value: made, .. } = &intent.body.stmts[1] else {
        panic!("let made")
    };
    assert!(
        matches!(made, Expr::Call { .. }),
        "`<T>(...)` here is a type-call"
    );
}

#[test]
fn obstruction_map_with_binders_and_payload_parses() {
    let src = "package a.b@1;\n\
        intent t(input: shape.In) returns shape.Out basis none budget <= p.b {\n\
          let blob = blobRef.ensure(input.candidate)\n\
            else {\n\
              mismatch(fault) => rope.TextBlobHashConflict({ observed: fault.existing }),\n\
              boundExceeded => rope.ReadBoundExceeded,\n\
            };\n\
          return { blob };\n\
        }";
    let m = parse_module(src).expect("obstruction map parses");
    let Decl::Intent(intent) = &m.decls[0] else {
        panic!("intent")
    };
    let Stmt::Let {
        els: Some(ObstructionHandler::Map(arms)),
        ..
    } = &intent.body.stmts[0]
    else {
        panic!("let with obstruction map");
    };
    assert_eq!(arms.len(), 2);
    assert_eq!(arms[0].failure, "mismatch");
    assert_eq!(arms[0].binder.as_deref(), Some("fault"));
    assert!(arms[0].target.payload.is_some());
    assert_eq!(arms[1].failure, "boundExceeded");
    assert!(arms[1].binder.is_none());
    assert!(arms[1].target.payload.is_none());
}

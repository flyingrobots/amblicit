//! Conformance: enum declarations, `variant` types, variant-literal
//! constructors (`Qual.Type::Case(payload)`), and `match` expressions.
//!
//! Grammar: `enum-decl`, `variant-type`, `variant-lit`, `match-expr`
//! (SPEC Edict Language v1).

mod common;
use amblicit_syntax::ast::{Decl, Expr, Stmt, TypeExpr};
use amblicit_syntax::parse_module;
use common::parse_ok;

const PALETTE: &str = include_str!("../../../fixtures/lang/types/color-match.amblicit");

#[test]
fn enum_decl_parses() {
    let m = parse_module("package a.b@1;\nenum Channel { Red, Green, Blue }").expect("enum parses");
    let Decl::Enum(e) = &m.decls[0] else {
        panic!("decl 0 is an enum");
    };
    assert_eq!(e.name, "Channel");
    assert_eq!(e.cases, vec!["Red", "Green", "Blue"]);
}

#[test]
fn variant_type_with_and_without_payloads_parses() {
    let src = "package a.b@1;\n\
        type Paint = variant { Solid(shape.Rgb), Named(shape.Name), Transparent };";
    let m = parse_module(src).expect("variant type parses");
    let Decl::Type(t) = &m.decls[0] else {
        panic!("decl 0 is a type");
    };
    let TypeExpr::Variant(cases) = &t.body else {
        panic!("body is a variant");
    };
    assert_eq!(cases.len(), 3);
    assert_eq!(cases[0].name, "Solid");
    assert!(cases[0].payload.is_some(), "Solid carries a payload");
    assert_eq!(cases[2].name, "Transparent");
    assert!(cases[2].payload.is_none(), "Transparent is payload-free");
}

fn first_let_value(src: &str) -> Expr {
    let Decl::Intent(intent) = parse_ok(src).decls.into_iter().next().expect("a decl") else {
        panic!("decl 0 is an intent");
    };
    let Stmt::Let { value, .. } = intent.body.stmts.into_iter().next().expect("a stmt") else {
        panic!("stmt 0 is a let");
    };
    value
}

#[test]
fn variant_literal_with_payload_parses() {
    let src = "package a.b@1;\n\
        intent t(input: shape.In) returns shape.Out basis none budget <= p.b {\n\
          let x = shape.Paint::Solid(input.rgb);\n\
          return { x };\n\
        }";
    let Expr::VariantLit {
        ty_path,
        case,
        payload,
        ..
    } = first_let_value(src)
    else {
        panic!("rhs is a variant literal");
    };
    assert_eq!(ty_path, vec!["shape", "Paint"]);
    assert_eq!(case, "Solid");
    assert!(payload.is_some(), "Solid(input.rgb) carries a payload");
}

#[test]
fn variant_literal_without_payload_parses() {
    let src = "package a.b@1;\n\
        intent t(input: shape.In) returns shape.Out basis none budget <= p.b {\n\
          let x = shape.Paint::Transparent;\n\
          return { x };\n\
        }";
    let Expr::VariantLit { case, payload, .. } = first_let_value(src) else {
        panic!("rhs is a variant literal");
    };
    assert_eq!(case, "Transparent");
    assert!(payload.is_none(), "a bare case has no payload");
}

#[test]
fn match_expr_with_binders_parses() {
    let src = "package a.b@1;\n\
        intent t(input: shape.In) returns shape.Out basis none budget <= p.b {\n\
          let label = match input.paint {\n\
            Solid(rgb) => rgb.hex,\n\
            Named(name) => name.text,\n\
            Transparent => input.fallback,\n\
          };\n\
          return { label };\n\
        }";
    let Expr::Match { arms, .. } = first_let_value(src) else {
        panic!("rhs is a match");
    };
    assert_eq!(arms.len(), 3);
    assert_eq!(arms[0].case, "Solid");
    assert_eq!(arms[0].binder.as_deref(), Some("rgb"));
    assert_eq!(arms[2].case, "Transparent");
    assert!(arms[2].binder.is_none(), "the bare case binds nothing");
}

#[test]
fn empty_match_is_rejected() {
    let src = "package a.b@1;\n\
        intent t(input: shape.In) returns shape.Out basis none budget <= p.b {\n\
          let x = match input.paint { };\n\
          return { x };\n\
        }";
    assert!(
        parse_module(src).is_err(),
        "a match with no arms must reject"
    );
}

#[test]
fn palette_fixture_parses() {
    let m = parse_module(PALETTE).expect("color-match fixture parses");
    // enum, variant type, intent
    assert!(matches!(m.decls[0], Decl::Enum(_)));
    assert!(matches!(m.decls[1], Decl::Type(_)));
    assert!(matches!(m.decls[2], Decl::Intent(_)));
    let Decl::Intent(intent) = &m.decls[2] else {
        panic!("decl 2 is an intent");
    };
    let Stmt::Let {
        value: Expr::Match { arms, .. },
        ..
    } = &intent.body.stmts[0]
    else {
        panic!("stmt 0 is a match let");
    };
    assert_eq!(arms.len(), 3);
    let Stmt::Let {
        value: Expr::VariantLit { case, .. },
        ..
    } = &intent.body.stmts[1]
    else {
        panic!("stmt 1 is a variant-literal let");
    };
    assert_eq!(case, "Solid");
}

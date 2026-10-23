//! Regression coverage for PR #9 review findings.
//!
//! These cases pin syntax-boundary behavior through the public `parse_module`
//! API and assert stable AST/error identities.

mod common;
use amblicit_syntax::ast::{
    BinOp, BoundRef, BytesRefine, Decl, Expr, ImportKind, RequireElseArm, Stmt, TypeExpr, TypeRef,
};
use amblicit_syntax::token::IntSuffix;
use amblicit_syntax::{parse_module, ParseErrorKind};
use common::{body, intent_of};

const ZERO_DIGEST: &str = "sha256:0000000000000000000000000000000000000000000000000000000000000000";
const STALE_BASIS_OBSTRUCTION_STRAND: &str =
    include_str!("../../../fixtures/obstruction-strands/v0/stale-basis/source.edict");

fn reject_kind(src: &str, kind: ParseErrorKind) {
    let err = parse_module(src).expect_err("source must reject");
    assert_eq!(err.kind, kind, "wrong error for source:\n{src}");
}

fn first_let_value(src: &str) -> Expr {
    let m = parse_module(src).expect("module parses");
    let intent = intent_of(&m);
    let Stmt::Let { value, .. } = &intent.body.stmts[0] else {
        panic!("first statement is a let");
    };
    value.clone()
}

#[test]
fn package_versions_preserve_underscores() {
    let m = parse_module("package a.b@1_2.3-beta;").expect("package parses");
    assert_eq!(m.package.version, "1_2.3-beta");

    let m = parse_module("package a.b@1_beta;").expect("package version label parses");
    assert_eq!(m.package.version, "1_beta");
}

#[test]
fn import_versions_preserve_underscore_labels() {
    let src =
        format!("package a.b@1;\nuse lawpack pkg.lib@1_beta digest \"{ZERO_DIGEST}\" as lib;");
    let m = parse_module(&src).expect("import version label parses");
    let package = m.imports[0]
        .package
        .as_ref()
        .expect("lawpack import has a package ref");
    assert_eq!(package.version, "1_beta");
}

#[test]
fn reserved_keywords_are_rejected_as_import_aliases() {
    reject_kind(
        "package a.b@1;\nuse lawpack x.y@1 as return;",
        ParseErrorKind::ReservedKeyword,
    );
    reject_kind(
        "package a.b@1;\nuse target x.y@1 as if;",
        ParseErrorKind::ReservedKeyword,
    );
}

#[test]
fn capability_imports_parse_as_external_operation_references() {
    let source = format!("package a.b@1;\nuse capability c.d@1 digest \"{ZERO_DIGEST}\" as c;");
    let module = parse_module(&source).expect("capability import parses");
    assert_eq!(module.imports[0].kind, ImportKind::Capability);
    assert_eq!(module.imports[0].digest.as_deref(), Some(ZERO_DIGEST));
}

#[test]
fn import_digest_literals_are_validated() {
    reject_kind(
        "package a.b@1;\nuse lawpack x.y@1 digest \"not-a-digest\" as x;",
        ParseErrorKind::InvalidDigest,
    );

    let src = format!("package a.b@1;\nuse lawpack x.y@1 digest \"{ZERO_DIGEST}\" as x;");
    let m = parse_module(&src).expect("valid digest import parses");
    assert_eq!(m.imports[0].digest.as_deref(), Some(ZERO_DIGEST));
}

#[test]
fn bytes_accept_coordinate_bounds() {
    let m = parse_module("package a.b@1;\ntype T = { bytes: Bytes<max=limits.maxBytes>, };")
        .expect("coordinate-bounded bytes parse");
    let Decl::Type(t) = &m.decls[0] else {
        panic!("decl 0 is type");
    };
    let TypeExpr::Record(fields) = &t.body else {
        panic!("T is a record");
    };
    assert_eq!(
        fields[0].ty,
        TypeRef::BytesTy(Some(BytesRefine {
            min: None,
            max: BoundRef::Coord(vec!["limits".into(), "maxBytes".into()]),
        }))
    );
}

#[test]
fn bytes_accept_exact_bounds() {
    let m = parse_module("package a.b@1;\ntype T = { identity: Bytes<exact=32>, };")
        .expect("exact byte bounds parse");
    let Decl::Type(t) = &m.decls[0] else {
        panic!("decl 0 is type");
    };
    let TypeExpr::Record(fields) = &t.body else {
        panic!("T is a record");
    };
    let exact = BoundRef::Int {
        value: 32,
        suffix: None,
    };
    assert_eq!(
        fields[0].ty,
        TypeRef::BytesTy(Some(BytesRefine {
            min: Some(exact.clone()),
            max: exact,
        }))
    );
}

#[test]
fn bound_integer_suffixes_are_preserved() {
    let m = parse_module("package a.b@1;\ntype T = { name: String<max=1u32>, bytes: Bytes<max=2i64>, items: List<shape.Item, max=3i32>, };")
        .expect("suffixed integer bounds parse");
    let Decl::Type(t) = &m.decls[0] else {
        panic!("decl 0 is type");
    };
    let TypeExpr::Record(fields) = &t.body else {
        panic!("T is a record");
    };

    let TypeRef::StringTy(Some(string_refine)) = &fields[0].ty else {
        panic!("name is a refined string");
    };
    assert_eq!(
        string_refine.max,
        BoundRef::Int {
            value: 1,
            suffix: Some(IntSuffix::U32)
        }
    );

    assert_eq!(
        fields[1].ty,
        TypeRef::BytesTy(Some(BytesRefine {
            min: None,
            max: BoundRef::Int {
                value: 2,
                suffix: Some(IntSuffix::I64)
            },
        }))
    );

    let TypeRef::List { max, .. } = &fields[2].ty else {
        panic!("items is a list");
    };
    assert_eq!(
        *max,
        BoundRef::Int {
            value: 3,
            suffix: Some(IntSuffix::I32)
        }
    );
}

#[test]
fn bool_and_digest_literals_are_real_literals() {
    let yes = first_let_value(&body("  let ok = true;\n  return { ok };"));
    assert!(matches!(yes, Expr::Bool { value: true, .. }));

    let no = first_let_value(&body("  let ok = false;\n  return { ok };"));
    assert!(matches!(no, Expr::Bool { value: false, .. }));

    let src = body(&format!(
        "  let h = digest(\"{ZERO_DIGEST}\");\n  return {{ h }};"
    ));
    let digest = first_let_value(&src);
    assert!(matches!(digest, Expr::Digest { value, .. } if value == ZERO_DIGEST));
}

#[test]
fn upper_ident_productions_are_enforced() {
    reject_kind(
        "package a.b@1;\ntype bad = { x: Bytes<max=1>, };",
        ParseErrorKind::InvalidName,
    );
    reject_kind(
        "package a.b@1;\nenum channel { Red }",
        ParseErrorKind::InvalidName,
    );
    reject_kind(
        "package a.b@1;\ntype T = variant { bad };",
        ParseErrorKind::InvalidName,
    );
    reject_kind(
        &body("  let x = shape.Paint::bad;\n  return { x };"),
        ParseErrorKind::InvalidName,
    );
}

#[test]
fn empty_enum_and_empty_obstruction_maps_reject() {
    reject_kind("package a.b@1;\nenum E {}", ParseErrorKind::EmptyEnum);
    reject_kind(
        &body("  target.effect() else {};\n  return { input };"),
        ParseErrorKind::EmptyObstructionMap,
    );
}

#[test]
fn reserved_words_are_rejected_in_all_binder_positions() {
    reject_kind(
        &body("  for match in input.items bounded 1 { }\n  return { input };"),
        ParseErrorKind::ReservedKeyword,
    );
    reject_kind(
        &body("  target.effect() else { else => domain.Oops };\n  return { input };"),
        ParseErrorKind::ReservedKeyword,
    );
    reject_kind(
        &body("  target.effect() else { mismatch(return) => domain.Oops };\n  return { input };"),
        ParseErrorKind::ReservedKeyword,
    );
    reject_kind(
        &body("  let true = input.x;\n  return { input };"),
        ParseErrorKind::ReservedKeyword,
    );
    reject_kind(
        "package a.b@1;\nintent t(false: shape.In) returns shape.Out basis none budget <= p.b { return { false }; }",
        ParseErrorKind::ReservedKeyword,
    );
    reject_kind(
        &body("  for false in input.items bounded 1 { }\n  return { input };"),
        ParseErrorKind::ReservedKeyword,
    );
    reject_kind(
        &body("  target.effect() else { mismatch(false) => domain.Oops };\n  return { input };"),
        ParseErrorKind::ReservedKeyword,
    );
}

#[test]
fn reserved_words_are_rejected_as_record_shorthand() {
    reject_kind(
        &body("  return { return };"),
        ParseErrorKind::ReservedKeyword,
    );
    parse_module(&body("  return { returnValue: input.x };"))
        .expect("reserved-looking field label with explicit value is legal");
}

#[test]
fn reserved_words_are_rejected_as_coordinate_roots() {
    reject_kind(
        &body("  for x in input.items bounded return {}\n  return { input };"),
        ParseErrorKind::ReservedKeyword,
    );
    reject_kind(
        &body("  require input.ok else if.Blocked;\n  return { input };"),
        ParseErrorKind::ReservedKeyword,
    );
}

#[test]
fn require_statement_parses_terminal_obstruction_source_shape() {
    let m = parse_module(&body(
        "  require input.ok else domain.Blocked({ reason: input.reason });\n  return { input };",
    ))
    .expect("require statement parses");
    let intent = intent_of(&m);
    let Stmt::Require { predicate, arm, .. } = &intent.body.stmts[0] else {
        panic!("first statement is a require");
    };

    assert!(matches!(
        predicate,
        Expr::Field { field, .. } if field == "ok"
    ));
    let RequireElseArm::Terminal(obstruction) = arm else {
        panic!("require terminal obstruction remains terminal");
    };
    assert_eq!(obstruction.coordinate, vec!["domain", "Blocked"]);
    assert!(matches!(obstruction.payload, Some(Expr::Record { .. }),));
}

#[test]
fn continue_obstructed_source_arm_parses() {
    let m = parse_module(&body(
        "  require input.ok else continue obstructed { reason: jim.EditObstruction.StaleBase, basis: input.basis };\n  return { input };",
    ))
    .expect("continue-obstructed require arm parses");
    let intent = intent_of(&m);
    let Stmt::Require { predicate, arm, .. } = &intent.body.stmts[0] else {
        panic!("first statement is a require");
    };

    assert!(matches!(
        predicate,
        Expr::Field { field, .. } if field == "ok"
    ));
    let RequireElseArm::ContinueObstructed(obstruction) = arm else {
        panic!("require arm is first-class continue-obstructed syntax");
    };
    assert!(matches!(
        &obstruction.reason,
        Expr::Field { field, .. } if field == "StaleBase"
    ));
    assert_eq!(obstruction.payload.len(), 2);
}

#[test]
fn continue_obstructed_requires_reason_field() {
    reject_kind(
        &body(
            "  require input.ok else continue obstructed { basis: input.basis };\n  return { input };",
        ),
        ParseErrorKind::MissingRequiredField,
    );
}

#[test]
fn continue_obstructed_rejects_duplicate_reason_field() {
    let src = body(
        "  require input.ok else continue obstructed { reason: jim.EditObstruction.StaleBase, reason: jim.EditObstruction.Other };\n  return { input };",
    );
    let err = parse_module(&src).expect_err("duplicate reason rejects");
    assert_eq!(err.kind, ParseErrorKind::DuplicateField);
    assert_eq!(
        err.span.start,
        src.find("jim.EditObstruction.Other")
            .expect("source contains second reason value"),
    );
}

#[test]
fn continue_obstructed_is_contextual_to_require_else() {
    reject_kind(
        &body(
            "  let x = continue obstructed { reason: jim.EditObstruction.StaleBase };\n  return { x };",
        ),
        ParseErrorKind::ExpectedToken,
    );
}

#[test]
fn helper_shaped_continue_in_obstructed_strand_is_terminal() {
    let m = parse_module(&body(
        "  require input.ok else continueInObstructedStrand({ reason: jim.EditObstruction.StaleBase });\n  return { input };",
    ))
    .expect("helper-shaped obstruction constructor remains terminal");
    let intent = intent_of(&m);
    let Stmt::Require { arm, .. } = &intent.body.stmts[0] else {
        panic!("first statement is a require");
    };
    let RequireElseArm::Terminal(obstruction) = arm else {
        panic!("helper-shaped constructor must not become continuation syntax");
    };

    assert_eq!(obstruction.coordinate, vec!["continueInObstructedStrand"]);
    assert!(matches!(obstruction.payload, Some(Expr::Record { .. }),));
}

#[test]
fn stale_basis_obstruction_strand_fixture_parses() {
    let m = parse_module(STALE_BASIS_OBSTRUCTION_STRAND)
        .expect("stale-basis obstruction strand fixture parses");
    let intent = intent_of(&m);
    let Stmt::Require { arm, .. } = &intent.body.stmts[0] else {
        panic!("first fixture statement is a require");
    };
    let RequireElseArm::ContinueObstructed(obstruction) = arm else {
        panic!("fixture uses first-class continue-obstructed syntax");
    };

    assert!(matches!(
        &obstruction.reason,
        Expr::Field { field, .. } if field == "StaleBase"
    ));
    assert_eq!(obstruction.payload.len(), 4);
}

#[test]
fn effect_positions_must_be_calls() {
    reject_kind(
        &body("  input.value;\n  return { input };"),
        ParseErrorKind::NonCallEffect,
    );
    reject_kind(
        &body("  let x = input.value else domain.Oops;\n  return { x };"),
        ParseErrorKind::NonCallEffect,
    );
}

#[test]
fn yield_blocks_reject_return_statements() {
    reject_kind(
        &body(
            "  let x = if input.ok { return input.a; yield input.a; } else { yield input.b; };\n\
             \x20 return { x };",
        ),
        ParseErrorKind::ReturnInYieldBlock,
    );
}

#[test]
fn type_call_suffix_requires_adjacency_to_call_paren() {
    let value = first_let_value(&body("  let x = input.lo<input.hi> (0);\n  return { x };"));
    assert!(
        matches!(value, Expr::Binary { op: BinOp::Gt, .. }),
        "spaced `> (` must parse as relation, not a type-call suffix: {value:?}",
    );
}

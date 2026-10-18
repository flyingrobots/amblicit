//! Check the generated closure against its normative CDDL with an independent codec.
use cddl_cat::cbor::validate_cbor_bytes;
use cddl_cat::ValidateError;
use edict_syntax::source_closure::{resolve_source_closure, SourceClosureLimits};

#[test]
fn source_closure_artifact_inhabits_its_closed_normative_wire_schema() {
    let schema = format!(
        "{}\n{}",
        include_str!("../../../docs/abi/edict-common.cddl"),
        include_str!("../../../docs/abi/edict-source-closure.cddl")
    );
    let closure = resolve_source_closure(
        "package example.source@1;",
        &[],
        SourceClosureLimits::default(),
    )
    .unwrap();
    validate_cbor_bytes("source-closure", &schema, closure.canonical_bytes())
        .expect("compiler-owned artifact matches normative CDDL");
    let mut artifact: ciborium::Value = ciborium::from_reader(closure.canonical_bytes()).unwrap();
    let ciborium::Value::Map(ref mut fields) = artifact else {
        panic!("artifact map")
    };
    fields.push((
        ciborium::Value::Text("extra".into()),
        ciborium::Value::Bool(true),
    ));
    let mut bytes = Vec::new();
    ciborium::into_writer(&artifact, &mut bytes).unwrap();
    let failure = validate_cbor_bytes("source-closure", &schema, &bytes).unwrap_err();
    assert!(
        matches!(failure, ValidateError::Mismatch(_)),
        "undeclared field must cause a schema mismatch, got {failure:?}"
    );
}

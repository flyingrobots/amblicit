//! Exact lawpack signature diagnostics preserve compatibility and call identity.
use edict_syntax::{
    compile_to_core, decode_lawpack_adapter, decode_lawpack_bundle, parse_module,
    prepare_lawpack_compilation, CompilerError, CompilerErrorKind, CompilerStage, CoreModule,
};

const SOURCE: &str = include_str!("../../../fixtures/lawpack/hello-echo/create-greeting.edict");
const MANIFEST: &[u8] = include_bytes!("../../../fixtures/lawpack/hello-echo/manifest.cbor");
const EXPORTS: &[u8] = include_bytes!("../../../fixtures/lawpack/hello-echo/exports.cbor");
const ADAPTER: &[u8] = include_bytes!("../../../fixtures/lawpack/hello-echo/adapter.cbor");
const MANIFEST_DIGEST: &str = include_str!("../../../fixtures/lawpack/hello-echo/manifest.sha256");

use edict_syntax::{CanonicalValue, SignatureMismatchPosition, SignaturePathSegment};
use sha2::{Digest, Sha256};
use std::fmt::Write;

fn field_mut<'a>(value: &'a mut CanonicalValue, name: &str) -> &'a mut CanonicalValue {
    let CanonicalValue::Map(fields) = value else {
        panic!("fixture map")
    };
    fields
        .iter_mut()
        .find_map(|(key, value)| (key == &CanonicalValue::Text(name.to_owned())).then_some(value))
        .expect("fixture field")
}

fn digest(domain: &str, value: &CanonicalValue) -> [u8; 32] {
    let frame = CanonicalValue::Array(vec![
        CanonicalValue::Text("edict.digest/v1".into()),
        CanonicalValue::Text(domain.into()),
        value.clone(),
    ]);
    Sha256::digest(edict_syntax::encode_canonical_cbor(&frame).expect("canonical frame")).into()
}

fn compile_with_input_definition(
    source: &str,
    definition: &str,
) -> Result<CoreModule, Vec<CompilerError>> {
    // Exported String definitions are self-describing and require their policy.
    let mut definition = definition.to_owned();
    for max in [8, 32, 64, 128, 256] {
        definition = definition.replace(
            &format!("String<max={max}>"),
            &format!("String<max={max},canonical=raw-utf8>"),
        );
    }
    let mut exports = edict_syntax::decode_canonical_cbor(EXPORTS).expect("exports");
    let CanonicalValue::Array(types) = field_mut(&mut exports, "types") else {
        panic!("types")
    };
    *field_mut(&mut types[0], "definition") = CanonicalValue::Text(definition.clone());
    if definition.contains("hello.echo@1.ExpectedKey") {
        for coordinate in ["hello.echo@1.ExpectedKey", "hello.echo@1.ActualKey"] {
            let mut exported = types[0].clone();
            *field_mut(&mut exported, "coordinate") = CanonicalValue::Text(coordinate.into());
            *field_mut(&mut exported, "definition") =
                CanonicalValue::Text("Nominal<Bytes<exact=32>>".into());
            types.push(exported);
        }
    }
    let export_bytes = edict_syntax::encode_canonical_cbor(&exports).expect("encode exports");
    let mut manifest = edict_syntax::decode_canonical_cbor(MANIFEST).expect("manifest");
    *field_mut(field_mut(&mut manifest, "exports"), "digest") = CanonicalValue::Array(vec![
        CanonicalValue::Text("sha256".into()),
        CanonicalValue::Bytes(digest("hello.echo.exports/v1", &exports).to_vec()),
    ]);
    let manifest_bytes = edict_syntax::encode_canonical_cbor(&manifest).expect("encode manifest");
    let bundle = decode_lawpack_bundle(&manifest_bytes, &export_bytes)
        .expect("authenticated mutated exports");
    let mut import_digest = String::from("sha256:");
    for byte in bundle.manifest_digest() {
        write!(import_digest, "{byte:02x}").expect("format digest");
    }
    assert_eq!(source.matches(MANIFEST_DIGEST.trim()).count(), 1);
    let source = source.replace(MANIFEST_DIGEST.trim(), &import_digest);
    let module = parse_module(&source).expect("signature fixture parses");
    let adapter = decode_lawpack_adapter(&bundle, "echo.dpo@1", ADAPTER).expect("adapter");
    let preparation = prepare_lawpack_compilation(&module, &bundle, &adapter).expect("preparation");
    compile_to_core(&module, preparation.compiler_context())
}

fn input_source(declarations: &str, fields: &str) -> String {
    SOURCE
        .replace(
            "type GreetingCreated",
            &format!("{declarations}\ntype ActualInput = {{ {fields} }};\ntype GreetingCreated"),
        )
        .replace("input: hello.CreateGreetingInput", "input: ActualInput")
}

fn compile(source: &str) -> Result<CoreModule, Vec<CompilerError>> {
    let module = parse_module(source).expect("signature fixture parses");
    let bundle = decode_lawpack_bundle(MANIFEST, EXPORTS).expect("load exact lawpack");
    let adapter =
        decode_lawpack_adapter(&bundle, "echo.dpo@1", ADAPTER).expect("load exact adapter");
    let preparation = prepare_lawpack_compilation(&module, &bundle, &adapter)
        .expect("authenticate signature fixture");
    compile_to_core(&module, preparation.compiler_context())
}

fn assert_signature_cause(source: &str, path: &str, expected: &str, actual: &str) {
    let errors = compile(source).expect_err("exported signature must reject");
    let cause = &errors[0];
    assert_eq!(cause.kind, CompilerErrorKind::TypeMismatch);
    assert_eq!(cause.stage, CompilerStage::TypeCheck);
    let call = "hello.createGreeting(input)";
    assert_eq!(source.matches(call).count(), 1);
    let start = source.find(call).expect("effect call");
    assert_eq!(cause.span.start, start);
    assert_eq!(cause.span.end, start + call.len());
    assert!(
        cause.message.contains(path),
        "missing field path: {cause:?}"
    );
    assert!(
        cause.message.contains(expected),
        "missing expected bound: {cause:?}"
    );
    assert!(
        cause.message.contains(actual),
        "missing actual bound: {cause:?}"
    );
    let detail = cause
        .signature_mismatch
        .as_ref()
        .expect("typed signature detail");
    assert_eq!(detail.effect, "hello.createGreeting");
    assert_eq!(
        detail.position,
        if path.starts_with("input") {
            SignatureMismatchPosition::Input
        } else {
            SignatureMismatchPosition::Receipt
        },
    );
    assert_eq!(
        detail.path,
        vec![SignaturePathSegment::Field { name: "key".into() }]
    );
    assert!(detail.expected_type.as_ref().unwrap().starts_with(expected));
    assert!(detail.actual_type.as_ref().unwrap().starts_with(actual));
}

#[test]
fn input_signature_error_identifies_first_incompatible_field() {
    let declaration = "type WideInput = { basis: String<max=128>, key: String<max=65>, message: String<max=256>, };\n\n";
    let source = SOURCE
        .replace(
            "type GreetingCreated",
            &format!("{declaration}type GreetingCreated"),
        )
        .replace("input: hello.CreateGreetingInput", "input: WideInput");
    let control = source.replace("key: String<max=65>", "key: String<max=64>");
    compile(&control).expect("equivalent structural input compiles");
    assert_signature_cause(&source, "input.key", "String<max=64", "String<max=65");
}

#[test]
fn receipt_signature_error_identifies_first_incompatible_field() {
    let declaration = "type TightReceipt = { key: String<max=63>, };\n\n";
    let source = SOURCE
        .replace(
            "type GreetingCreated",
            &format!("{declaration}type GreetingCreated"),
        )
        .replace(
            "let receipt: hello.GreetingReceipt",
            "let receipt: TightReceipt",
        );
    let control = source.replace("key: String<max=63>", "key: String<max=64>");
    compile(&control).expect("equivalent receipt annotation compiles");
    assert_signature_cause(&source, "receipt.key", "String<max=63", "String<max=64");
}

#[test]
fn signature_mismatch_context_is_structured_at_the_input_root() {
    let source = SOURCE.replace("hello.createGreeting(input)", "hello.createGreeting(true)");
    let errors = compile(&source).expect_err("Boolean is not the exported input");
    let cause = &errors[0];
    assert_eq!(cause.kind, CompilerErrorKind::TypeMismatch);
    let detail = cause
        .signature_mismatch
        .as_ref()
        .expect("typed signature context");
    assert_eq!(detail.effect, "hello.createGreeting");
    assert_eq!(
        detail.position,
        edict_syntax::SignatureMismatchPosition::Input
    );
    assert_eq!(detail.path, Vec::<SignaturePathSegment>::new());
    assert!(detail
        .expected_type
        .as_deref()
        .expect("expected record")
        .starts_with("Record<"));
    assert_eq!(detail.actual_type.as_deref(), Some("Bool"));
}

#[test]
fn signature_first_difference_is_lexical_and_preserves_independent_errors() {
    let source = input_source(
        "",
        "message: String<max=257>, key: String<max=65>, basis: String<max=129>,",
    );
    let control = source
        .replace("max=257", "max=256")
        .replace("max=65", "max=64")
        .replace("max=129", "max=128");
    compile(&control).expect("equivalent unordered fields compile");
    let source = source.replace("key: receipt.key", "key: unrelatedMissing");
    let errors = compile(&source).expect_err("independent errors");
    let cause = &errors[0];
    assert_eq!(cause.kind, CompilerErrorKind::TypeMismatch);
    let detail = cause
        .signature_mismatch
        .as_ref()
        .expect("signature context");
    assert_eq!(
        detail.path,
        vec![SignaturePathSegment::Field {
            name: "basis".into()
        }]
    );
    assert!(errors
        .iter()
        .any(|error| error.kind == CompilerErrorKind::UnresolvedType
            && source[error.span.start..error.span.end].contains("unrelatedMissing")));
}

#[test]
fn signature_missing_and_extra_fields_have_typed_absence() {
    let expected = "Record<basis:String<max=128>,key:String<max=64>,message:String<max=256>>";
    for (fields, name, missing_expected) in [
        ("basis: String<max=128>, message: String<max=256>,", "key", false),
        ("aardvark: Bool, basis: String<max=128>, key: String<max=64>, message: String<max=256>,", "aardvark", true),
    ] {
        let source = input_source("", fields);
        let errors = compile_with_input_definition(&source, expected).expect_err("different fields reject");
        let cause = &errors[0];
        assert_eq!(cause.kind, CompilerErrorKind::TypeMismatch);
        let detail = cause.signature_mismatch.as_ref().expect("typed field context");
        assert_eq!(detail.position, SignatureMismatchPosition::Input);
        assert_eq!(detail.path, vec![SignaturePathSegment::Field { name: name.into() }]);
        assert_eq!(detail.expected_type.is_none(), missing_expected);
        assert_eq!(detail.actual_type.is_none(), !missing_expected);
    }
}

#[test]
fn nested_and_list_signature_paths_preserve_bounds_and_order() {
    let base = "basis: String<max=128>, key: Nested, message: String<max=256>,";
    let source = input_source(
        "type Nested = { zeta: String<max=9>, alpha: String<max=9>, };",
        base,
    );
    let control = source.replace("max=9", "max=8");
    let expected = "Record<basis:String<max=128>,key:Record<alpha:String<max=8>,zeta:String<max=8>>,message:String<max=256>>";
    compile_with_input_definition(&control, expected).expect("nested equivalent control");
    let errors =
        compile_with_input_definition(&source, expected).expect_err("nested bounds differ");
    let detail = errors[0]
        .signature_mismatch
        .as_ref()
        .expect("nested context");
    assert_eq!(
        detail.path,
        vec![
            SignaturePathSegment::Field { name: "key".into() },
            SignaturePathSegment::Field {
                name: "alpha".into()
            }
        ]
    );
    assert!(detail
        .expected_type
        .as_ref()
        .unwrap()
        .starts_with("String<max=8,"));
    assert!(detail
        .actual_type
        .as_ref()
        .unwrap()
        .starts_with("String<max=9,"));
    for (actual, path_len) in [
        ("List<String<max=33>, max=4>", 2),
        ("List<String<max=32>, max=5>", 1),
    ] {
        let fields = format!("basis: String<max=128>, key: {actual}, message: String<max=256>,");
        let source = input_source("", &fields);
        let expected =
            "Record<basis:String<max=128>,key:List<String<max=32>,max=4>,message:String<max=256>>";
        let errors =
            compile_with_input_definition(&source, expected).expect_err("list bound differs");
        assert_eq!(errors[0].kind, CompilerErrorKind::TypeMismatch);
        let detail = errors[0].signature_mismatch.as_ref().expect("list context");
        assert_eq!(detail.path.len(), path_len);
        assert_eq!(
            detail.path[0],
            SignaturePathSegment::Field { name: "key".into() }
        );
        if path_len == 2 {
            assert_eq!(detail.path[1], SignaturePathSegment::ListItem);
        }
    }
}

#[test]
fn nominal_signature_mismatch_names_exact_identity_without_unwrapping() {
    let expected =
        "Record<basis:String<max=128>,key:hello.echo@1.ExpectedKey,message:String<max=256>>";
    let source = input_source(
        "",
        "basis: String<max=128>, key: hello.ActualKey, message: String<max=256>,",
    );
    let control = source.replace("hello.ActualKey", "hello.ExpectedKey");
    compile_with_input_definition(&control, expected).expect("same nominal identity compiles");
    let errors =
        compile_with_input_definition(&source, expected).expect_err("nominal identities differ");
    assert_eq!(errors[0].kind, CompilerErrorKind::TypeMismatch);
    let detail = errors[0]
        .signature_mismatch
        .as_ref()
        .expect("nominal context");
    assert_eq!(
        detail.path,
        vec![SignaturePathSegment::Field { name: "key".into() }]
    );
    assert!(detail
        .expected_type
        .as_ref()
        .unwrap()
        .contains("hello.echo@1.ExpectedKey"));
    assert!(detail
        .actual_type
        .as_ref()
        .unwrap()
        .contains("hello.echo@1.ActualKey"));
}

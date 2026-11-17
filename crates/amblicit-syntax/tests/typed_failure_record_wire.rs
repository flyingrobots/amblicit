//! Ordered record entries retain evaluation order in the canonical wire form.
use amblicit_syntax::{
    compile_to_core, decode_canonical_cbor, encode_core_module, parse_module, CanonicalValue as V,
    CompilerContext,
};

fn field<'a>(value: &'a V, name: &str) -> &'a V {
    let V::Map(fields) = value else { panic!("map") };
    fields
        .iter()
        .find_map(|(key, value)| (key == &V::Text(name.to_owned())).then_some(value))
        .expect("field")
}

fn map(fields: Vec<(&str, V)>) -> V {
    V::Map(
        fields
            .into_iter()
            .map(|(key, value)| (V::Text(key.into()), value))
            .collect(),
    )
}

#[test]
fn canonical_fallible_record_wire_preserves_the_exact_source_entry_order() {
    let source = r"package order.example@1;
failure First;
failure Second;
type Pair = { z: U64, a: U64 };
fn pair() -> Pair fails First, Second { return { z: fail First, a: fail Second }; }
fn total() -> Pair { return { z: 1u64, a: 2u64 }; }
";
    let core = compile_to_core(
        &parse_module(source).expect("source"),
        &CompilerContext::new(),
    )
    .expect("fallible record source compiles");
    let value = decode_canonical_cbor(&encode_core_module(&core).expect("Core encodes"))
        .expect("canonical Core");
    let functions = field(&value, "functions");
    let result = field(field(field(functions, "pair"), "body"), "result");
    let entries = [("z", "First"), ("a", "Second")]
        .into_iter()
        .map(|(name, failure)| {
            map(vec![
                ("name", V::Text(name.into())),
                (
                    "value",
                    map(vec![
                        ("kind", V::Text("fail".into())),
                        ("failure", V::Text(format!("order.example@1.{failure}"))),
                    ]),
                ),
            ])
        })
        .collect();
    assert_eq!(
        result,
        &map(vec![
            ("kind", V::Text("orderedRecord".into())),
            ("fields", V::Array(entries)),
        ])
    );
    let total = field(field(field(functions, "total"), "body"), "result");
    assert_eq!(field(total, "kind"), &V::Text("record".into()));
    assert!(
        matches!(field(total, "fields"), V::Map(_)),
        "total record keeps its map wire shape"
    );
}

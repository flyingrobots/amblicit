//! The current schema owns ordered record shape; Core owns unique-name semantics.
use super::*;

fn produced() -> CanonicalValue {
    let source = r"package order.example@1;
failure First;
failure Second;
type Pair = { z: U64, a: U64 };
type Input = {};
fn pair() -> Pair fails First, Second { return { z: fail First, a: fail Second }; }
intent evaluate(input: Input) returns Pair profile p.read basis none budget <= p.small {
  let result = pair() else { First => text.First, Second => text.Second };
  return result;
}
";
    let context = CompilerContext::new()
        .with_operation_profile("p.read", "continuum.profile.read-only/v1")
        .with_budget(
            "p.small",
            CoreBudget {
                max_steps: 100_000,
                max_allocated_bytes: 1_048_576,
                max_output_bytes: 4096,
            },
        );
    let core = compile_to_core(&parse_module(source).expect("source"), &context)
        .expect("fallible record source compiles");
    decode_canonical_cbor(&encode_core_module(&core).expect("Core encodes"))
        .expect("canonical Core")
}

fn result_mut(value: &mut CanonicalValue) -> &mut CanonicalValue {
    let functions = map_value_mut(value, "functions");
    let pair = map_value_mut(functions, "pair");
    map_value_mut(map_value_mut(pair, "body"), "result")
}

#[test]
fn current_schema_accepts_ordered_records_and_refuses_malformed_entry_shapes() {
    let pack = assemble(canonical_target_profile_contract_resources());
    let valid = produced();
    pack.validate_domain(CORE_MODULE_DIGEST_DOMAIN, &valid)
        .expect("current ordered record schema");
    for mutation in 0..6 {
        let mut value = valid.clone();
        let fields = map_value_mut(result_mut(&mut value), "fields");
        let CanonicalValue::Array(entries) = fields else {
            panic!("ordered entries")
        };
        match mutation {
            0 => *fields = CanonicalValue::Map(Vec::new()),
            1 => entries.clear(),
            2 => remove_map_field(&mut entries[0], "name"),
            3 => remove_map_field(&mut entries[0], "value"),
            4 => *map_value_mut(&mut entries[0], "name") = CanonicalValue::Integer(1),
            5 => {
                let CanonicalValue::Map(entry) = &mut entries[0] else {
                    panic!("entry")
                };
                entry.push((CanonicalValue::Text("extra".into()), CanonicalValue::Null));
            }
            _ => unreachable!(),
        }
        assert_eq!(
            pack.validate_domain(CORE_MODULE_DIGEST_DOMAIN, &value),
            Err(ProviderArtifactSchemaValidationErrorKind::SchemaMismatch),
            "mutation {mutation}"
        );
    }
}

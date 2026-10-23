//! Canonical Core encoding behavior for the v0.3 encoder slice.
//!
//! These tests assert public behavior: deterministic canonical bytes,
//! canonical-CBOR validation, mutation sensitivity, and alpha-stable source
//! lowering. Reviewed Core golden bytes and exact digest values are covered by
//! the `core_golden_fixtures` test target.

mod common;

use std::collections::BTreeMap;
use std::fmt::Write as _;

use amblicit_syntax::{
    compile_to_core, decode_canonical_cbor, digest_core_module, encode_canonical_cbor,
    encode_core_module, parse_module, CanonicalErrorKind, CanonicalValue, CompilerContext,
    CoreBudget, CoreImport, CoreImportKind, CoreModule, CoreNode, CorePredicate, CoreType,
    InputConstraint, InputConstraintSource, ResourceRef,
};
use common::{bounded_hello_core, hello_context, BOUNDED_HELLO};

const STATEMENT_BRANCH: &str = "package a.b@1;\n\
    type Input = { choose: U32, value: U64, };\n\
    type Output = { value: U64, };\n\
    intent t(input: Input) returns Output\n\
      profile p.read\n\
      basis none\n\
      budget <= p.tiny {\n\
      if input.choose == 0u32 {\n\
        require input.value <= 10u64 else example.TooLarge;\n\
      } else {\n\
        require input.value == input.value else example.Impossible;\n\
      }\n\
      return { value: input.value };\n\
    }";
const STATEMENT_BRANCH_BYTES_HEX: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../fixtures/core/canonical/statement-branch.core.hex"
));
const STATEMENT_BRANCH_DIGEST: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../fixtures/core/canonical/statement-branch.core.sha256"
));

fn statement_branch_context() -> CompilerContext {
    CompilerContext::new()
        .with_operation_profile("p.read", "continuum.profile.read-only/v1")
        .with_budget(
            "p.tiny",
            CoreBudget {
                max_steps: 64,
                max_allocated_bytes: 4096,
                max_output_bytes: 1024,
            },
        )
}

fn find_branch_map(value: &CanonicalValue) -> Option<&[(CanonicalValue, CanonicalValue)]> {
    match value {
        CanonicalValue::Map(entries) => {
            let is_branch = entries.iter().any(|(key, value)| {
                key == &CanonicalValue::Text("kind".to_owned())
                    && value == &CanonicalValue::Text("branch".to_owned())
            });
            if is_branch {
                Some(entries)
            } else {
                entries.iter().find_map(|(_, value)| find_branch_map(value))
            }
        }
        CanonicalValue::Array(values) => values.iter().find_map(find_branch_map),
        CanonicalValue::Null
        | CanonicalValue::Bool(_)
        | CanonicalValue::Integer(_)
        | CanonicalValue::Bytes(_)
        | CanonicalValue::Text(_) => None,
    }
}

fn map_field<'a>(value: &'a CanonicalValue, field: &str) -> &'a CanonicalValue {
    let CanonicalValue::Map(entries) = value else {
        panic!("expected map while looking up {field:?}");
    };
    entries
        .iter()
        .find_map(|(key, value)| (key == &CanonicalValue::Text(field.to_owned())).then_some(value))
        .unwrap_or_else(|| panic!("missing map field {field:?}"))
}

#[test]
fn max_only_bytes_preserve_prior_canonical_shape() {
    let source = "package a.b@1;\n\
        type Input = { value: Bytes<max=32>, };\n\
        type Output = { value: Bytes<max=32>, };\n\
        intent t(input: Input) returns Output\n\
          profile p.read\n\
          basis none\n\
          budget <= p.tiny {\n\
          return { value: input.value };\n\
        }";
    let module = parse_module(source).expect("max-only bytes parse");
    let core = compile_to_core(&module, &statement_branch_context())
        .expect("max-only bytes compile to Core");
    let canonical =
        decode_canonical_cbor(&encode_core_module(&core).expect("max-only byte Core encodes"))
            .expect("max-only byte Core decodes");
    let input = map_field(map_field(&canonical, "types"), "Input");
    let value = map_field(map_field(input, "fields"), "value");
    assert_eq!(value, &CanonicalValue::Text("Bytes<max=32>".to_owned()));
    let CanonicalValue::Map(types) = map_field(&canonical, "types") else {
        panic!("Core types are a canonical map");
    };
    assert!(types.iter().all(|(key, _)| {
        key != &CanonicalValue::Text("Input.value".to_owned())
            && key != &CanonicalValue::Text("Output.value".to_owned())
    }));
}

#[test]
fn invalid_byte_interval_rejects_before_core_identity() {
    let mut core = bounded_hello_core();
    core.types.insert(
        "invalid.bytes".to_owned(),
        CoreType::Bytes {
            min: Some(33),
            max: 32,
        },
    );
    let failure = encode_core_module(&core).expect_err("invalid byte interval must reject");
    assert_eq!(failure.kind(), CanonicalErrorKind::UnsupportedValue);
}

#[test]
fn core_type_table_rejects_self_describing_reference_keys() {
    for (case, coordinate) in [
        ("intrinsic bool", "Bool"),
        ("intrinsic unit", "Unit"),
        (
            "structural",
            "Record<inner:Record<value:U64>,values:List<U64,max=2>>",
        ),
    ] {
        let mut core = bounded_hello_core();
        core.types.insert(coordinate.to_owned(), CoreType::Bool);

        let failure = encode_core_module(&core)
            .expect_err("self-describing key must reject before canonical identity");
        assert_eq!(
            failure.kind(),
            CanonicalErrorKind::UnsupportedValue,
            "{case}"
        );
    }
}

#[test]
fn canonical_core_rejects_every_invalid_named_definition_including_unused_entries() {
    let mut accepted = Vec::new();
    for (case, core) in invalid_named_definition_cases() {
        let encoded = encode_core_module(&core);
        let digested = digest_core_module(&core);
        if encoded.is_ok() || digested.is_ok() {
            accepted.push(case);
            continue;
        }
        assert_eq!(
            encoded
                .expect_err("invalid definition must not encode")
                .kind(),
            CanonicalErrorKind::UnsupportedValue,
            "{case}"
        );
        assert_eq!(
            digested
                .expect_err("invalid definition must not digest")
                .kind(),
            CanonicalErrorKind::UnsupportedValue,
            "{case}"
        );
    }
    assert!(
        accepted.is_empty(),
        "invalid definitions crossed canonical boundaries: {accepted:?}"
    );
}

fn invalid_named_definition_cases() -> Vec<(&'static str, CoreModule)> {
    let over_depth = (0..=128).fold("U64".to_owned(), |inner, _| format!("List<{inner},max=1>"));
    let mut shallow_cached_over_depth = bounded_hello_core();
    shallow_cached_over_depth.types.insert(
        "A.DepthBase".to_owned(),
        CoreType::Option {
            item: "U64".to_owned(),
        },
    );
    shallow_cached_over_depth.types.insert(
        "Z.DepthOverflow".to_owned(),
        CoreType::Option {
            item: (0..127).fold("A.DepthBase".to_owned(), |inner, _| {
                format!("Option<{inner}>")
            }),
        },
    );
    let mut cycle = bounded_hello_core();
    cycle.types.insert(
        "CycleA".to_owned(),
        CoreType::Option {
            item: "CycleB".to_owned(),
        },
    );
    cycle.types.insert(
        "CycleB".to_owned(),
        CoreType::Option {
            item: "CycleA".to_owned(),
        },
    );

    let definitions = [
        (
            "noncanonical nested reference",
            CoreType::List {
                item: "List<U64,max=01>".to_owned(),
                max: 2,
            },
        ),
        (
            "unsupported integer width",
            CoreType::Int {
                width: "I128".to_owned(),
            },
        ),
        (
            "invalid string canonicalization",
            CoreType::String {
                max: 8,
                canonical: "not-a-canonicalization".to_owned(),
            },
        ),
        (
            "incoherent byte bounds",
            CoreType::Bytes {
                min: Some(9),
                max: 8,
            },
        ),
        (
            "unresolved named child",
            CoreType::Option {
                item: "MissingType".to_owned(),
            },
        ),
        (
            "nominal contract mismatch",
            CoreType::Nominal {
                contract: "DifferentName".to_owned(),
                representation: "U64".to_owned(),
            },
        ),
        (
            "invalid record field",
            CoreType::Record {
                fields: BTreeMap::from([("bad field".to_owned(), "U64".to_owned())]),
            },
        ),
        (
            "empty variant",
            CoreType::Variant {
                cases: BTreeMap::new(),
            },
        ),
        ("over-depth child", CoreType::Option { item: over_depth }),
    ];

    let mut cases = definitions
        .into_iter()
        .map(|(case, definition)| {
            let mut core = bounded_hello_core();
            core.types.insert(case.replace(' ', "_"), definition);
            (case, core)
        })
        .collect::<Vec<_>>();
    cases.push((
        "shallow-cached over-depth named expansion",
        shallow_cached_over_depth,
    ));
    cases.push(("named-reference cycle", cycle));
    cases
}

#[test]
fn valid_unused_named_definition_remains_hash_significant() {
    let baseline = bounded_hello_core();
    let mut with_unused = baseline.clone();
    with_unused.types.insert(
        "UnusedButAuthored".to_owned(),
        CoreType::List {
            item: "U64".to_owned(),
            max: 4,
        },
    );

    encode_core_module(&with_unused).expect("valid unused named definition encodes");
    assert_ne!(
        digest_core_module(&baseline).expect("baseline Core digests"),
        digest_core_module(&with_unused).expect("unused authored name remains hash-significant")
    );
}

#[test]
fn statement_branch_omits_binding_and_preserves_exact_canonical_identity() {
    let module = parse_module(STATEMENT_BRANCH).expect("statement branch parses");
    let core = compile_to_core(&module, &statement_branch_context())
        .expect("statement branch compiles to Core");
    let bytes = encode_core_module(&core).expect("statement branch Core encodes");
    let decoded = decode_canonical_cbor(&bytes).expect("statement branch bytes decode");
    let branch = find_branch_map(&decoded).expect("canonical Core contains branch node");
    let actual_hex = bytes.iter().fold(
        String::with_capacity(bytes.len() * 2),
        |mut output, byte| {
            write!(&mut output, "{byte:02x}").expect("writing bytes to String cannot fail");
            output
        },
    );
    assert!(branch
        .iter()
        .all(|(key, _)| { key != &CanonicalValue::Text("binding".to_owned()) }));
    assert_eq!(actual_hex, STATEMENT_BRANCH_BYTES_HEX.trim());
    assert_eq!(
        format!(
            "{}\n",
            digest_core_module(&core).expect("statement branch Core digests")
        ),
        STATEMENT_BRANCH_DIGEST
    );
}

#[test]
fn canonical_core_bytes_are_independent_of_map_construction_order() {
    let core = bounded_hello_core();

    let mut reordered = core.clone();
    reordered.types = core
        .types
        .iter()
        .rev()
        .map(|(name, ty)| (name.clone(), ty.clone()))
        .collect();
    reordered.intents = core
        .intents
        .iter()
        .rev()
        .map(|(name, intent)| (name.clone(), intent.clone()))
        .collect();

    assert_eq!(
        encode_core_module(&core).expect("canonical encoding succeeds"),
        encode_core_module(&reordered).expect("canonical encoding succeeds")
    );
}

#[test]
fn canonical_core_bytes_change_when_core_meaning_changes() {
    let module = parse_module(BOUNDED_HELLO).expect("fixture parses");
    let core = compile_to_core(&module, &hello_context()).expect("fixture compiles to Core");
    let mut changed = core.clone();
    changed
        .intents
        .get_mut("sayHello")
        .expect("intent exists")
        .core_evaluation_budget
        .max_steps += 1;

    assert_ne!(
        encode_core_module(&core).expect("canonical encoding succeeds"),
        encode_core_module(&changed).expect("canonical encoding succeeds")
    );
}

#[test]
fn canonical_core_bytes_change_when_effect_coordinate_changes() {
    let mut core = bounded_hello_core();
    let binding = core
        .intents
        .get("sayHello")
        .expect("intent exists")
        .body
        .locals
        .last()
        .expect("local exists")
        .clone();
    core.intents
        .get_mut("sayHello")
        .expect("intent exists")
        .body
        .nodes
        .push(CoreNode::Effect {
            binding,
            effect: "target.replace".to_owned(),
            input: amblicit_syntax::CoreExpr::Const(amblicit_syntax::CoreValue::String(
                "input".to_owned(),
            )),
            obstruction_map: BTreeMap::default(),
        });
    let mut changed = core.clone();
    let CoreNode::Effect { effect, .. } = changed
        .intents
        .get_mut("sayHello")
        .expect("intent exists")
        .body
        .nodes
        .last_mut()
        .expect("effect node exists")
    else {
        panic!("effect node");
    };
    *effect = "target.archive".to_owned();

    assert_ne!(
        encode_core_module(&core).expect("canonical encoding succeeds"),
        encode_core_module(&changed).expect("changed effect encoding succeeds")
    );
}

#[test]
fn canonical_core_bytes_change_when_local_identity_changes() {
    let core = bounded_hello_core();
    let mut changed = core.clone();
    changed
        .intents
        .get_mut("sayHello")
        .expect("intent exists")
        .body
        .locals
        .first_mut()
        .expect("input local exists")
        .id = "arg.changed".to_owned();

    assert_ne!(
        encode_core_module(&core).expect("canonical encoding succeeds"),
        encode_core_module(&changed).expect("changed Core encoding succeeds")
    );
}

#[test]
fn canonical_core_bytes_decode_and_reencode_stably() {
    let module = parse_module(BOUNDED_HELLO).expect("fixture parses");
    let core = compile_to_core(&module, &hello_context()).expect("fixture compiles to Core");
    let bytes = encode_core_module(&core).expect("canonical encoding succeeds");

    let decoded = decode_canonical_cbor(&bytes).expect("canonical bytes decode");

    assert_eq!(
        encode_canonical_cbor(&decoded).expect("canonical value re-encodes"),
        bytes
    );
}

#[test]
fn canonical_core_rejects_unresolved_import_digest() {
    let module = parse_module(BOUNDED_HELLO).expect("fixture parses");
    let mut core = compile_to_core(&module, &hello_context()).expect("fixture compiles to Core");
    core.imports
        .first_mut()
        .expect("fixture has an import")
        .resource
        .digest = None;

    let err = encode_core_module(&core).expect_err("unresolved import digest rejects");

    assert_eq!(err.kind(), CanonicalErrorKind::UnresolvedDigest);
}

#[test]
fn canonical_core_bytes_ignore_import_alias_spelling() {
    let module = parse_module(BOUNDED_HELLO).expect("fixture parses");
    let core = compile_to_core(&module, &hello_context()).expect("fixture compiles to Core");
    let mut changed = core.clone();
    changed
        .imports
        .first_mut()
        .expect("fixture has an import")
        .alias = Some("renamed".to_owned());

    assert_eq!(
        encode_core_module(&core).expect("canonical encoding succeeds"),
        encode_core_module(&changed).expect("canonical encoding succeeds")
    );
}

#[test]
fn canonical_core_bytes_normalize_digest_hex_case() {
    let module = parse_module(BOUNDED_HELLO).expect("fixture parses");
    let core = compile_to_core(&module, &hello_context()).expect("fixture compiles to Core");
    let mut changed = core.clone();
    changed
        .imports
        .first_mut()
        .expect("fixture has an import")
        .resource
        .digest =
        Some("sha256:ABCDEFABCDEFABCDEFABCDEFABCDEFABCDEFABCDEFABCDEFABCDEFABCDEFABCD".to_owned());
    let mut normalized = changed.clone();
    normalized
        .imports
        .first_mut()
        .expect("fixture has an import")
        .resource
        .digest =
        Some("sha256:abcdefabcdefabcdefabcdefabcdefabcdefabcdefabcdefabcdefabcdefabcd".to_owned());

    assert_eq!(
        encode_core_module(&changed).expect("canonical encoding succeeds"),
        encode_core_module(&normalized).expect("canonical encoding succeeds")
    );
    assert_ne!(
        encode_core_module(&core).expect("canonical encoding succeeds"),
        encode_core_module(&normalized).expect("canonical encoding succeeds")
    );
}

#[test]
fn canonical_core_bytes_are_independent_of_import_order() {
    let module = parse_module(BOUNDED_HELLO).expect("fixture parses");
    let mut core = compile_to_core(&module, &hello_context()).expect("fixture compiles to Core");
    core.imports.push(CoreImport {
        kind: CoreImportKind::Core,
        resource: ResourceRef {
            coordinate: "core.collections@1".to_owned(),
            digest: Some(
                "sha256:1111111111111111111111111111111111111111111111111111111111111111"
                    .to_owned(),
            ),
        },
        alias: Some("collections".to_owned()),
    });
    let mut reordered = core.clone();
    reordered.imports.reverse();

    assert_eq!(
        encode_core_module(&core).expect("canonical encoding succeeds"),
        encode_core_module(&reordered).expect("canonical encoding succeeds")
    );
}

#[test]
fn canonical_core_bytes_treat_required_capabilities_as_a_set() {
    let module = parse_module(BOUNDED_HELLO).expect("fixture parses");
    let mut core = compile_to_core(&module, &hello_context()).expect("fixture compiles to Core");
    core.required_core_capabilities = vec![
        "core.variant/v1".to_owned(),
        "core.map/v1".to_owned(),
        "core.map/v1".to_owned(),
    ];
    let mut normalized = core.clone();
    normalized.required_core_capabilities =
        vec!["core.map/v1".to_owned(), "core.variant/v1".to_owned()];

    assert_eq!(
        encode_core_module(&core).expect("canonical encoding succeeds"),
        encode_core_module(&normalized).expect("canonical encoding succeeds")
    );
}

#[test]
fn canonical_core_bytes_are_independent_of_input_constraint_order() {
    let module = parse_module(BOUNDED_HELLO).expect("fixture parses");
    let mut core = compile_to_core(&module, &hello_context()).expect("fixture compiles to Core");
    core.intents
        .get_mut("sayHello")
        .expect("intent exists")
        .input_constraints
        .push(InputConstraint {
            coordinate: "compiler.0".to_owned(),
            source: InputConstraintSource::Compiler,
            predicate: CorePredicate::True,
        });
    let mut reordered = core.clone();
    reordered
        .intents
        .get_mut("sayHello")
        .expect("intent exists")
        .input_constraints
        .reverse();

    assert_eq!(
        encode_core_module(&core).expect("canonical encoding succeeds"),
        encode_core_module(&reordered).expect("canonical encoding succeeds")
    );
}

#[test]
fn noncanonical_cbor_bytes_reject_with_stable_error_kind() {
    let err = decode_canonical_cbor(&[0x18, 0x00]).expect_err("non-minimal zero rejects");

    assert_eq!(err.kind(), CanonicalErrorKind::NonCanonical);
}

#[test]
fn canonical_cbor_rejects_duplicate_map_keys_on_encode() {
    let err = encode_canonical_cbor(&CanonicalValue::Map(vec![
        (
            CanonicalValue::Text("key".to_owned()),
            CanonicalValue::Integer(1),
        ),
        (
            CanonicalValue::Text("key".to_owned()),
            CanonicalValue::Integer(2),
        ),
    ]))
    .expect_err("duplicate map keys reject");

    assert_eq!(err.kind(), CanonicalErrorKind::DuplicateMapKey);
}

#[test]
fn oversized_cbor_array_length_returns_error_without_panicking() {
    let result = std::panic::catch_unwind(|| {
        decode_canonical_cbor(&[0x9b, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff])
    });

    let err = result
        .expect("decoder returns an error instead of panicking")
        .expect_err("declared array length exceeds available input");

    assert_eq!(err.kind(), CanonicalErrorKind::UnexpectedEof);
}

#[test]
fn canonical_cbor_integer_widths_are_platform_independent() {
    assert_eq!(
        encode_canonical_cbor(&CanonicalValue::Integer(23)).expect("integer encodes"),
        vec![0x17]
    );
    assert_eq!(
        encode_canonical_cbor(&CanonicalValue::Integer(24)).expect("integer encodes"),
        vec![0x18, 0x18]
    );
    assert_eq!(
        encode_canonical_cbor(&CanonicalValue::Integer(256)).expect("integer encodes"),
        vec![0x19, 0x01, 0x00]
    );
    assert_eq!(
        encode_canonical_cbor(&CanonicalValue::Integer(-1)).expect("integer encodes"),
        vec![0x20]
    );
}

#[test]
fn canonical_core_bytes_are_source_alpha_rename_invariant() {
    let renamed = BOUNDED_HELLO
        .replace("let message = ", "let greeting = ")
        .replace("return { message };", "return { message: greeting };");

    let original = parse_module(BOUNDED_HELLO).expect("fixture parses");
    let renamed = parse_module(&renamed).expect("renamed source parses");
    let original_core =
        compile_to_core(&original, &hello_context()).expect("original compiles to Core");
    let renamed_core = compile_to_core(&renamed, &hello_context()).expect("renamed compiles");

    assert_eq!(
        encode_core_module(&original_core).expect("canonical encoding succeeds"),
        encode_core_module(&renamed_core).expect("canonical encoding succeeds")
    );
}

#[test]
fn canonical_core_bytes_are_parameter_alpha_rename_invariant() {
    let renamed = BOUNDED_HELLO
        .replace(
            "sayHello(input: HelloInput)",
            "sayHello(person: HelloInput)",
        )
        .replace("input.name", "person.name");

    let original = parse_module(BOUNDED_HELLO).expect("fixture parses");
    let renamed = parse_module(&renamed).expect("renamed source parses");
    let original_core =
        compile_to_core(&original, &hello_context()).expect("original compiles to Core");
    let renamed_core = compile_to_core(&renamed, &hello_context()).expect("renamed compiles");

    assert_eq!(
        encode_core_module(&original_core).expect("canonical encoding succeeds"),
        encode_core_module(&renamed_core).expect("canonical encoding succeeds")
    );
}

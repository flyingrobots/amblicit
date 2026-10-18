//! Source closure witnesses use actual source bytes, not caller-authored ASTs.
use edict_syntax::source_closure::{
    resolve_source_closure, source_module_digest, SourceClosureErrorKind, SourceClosureLimits,
};
use edict_syntax::{decode_canonical_cbor, CanonicalValue};
use std::fmt::Write;

const LEAF: &str = "package library.bytes@1;\nfn marker() -> Bytes<max=1> { return b\"A\"; }\n";

fn imports(package: &str, dependency: &str, source: &str, alias: &str) -> String {
    format!(
        "package {package}; use module {dependency} digest \"{}\" as {alias};",
        source_module_digest(source).unwrap().to_review_string(),
    )
}

#[test]
fn source_digest_matches_an_independently_framed_fixed_vector() {
    // SHA-256 of canonical CBOR ["edict.digest/v1", "edict.source/v1", bstr(LEAF)],
    // independently encoded with Python byte framing and hashlib.
    assert_eq!(
        source_module_digest(LEAF).unwrap().to_review_string(),
        "sha256:b69a636a99599fbc566e827918a6c34208cde8c8a62c47c2343dbc64b9eb7f82"
    );
}

fn field<'a>(value: &'a CanonicalValue, key: &str) -> &'a CanonicalValue {
    let CanonicalValue::Map(fields) = value else {
        panic!("map expected")
    };
    fields
        .iter()
        .find_map(|(name, value)| (name == &CanonicalValue::Text(key.into())).then_some(value))
        .expect("field")
}

#[test]
fn source_closure_wire_binds_root_exact_bytes_and_typed_digests() {
    let root = imports("app.example@1", "library.bytes@1", LEAF, "leaf");
    let closure = resolve_source_closure(&root, &[LEAF], SourceClosureLimits::default()).unwrap();
    let artifact = decode_canonical_cbor(closure.canonical_bytes()).unwrap();
    assert_eq!(
        field(&artifact, "apiVersion"),
        &CanonicalValue::Text("edict.source-closure/v1".into())
    );
    assert_eq!(
        field(&artifact, "root"),
        &CanonicalValue::Text("app.example@1".into())
    );
    let module = field(field(&artifact, "modules"), "library.bytes@1");
    assert_eq!(
        field(module, "source"),
        &CanonicalValue::Bytes(LEAF.as_bytes().to_vec())
    );
    assert_eq!(
        field(module, "digest"),
        &CanonicalValue::Array(vec![
            CanonicalValue::Text("sha256".into()),
            CanonicalValue::Bytes(source_module_digest(LEAF).unwrap().bytes().to_vec()),
        ])
    );
}

#[test]
fn shared_dependency_cache_does_not_hide_a_deeper_import_path() {
    let middle = imports("library.middle@1", "library.bytes@1", LEAF, "leaf");
    let mut root = imports("app.example@1", "library.bytes@1", LEAF, "shallow");
    write!(
        root,
        " use module library.middle@1 digest \"{}\" as deep;",
        source_module_digest(&middle).unwrap()
    )
    .unwrap();
    let limits = SourceClosureLimits {
        max_depth: 2,
        ..SourceClosureLimits::default()
    };
    let failure = resolve_source_closure(&root, &[LEAF, &middle], limits).unwrap_err();
    assert_eq!(failure.kind(), SourceClosureErrorKind::BudgetExceeded);
    let exact = SourceClosureLimits {
        max_depth: 3,
        ..limits
    };
    assert!(resolve_source_closure(&root, &[&middle, LEAF], exact).is_ok());
}

#[test]
fn source_closure_is_complete_deterministic_and_preserves_exact_bytes() {
    let middle = imports("library.middle@1", "library.bytes@1", LEAF, "leaf");
    let root = imports("app.example@1", "library.middle@1", &middle, "middle");
    let limits = SourceClosureLimits::default();
    let first = resolve_source_closure(&root, &[&middle, LEAF], limits).unwrap();
    let reordered = resolve_source_closure(&root, &[LEAF, &middle], limits).unwrap();
    assert_eq!(first.root_coordinate(), "app.example@1");
    assert_eq!(first.modules().len(), 3);
    assert_eq!(first.modules()["library.bytes@1"].source(), LEAF);
    assert_eq!(
        first.modules()["library.middle@1"].parsed().imports[0].alias,
        "leaf"
    );
    assert_eq!(first.canonical_bytes(), reordered.canonical_bytes());
    assert_eq!(first.digest(), reordered.digest());
    let changed_leaf = format!("{LEAF}// retained source bytes matter\n");
    let changed_middle = imports("library.middle@1", "library.bytes@1", &changed_leaf, "leaf");
    let changed_root = imports(
        "app.example@1",
        "library.middle@1",
        &changed_middle,
        "middle",
    );
    let changed =
        resolve_source_closure(&changed_root, &[&changed_leaf, &changed_middle], limits).unwrap();
    assert_ne!(first.digest(), changed.digest());
}

#[test]
fn source_closure_rejects_missing_substituted_duplicate_and_disconnected_inputs() {
    let root = imports("app.example@1", "library.bytes@1", LEAF, "leaf");
    for (inputs, kind) in [
        (vec![], SourceClosureErrorKind::MissingModule),
        (vec![LEAF, LEAF], SourceClosureErrorKind::DuplicateModule),
        (
            vec!["package other.library@1;", LEAF],
            SourceClosureErrorKind::DisconnectedModule,
        ),
        (
            vec!["package library.bytes@1;"],
            SourceClosureErrorKind::DigestMismatch,
        ),
    ] {
        let failure =
            resolve_source_closure(&root, &inputs, SourceClosureLimits::default()).unwrap_err();
        assert_eq!(failure.kind(), kind);
    }
}

#[test]
fn source_closure_rejects_cycles_before_treating_any_source_as_authenticated() {
    // This is a topology refusal, not a claim that a SHA-256-pinned cycle exists.
    let root = format!(
        "package app.example@1; use module app.example@1 digest \"sha256:{}\" as selfModule;",
        "1".repeat(64)
    );
    let failure = resolve_source_closure(&root, &[], SourceClosureLimits::default()).unwrap_err();
    assert_eq!(failure.kind(), SourceClosureErrorKind::ImportCycle);
}

#[test]
fn source_closure_enforces_exact_module_byte_and_depth_apertures() {
    let root = imports("app.example@1", "library.bytes@1", LEAF, "leaf");
    let exact = SourceClosureLimits {
        max_modules: 2,
        max_source_bytes: root.len() + LEAF.len(),
        max_depth: 2,
    };
    assert!(resolve_source_closure(&root, &[LEAF], exact).is_ok());
    for limits in [
        SourceClosureLimits {
            max_modules: 1,
            ..exact
        },
        SourceClosureLimits {
            max_source_bytes: exact.max_source_bytes - 1,
            ..exact
        },
        SourceClosureLimits {
            max_depth: 1,
            ..exact
        },
    ] {
        let failure = resolve_source_closure(&root, &[LEAF], limits).unwrap_err();
        assert_eq!(failure.kind(), SourceClosureErrorKind::BudgetExceeded);
    }
}

#[test]
fn callers_cannot_raise_the_fixed_source_closure_ceilings() {
    let unlimited = SourceClosureLimits {
        max_modules: usize::MAX,
        max_source_bytes: usize::MAX,
        max_depth: usize::MAX,
    };
    let oversized = " ".repeat(1024 * 1024 + 1);
    assert_eq!(
        resolve_source_closure(&oversized, &[], unlimited)
            .unwrap_err()
            .kind(),
        SourceClosureErrorKind::BudgetExceeded
    );
    let sources = (0..64)
        .map(|index| format!("package library.leaf{index}@1;"))
        .collect::<Vec<_>>();
    let mut root = String::from("package app.example@1;");
    for (index, source) in sources[..63].iter().enumerate() {
        write!(
            root,
            " use module library.leaf{index}@1 digest \"{}\" as leaf{index};",
            source_module_digest(source).unwrap()
        )
        .unwrap();
    }
    let inputs = sources.iter().map(String::as_str).collect::<Vec<_>>();
    assert!(resolve_source_closure(&root, &inputs[..63], unlimited).is_ok());
    assert_eq!(
        resolve_source_closure(&root, &inputs, unlimited)
            .unwrap_err()
            .kind(),
        SourceClosureErrorKind::BudgetExceeded
    );
    let mut chain = vec![String::from("package library.layer0@1;")];
    for index in 1..33 {
        chain.push(imports(
            &format!("library.layer{index}@1"),
            &format!("library.layer{}@1", index - 1),
            &chain[index - 1],
            "previous",
        ));
    }
    let dependencies = chain[..31].iter().map(String::as_str).collect::<Vec<_>>();
    assert!(resolve_source_closure(&chain[31], &dependencies, unlimited).is_ok());
    let dependencies = chain[..32].iter().map(String::as_str).collect::<Vec<_>>();
    assert_eq!(
        resolve_source_closure(&chain[32], &dependencies, unlimited)
            .unwrap_err()
            .kind(),
        SourceClosureErrorKind::BudgetExceeded
    );
}

#[test]
fn source_closure_reports_invalid_source_and_rejects_duplicate_aliases() {
    let failure =
        resolve_source_closure("package broken", &[], SourceClosureLimits::default()).unwrap_err();
    assert_eq!(failure.kind(), SourceClosureErrorKind::InvalidSource);
    let root = imports("app.example@1", "library.bytes@1", LEAF, "leaf");
    let duplicate = format!(
        "{root} use module library.bytes@1 digest \"{}\" as leaf;",
        source_module_digest(LEAF).unwrap()
    );
    let failure =
        resolve_source_closure(&duplicate, &[LEAF], SourceClosureLimits::default()).unwrap_err();
    assert_eq!(failure.kind(), SourceClosureErrorKind::InvalidSource);
}

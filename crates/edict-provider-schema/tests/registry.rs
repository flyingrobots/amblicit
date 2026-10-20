use std::sync::Arc;

use edict_provider_schema::{
    ProviderArtifactSchemaRegistry, ProviderSchemaRegistryFailureKind,
    ResolvedProviderSchemaArtifact, PROVIDER_SCHEMA_VALIDATION_MAX_NESTING_DEPTH,
};
use edict_syntax::{
    bind_target_provider_manifest, decode_canonical_cbor, CanonicalValue, ProviderArtifactKind,
    ProviderArtifactRef, ProviderArtifactSchemaValidationErrorKind,
    ProviderArtifactSchemaValidator, ProviderArtifactSource, ProviderSchemaBinding,
    ProviderSchemaFormat, ResourceRef, TargetProviderManifest, TARGET_PROVIDER_ABI,
    TARGET_PROVIDER_MANIFEST_API_VERSION,
};
use sha2::{Digest, Sha256};

const GENERATED_SCHEMA: &[u8] = br#"
generated-artifact = {
  kind: "generated",
  value: uint,
}
"#;

const REVIEW_SCHEMA: &[u8] = br#"
review-payload = { approved: bool }
verifier-report = { status: "valid" / "invalid" }
"#;

const DISCRIMINATED_RECURSIVE_SCHEMA: &[u8] = br#"
generated-artifact = leaf / short-child / long-child
leaf = { kind: "leaf", value: uint }
short-child = { a: generated-artifact, kind: "short" }
long-child = { kind: "long", payload: generated-artifact }
"#;

const REQUIRED_KEY_RECURSIVE_SCHEMA: &[u8] = br#"
generated-artifact = closed / legacy
closed = {
  kind: "artifact",
  semanticClosure: uint,
  ? child: generated-artifact,
}
legacy = {
  kind: "artifact",
  value: uint,
  ? child: generated-artifact,
}
"#;

const PROVIDER_CONTRACT_SCHEMA: &[u8] =
    include_bytes!("../../../fixtures/provider-contracts/v1/edict-provider-contracts.cddl");
const CORE_FIXTURE: &[u8] =
    include_bytes!("../../../fixtures/core/canonical/bounded-hello.core.cbor");

fn digest(bytes: &[u8]) -> String {
    format!("sha256:{:x}", Sha256::digest(bytes))
}

fn resource(coordinate: &str, digest: String) -> ResourceRef {
    ResourceRef {
        coordinate: coordinate.to_owned(),
        digest: Some(digest),
    }
}

fn schema_artifact(role: &str, coordinate: &str, bytes: &[u8]) -> ProviderArtifactRef {
    ProviderArtifactRef {
        role: role.to_owned(),
        artifact_kind: ProviderArtifactKind::ArtifactSchema,
        resource: resource(coordinate, digest(bytes)),
        source: ProviderArtifactSource::Generated {
            semantic_source: resource(
                "runtime.semantic-source@1",
                format!("sha256:{}", "1".repeat(64)),
            ),
            generator: resource(
                "runtime.provider-generator@1",
                format!("sha256:{}", "2".repeat(64)),
            ),
        },
    }
}

fn manifest(generated_schema: &[u8], review_schema: &[u8]) -> TargetProviderManifest {
    TargetProviderManifest {
        api_version: TARGET_PROVIDER_MANIFEST_API_VERSION.to_owned(),
        provider_abi: TARGET_PROVIDER_ABI.to_owned(),
        provider: resource("runtime.provider@1", format!("sha256:{}", "3".repeat(64))),
        artifacts: vec![
            schema_artifact(
                "schema.generated",
                "runtime.generated-artifact.cddl@1",
                generated_schema,
            ),
            schema_artifact(
                "schema.review",
                "runtime.review-artifacts.cddl@1",
                review_schema,
            ),
        ],
        schema_bindings: vec![
            ProviderSchemaBinding {
                domain: "runtime.generated-artifact/v1".to_owned(),
                schema_role: "schema.generated".to_owned(),
                format: ProviderSchemaFormat::SelfContainedCddlV1,
                root_rule: "generated-artifact".to_owned(),
            },
            ProviderSchemaBinding {
                domain: "runtime.review-payload/v1".to_owned(),
                schema_role: "schema.review".to_owned(),
                format: ProviderSchemaFormat::SelfContainedCddlV1,
                root_rule: "review-payload".to_owned(),
            },
            ProviderSchemaBinding {
                domain: "runtime.verifier-report/v1".to_owned(),
                schema_role: "schema.review".to_owned(),
                format: ProviderSchemaFormat::SelfContainedCddlV1,
                root_rule: "verifier-report".to_owned(),
            },
        ],
    }
}

fn resolved() -> Vec<ResolvedProviderSchemaArtifact> {
    vec![
        ResolvedProviderSchemaArtifact {
            role: "schema.generated".to_owned(),
            bytes: Arc::from(GENERATED_SCHEMA),
        },
        ResolvedProviderSchemaArtifact {
            role: "schema.review".to_owned(),
            bytes: Arc::from(REVIEW_SCHEMA),
        },
    ]
}

fn map(entries: &[(&str, CanonicalValue)]) -> CanonicalValue {
    CanonicalValue::Map(
        entries
            .iter()
            .map(|(key, value)| (CanonicalValue::Text((*key).to_owned()), value.clone()))
            .collect(),
    )
}

fn nested_array(depth: usize, leaf: CanonicalValue) -> CanonicalValue {
    (0..depth).fold(leaf, |value, _| CanonicalValue::Array(vec![value]))
}

fn effectful_core_with_obstruction_map() -> CanonicalValue {
    let local = map(&[
        ("id", CanonicalValue::Text("local@0".to_owned())),
        ("alphaName", CanonicalValue::Text("value".to_owned())),
        ("type", CanonicalValue::Text("Unit".to_owned())),
    ]);
    let null = map(&[("kind", CanonicalValue::Text("null".to_owned()))]);
    let expression = map(&[
        ("kind", CanonicalValue::Text("const".to_owned())),
        ("value", null),
    ]);
    let effect = map(&[
        ("kind", CanonicalValue::Text("effect".to_owned())),
        ("binding", local.clone()),
        (
            "effect",
            CanonicalValue::Text("echo.test@1.apply".to_owned()),
        ),
        ("input", expression.clone()),
        (
            "obstructionMap",
            map(&[(
                "Okay_1",
                map(&[("binder", local), ("value", expression.clone())]),
            )]),
        ),
    ]);
    let intent = map(&[
        ("input", CanonicalValue::Text("Unit".to_owned())),
        ("output", CanonicalValue::Text("Unit".to_owned())),
        (
            "requiredOperationProfile",
            CanonicalValue::Text("echo.test@1.profile".to_owned()),
        ),
        ("inputConstraints", CanonicalValue::Array(Vec::new())),
        (
            "coreEvaluationBudget",
            map(&[
                ("maxSteps", CanonicalValue::Integer(100)),
                ("maxAllocatedBytes", CanonicalValue::Integer(1_024)),
                ("maxOutputBytes", CanonicalValue::Integer(1_024)),
            ]),
        ),
        (
            "body",
            map(&[
                ("locals", CanonicalValue::Array(Vec::new())),
                ("nodes", CanonicalValue::Array(vec![effect])),
                ("result", expression),
            ]),
        ),
    ]);
    map(&[
        (
            "apiVersion",
            CanonicalValue::Text("edict.core/v1".to_owned()),
        ),
        (
            "coordinate",
            CanonicalValue::Text("test.effectful-core@1".to_owned()),
        ),
        ("imports", CanonicalValue::Array(Vec::new())),
        ("types", map(&[])),
        ("intents", map(&[("apply", intent)])),
        (
            "requiredCoreCapabilities",
            CanonicalValue::Array(Vec::new()),
        ),
    ])
}

fn registry() -> ProviderArtifactSchemaRegistry {
    let manifest = Box::leak(Box::new(manifest(GENERATED_SCHEMA, REVIEW_SCHEMA)));
    let validated = bind_target_provider_manifest(manifest).expect("manifest validates");
    ProviderArtifactSchemaRegistry::from_manifest(
        &validated,
        resolved(),
        [
            "runtime.generated-artifact/v1",
            "runtime.review-payload/v1",
            "runtime.verifier-report/v1",
        ],
    )
    .expect("registry constructs")
}

fn registry_with_generated_schema(
    schema: &[u8],
) -> Result<ProviderArtifactSchemaRegistry, edict_provider_schema::ProviderSchemaRegistryFailure> {
    registry_with_generated_schema_root(schema, "generated-artifact")
}

fn registry_with_generated_schema_root(
    schema: &[u8],
    root: &str,
) -> Result<ProviderArtifactSchemaRegistry, edict_provider_schema::ProviderSchemaRegistryFailure> {
    let mut manifest = manifest(schema, REVIEW_SCHEMA);
    root.clone_into(&mut manifest.schema_bindings[0].root_rule);
    let manifest = Box::leak(Box::new(manifest));
    let validated = bind_target_provider_manifest(manifest).expect("manifest validates");
    ProviderArtifactSchemaRegistry::from_manifest(
        &validated,
        vec![
            ResolvedProviderSchemaArtifact {
                role: "schema.generated".to_owned(),
                bytes: Arc::from(schema),
            },
            resolved().remove(1),
        ],
        ["runtime.generated-artifact/v1"],
    )
}

fn assert_generated_schema_compile_failure(name: &str, schema: &[u8]) {
    cddl_cat::flatten::flatten_from_str(std::str::from_utf8(schema).expect("schema case is UTF-8"))
        .unwrap_or_else(|_| panic!("{name} must be syntactically valid CDDL"));
    let Err(failure) = registry_with_generated_schema(schema) else {
        panic!("{name} must reject during registry construction");
    };
    assert_eq!(
        failure.kind(),
        ProviderSchemaRegistryFailureKind::SchemaCompileFailed,
        "{name}"
    );
    assert_eq!(
        failure.domain(),
        Some("runtime.generated-artifact/v1"),
        "{name}"
    );
    assert_eq!(failure.schema_role(), Some("schema.generated"), "{name}");
}

#[test]
fn every_registered_domain_performs_real_schema_instance_validation() {
    let registry = registry();
    let cases = [
        (
            "runtime.generated-artifact/v1",
            map(&[
                ("kind", CanonicalValue::Text("generated".to_owned())),
                ("value", CanonicalValue::Integer(7)),
            ]),
            map(&[
                ("kind", CanonicalValue::Text("generated".to_owned())),
                ("value", CanonicalValue::Text("seven".to_owned())),
            ]),
        ),
        (
            "runtime.review-payload/v1",
            map(&[("approved", CanonicalValue::Bool(true))]),
            map(&[("approved", CanonicalValue::Text("yes".to_owned()))]),
        ),
        (
            "runtime.verifier-report/v1",
            map(&[("status", CanonicalValue::Text("valid".to_owned()))]),
            map(&[("status", CanonicalValue::Text("unknown".to_owned()))]),
        ),
    ];

    assert_eq!(registry.bindings().len(), cases.len());
    for (domain, valid, invalid) in cases {
        assert!(registry.supports_domain(domain));
        registry
            .validate_canonical_value(domain, &valid)
            .expect("valid instance passes");
        assert_eq!(
            registry.validate_canonical_value(domain, &invalid),
            Err(ProviderArtifactSchemaValidationErrorKind::SchemaMismatch)
        );
    }
    assert_eq!(
        registry.validate_canonical_value("runtime.absent/v1", &CanonicalValue::Null),
        Err(ProviderArtifactSchemaValidationErrorKind::UnsupportedDomain)
    );
}

#[test]
fn construction_rejects_missing_ambiguous_and_digest_mismatched_schema_bytes() {
    let manifest = manifest(GENERATED_SCHEMA, REVIEW_SCHEMA);
    let validated = bind_target_provider_manifest(&manifest).expect("manifest validates");

    let missing = ProviderArtifactSchemaRegistry::from_manifest(
        &validated,
        vec![resolved().remove(0)],
        ["runtime.generated-artifact/v1"],
    )
    .expect_err("missing schema rejects");
    assert_eq!(
        missing.kind(),
        ProviderSchemaRegistryFailureKind::SchemaArtifactMissing
    );

    let mut ambiguous_artifacts = resolved();
    ambiguous_artifacts.push(ambiguous_artifacts[0].clone());
    let ambiguous = ProviderArtifactSchemaRegistry::from_manifest(
        &validated,
        ambiguous_artifacts,
        ["runtime.generated-artifact/v1"],
    )
    .expect_err("duplicate resolved role rejects");
    assert_eq!(
        ambiguous.kind(),
        ProviderSchemaRegistryFailureKind::SchemaArtifactAmbiguous
    );

    let mut mismatched = resolved();
    mismatched[0].bytes = Arc::from(b"generated-artifact = null".as_slice());
    let mismatch = ProviderArtifactSchemaRegistry::from_manifest(
        &validated,
        mismatched,
        ["runtime.generated-artifact/v1"],
    )
    .expect_err("digest mismatch rejects");
    assert_eq!(
        mismatch.kind(),
        ProviderSchemaRegistryFailureKind::SchemaArtifactDigestMismatch
    );

    let mut unsupported_artifacts = resolved();
    unsupported_artifacts.push(ResolvedProviderSchemaArtifact {
        role: "schema.unbound".to_owned(),
        bytes: Arc::from(b"unbound = null".as_slice()),
    });
    let unsupported = ProviderArtifactSchemaRegistry::from_manifest(
        &validated,
        unsupported_artifacts,
        ["runtime.generated-artifact/v1"],
    )
    .expect_err("resolver roles outside the manifest schema closure reject");
    assert_eq!(
        unsupported.kind(),
        ProviderSchemaRegistryFailureKind::SchemaArtifactUnsupported
    );
}

#[test]
fn construction_requires_complete_domain_closure_and_compilable_roots() {
    let base_manifest = manifest(GENERATED_SCHEMA, REVIEW_SCHEMA);
    let validated = bind_target_provider_manifest(&base_manifest).expect("manifest validates");
    let missing_domain = ProviderArtifactSchemaRegistry::from_manifest(
        &validated,
        resolved(),
        ["runtime.absent/v1"],
    )
    .expect_err("required unbound domain rejects");
    assert_eq!(
        missing_domain.kind(),
        ProviderSchemaRegistryFailureKind::RequiredDomainMissing
    );

    let malformed = b"generated-artifact = {";
    let malformed_manifest = manifest(malformed, REVIEW_SCHEMA);
    let validated =
        bind_target_provider_manifest(&malformed_manifest).expect("manifest envelope validates");
    let compile = ProviderArtifactSchemaRegistry::from_manifest(
        &validated,
        vec![
            ResolvedProviderSchemaArtifact {
                role: "schema.generated".to_owned(),
                bytes: Arc::from(malformed.as_slice()),
            },
            resolved().remove(1),
        ],
        ["runtime.generated-artifact/v1"],
    )
    .expect_err("malformed CDDL rejects during construction");
    assert_eq!(
        compile.kind(),
        ProviderSchemaRegistryFailureKind::SchemaCompileFailed
    );

    let external = b"generated-artifact = absent-external-rule";
    let external_manifest = manifest(external, REVIEW_SCHEMA);
    let validated =
        bind_target_provider_manifest(&external_manifest).expect("manifest envelope validates");
    let external_reference = ProviderArtifactSchemaRegistry::from_manifest(
        &validated,
        vec![
            ResolvedProviderSchemaArtifact {
                role: "schema.generated".to_owned(),
                bytes: Arc::from(external.as_slice()),
            },
            resolved().remove(1),
        ],
        ["runtime.generated-artifact/v1"],
    )
    .expect_err("external rule resolution rejects during construction");
    assert_eq!(
        external_reference.kind(),
        ProviderSchemaRegistryFailureKind::SchemaCompileFailed
    );

    let mut missing_root_manifest = manifest(GENERATED_SCHEMA, REVIEW_SCHEMA);
    missing_root_manifest.schema_bindings[0].root_rule = "absent-root".to_owned();
    let validated = bind_target_provider_manifest(&missing_root_manifest)
        .expect("nonempty root remains a valid manifest envelope");
    let missing_root = ProviderArtifactSchemaRegistry::from_manifest(
        &validated,
        resolved(),
        ["runtime.generated-artifact/v1"],
    )
    .expect_err("missing root rejects during construction");
    assert_eq!(
        missing_root.kind(),
        ProviderSchemaRegistryFailureKind::SchemaRootRuleMissing
    );
}

#[test]
fn construction_rejects_latent_or_non_progressing_validator_shapes() {
    let cases: &[(&str, &[u8])] = &[
        ("generic root", b"generated-artifact<T> = T"),
        (
            "wrong generic arity",
            b"generated-artifact = pair<uint>\npair<A, B> = [A, B]",
        ),
        (
            "standalone group",
            b"generated-artifact = (one: int, two: int)",
        ),
        ("direct cycle", b"generated-artifact = generated-artifact"),
        (
            "mutual alias cycle",
            b"generated-artifact = alias\nalias = generated-artifact",
        ),
        (
            "choice-only cycle",
            b"generated-artifact = generated-artifact / uint",
        ),
        (
            "guarded branch cannot launder direct cycle",
            b"generated-artifact = uint / [generated-artifact] / generated-artifact",
        ),
        (
            "zero-progress array repetition",
            b"generated-artifact = [* ()]",
        ),
        (
            "zero-progress map repetition",
            b"generated-artifact = {* ()}",
        ),
        (
            "nested zero-progress repetition",
            b"empty = ()\ngenerated-artifact = [* empty]",
        ),
        (
            "context-invalid unwrap",
            b"generated-artifact = ~array-type\narray-type = [uint]",
        ),
        (
            "context-invalid choiceify",
            b"generated-artifact = &scalar\nscalar = uint",
        ),
        (
            "structurally invalid control",
            b"generated-artifact = int .size 2",
        ),
    ];

    for (name, schema) in cases {
        cddl_cat::flatten::flatten_from_str(
            std::str::from_utf8(schema).expect("schema case is UTF-8"),
        )
        .unwrap_or_else(|_| panic!("{name} must be syntactically valid CDDL"));
        let Err(failure) = registry_with_generated_schema(schema) else {
            panic!("{name} must reject during registry construction");
        };
        assert_eq!(
            failure.kind(),
            ProviderSchemaRegistryFailureKind::SchemaCompileFailed,
            "{name}"
        );
    }
}

#[test]
fn construction_rejects_ambiguous_recursive_choice_shapes() {
    let cases: &[(&str, &[u8])] = &[
        (
            "duplicate recursive arrays",
            b"generated-artifact = [generated-artifact] / [generated-artifact] / tstr",
        ),
        (
            "overlapping recursive arrays",
            b"generated-artifact = [generated-artifact] / [generated-artifact, * uint] / tstr",
        ),
        (
            "same-tag recursive maps",
            b"generated-artifact = branch-a / branch-b / tstr\nbranch-a = { kind: \"branch\", child: generated-artifact }\nbranch-b = { kind: \"branch\", child: generated-artifact, ? extra: uint }",
        ),
        (
            "overlapping discriminator value sets",
            b"generated-artifact = branch-a / branch-b / tstr\nbranch-a = { kind: \"branch-a\" / \"shared\", child: generated-artifact }\nbranch-b = { kind: \"branch-b\" / \"shared\", child: generated-artifact }",
        ),
        (
            "multiple optional recursive array members",
            b"generated-artifact = tstr / [? generated-artifact, ? generated-artifact]",
        ),
        (
            "multiple optional recursive map fallbacks",
            b"generated-artifact = tstr / { ? tstr => generated-artifact, ? uint => generated-artifact }",
        ),
        (
            "multiple variable array members under recursive dispatch",
            b"generated-artifact = leaf / left / right\nleaf = { kind: \"leaf\", value: uint }\nleft = { kind: \"left\", child: generated-artifact, items: [* uint, * tstr] }\nright = { kind: \"right\", child: generated-artifact }",
        ),
        (
            "non-final variable array member under recursive dispatch",
            b"generated-artifact = leaf / left / right\nleaf = { kind: \"leaf\", value: uint }\nleft = { kind: \"left\", child: generated-artifact, items: [* uint, uint] }\nright = { kind: \"right\", child: generated-artifact }",
        ),
        (
            "mixed literal and flexible map keys under recursive dispatch",
            b"generated-artifact = leaf / left / right\nleaf = { kind: \"leaf\", value: uint }\nleft = { kind: \"left\", child: generated-artifact, items: { fixed: uint, * tstr => uint } }\nright = { kind: \"right\", child: generated-artifact }",
        ),
    ];

    for (name, schema) in cases {
        assert_generated_schema_compile_failure(name, schema);
    }

    let overflow = format!(
        "generated-artifact = leaf / left / right\nleaf = {{ kind: \"leaf\", value: uint }}\nleft = {{ kind: \"left\", child: generated-artifact, items: [{0}*{0} uint, uint] }}\nright = {{ kind: \"right\", child: generated-artifact }}",
        usize::MAX
    );
    assert_generated_schema_compile_failure(
        "overflowing fixed array cardinality total",
        overflow.as_bytes(),
    );
}

#[test]
fn recursive_map_discriminator_dispatch_is_independent_of_encoded_key_order() {
    let registry = registry_with_generated_schema(DISCRIMINATED_RECURSIVE_SCHEMA)
        .expect("distinct singleton map discriminators must admit bounded dispatch");
    let leaf = map(&[
        ("kind", CanonicalValue::Text("leaf".to_owned())),
        ("value", CanonicalValue::Integer(1)),
    ]);

    // Canonical CBOR orders the one-byte key `a` before `kind`, so this value
    // encodes its recursive child before its discriminator.
    let discriminator_after_child = map(&[
        ("a", leaf.clone()),
        ("kind", CanonicalValue::Text("short".to_owned())),
    ]);
    registry
        .validate_canonical_value("runtime.generated-artifact/v1", &discriminator_after_child)
        .expect("dispatch must find a discriminator encoded after the recursive child");

    // Canonical CBOR orders `kind` before the longer key `payload`, so this
    // value encodes its discriminator before its recursive child.
    let discriminator_before_child = map(&[
        ("kind", CanonicalValue::Text("long".to_owned())),
        ("payload", leaf.clone()),
    ]);
    registry
        .validate_canonical_value("runtime.generated-artifact/v1", &discriminator_before_child)
        .expect("dispatch must find a discriminator encoded before the recursive child");

    let alternating_arms = map(&[
        (
            "a",
            map(&[
                ("kind", CanonicalValue::Text("long".to_owned())),
                (
                    "payload",
                    map(&[
                        ("a", leaf),
                        ("kind", CanonicalValue::Text("short".to_owned())),
                    ]),
                ),
            ]),
        ),
        ("kind", CanonicalValue::Text("short".to_owned())),
    ]);
    registry
        .validate_canonical_value("runtime.generated-artifact/v1", &alternating_arms)
        .expect("nested occurrences may select different recursive alternatives");
}

#[test]
fn recursive_required_key_dispatch_accepts_compatible_untagged_arms() {
    let registry = registry_with_generated_schema(REQUIRED_KEY_RECURSIVE_SCHEMA)
        .expect("one disjoint required key must admit bounded recursive dispatch");
    let closed = map(&[
        ("kind", CanonicalValue::Text("artifact".to_owned())),
        ("semanticClosure", CanonicalValue::Integer(1)),
    ]);
    let legacy = map(&[
        ("kind", CanonicalValue::Text("artifact".to_owned())),
        ("value", CanonicalValue::Integer(2)),
    ]);

    for value in [
        closed.clone(),
        legacy.clone(),
        map(&[
            ("child", legacy),
            ("kind", CanonicalValue::Text("artifact".to_owned())),
            ("semanticClosure", CanonicalValue::Integer(3)),
        ]),
        map(&[
            ("child", closed),
            ("kind", CanonicalValue::Text("artifact".to_owned())),
            ("value", CanonicalValue::Integer(4)),
        ]),
    ] {
        registry
            .validate_canonical_value("runtime.generated-artifact/v1", &value)
            .expect("required-key dispatch validates the selected recursive arm");
    }
}

#[test]
fn checked_target_ir_root_constructs_through_required_key_dispatch() {
    registry_with_generated_schema_root(PROVIDER_CONTRACT_SCHEMA, "target-ir-artifact")
        .expect("the checked Target IR compatibility union must construct");
}

#[test]
fn nested_tagged_families_keep_required_key_dispatch_bounded_and_exact() {
    let schema = br#"
generated-artifact = phased / ordered
phased = phased-closed / phased-legacy
ordered = ordered-closed / ordered-legacy
phased-closed = {kind: "phased", closure: uint, ? child: generated-artifact}
phased-legacy = {kind: "phased", value: uint, ? child: generated-artifact}
ordered-closed = {kind: "ordered", closure: uint, order: [* uint], ? child: generated-artifact}
ordered-legacy = {kind: "ordered", value: uint, order: [* uint], ? child: generated-artifact}
"#;
    let registry = registry_with_generated_schema(schema).expect("nested families compile");
    let phased = map(&[
        ("kind", CanonicalValue::Text("phased".to_owned())),
        ("value", CanonicalValue::Integer(1)),
    ]);
    let ordered = map(&[
        ("kind", CanonicalValue::Text("ordered".to_owned())),
        ("closure", CanonicalValue::Integer(1)),
        ("order", CanonicalValue::Array(Vec::new())),
        ("child", phased.clone()),
    ]);
    for value in [phased, ordered] {
        registry
            .validate_canonical_value("runtime.generated-artifact/v1", &value)
            .expect("valid selected family");
    }
    for kind in ["ordered", "unknown"] {
        let missing_order = map(&[
            ("kind", CanonicalValue::Text(kind.to_owned())),
            ("value", CanonicalValue::Integer(1)),
        ]);
        assert_eq!(
            registry.validate_canonical_value("runtime.generated-artifact/v1", &missing_order),
            Err(ProviderArtifactSchemaValidationErrorKind::SchemaMismatch)
        );
    }
}

#[test]
fn recursive_map_discriminator_dispatch_rejects_invalid_discriminators() {
    let registry = registry_with_generated_schema(DISCRIMINATED_RECURSIVE_SCHEMA)
        .expect("distinct singleton map discriminators must admit bounded dispatch");
    let leaf = map(&[
        ("kind", CanonicalValue::Text("leaf".to_owned())),
        ("value", CanonicalValue::Integer(1)),
    ]);

    let cases = [
        ("missing", map(&[("payload", leaf.clone())])),
        (
            "unknown",
            map(&[
                (
                    "a",
                    nested_array(
                        PROVIDER_SCHEMA_VALIDATION_MAX_NESTING_DEPTH - 2,
                        CanonicalValue::Text("malicious".to_owned()),
                    ),
                ),
                ("kind", CanonicalValue::Text("unknown".to_owned())),
            ]),
        ),
        (
            "mismatching",
            map(&[
                (
                    "a",
                    nested_array(
                        PROVIDER_SCHEMA_VALIDATION_MAX_NESTING_DEPTH - 2,
                        CanonicalValue::Text("malicious".to_owned()),
                    ),
                ),
                ("kind", CanonicalValue::Text("long".to_owned())),
            ]),
        ),
    ];

    for (name, value) in cases {
        assert_eq!(
            registry.validate_canonical_value("runtime.generated-artifact/v1", &value),
            Err(ProviderArtifactSchemaValidationErrorKind::SchemaMismatch),
            "{name} discriminator must reject with the stable schema failure",
        );
    }

    let duplicate = CanonicalValue::Map(vec![
        (
            CanonicalValue::Text("kind".to_owned()),
            CanonicalValue::Text("short".to_owned()),
        ),
        (
            CanonicalValue::Text("kind".to_owned()),
            CanonicalValue::Text("long".to_owned()),
        ),
        (CanonicalValue::Text("a".to_owned()), leaf),
    ]);
    assert_eq!(
        registry.validate_canonical_value("runtime.generated-artifact/v1", &duplicate),
        Err(ProviderArtifactSchemaValidationErrorKind::SchemaMismatch),
        "duplicate discriminator keys must reject with the stable schema failure",
    );
}

#[test]
fn recursive_dispatch_preserves_native_regexp_map_key_semantics() {
    let schema = br#"
generated-artifact = leaf / branch
leaf = { kind: "leaf", value: uint }
branch = {
  kind: "branch",
  child: generated-artifact,
  obstructionMap: { * failure-ident => generated-artifact },
}
failure-ident = tstr .regexp "[A-Za-z_][A-Za-z0-9_]*"
"#;
    let registry = registry_with_generated_schema(schema)
        .expect("recursive regexp-key maps belong to the bounded dispatch subset");
    let leaf = map(&[
        ("kind", CanonicalValue::Text("leaf".to_owned())),
        ("value", CanonicalValue::Integer(1)),
    ]);
    let valid = map(&[
        ("kind", CanonicalValue::Text("branch".to_owned())),
        ("child", leaf.clone()),
        ("obstructionMap", map(&[("Okay_1", leaf.clone())])),
    ]);
    registry
        .validate_canonical_value("runtime.generated-artifact/v1", &valid)
        .expect("the pinned native regexp must admit a matching map key");

    let invalid = map(&[
        ("kind", CanonicalValue::Text("branch".to_owned())),
        ("child", leaf.clone()),
        ("obstructionMap", map(&[("---", leaf)])),
    ]);
    assert_eq!(
        registry.validate_canonical_value("runtime.generated-artifact/v1", &invalid),
        Err(ProviderArtifactSchemaValidationErrorKind::SchemaMismatch),
        "specialization must not erase the pinned native regexp predicate",
    );
}

#[test]
fn construction_accepts_guarded_recursive_roots() {
    let cases: &[(&str, &[u8], CanonicalValue)] = &[
        (
            "map child",
            b"generated-artifact = leaf / branch\nleaf = { kind: \"leaf\", value: uint }\nbranch = { kind: \"branch\", child: generated-artifact }",
            map(&[
                ("kind", CanonicalValue::Text("branch".to_owned())),
                (
                    "child",
                    map(&[
                        ("kind", CanonicalValue::Text("leaf".to_owned())),
                        ("value", CanonicalValue::Integer(1)),
                    ]),
                ),
            ]),
        ),
        (
            "array element",
            b"generated-artifact = uint / [* generated-artifact]",
            CanonicalValue::Array(vec![
                CanonicalValue::Integer(1),
                CanonicalValue::Array(vec![CanonicalValue::Integer(2)]),
            ]),
        ),
        (
            "map key",
            b"generated-artifact = tstr / { generated-artifact => uint }",
            CanonicalValue::Map(vec![(
                CanonicalValue::Map(vec![(
                    CanonicalValue::Text("leaf".to_owned()),
                    CanonicalValue::Integer(1),
                )]),
                CanonicalValue::Integer(2),
            )]),
        ),
        (
            "mutual map child",
            b"generated-artifact = left\nleft = { ? right: right }\nright = { ? left: left }",
            map(&[("right", map(&[("left", map(&[]))]))]),
        ),
    ];

    for (name, schema, value) in cases {
        let registry = registry_with_generated_schema(schema)
            .unwrap_or_else(|_| panic!("guarded recursive {name} schema must construct"));
        registry
            .validate_canonical_value("runtime.generated-artifact/v1", value)
            .unwrap_or_else(|_| panic!("finite guarded recursive {name} value must validate"));
    }

    let registry = registry_with_generated_schema_root(PROVIDER_CONTRACT_SCHEMA, "core-module")
        .expect("published recursive Core schema root must construct");
    let core = decode_canonical_cbor(CORE_FIXTURE).expect("reviewed Core fixture is canonical");
    registry
        .validate_canonical_value("runtime.generated-artifact/v1", &core)
        .expect("reviewed Core fixture validates through the published recursive root");

    registry
        .validate_canonical_value(
            "runtime.generated-artifact/v1",
            &effectful_core_with_obstruction_map(),
        )
        .expect("published Core root must admit a nonempty regexp-keyed effect obstruction map");
}

#[test]
fn guarded_recursive_validation_is_bounded_and_rejects_invalid_descendants() {
    let registry =
        registry_with_generated_schema(b"generated-artifact = uint / [generated-artifact]")
            .expect("guarded recursive array schema constructs");

    registry
        .validate_canonical_value(
            "runtime.generated-artifact/v1",
            &nested_array(
                PROVIDER_SCHEMA_VALIDATION_MAX_NESTING_DEPTH,
                CanonicalValue::Integer(1),
            ),
        )
        .expect("maximum-depth guarded recursive value validates");

    assert_eq!(
        registry.validate_canonical_value(
            "runtime.generated-artifact/v1",
            &nested_array(
                PROVIDER_SCHEMA_VALIDATION_MAX_NESTING_DEPTH + 1,
                CanonicalValue::Integer(1),
            ),
        ),
        Err(ProviderArtifactSchemaValidationErrorKind::SchemaMismatch),
        "one-over-depth value must reject before native CDDL validation",
    );

    assert_eq!(
        registry.validate_canonical_value(
            "runtime.generated-artifact/v1",
            &CanonicalValue::Array(vec![CanonicalValue::Array(vec![CanonicalValue::Text(
                "invalid".to_owned()
            ),])]),
        ),
        Err(ProviderArtifactSchemaValidationErrorKind::SchemaMismatch),
        "invalid nested recursive child must retain the stable schema failure",
    );
}

#[test]
fn construction_accepts_unbounded_occurrences_that_must_consume_input() {
    let cases: &[(&str, &[u8], CanonicalValue)] = &[
        (
            "array",
            b"generated-artifact = [* uint]",
            CanonicalValue::Array(vec![CanonicalValue::Integer(1), CanonicalValue::Integer(2)]),
        ),
        (
            "map",
            b"generated-artifact = {* tstr => uint}",
            map(&[("one", CanonicalValue::Integer(1))]),
        ),
    ];

    for (name, schema, value) in cases {
        let registry = registry_with_generated_schema(schema)
            .unwrap_or_else(|_| panic!("progressing {name} occurrence must construct"));
        registry
            .validate_canonical_value("runtime.generated-artifact/v1", value)
            .unwrap_or_else(|_| panic!("progressing {name} occurrence must validate"));
    }
}

#[test]
fn registry_receipt_and_behavior_are_independent_of_input_order() {
    let manifest = manifest(GENERATED_SCHEMA, REVIEW_SCHEMA);
    let validated = bind_target_provider_manifest(&manifest).expect("manifest validates");
    let first = ProviderArtifactSchemaRegistry::from_manifest(
        &validated,
        resolved(),
        ["runtime.generated-artifact/v1", "runtime.review-payload/v1"],
    )
    .expect("first registry constructs");
    let mut reversed = resolved();
    reversed.reverse();
    let second = ProviderArtifactSchemaRegistry::from_manifest(
        &validated,
        reversed,
        ["runtime.review-payload/v1", "runtime.generated-artifact/v1"],
    )
    .expect("second registry constructs");

    assert_eq!(first.bindings(), second.bindings());
    let value = map(&[
        ("kind", CanonicalValue::Text("generated".to_owned())),
        ("value", CanonicalValue::Integer(7)),
    ]);
    assert_eq!(
        first.validate_canonical_value("runtime.generated-artifact/v1", &value),
        second.validate_canonical_value("runtime.generated-artifact/v1", &value)
    );
}

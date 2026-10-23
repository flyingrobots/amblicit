//! Executable compatibility between the Edict authority-facts CDDL and codec.

use std::sync::Arc;

use amblicit_provider_schema::{ProviderArtifactSchemaRegistry, ResolvedProviderSchemaArtifact};
use amblicit_syntax::{
    bind_target_provider_manifest, decode_canonical_cbor, encode_authority_facts_cbor,
    AuthorityFactSource, AuthorityFactSourceKind, AuthorityFactsDocument, BudgetFact,
    CanonicalValue, CoreBudget, EffectWriteClassFact, OperationProfileFact, ProviderArtifactKind,
    ProviderArtifactRef, ProviderArtifactSchemaValidationErrorKind,
    ProviderArtifactSchemaValidator, ProviderArtifactSource, ProviderSchemaBinding,
    ProviderSchemaFormat, ResourceRef, TargetProviderManifest, WriteClass,
    AUTHORITY_FACTS_API_VERSION, AUTHORITY_FACTS_CDDL_ROOT, TARGET_PROVIDER_ABI,
    TARGET_PROVIDER_MANIFEST_API_VERSION,
};
use sha2::{Digest, Sha256};

const AUTHORITY_FACTS_CDDL: &str = concat!(
    include_str!("../../../docs/abi/edict-common.cddl"),
    "\n",
    include_str!("../../../docs/abi/edict-authority-facts.cddl")
);

#[test]
fn authority_facts_cddl_accepts_only_the_frozen_root() {
    assert_eq!(AUTHORITY_FACTS_CDDL_ROOT, "authority-facts");

    let document = AuthorityFactsDocument {
        api_version: AUTHORITY_FACTS_API_VERSION.to_owned(),
        source: AuthorityFactSource {
            kind: AuthorityFactSourceKind::TargetProfile,
            coordinate: "example.target@1".to_owned(),
            digest: format!("sha256:{}", "1".repeat(64)),
        },
        operation_profiles: vec![OperationProfileFact {
            source: "p.effectful".to_owned(),
            core: "continuum.profile.write/v1".to_owned(),
            allowed_write_classes: vec![WriteClass::Read, WriteClass::Replace],
        }],
        effect_write_classes: vec![EffectWriteClassFact {
            effect: "target.replace".to_owned(),
            write_class: WriteClass::Replace,
        }],
        budgets: vec![BudgetFact {
            source: "p.tiny".to_owned(),
            budget: CoreBudget {
                max_steps: 8,
                max_allocated_bytes: 1024,
                max_output_bytes: 256,
            },
        }],
    };
    let encoded = encode_authority_facts_cbor(&document).expect("authority facts encode");
    assert_eq!(
        encoded,
        include_bytes!(
            "../../../fixtures/authority-facts/canonical/example-effectful.authority-facts.cbor"
        )
    );
    let canonical = decode_canonical_cbor(&encoded).expect("canonical value decodes");
    let registry = authority_facts_registry();

    registry
        .validate_canonical_value(AUTHORITY_FACTS_API_VERSION, &canonical)
        .expect("codec output satisfies frozen CDDL root");

    let mut lawpack_document = document.clone();
    lawpack_document.source.kind = AuthorityFactSourceKind::Lawpack;
    lawpack_document.source.coordinate = "hello.lawpack@1".to_owned();
    let lawpack_encoded =
        encode_authority_facts_cbor(&lawpack_document).expect("lawpack authority facts encode");
    let lawpack_canonical =
        decode_canonical_cbor(&lawpack_encoded).expect("lawpack canonical value decodes");
    registry
        .validate_canonical_value(AUTHORITY_FACTS_API_VERSION, &lawpack_canonical)
        .expect("lawpack source satisfies frozen CDDL root");

    assert_eq!(
        registry.validate_canonical_value(AUTHORITY_FACTS_API_VERSION, &CanonicalValue::Null),
        Err(ProviderArtifactSchemaValidationErrorKind::SchemaMismatch)
    );

    let mut legacy_array_set = canonical;
    *nested_map_value_mut(
        &mut legacy_array_set,
        &["operationProfiles", "p.effectful", "allowedWriteClasses"],
    ) = CanonicalValue::Array(vec![CanonicalValue::Text("replace".to_owned())]);
    assert_eq!(
        registry.validate_canonical_value(AUTHORITY_FACTS_API_VERSION, &legacy_array_set),
        Err(ProviderArtifactSchemaValidationErrorKind::SchemaMismatch),
        "write-class sets must be structurally canonical maps",
    );

    let mut unsupported_source = lawpack_canonical;
    *nested_map_value_mut(&mut unsupported_source, &["source", "kind"]) =
        CanonicalValue::Text("runtime".to_owned());
    assert_eq!(
        registry.validate_canonical_value(AUTHORITY_FACTS_API_VERSION, &unsupported_source),
        Err(ProviderArtifactSchemaValidationErrorKind::SchemaMismatch),
        "only lawpack and target-profile authority sources are admissible",
    );
}

fn authority_facts_registry() -> ProviderArtifactSchemaRegistry {
    let schema_bytes = AUTHORITY_FACTS_CDDL.as_bytes();
    let schema_role = "schema.edict-authority-facts";
    let artifact = ProviderArtifactRef {
        role: schema_role.to_owned(),
        artifact_kind: ProviderArtifactKind::ArtifactSchema,
        resource: resource(
            "edict.authority-facts.cddl@1",
            format!("sha256:{:x}", Sha256::digest(schema_bytes)),
        ),
        source: ProviderArtifactSource::Generated {
            semantic_source: resource(
                "edict.authority-facts-contract@1",
                format!("sha256:{}", "1".repeat(64)),
            ),
            generator: resource(
                "edict.schema-assembly@1",
                format!("sha256:{}", "2".repeat(64)),
            ),
        },
    };
    let manifest = TargetProviderManifest {
        api_version: TARGET_PROVIDER_MANIFEST_API_VERSION.to_owned(),
        provider_abi: TARGET_PROVIDER_ABI.to_owned(),
        provider: resource(
            "test.authority-facts-provider@1",
            format!("sha256:{}", "3".repeat(64)),
        ),
        artifacts: vec![artifact],
        schema_bindings: vec![ProviderSchemaBinding {
            domain: AUTHORITY_FACTS_API_VERSION.to_owned(),
            schema_role: schema_role.to_owned(),
            format: ProviderSchemaFormat::SelfContainedCddlV1,
            root_rule: AUTHORITY_FACTS_CDDL_ROOT.to_owned(),
        }],
    };
    let validated = bind_target_provider_manifest(&manifest).expect("manifest validates");
    ProviderArtifactSchemaRegistry::from_manifest(
        &validated,
        [ResolvedProviderSchemaArtifact {
            role: schema_role.to_owned(),
            bytes: Arc::from(schema_bytes),
        }],
        [AUTHORITY_FACTS_API_VERSION],
    )
    .expect("authority-facts schema registry constructs")
}

fn resource(coordinate: &str, digest: String) -> ResourceRef {
    ResourceRef {
        coordinate: coordinate.to_owned(),
        digest: Some(digest),
    }
}

fn nested_map_value_mut<'a>(
    value: &'a mut CanonicalValue,
    path: &[&str],
) -> &'a mut CanonicalValue {
    let Some((field, tail)) = path.split_first() else {
        return value;
    };
    let CanonicalValue::Map(entries) = value else {
        panic!("{field} parent must be a map");
    };
    let child = entries
        .iter_mut()
        .find_map(|(key, value)| {
            (key == &CanonicalValue::Text((*field).to_owned())).then_some(value)
        })
        .unwrap_or_else(|| panic!("missing map field {field}"));
    nested_map_value_mut(child, tail)
}

//! Separate consumer crate: every Edict name comes from the curated facade.
use std::collections::BTreeMap;

use edict::{
    artifact::{
        decode_canonical_cbor, decode_result_projection, digest_core_module,
        digest_result_projection, digest_target_ir_artifact, encode_core_module,
        encode_result_projection, encode_target_ir_artifact, verify_result_projection,
        CanonicalValue, CoreBlock, CoreBudget, CoreExpr, CoreIntent, CoreModule, CoreType,
        LocalRef, ResourceRef, ResultProjection, ResultProjectionExpr, ResultProjectionSource,
        TargetIrArtifact, TargetIrIntent, TargetIrSemanticClosure, VerifiedResultProjection,
    },
    check,
    diagnostic::Span,
    CheckOutcome,
};

fn core_fixture() -> CoreModule {
    let input = LocalRef {
        id: "arg.0".to_owned(),
        alpha_name: "input".to_owned(),
        ty: "examples.facade@1.Input".to_owned(),
    };
    let result = CoreExpr::Local {
        reference: input.clone(),
    };
    CoreModule {
        functions: BTreeMap::new(),
        api_version: "edict.core/v1".to_owned(),
        coordinate: "examples.facade@1".to_owned(),
        imports: Vec::new(),
        types: BTreeMap::from([(
            "Input".to_owned(),
            CoreType::Record {
                fields: BTreeMap::from([("ok".to_owned(), "Bool".to_owned())]),
            },
        )]),
        intents: BTreeMap::from([(
            "echo".to_owned(),
            CoreIntent {
                input: input.ty.clone(),
                output: input.ty.clone(),
                required_operation_profile: "continuum.profile.read-only/v1".to_owned(),
                basis: Some(result.clone()),
                input_constraints: Vec::new(),
                core_evaluation_budget: CoreBudget {
                    max_steps: 8,
                    max_allocated_bytes: 256,
                    max_output_bytes: 64,
                },
                body: CoreBlock {
                    locals: vec![input],
                    nodes: Vec::new(),
                    result,
                },
            },
        )]),
        required_core_capabilities: Vec::new(),
    }
}

fn target_fixture(core: &CoreModule) -> TargetIrArtifact {
    let intent = &core.intents["echo"];
    TargetIrArtifact {
        domain: "echo.span-ir/v1".to_owned(),
        target_profile: ResourceRef {
            coordinate: "echo.dpo@1".to_owned(),
            digest: Some(format!("sha256:{}", "1".repeat(64))),
        },
        source_core_coordinate: core.coordinate.clone(),
        semantic_closure: Some(TargetIrSemanticClosure {
            source_core: ResourceRef {
                coordinate: core.coordinate.clone(),
                digest: Some(
                    digest_core_module(core)
                        .expect("Core digest")
                        .to_review_string(),
                ),
            },
            lawpacks: Vec::new(),
            capabilities: Vec::new(),
        }),
        intents: BTreeMap::from([(
            "echo".to_owned(),
            TargetIrIntent {
                execution_order: None,
                operation_profile: intent.required_operation_profile.clone(),
                basis: intent.basis.clone(),
                input_constraints: intent.input_constraints.clone(),
                core_evaluation_budget: intent.core_evaluation_budget.clone(),
                pure_bindings: Vec::new(),
                requirements: Vec::new(),
                steps: Vec::new(),
                external_action_requests: Vec::new(),
                result: intent.body.result.clone(),
            },
        )]),
    }
}

#[test]
fn facade_consumer_constructs_and_verifies_artifacts() {
    let core = core_fixture();
    let target = target_fixture(&core);
    let projection = ResultProjection {
        api_version: "edict.result-projection/v1".to_owned(),
        operation_coordinate: "examples.facade@1.echo".to_owned(),
        output_type: "examples.facade@1.Input".to_owned(),
        max_output_bytes: 64,
        expression: ResultProjectionExpr::Source {
            source: ResultProjectionSource::ApplicationInput,
            path: Vec::new(),
        },
    };

    let core_bytes = encode_core_module(&core).expect("encode consumer Core");
    let target_bytes = encode_target_ir_artifact(&target).expect("encode consumer Target IR");
    let projection_bytes = encode_result_projection(&projection).expect("encode projection");
    let decoded: CanonicalValue = decode_canonical_cbor(&core_bytes).expect("decode Core value");
    assert!(matches!(decoded, CanonicalValue::Map(_)));
    assert!(matches!(
        decode_canonical_cbor(&target_bytes).expect("decode Target IR value"),
        CanonicalValue::Map(_)
    ));
    assert_eq!(
        decode_result_projection(&projection_bytes).expect("decode projection"),
        projection
    );
    let projection_digest = digest_result_projection(&projection).expect("projection digest");
    let verified: VerifiedResultProjection =
        verify_result_projection(&core, &target, "echo", &projection_bytes, projection_digest)
            .expect("independently verify consumer projection");
    assert_eq!(verified.projection(), &projection);
    assert_eq!(verified.digest(), projection_digest);
    assert_ne!(
        digest_core_module(&core).expect("Core identity"),
        digest_target_ir_artifact(&target).expect("Target IR identity")
    );
}

#[test]
fn facade_consumer_names_diagnostic_spans() {
    let CheckOutcome::ParseFailed(error) = check("package ;") else {
        panic!("malformed package must fail parsing");
    };
    let span: Span = error.span;
    assert_eq!(span, Span { start: 8, end: 9 });
}

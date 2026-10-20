//! Compiler-owned result-projection contract and independent verification.

use std::collections::BTreeMap;

use edict_syntax::{
    decode_canonical_cbor, decode_lawpack_adapter, decode_lawpack_bundle, decode_result_projection,
    digest_core_module, digest_result_projection, emit_result_projection, encode_canonical_cbor,
    encode_result_projection, lower_to_target_ir, parse_module, prepare_lawpack_compilation,
    verify_result_projection, CanonicalValue, CompilerContext, CoreBlock, CoreBound, CoreBudget,
    CoreExpr, CoreModule, CoreNode, CorePredicate, CoreType, LocalRef, ResourceRef,
    ResultProjection, ResultProjectionArtifact, ResultProjectionExpr, ResultProjectionFailureKind,
    ResultProjectionSource, TargetIrArtifact, TargetIrLoweringFacts, TargetLoweringReport,
    TargetLoweringStatus, MAX_RESULT_PROJECTION_ARTIFACT_BYTES, MAX_RESULT_PROJECTION_NODES,
    MAX_RESULT_PROJECTION_PATH_SEGMENTS, MAX_RESULT_PROJECTION_TEXT_BYTES,
    RESULT_PROJECTION_API_VERSION,
};

const MANIFEST_BYTES: &[u8] = include_bytes!("../../../fixtures/lawpack/hello-echo/manifest.cbor");
const EXPORTS_BYTES: &[u8] = include_bytes!("../../../fixtures/lawpack/hello-echo/exports.cbor");
const ADAPTER_BYTES: &[u8] = include_bytes!("../../../fixtures/lawpack/hello-echo/adapter.cbor");
const SOURCE: &str = include_str!("../../../fixtures/lawpack/hello-echo/create-greeting.edict");
const PROJECTION_BYTES: &[u8] =
    include_bytes!("../../../fixtures/lawpack/hello-echo/create-greeting.result-projection.cbor");
const PROJECTION_DIGEST: &str =
    include_str!("../../../fixtures/lawpack/hello-echo/create-greeting.result-projection.sha256");
const PROPERTY_SEED: u64 = 0x1730_5eed_cafe_babe;
const PURE_SOURCE: &str = "package examples.pure@1;\n\
    use lawpack examples.pure.law@1 digest \"sha256:0000000000000000000000000000000000000000000000000000000000000000\" as law;\n\
    type Input = { name: String<max=16>, };\n\
    type Output = { message: String<max=18>, };\n\
    intent greet(input: Input) returns Output\n\
      profile law.read\n\
      basis input.name\n\
      budget <= law.tiny {\n\
      let prefix = \"hi\";\n\
      let message = prefix + input.name;\n\
      return { message };\n\
    }";

fn hello_echo_core_and_facts() -> (CoreModule, TargetIrLoweringFacts) {
    let bundle =
        decode_lawpack_bundle(MANIFEST_BYTES, EXPORTS_BYTES).expect("load Hello Echo lawpack");
    let adapter = decode_lawpack_adapter(&bundle, "echo.dpo@1", ADAPTER_BYTES)
        .expect("load Hello Echo adapter");
    let module = parse_module(SOURCE).expect("parse Hello Echo source");
    let preparation = prepare_lawpack_compilation(&module, &bundle, &adapter)
        .expect("prepare Hello Echo compilation");
    let core = edict_syntax::compile_to_core(&module, preparation.compiler_context())
        .expect("compile Hello Echo Core");
    (core, preparation.target_ir_facts().clone())
}

fn hello_echo_lowering() -> (CoreModule, TargetLoweringReport) {
    let (core, facts) = hello_echo_core_and_facts();
    let target = lower_to_target_ir(&core, &facts);
    assert_eq!(target.status, TargetLoweringStatus::Lowered);
    (core, target)
}

fn hello_echo() -> (CoreModule, TargetIrArtifact) {
    let (core, target) = hello_echo_lowering();
    (core, target.artifact.expect("lower Hello Echo Target IR"))
}

fn pure_lowering() -> (CoreModule, TargetLoweringReport) {
    let module = parse_module(PURE_SOURCE).expect("parse pure source");
    let core = edict_syntax::compile_to_core(
        &module,
        &CompilerContext::new()
            .with_operation_profile("law.read", "continuum.profile.read-only/v1")
            .with_budget(
                "law.tiny",
                CoreBudget {
                    max_steps: 8,
                    max_allocated_bytes: 256,
                    max_output_bytes: 128,
                },
            ),
    )
    .expect("compile pure Core");
    let facts = TargetIrLoweringFacts {
        target_profile: ResourceRef {
            coordinate: "echo.dpo@1".to_owned(),
            digest: Some(format!("sha256:{}", "1".repeat(64))),
        },
        target_ir_domain: "echo.span-ir/v1".to_owned(),
        operation_profiles: vec!["continuum.profile.read-only/v1".to_owned()],
        obstruction_coordinates: Vec::new(),
        effect_lowerings: Vec::new(),
        effect_signatures: Vec::new(),
        pure_functions: Vec::new(),
    };
    let report = lower_to_target_ir(&core, &facts);
    (core, report)
}

fn expected_expression() -> ResultProjectionExpr {
    ResultProjectionExpr::Record {
        fields: BTreeMap::from([
            (
                "key".to_owned(),
                ResultProjectionExpr::Source {
                    source: ResultProjectionSource::CapabilityResult {
                        step_id: "createGreeting.step.0".to_owned(),
                    },
                    path: vec!["key".to_owned()],
                },
            ),
            (
                "message".to_owned(),
                ResultProjectionExpr::Source {
                    source: ResultProjectionSource::ApplicationInput,
                    path: vec!["message".to_owned()],
                },
            ),
        ]),
    }
}

fn projection_with_fields(count: usize) -> ResultProjection {
    let fields = (0..count)
        .map(|index| {
            (
                format!("field{index:04}"),
                ResultProjectionExpr::Source {
                    source: ResultProjectionSource::ApplicationInput,
                    path: vec![format!("source{index:04}")],
                },
            )
        })
        .collect();
    ResultProjection {
        api_version: RESULT_PROJECTION_API_VERSION.to_owned(),
        operation_coordinate: "examples.boundary@1.project".to_owned(),
        output_type: "examples.boundary@1.Output".to_owned(),
        max_output_bytes: 65_536,
        expression: ResultProjectionExpr::Record { fields },
    }
}

fn mutate_results(core: &mut CoreModule, target: &mut TargetIrArtifact, expression: CoreExpr) {
    core.intents
        .get_mut("createGreeting")
        .expect("Core intent")
        .body
        .result = expression.clone();
    target
        .intents
        .get_mut("createGreeting")
        .expect("Target IR intent")
        .result = expression;
    repin_target_core(core, target);
}

fn repin_target_core(core: &CoreModule, target: &mut TargetIrArtifact) {
    target
        .semantic_closure
        .as_mut()
        .expect("semantic closure")
        .source_core
        .digest = Some(
        digest_core_module(core)
            .expect("digest mutated Core")
            .to_review_string(),
    );
}

fn remove_canonical_field(value: &mut CanonicalValue, field: &str) {
    let CanonicalValue::Map(entries) = value else {
        panic!("canonical projection root is a map");
    };
    entries.retain(|(key, _)| key != &CanonicalValue::Text(field.to_owned()));
}

fn canonical_field_mut<'a>(value: &'a mut CanonicalValue, field: &str) -> &'a mut CanonicalValue {
    let CanonicalValue::Map(entries) = value else {
        panic!("canonical projection root is a map");
    };
    entries
        .iter_mut()
        .find_map(|(key, value)| (key == &CanonicalValue::Text(field.to_owned())).then_some(value))
        .unwrap_or_else(|| panic!("missing canonical field {field}"))
}

#[test]
fn exact_core_and_target_ir_emit_the_typed_hello_echo_projection() {
    let (core, target) = hello_echo();
    let artifact = emit_result_projection(&core, &target, "createGreeting")
        .expect("emit exact result projection");

    assert_eq!(
        artifact.projection().api_version,
        RESULT_PROJECTION_API_VERSION
    );
    assert_eq!(
        artifact.projection().operation_coordinate,
        "examples.hello_echo@1.createGreeting"
    );
    assert_eq!(
        artifact.projection().output_type,
        "examples.hello_echo@1.GreetingCreated"
    );
    assert_eq!(artifact.projection().max_output_bytes, 2048);
    assert_eq!(artifact.projection().expression, expected_expression());
    assert_eq!(
        decode_result_projection(artifact.canonical_bytes()).expect("decode emitted projection"),
        *artifact.projection()
    );
    assert_eq!(
        digest_result_projection(artifact.projection()).expect("digest projection"),
        artifact.digest()
    );
    assert_eq!(artifact.canonical_bytes(), PROJECTION_BYTES);
    assert_eq!(
        artifact.digest().to_review_string(),
        PROJECTION_DIGEST.trim()
    );
}

#[test]
fn echo_target_lowering_emits_the_verified_result_projection() {
    let (core, report) = hello_echo_lowering();
    let artifact = report.artifact.as_ref().expect("lowered Target IR");
    let projection = report
        .result_projections
        .get("createGreeting")
        .expect("target lowerer emits the application result projection");

    assert_eq!(projection.canonical_bytes(), PROJECTION_BYTES);
    assert_eq!(
        projection.digest().to_review_string(),
        PROJECTION_DIGEST.trim()
    );
    verify_result_projection(
        &core,
        artifact,
        "createGreeting",
        projection.canonical_bytes(),
        projection.digest(),
    )
    .expect("independent verifier reconstructs the lowerer output");
}

#[test]
fn pure_binding_result_projection_round_trips_through_independent_verification() {
    let (core, report) = pure_lowering();
    assert_eq!(report.status, TargetLoweringStatus::Lowered);
    let target = report.artifact.as_ref().expect("pure Target IR");
    let projection = report
        .result_projections
        .get("greet")
        .expect("pure result projection");
    let ResultProjectionExpr::Record { fields } = &projection.projection().expression else {
        panic!("pure result remains a record");
    };
    assert_eq!(
        fields["message"],
        ResultProjectionExpr::Source {
            source: ResultProjectionSource::PureBinding {
                binding_id: "greet.binding.1".to_owned(),
            },
            path: Vec::new(),
        }
    );
    assert_eq!(
        decode_result_projection(projection.canonical_bytes()).expect("decode pure projection"),
        *projection.projection()
    );
    verify_result_projection(
        &core,
        target,
        "greet",
        projection.canonical_bytes(),
        projection.digest(),
    )
    .expect("independent verifier reconstructs the pure binding result");
}

#[test]
fn pure_binding_projection_rejects_missing_substituted_reordered_and_duplicate_target_authority() {
    let (core, report) = pure_lowering();
    let target = report.artifact.expect("pure Target IR");
    let projection = report
        .result_projections
        .get("greet")
        .expect("pure result projection");

    let mut missing = target.clone();
    missing
        .intents
        .get_mut("greet")
        .expect("pure intent")
        .pure_bindings
        .pop();
    assert_pure_target_authority_rejected(&core, &missing, projection);

    let mut substituted = target.clone();
    substituted
        .intents
        .get_mut("greet")
        .expect("pure intent")
        .pure_bindings[1]
        .value = CoreExpr::Const(edict_syntax::CoreValue::String("wrong".to_owned()));
    assert_pure_target_authority_rejected(&core, &substituted, projection);

    let mut reordered = target.clone();
    reordered
        .intents
        .get_mut("greet")
        .expect("pure intent")
        .pure_bindings
        .swap(0, 1);
    assert_pure_target_authority_rejected(&core, &reordered, projection);

    let mut duplicated = target;
    let bindings = &mut duplicated
        .intents
        .get_mut("greet")
        .expect("pure intent")
        .pure_bindings;
    bindings[1].id = bindings[0].id.clone();
    assert_pure_target_authority_rejected(&core, &duplicated, projection);
}

#[test]
fn projection_rejects_local_identity_shared_with_application_input() {
    let (mut core, report) = pure_lowering();
    let mut target = report.artifact.expect("pure Target IR");
    let intent = core.intents.get_mut("greet").expect("pure Core intent");
    let CoreNode::Let { binding, .. } = &mut intent.body.nodes[0] else {
        panic!("pure fixture starts with a let");
    };
    binding.id = "arg.0".to_owned();
    intent.body.locals[1].id = "arg.0".to_owned();
    target
        .intents
        .get_mut("greet")
        .expect("pure Target IR intent")
        .pure_bindings[0]
        .binding
        .id = "arg.0".to_owned();
    repin_target_core(&core, &mut target);

    let failure = emit_result_projection(&core, &target, "greet")
        .expect_err("one local identity cannot name input and pure-binding producers");

    assert_eq!(
        failure.kind(),
        ResultProjectionFailureKind::CoreTargetMismatch
    );
}

fn assert_pure_target_authority_rejected(
    core: &CoreModule,
    target: &TargetIrArtifact,
    projection: &ResultProjectionArtifact,
) {
    assert_eq!(
        emit_result_projection(core, target, "greet")
            .expect_err("mutated pure binding authority must reject during emission")
            .kind(),
        ResultProjectionFailureKind::CoreTargetMismatch
    );
    assert_eq!(
        verify_result_projection(
            core,
            target,
            "greet",
            projection.canonical_bytes(),
            projection.digest(),
        )
        .expect_err("mutated pure binding authority must reject during verification")
        .kind(),
        ResultProjectionFailureKind::CoreTargetMismatch
    );
}

#[test]
fn target_lowering_exposes_an_unsupported_result_projection_without_claiming_one() {
    let (mut core, facts) = hello_echo_core_and_facts();
    let intent = core.intents.get_mut("createGreeting").expect("Core intent");
    intent.output = "String<max=0,canonical=raw-utf8>".to_owned();
    intent.body.result = CoreExpr::Call {
        callee: "core.string.concat".to_owned(),
        type_args: Vec::new(),
        args: Vec::new(),
    };

    let report = lower_to_target_ir(&core, &facts);

    assert_eq!(report.status, TargetLoweringStatus::Lowered);
    assert!(report.artifact.is_some());
    assert!(report.result_projections.is_empty());
    assert_eq!(
        report.failures,
        [] as [edict_syntax::TargetLoweringFailure; 0]
    );
    assert_eq!(
        report.result_projection_failures["createGreeting"].kind(),
        ResultProjectionFailureKind::UnsupportedExpression
    );
}

#[test]
fn structured_core_effects_cannot_disappear_from_projection_validation() {
    let (mut core, mut target) = hello_echo();
    let core_intent = core.intents.get_mut("createGreeting").expect("Core intent");
    let input = core_intent.body.locals[0].clone();
    let effect = core_intent.body.nodes.remove(0);
    core_intent.output.clone_from(&core_intent.input);
    core_intent.body.result = CoreExpr::Local {
        reference: input.clone(),
    };
    core_intent.body.nodes = vec![CoreNode::For {
        binder: LocalRef {
            id: "nested.effect".to_owned(),
            alpha_name: "$nestedEffect".to_owned(),
            ty: "U64".to_owned(),
        },
        iter: CoreExpr::Const(edict_syntax::CoreValue::Null),
        bound: CoreBound::Literal(1),
        body: CoreBlock {
            locals: Vec::new(),
            nodes: vec![CoreNode::Branch {
                binding: None,
                predicate: CorePredicate::True,
                then_block: CoreBlock {
                    locals: Vec::new(),
                    nodes: vec![effect],
                    result: CoreExpr::Const(edict_syntax::CoreValue::Null),
                },
                else_block: CoreBlock {
                    locals: Vec::new(),
                    nodes: Vec::new(),
                    result: CoreExpr::Const(edict_syntax::CoreValue::Null),
                },
            }],
            result: CoreExpr::Const(edict_syntax::CoreValue::Null),
        },
    }];
    let target_intent = target
        .intents
        .get_mut("createGreeting")
        .expect("Target IR intent");
    target_intent.steps.clear();
    target_intent.result = CoreExpr::Local { reference: input };
    repin_target_core(&core, &mut target);

    let failure = emit_result_projection(&core, &target, "createGreeting")
        .expect_err("structured Core effect must fail closed");

    assert_eq!(
        failure.kind(),
        ResultProjectionFailureKind::CoreTargetMismatch
    );
}

#[test]
fn independent_verifier_reconstructs_the_authored_core_result() {
    let (core, target) = hello_echo();
    let artifact =
        emit_result_projection(&core, &target, "createGreeting").expect("emit projection");
    let verified = verify_result_projection(
        &core,
        &target,
        "createGreeting",
        artifact.canonical_bytes(),
        artifact.digest(),
    )
    .expect("independent verifier accepts projection");

    assert_eq!(verified.projection(), artifact.projection());
    assert_eq!(verified.digest(), artifact.digest());
}

#[test]
fn mutated_target_result_fails_closed() {
    let (core, mut target) = hello_echo();
    target
        .intents
        .get_mut("createGreeting")
        .expect("Target IR intent")
        .result = CoreExpr::Local {
        reference: LocalRef {
            id: "arg.0".to_owned(),
            alpha_name: "$arg0".to_owned(),
            ty: "examples.hello_echo@1.CreateGreetingInput".to_owned(),
        },
    };

    let failure = emit_result_projection(&core, &target, "createGreeting")
        .expect_err("mutated Target IR result must reject");
    assert_eq!(
        failure.kind(),
        ResultProjectionFailureKind::TargetResultMismatch
    );
}

#[test]
fn mutated_target_core_closure_fails_closed() {
    let (core, mut target) = hello_echo();
    target
        .semantic_closure
        .as_mut()
        .expect("semantic closure")
        .source_core
        .digest = Some(format!("sha256:{}", "0".repeat(64)));

    let failure = emit_result_projection(&core, &target, "createGreeting")
        .expect_err("mutated source Core identity must reject");
    assert_eq!(
        failure.kind(),
        ResultProjectionFailureKind::CoreTargetMismatch
    );
}

#[test]
fn malformed_raw_core_cannot_emit_or_verify_a_result_projection() {
    let (core, facts) = hello_echo_core_and_facts();
    let baseline = lower_to_target_ir(&core, &facts);
    let baseline_target = baseline.artifact.expect("baseline Target IR");
    let projection = emit_result_projection(&core, &baseline_target, "createGreeting")
        .expect("baseline projection emits");

    let mut malformed = core;
    malformed.types.insert(
        "UnusedInvalid".to_owned(),
        CoreType::List {
            item: "List<U64,max=01>".to_owned(),
            max: 1,
        },
    );
    let target = lower_to_target_ir(&malformed, &facts)
        .artifact
        .unwrap_or_else(|| baseline_target.clone());

    let emitted = emit_result_projection(&malformed, &target, "createGreeting");
    let verified = verify_result_projection(
        &malformed,
        &target,
        "createGreeting",
        projection.canonical_bytes(),
        projection.digest(),
    );
    let (Err(emission_failure), Err(verification_failure)) = (emitted, verified) else {
        panic!("malformed raw Core crossed a public projection boundary");
    };
    assert_eq!(
        emission_failure.kind(),
        ResultProjectionFailureKind::CoreTargetMismatch
    );
    assert_eq!(emission_failure.subject(), "types.UnusedInvalid.item");
    assert_eq!(
        verification_failure.kind(),
        ResultProjectionFailureKind::CoreTargetMismatch
    );
    assert_eq!(verification_failure.subject(), "types.UnusedInvalid.item");
}

#[test]
fn over_depth_named_expansion_cannot_emit_or_verify_a_result_projection() {
    let (core, facts) = hello_echo_core_and_facts();
    let baseline = lower_to_target_ir(&core, &facts);
    let baseline_target = baseline.artifact.expect("baseline Target IR");
    let projection = emit_result_projection(&core, &baseline_target, "createGreeting")
        .expect("baseline projection emits");

    let mut over_depth = core;
    over_depth.types.insert(
        "A.DepthBase".to_owned(),
        CoreType::Option {
            item: "U64".to_owned(),
        },
    );
    over_depth.types.insert(
        "Z.DepthOverflow".to_owned(),
        CoreType::Option {
            item: (0..127).fold("A.DepthBase".to_owned(), |inner, _| {
                format!("Option<{inner}>")
            }),
        },
    );
    let target = lower_to_target_ir(&over_depth, &facts)
        .artifact
        .unwrap_or_else(|| baseline_target.clone());

    let emitted = emit_result_projection(&over_depth, &target, "createGreeting");
    let verified = verify_result_projection(
        &over_depth,
        &target,
        "createGreeting",
        projection.canonical_bytes(),
        projection.digest(),
    );
    let (Err(emission_failure), Err(verification_failure)) = (emitted, verified) else {
        panic!("over-depth named expansion crossed a public projection boundary");
    };
    assert_eq!(
        emission_failure.kind(),
        ResultProjectionFailureKind::CoreTargetMismatch
    );
    assert_eq!(emission_failure.subject(), "types.Z.DepthOverflow");
    assert_eq!(
        verification_failure.kind(),
        ResultProjectionFailureKind::CoreTargetMismatch
    );
    assert_eq!(verification_failure.subject(), "types.Z.DepthOverflow");
}

#[test]
fn mutated_target_lawpack_closure_fails_closed() {
    let (core, mut target) = hello_echo();
    target
        .semantic_closure
        .as_mut()
        .expect("semantic closure")
        .lawpacks[0]
        .digest = Some(format!("sha256:{}", "0".repeat(64)));

    let failure = emit_result_projection(&core, &target, "createGreeting")
        .expect_err("mutated lawpack closure must reject");
    assert_eq!(
        failure.kind(),
        ResultProjectionFailureKind::CoreTargetMismatch
    );
}

#[test]
fn undeclared_locals_and_unsupported_calls_fail_closed() {
    let (mut core, mut target) = hello_echo();
    mutate_results(
        &mut core,
        &mut target,
        CoreExpr::Local {
            reference: LocalRef {
                id: "local.999".to_owned(),
                alpha_name: "$999".to_owned(),
                ty: "examples.hello_echo@1.GreetingCreated".to_owned(),
            },
        },
    );
    let undeclared = emit_result_projection(&core, &target, "createGreeting")
        .expect_err("undeclared local must reject");
    assert_eq!(
        undeclared.kind(),
        ResultProjectionFailureKind::UndeclaredProjectionSource
    );

    let (mut core, mut target) = hello_echo();
    mutate_results(
        &mut core,
        &mut target,
        CoreExpr::Call {
            callee: "examples.hidden@1.callback".to_owned(),
            type_args: Vec::new(),
            args: Vec::new(),
        },
    );
    let call = emit_result_projection(&core, &target, "createGreeting")
        .expect_err("call expression must reject");
    assert_eq!(
        call.kind(),
        ResultProjectionFailureKind::UnsupportedExpression
    );
}

#[test]
fn incomplete_output_and_zero_output_bound_fail_closed() {
    let (mut core, mut target) = hello_echo();
    let key = core.intents["createGreeting"].body.result.clone();
    let CoreExpr::Record { mut fields } = key else {
        panic!("Hello Echo result is a record");
    };
    fields.remove("message");
    mutate_results(&mut core, &mut target, CoreExpr::Record { fields });
    let incomplete = emit_result_projection(&core, &target, "createGreeting")
        .expect_err("incomplete output must reject");
    assert_eq!(
        incomplete.kind(),
        ResultProjectionFailureKind::OutputShapeMismatch
    );

    let (mut core, mut target) = hello_echo();
    core.intents
        .get_mut("createGreeting")
        .expect("Core intent")
        .core_evaluation_budget
        .max_output_bytes = 0;
    target
        .intents
        .get_mut("createGreeting")
        .expect("Target intent")
        .core_evaluation_budget
        .max_output_bytes = 0;
    repin_target_core(&core, &mut target);
    let unbounded = emit_result_projection(&core, &target, "createGreeting")
        .expect_err("zero output bound must reject");
    assert_eq!(
        unbounded.kind(),
        ResultProjectionFailureKind::InvalidOutputBound
    );
}

#[test]
fn exact_byte_projection_refuses_a_max_only_source() {
    let source = PURE_SOURCE
        .replace("name: String<max=16>", "name: Bytes<exact=16>")
        .replace("message: String<max=18>", "message: Bytes<exact=16>")
        .replace("  let prefix = \"hi\";\n", "")
        .replace("prefix + input.name", "input.name");
    let module = parse_module(&source).expect("exact-byte projection source parses");
    let mut core = edict_syntax::compile_to_core(
        &module,
        &CompilerContext::new()
            .with_operation_profile("law.read", "continuum.profile.read-only/v1")
            .with_budget(
                "law.tiny",
                CoreBudget {
                    max_steps: 8,
                    max_allocated_bytes: 256,
                    max_output_bytes: 128,
                },
            ),
    )
    .expect("exact-byte projection Core compiles");
    let facts = TargetIrLoweringFacts {
        target_profile: ResourceRef {
            coordinate: "echo.dpo@1".to_owned(),
            digest: Some(format!("sha256:{}", "1".repeat(64))),
        },
        target_ir_domain: "echo.span-ir/v1".to_owned(),
        operation_profiles: vec!["continuum.profile.read-only/v1".to_owned()],
        obstruction_coordinates: Vec::new(),
        effect_lowerings: Vec::new(),
        effect_signatures: Vec::new(),
        pure_functions: Vec::new(),
    };
    let mut target = lower_to_target_ir(&core, &facts)
        .artifact
        .expect("exact-byte projection Target IR lowers");
    emit_result_projection(&core, &target, "greet")
        .expect("exact byte source fits exact byte output");

    let Some(CoreType::Record { fields }) = core.types.get_mut("Input") else {
        panic!("Input remains a named record");
    };
    assert_eq!(
        fields.insert("name".to_owned(), "Bytes<max=16>".to_owned()),
        Some("Bytes<exact=16>".to_owned())
    );
    assert!(!core.types.contains_key("Input.name"));
    let core_intent = core.intents.get_mut("greet").expect("greet Core intent");
    let CoreNode::Let { binding, .. } = core_intent
        .body
        .nodes
        .last_mut()
        .expect("greet has a message binding")
    else {
        panic!("greet ends with its message binding");
    };
    binding.ty = "Bytes<max=16>".to_owned();
    let binding_id = binding.id.clone();
    core_intent
        .body
        .locals
        .iter_mut()
        .find(|local| local.id == binding_id)
        .expect("message binding appears in the local table")
        .ty = "Bytes<max=16>".to_owned();
    let CoreExpr::Record { fields } = &mut core_intent.body.result else {
        panic!("greet returns a record");
    };
    let CoreExpr::Local { reference } = fields.get_mut("message").expect("message result field")
    else {
        panic!("message result is the pure binding");
    };
    reference.ty = "Bytes<max=16>".to_owned();

    let target_intent = target
        .intents
        .get_mut("greet")
        .expect("greet Target intent");
    target_intent
        .pure_bindings
        .last_mut()
        .expect("Target greet has a message binding")
        .binding
        .ty = "Bytes<max=16>".to_owned();
    let CoreExpr::Record { fields } = &mut target_intent.result else {
        panic!("Target greet returns a record");
    };
    let CoreExpr::Local { reference } = fields.get_mut("message").expect("Target message field")
    else {
        panic!("Target message result is the pure binding");
    };
    reference.ty = "Bytes<max=16>".to_owned();
    repin_target_core(&core, &mut target);
    let failure = emit_result_projection(&core, &target, "greet")
        .expect_err("max-only source cannot prove exact-byte output");
    assert_eq!(
        failure.kind(),
        ResultProjectionFailureKind::OutputShapeMismatch
    );
}

#[test]
fn canonical_bytes_and_digest_are_independently_enforced() {
    let (core, target) = hello_echo();
    let artifact =
        emit_result_projection(&core, &target, "createGreeting").expect("emit projection");

    let mut malformed = artifact.canonical_bytes().to_vec();
    malformed.push(0);
    let malformed_failure = verify_result_projection(
        &core,
        &target,
        "createGreeting",
        &malformed,
        artifact.digest(),
    )
    .expect_err("trailing data must reject");
    assert_eq!(
        malformed_failure.kind(),
        ResultProjectionFailureKind::InvalidCanonicalArtifact
    );

    let different = digest_result_projection(&projection_with_fields(1))
        .expect("digest different canonical projection");
    let digest_failure = verify_result_projection(
        &core,
        &target,
        "createGreeting",
        artifact.canonical_bytes(),
        different,
    )
    .expect_err("substituted digest must reject");
    assert_eq!(
        digest_failure.kind(),
        ResultProjectionFailureKind::DigestMismatch
    );
}

#[test]
fn projection_node_limit_accepts_the_boundary_and_rejects_the_next_node() {
    let boundary = projection_with_fields(MAX_RESULT_PROJECTION_NODES - 1);
    encode_result_projection(&boundary).expect("exact node boundary encodes");

    let oversized = projection_with_fields(MAX_RESULT_PROJECTION_NODES);
    let failure =
        encode_result_projection(&oversized).expect_err("one node beyond boundary must reject");
    assert_eq!(
        failure.kind(),
        ResultProjectionFailureKind::ProjectionLimitExceeded
    );
}

#[test]
fn path_text_artifact_and_structure_bounds_fail_closed() {
    let mut path_projection = projection_with_fields(1);
    path_projection.expression = ResultProjectionExpr::Source {
        source: ResultProjectionSource::ApplicationInput,
        path: vec!["field".to_owned(); MAX_RESULT_PROJECTION_PATH_SEGMENTS + 1],
    };
    assert_eq!(
        encode_result_projection(&path_projection)
            .expect_err("overlong source path must reject")
            .kind(),
        ResultProjectionFailureKind::ProjectionLimitExceeded
    );

    let mut text_projection = projection_with_fields(1);
    text_projection.operation_coordinate = "x".repeat(MAX_RESULT_PROJECTION_TEXT_BYTES + 1);
    assert_eq!(
        encode_result_projection(&text_projection)
            .expect_err("overlong coordinate must reject")
            .kind(),
        ResultProjectionFailureKind::ProjectionLimitExceeded
    );

    assert_eq!(
        decode_result_projection(&vec![0; MAX_RESULT_PROJECTION_ARTIFACT_BYTES + 1])
            .expect_err("oversized artifact must reject before decoding")
            .kind(),
        ResultProjectionFailureKind::ProjectionLimitExceeded
    );

    let bytes =
        encode_result_projection(&projection_with_fields(1)).expect("encode complete projection");
    let mut incomplete = decode_canonical_cbor(&bytes).expect("decode projection value");
    remove_canonical_field(&mut incomplete, "outputType");
    let incomplete_bytes =
        encode_canonical_cbor(&incomplete).expect("encode incomplete canonical value");
    assert_eq!(
        decode_result_projection(&incomplete_bytes)
            .expect_err("incomplete projection must reject")
            .kind(),
        ResultProjectionFailureKind::InvalidCanonicalArtifact
    );
}

#[test]
fn hostile_decoded_values_fail_closed_before_semantic_admission() {
    let mut empty_text = projection_with_fields(1);
    empty_text.operation_coordinate.clear();
    assert_eq!(
        encode_result_projection(&empty_text)
            .expect_err("empty required text must reject")
            .kind(),
        ResultProjectionFailureKind::InvalidCanonicalArtifact
    );

    let bytes =
        encode_result_projection(&projection_with_fields(1)).expect("encode bounded projection");
    let mut zero_output_bound = decode_canonical_cbor(&bytes).expect("decode projection value");
    *canonical_field_mut(&mut zero_output_bound, "maxOutputBytes") = CanonicalValue::Integer(0);
    let zero_output_bytes =
        encode_canonical_cbor(&zero_output_bound).expect("encode hostile canonical value");
    assert_eq!(
        decode_result_projection(&zero_output_bytes)
            .expect_err("decoded zero output bound must reject")
            .kind(),
        ResultProjectionFailureKind::InvalidOutputBound
    );
}

#[test]
fn unknown_steps_and_invalid_application_input_bindings_fail_closed() {
    let (core, target) = hello_echo();
    let artifact =
        emit_result_projection(&core, &target, "createGreeting").expect("emit projection");
    let mut unknown_step = artifact.projection().clone();
    let ResultProjectionExpr::Record { fields } = &mut unknown_step.expression else {
        panic!("Hello Echo projection is a record");
    };
    let ResultProjectionExpr::Source { source, .. } =
        fields.get_mut("key").expect("Hello Echo key projection")
    else {
        panic!("Hello Echo key projection is a source");
    };
    *source = ResultProjectionSource::CapabilityResult {
        step_id: "createGreeting.step.missing".to_owned(),
    };
    let unknown_step_bytes =
        encode_result_projection(&unknown_step).expect("encode hostile step projection");
    let unknown_step_digest =
        digest_result_projection(&unknown_step).expect("digest hostile step projection");
    assert_eq!(
        verify_result_projection(
            &core,
            &target,
            "createGreeting",
            &unknown_step_bytes,
            unknown_step_digest,
        )
        .expect_err("unknown capability step must reject")
        .kind(),
        ResultProjectionFailureKind::UnknownCapabilityStep
    );

    let (mut missing_core, mut missing_target) = hello_echo();
    missing_core
        .intents
        .get_mut("createGreeting")
        .expect("Core intent")
        .body
        .locals
        .retain(|local| local.id != "arg.0");
    repin_target_core(&missing_core, &mut missing_target);
    assert_eq!(
        emit_result_projection(&missing_core, &missing_target, "createGreeting")
            .expect_err("missing application input binding must reject")
            .kind(),
        ResultProjectionFailureKind::InvalidApplicationInput
    );

    let (mut duplicate_core, mut duplicate_target) = hello_echo();
    let duplicate_input = duplicate_core.intents["createGreeting"].body.locals[0].clone();
    duplicate_core
        .intents
        .get_mut("createGreeting")
        .expect("Core intent")
        .body
        .locals
        .push(duplicate_input);
    repin_target_core(&duplicate_core, &mut duplicate_target);
    assert_eq!(
        emit_result_projection(&duplicate_core, &duplicate_target, "createGreeting")
            .expect_err("duplicate application input binding must reject")
            .kind(),
        ResultProjectionFailureKind::InvalidApplicationInput
    );
}

#[test]
fn canonical_maps_are_insertion_order_independent_for_fixed_seed_cases() {
    let entries = (0..32)
        .map(|index| {
            (
                CanonicalValue::Text(format!("field{index:04}")),
                CanonicalValue::Integer(i128::from(index)),
            )
        })
        .collect::<Vec<_>>();
    let expected = encode_canonical_cbor(&CanonicalValue::Map(entries.clone()))
        .expect("encode ordered canonical map");
    let mut state = PROPERTY_SEED;

    for _case in 0..64 {
        let mut indexes = (0..32).collect::<Vec<_>>();
        for index in (1..indexes.len()).rev() {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            let selected = usize::try_from(state % u64::try_from(index + 1).expect("small index"))
                .expect("selected index");
            indexes.swap(index, selected);
        }
        let candidate = CanonicalValue::Map(
            indexes
                .into_iter()
                .map(|index| entries[index].clone())
                .collect(),
        );
        assert_eq!(
            encode_canonical_cbor(&candidate).expect("encode property candidate"),
            expected
        );
    }
}

#[test]
fn repeated_emit_and_verify_is_stable_under_bounded_stress() {
    let (core, target) = hello_echo();
    let first = emit_result_projection(&core, &target, "createGreeting").expect("emit projection");
    for _attempt in 0..128 {
        let next =
            emit_result_projection(&core, &target, "createGreeting").expect("repeat emission");
        assert_eq!(next, first);
        verify_result_projection(
            &core,
            &target,
            "createGreeting",
            next.canonical_bytes(),
            next.digest(),
        )
        .expect("repeat verification");
    }
}

//! Canonical lawpack loading and dependency validation.
//!
//! These tests enter through exact canonical bytes and assert typed exports or
//! stable failure kinds. They do not construct an already-trusted manifest.

use amblicit_syntax::{
    compile_to_core, decode_canonical_cbor, decode_lawpack_adapter, decode_lawpack_bundle,
    digest_core_module, digest_target_ir_artifact, encode_canonical_cbor, encode_core_module,
    encode_target_ir_artifact, lower_to_target_ir, parse_module, prepare_lawpack_compilation,
    validate_lawpack_dependency_graph, CanonicalValue, CompilerErrorKind, CompilerStage, CoreBound,
    CoreExpr, CoreNode, LawpackAdapterFailureKind, LawpackExecutionClass,
    LawpackPureFunctionImplementation, LawpackValidationFailureKind, LawpackVerifierClass,
    TargetLoweringStatus, ValidatedLawpackBundle,
};
use sha2::{Digest, Sha256};

const DIGEST_FRAME: &str = "edict.digest/v1";
const EXPORTS_COORDINATE: &str = "hello.echo.exports/v1";
const ADAPTER_COORDINATE: &str = "hello.echo.echo-dpo-adapter/v1";
const MANIFEST_BYTES: &[u8] = include_bytes!("../../../fixtures/lawpack/hello-echo/manifest.cbor");
const EXPORTS_BYTES: &[u8] = include_bytes!("../../../fixtures/lawpack/hello-echo/exports.cbor");
const ADAPTER_BYTES: &[u8] = include_bytes!("../../../fixtures/lawpack/hello-echo/adapter.cbor");
const ADAPTER_DIGEST: &str = include_str!("../../../fixtures/lawpack/hello-echo/adapter.sha256");
const TARGET_CONFIGURATION_COORDINATE: &str = "hello.echo.echo-operation-configuration/v1";
const TARGET_CONFIGURATION_BYTES: &[u8] =
    include_bytes!("../../../fixtures/lawpack/hello-echo/echo-operation-configuration.cbor");
const TARGET_CONFIGURATION_DIGEST: &str =
    include_str!("../../../fixtures/lawpack/hello-echo/echo-operation-configuration.sha256");
const MANIFEST_DIGEST: &str = include_str!("../../../fixtures/lawpack/hello-echo/manifest.sha256");
const CREATE_GREETING_SOURCE: &str =
    include_str!("../../../fixtures/lawpack/hello-echo/create-greeting.amblicit");
const CREATE_GREETING_CORE_BYTES: &[u8] =
    include_bytes!("../../../fixtures/lawpack/hello-echo/create-greeting.core.cbor");
const CREATE_GREETING_CORE_DIGEST: &str =
    include_str!("../../../fixtures/lawpack/hello-echo/create-greeting.core.sha256");
const CREATE_GREETING_TARGET_IR_BYTES: &[u8] =
    include_bytes!("../../../fixtures/lawpack/hello-echo/create-greeting.target-ir.cbor");
const CREATE_GREETING_TARGET_IR_DIGEST: &str =
    include_str!("../../../fixtures/lawpack/hello-echo/create-greeting.target-ir.sha256");

#[test]
fn hello_echo_lawpack_bundle_loads_from_exact_canonical_resources() {
    let bundle =
        decode_lawpack_bundle(MANIFEST_BYTES, EXPORTS_BYTES).expect("load Hello Echo lawpack");
    let source = parse_module(CREATE_GREETING_SOURCE).expect("parse createGreeting source");

    assert_eq!(bundle.manifest().id, "hello.echo");
    assert_eq!(bundle.manifest().version, "1");
    assert_eq!(
        bundle
            .exports()
            .types
            .iter()
            .map(|exported| exported.coordinate.as_str())
            .collect::<Vec<_>>(),
        vec![
            "hello.echo@1.CreateGreetingInput",
            "hello.echo@1.GreetingReceipt",
            "hello.echo@1.ExistingGreeting",
        ]
    );
    assert_eq!(bundle.exports().effects.len(), 1);
    assert_eq!(
        bundle.exports().effects[0].coordinate,
        "hello.echo@1.createGreeting"
    );
    assert_eq!(
        bundle.exports().effects[0].execution_class,
        LawpackExecutionClass::Runtime
    );
    assert_eq!(
        bundle.exports().effects[0]
            .effect_failures
            .keys()
            .map(String::as_str)
            .collect::<Vec<_>>(),
        vec!["alreadyExists"]
    );
    assert_eq!(
        source.imports[0].digest.as_deref(),
        Some(MANIFEST_DIGEST.trim())
    );
    assert_eq!(
        bundle.manifest_digest_review_string(),
        MANIFEST_DIGEST.trim()
    );
}

#[test]
fn lawpack_effect_signatures_reject_source_type_substitution() {
    let bundle =
        decode_lawpack_bundle(MANIFEST_BYTES, EXPORTS_BYTES).expect("load Hello Echo lawpack");
    let adapter =
        decode_lawpack_adapter(&bundle, "echo.dpo@1", ADAPTER_BYTES).expect("load exact adapter");

    let wrong_input_source =
        CREATE_GREETING_SOURCE.replace("hello.createGreeting(input)", "hello.createGreeting(true)");
    assert_ne!(wrong_input_source, CREATE_GREETING_SOURCE);
    let wrong_output_source = CREATE_GREETING_SOURCE.replace(
        "GreetingReceipt = hello.createGreeting",
        "CreateGreetingInput = hello.createGreeting",
    );
    assert_ne!(wrong_output_source, CREATE_GREETING_SOURCE);

    for (case, source) in [
        ("input", wrong_input_source),
        ("output", wrong_output_source),
    ] {
        let module = parse_module(&source).expect("substituted effect source parses");
        let preparation = prepare_lawpack_compilation(&module, &bundle, &adapter)
            .expect("derive exact lawpack facts");
        let errors = compile_to_core(&module, preparation.compiler_context())
            .expect_err("effect signature substitution must reject before Core");
        assert!(
            errors
                .iter()
                .any(|error| error.kind == CompilerErrorKind::TypeMismatch),
            "{case}: {errors:?}"
        );
    }
}

#[test]
fn direct_bounded_effect_signature_types_enter_source_compilation() {
    let mut exports = hello_echo_exports();
    let effect = first_array_item_mut(field_mut(&mut exports, "effects"));
    let bounded_string = "String<max=64,canonical=raw-utf8>";
    replace_field(effect, "inputType", text(bounded_string));
    replace_field(effect, "outputType", text(bounded_string));
    let exports_bytes = encode_canonical_cbor(&exports).expect("encode scalar effect exports");
    let manifest = hello_echo_manifest(digest_value(EXPORTS_COORDINATE, &exports));
    let manifest_bytes = encode_canonical_cbor(&manifest).expect("encode scalar effect manifest");
    let bundle = decode_lawpack_bundle(&manifest_bytes, &exports_bytes)
        .expect("load scalar-signature lawpack");
    let source = CREATE_GREETING_SOURCE
        .replace(
            MANIFEST_DIGEST.trim(),
            &bundle.manifest_digest_review_string(),
        )
        .replace(
            "hello.GreetingReceipt = hello.createGreeting(input)",
            "String<max=64> = hello.createGreeting(input.key)",
        )
        .replace("key: receipt.key", "key: receipt");
    assert_ne!(source, CREATE_GREETING_SOURCE);
    let module = parse_module(&source).expect("bounded scalar effect source parses");
    let adapter =
        decode_lawpack_adapter(&bundle, "echo.dpo@1", ADAPTER_BYTES).expect("load exact adapter");
    let preparation = prepare_lawpack_compilation(&module, &bundle, &adapter)
        .expect("derive bounded scalar effect facts");

    let core = compile_to_core(&module, preparation.compiler_context())
        .expect("direct bounded signature coordinates compile to Core");
    let report = lower_to_target_ir(&core, preparation.target_ir_facts());

    assert_eq!(report.status, TargetLoweringStatus::Lowered);
    assert_eq!(
        report.failures,
        Vec::<amblicit_syntax::TargetLoweringFailure>::new()
    );
    assert!(report.artifact.is_some());
}

#[test]
fn bare_effect_without_declared_failures_reports_binding_limitation() {
    let mut exports = hello_echo_exports();
    let effect = first_array_item_mut(field_mut(&mut exports, "effects"));
    map_mut(field_mut(effect, "effectFailures")).clear();
    let mut adapter = decode_canonical_cbor(ADAPTER_BYTES).expect("decode adapter");
    let implementation = first_map_value_mut(field_mut(&mut adapter, "effectImplementations"));
    map_mut(field_mut(implementation, "failureMappings")).clear();
    let (bundle, adapter) = bundle_and_adapter(&exports, &adapter);
    let mapping = "\n    else { alreadyExists(existing) => hello.AlreadyExists }";
    assert_eq!(CREATE_GREETING_SOURCE.matches(mapping).count(), 1);
    let source = CREATE_GREETING_SOURCE
        .replace(
            MANIFEST_DIGEST.trim(),
            &bundle.manifest_digest_review_string(),
        )
        .replace(mapping, "");
    let module = parse_module(&source).expect("bare zero-failure effect parses");
    let preparation = prepare_lawpack_compilation(&module, &bundle, &adapter)
        .expect("authenticate a zero-failure signature");
    let errors = compile_to_core(&module, preparation.compiler_context())
        .expect_err("bare semantic effects remain unsupported");
    let cause = &errors[0];
    assert_eq!(cause.kind, CompilerErrorKind::EffectWithoutFailureMapping);
    assert_eq!(cause.stage, CompilerStage::TypeCheck);
    assert!(source[cause.span.start..cause.span.end].contains("hello.createGreeting"));
    assert!(cause.message.contains("no declared failures"));
    assert!(cause
        .message
        .contains("effect bindings without a failure map are not supported"));
    assert!(!cause.message.contains("with `else`"));
}

#[test]
fn generic_effect_reports_unsupported_shape_before_mapping_guidance() {
    let mut exports = hello_echo_exports();
    let effect = first_array_item_mut(field_mut(&mut exports, "effects"));
    replace_field(
        effect,
        "typeParameters",
        CanonicalValue::Array(vec![text("T")]),
    );
    let exports_bytes = encode_canonical_cbor(&exports).expect("encode generic exports");
    let manifest = hello_echo_manifest(digest_value(EXPORTS_COORDINATE, &exports));
    let manifest_bytes = encode_canonical_cbor(&manifest).expect("encode generic manifest");
    let bundle = decode_lawpack_bundle(&manifest_bytes, &exports_bytes)
        .expect("generic effect metadata is valid");
    let adapter =
        decode_lawpack_adapter(&bundle, "echo.dpo@1", ADAPTER_BYTES).expect("load exact adapter");
    let mapped = CREATE_GREETING_SOURCE.replace(
        MANIFEST_DIGEST.trim(),
        &bundle.manifest_digest_review_string(),
    );
    let mapping = "\n    else { alreadyExists(existing) => hello.AlreadyExists }";
    assert_eq!(mapped.matches(mapping).count(), 1);
    for source in [mapped.clone(), mapped.replace(mapping, "")] {
        let module = parse_module(&source).expect("generic effect source parses");
        let preparation = prepare_lawpack_compilation(&module, &bundle, &adapter)
            .expect("generic signature prepares");
        let errors = compile_to_core(&module, preparation.compiler_context())
            .expect_err("generic semantic effects remain unsupported");
        let cause = &errors[0];
        assert_eq!(cause.kind, CompilerErrorKind::UnsupportedSourceShape);
        assert_eq!(cause.stage, CompilerStage::TypeCheck);
        assert!(source[cause.span.start..cause.span.end].contains("hello.createGreeting(input)"));
        assert!(cause.message.contains("generic semantic effects"));
        assert!(!cause.message.contains("with `else`"));
    }
}

#[test]
fn authored_effect_type_arguments_refuse_mapping_guidance() {
    let bundle = decode_lawpack_bundle(MANIFEST_BYTES, EXPORTS_BYTES).expect("load lawpack");
    let adapter =
        decode_lawpack_adapter(&bundle, "echo.dpo@1", ADAPTER_BYTES).expect("load exact adapter");
    let mapped = CREATE_GREETING_SOURCE.replace(
        "hello.createGreeting(input)",
        "hello.createGreeting<U64>(input)",
    );
    assert_eq!(
        mapped.matches("hello.createGreeting<U64>(input)").count(),
        1
    );
    let mapping = "\n    else { alreadyExists(existing) => hello.AlreadyExists }";
    assert_eq!(mapped.matches(mapping).count(), 1);
    for source in [mapped.clone(), mapped.replace(mapping, "")] {
        let module = parse_module(&source).expect("authored type arguments parse");
        let preparation = prepare_lawpack_compilation(&module, &bundle, &adapter)
            .expect("authenticate nongeneric effect");
        let errors = compile_to_core(&module, preparation.compiler_context())
            .expect_err("effect type arguments remain unsupported");
        let cause = &errors[0];
        assert_eq!(cause.kind, CompilerErrorKind::UnsupportedSourceShape);
        assert_eq!(cause.stage, CompilerStage::TypeCheck);
        assert!(
            source[cause.span.start..cause.span.end].contains("hello.createGreeting<U64>(input)")
        );
        assert!(cause.message.contains("type arguments"));
        assert!(!cause.message.contains("with `else`"));
    }
}

#[test]
fn valid_function_does_not_poison_intent_effect_guidance() {
    let mut adapter_value = decode_canonical_cbor(ADAPTER_BYTES).expect("decode control adapter");
    let budget = field_mut(
        field_mut(&mut adapter_value, "budgets"),
        "hello.echo@1.smallCreateBudget",
    );
    replace_field(budget, "maxSteps", CanonicalValue::Integer(1024));
    replace_field(budget, "maxAllocatedBytes", CanonicalValue::Integer(32768));
    replace_field(budget, "maxOutputBytes", CanonicalValue::Integer(4096));
    let bundle = bundle_with_adapter(&adapter_value);
    let adapter_bytes = encode_canonical_cbor(&adapter_value).expect("encode control adapter");
    let adapter = decode_lawpack_adapter(&bundle, "echo.dpo@1", &adapter_bytes)
        .expect("authenticate control adapter");
    let function = "fn identity(value: U64) -> U64 { return value; }\n\n";
    let source = CREATE_GREETING_SOURCE
        .replace(
            MANIFEST_DIGEST.trim(),
            &bundle.manifest_digest_review_string(),
        )
        .replace(
            "intent createGreeting",
            &format!("{function}intent createGreeting"),
        );
    let module = parse_module(&source).expect("control source parses");
    let preparation =
        prepare_lawpack_compilation(&module, &bundle, &adapter).expect("prepare control closure");
    compile_to_core(&module, preparation.compiler_context())
        .expect("valid function and mapped intent compile");
    let mapping = "\n    else { alreadyExists(existing) => hello.AlreadyExists }";
    assert_eq!(source.matches(mapping).count(), 1);
    let bare = source.replace(mapping, "");
    let module = parse_module(&bare).expect("bare control parses");
    let preparation =
        prepare_lawpack_compilation(&module, &bundle, &adapter).expect("prepare bare control");
    let errors = compile_to_core(&module, preparation.compiler_context())
        .expect_err("bare intent effect still needs its mapping");
    assert_eq!(
        errors[0].kind,
        CompilerErrorKind::EffectWithoutFailureMapping
    );
    assert!(errors[0].message.contains("with `else`"));
}

#[test]
fn lawpack_effect_signature_requires_exported_type_closure() {
    let mut exports = hello_echo_exports();
    array_mut(field_mut(&mut exports, "types")).clear();
    let exports_bytes = encode_canonical_cbor(&exports).expect("encode typeless exports");
    let manifest = hello_echo_manifest(digest_value(EXPORTS_COORDINATE, &exports));
    let manifest_bytes = encode_canonical_cbor(&manifest).expect("encode typeless manifest");
    let bundle = decode_lawpack_bundle(&manifest_bytes, &exports_bytes)
        .expect("shape-valid bundle retains its unresolved signature metadata");
    let source = CREATE_GREETING_SOURCE.replace(
        MANIFEST_DIGEST.trim(),
        &bundle.manifest_digest_review_string(),
    );
    assert_ne!(source, CREATE_GREETING_SOURCE);
    let module = parse_module(&source).expect("typeless signature source parses");
    let adapter =
        decode_lawpack_adapter(&bundle, "echo.dpo@1", ADAPTER_BYTES).expect("load exact adapter");

    let failures = prepare_lawpack_compilation(&module, &bundle, &adapter)
        .expect_err("an unresolved effect signature must not become compiler or Target facts");

    assert_eq!(
        adapter_failure_kinds(&failures),
        vec![LawpackAdapterFailureKind::InvalidShape]
    );
}

#[test]
fn effect_failure_payload_types_enter_the_authenticated_signature_closure() {
    const PAYLOAD: &str = "hello.echo@1.ExistingGreeting";
    let complete_exports = hello_echo_exports();
    let complete_bundle = decode_lawpack_bundle(MANIFEST_BYTES, EXPORTS_BYTES)
        .expect("load payload-complete lawpack");
    let complete_module =
        parse_module(CREATE_GREETING_SOURCE).expect("payload-complete effect source parses");
    let complete_adapter = decode_lawpack_adapter(&complete_bundle, "echo.dpo@1", ADAPTER_BYTES)
        .expect("load exact adapter");
    let preparation =
        prepare_lawpack_compilation(&complete_module, &complete_bundle, &complete_adapter)
            .expect("authenticate the complete effect signature");

    let core = compile_to_core(&complete_module, preparation.compiler_context())
        .expect("compile an obstruction binder from its payload signature");
    assert!(core.types.contains_key(PAYLOAD));
    let effect = core
        .intents
        .get("createGreeting")
        .expect("createGreeting intent")
        .body
        .nodes
        .first()
        .expect("effect node");
    let CoreNode::Effect {
        obstruction_map, ..
    } = effect
    else {
        panic!("first createGreeting node is the effect");
    };
    assert_eq!(
        obstruction_map
            .get("alreadyExists")
            .expect("alreadyExists arm")
            .binder
            .ty,
        PAYLOAD
    );
    assert!(!core
        .types
        .contains_key("hello.createGreeting.alreadyExists"));

    let mut missing_exports = complete_exports;
    assert_eq!(
        array_mut(field_mut(&mut missing_exports, "types")).pop(),
        Some(map([
            ("coordinate", text(PAYLOAD)),
            (
                "definition",
                text(
                    "Record<key:String<max=64,canonical=raw-utf8>,message:String<max=256,canonical=raw-utf8>>",
                ),
            ),
        ]))
    );
    let missing_exports_bytes =
        encode_canonical_cbor(&missing_exports).expect("encode payload-incomplete exports");
    let missing_manifest = hello_echo_manifest(digest_value(EXPORTS_COORDINATE, &missing_exports));
    let missing_manifest_bytes =
        encode_canonical_cbor(&missing_manifest).expect("encode payload-incomplete manifest");
    let missing_bundle = decode_lawpack_bundle(&missing_manifest_bytes, &missing_exports_bytes)
        .expect("load shape-valid lawpack with unresolved failure payload");
    let missing_source = CREATE_GREETING_SOURCE.replace(
        MANIFEST_DIGEST.trim(),
        &missing_bundle.manifest_digest_review_string(),
    );
    let missing_module =
        parse_module(&missing_source).expect("payload-incomplete effect source parses");
    let missing_adapter = decode_lawpack_adapter(&missing_bundle, "echo.dpo@1", ADAPTER_BYTES)
        .expect("load exact adapter");

    let failures = prepare_lawpack_compilation(&missing_module, &missing_bundle, &missing_adapter)
        .expect_err("an unresolved failure payload must not become compiler or Target facts");

    assert_eq!(
        adapter_failure_kinds(&failures),
        vec![LawpackAdapterFailureKind::InvalidShape]
    );
}

#[test]
fn hello_echo_source_compiles_to_echo_target_ir_from_exact_lawpack_adapter() {
    let bundle =
        decode_lawpack_bundle(MANIFEST_BYTES, EXPORTS_BYTES).expect("load Hello Echo lawpack");
    let module = parse_module(CREATE_GREETING_SOURCE).expect("parse createGreeting source");
    let adapter =
        decode_lawpack_adapter(&bundle, "echo.dpo@1", ADAPTER_BYTES).expect("load exact adapter");
    assert_eq!(adapter.digest_review_string(), ADAPTER_DIGEST.trim());
    let effect = adapter
        .effects()
        .get("hello.echo@1.createGreeting")
        .expect("createGreeting adapter effect");
    assert_eq!(
        effect.target_configuration.id,
        TARGET_CONFIGURATION_COORDINATE
    );
    assert_eq!(
        effect.target_configuration.digest_review_string(),
        TARGET_CONFIGURATION_DIGEST.trim()
    );
    let target_configuration =
        decode_canonical_cbor(TARGET_CONFIGURATION_BYTES).expect("decode target configuration");
    assert_eq!(
        encode_canonical_cbor(&target_configuration).expect("re-encode target configuration"),
        TARGET_CONFIGURATION_BYTES
    );
    assert_eq!(
        digest_value(TARGET_CONFIGURATION_COORDINATE, &target_configuration),
        effect.target_configuration.digest
    );
    let preparation = prepare_lawpack_compilation(&module, &bundle, &adapter)
        .expect("derive compiler and target facts");
    let core = compile_to_core(&module, preparation.compiler_context())
        .expect("compile source-derived Core");
    let report = lower_to_target_ir(&core, preparation.target_ir_facts());

    assert_eq!(report.status, TargetLoweringStatus::Lowered);
    let artifact = report.artifact.expect("Echo Target IR artifact");
    assert_eq!(artifact.domain, "echo.span-ir/v1");
    assert_eq!(artifact.target_profile.coordinate, "echo.dpo@1");
    assert_eq!(
        artifact.target_profile.digest.as_deref(),
        Some("sha256:2e2494121aecf5e6a2d920f5fb85408825d394765fad41484c416397c920fb04")
    );
    let semantic_closure = artifact
        .semantic_closure
        .as_ref()
        .expect("lawpack-backed semantic closure");
    assert_eq!(semantic_closure.lawpacks.len(), 1);
    assert_eq!(semantic_closure.lawpacks[0].coordinate, "hello.echo@1");
    assert_eq!(
        semantic_closure.lawpacks[0].digest.as_deref(),
        Some(MANIFEST_DIGEST.trim())
    );
    let intent = artifact
        .intents
        .get("createGreeting")
        .expect("createGreeting intent");
    assert!(
        intent.basis.is_some(),
        "explicit basis must survive lowering"
    );
    assert_eq!(
        intent.steps[0].target_intrinsic,
        "echo.dpo@1.anchored-node-attachment-create-if-absent"
    );
    assert_eq!(
        intent.steps[0].obstruction_failures,
        vec!["echo.executable-operation/precondition-mismatch/v1"]
    );
    assert!(
        intent.steps[0]
            .obstruction_arms
            .contains_key("echo.executable-operation/precondition-mismatch/v1"),
        "adapter-mapped target obstruction must survive lowering"
    );
    assert_eq!(
        encode_core_module(&core).expect("encode createGreeting Core"),
        CREATE_GREETING_CORE_BYTES
    );
    assert_eq!(
        digest_core_module(&core)
            .expect("digest createGreeting Core")
            .to_review_string(),
        CREATE_GREETING_CORE_DIGEST.trim()
    );
    assert_eq!(
        encode_target_ir_artifact(&artifact).expect("encode createGreeting Target IR"),
        CREATE_GREETING_TARGET_IR_BYTES
    );
    assert_eq!(
        digest_target_ir_artifact(&artifact)
            .expect("digest createGreeting Target IR")
            .to_review_string(),
        CREATE_GREETING_TARGET_IR_DIGEST.trim()
    );
}

#[test]
fn lawpack_adapter_bytes_must_be_canonical_and_digest_bound() {
    let bundle =
        decode_lawpack_bundle(MANIFEST_BYTES, EXPORTS_BYTES).expect("load Hello Echo lawpack");
    let noncanonical = decode_lawpack_adapter(&bundle, "echo.dpo@1", &[0x18, 0x00])
        .expect_err("noncanonical adapter must reject");
    assert_eq!(
        adapter_failure_kinds(&noncanonical),
        vec![LawpackAdapterFailureKind::InvalidCanonicalCbor]
    );

    let mut substituted =
        decode_canonical_cbor(ADAPTER_BYTES).expect("decode canonical adapter fixture");
    replace_field(
        &mut substituted,
        "class",
        text("a different canonical adapter"),
    );
    let substituted_bytes =
        encode_canonical_cbor(&substituted).expect("encode substituted adapter");
    let failures = decode_lawpack_adapter(&bundle, "echo.dpo@1", &substituted_bytes)
        .expect_err("adapter digest substitution must reject");
    assert_eq!(
        adapter_failure_kinds(&failures),
        vec![LawpackAdapterFailureKind::AdapterDigestMismatch]
    );
}

#[test]
fn lawpack_adapter_requires_a_typed_target_configuration_reference() {
    let mut adapter = decode_canonical_cbor(ADAPTER_BYTES).expect("decode canonical adapter");
    let effect = first_map_value_mut(field_mut(&mut adapter, "effectImplementations"));
    let target_configuration = field_mut(effect, "targetConfiguration");
    let digest = field_mut(target_configuration, "digest");
    let CanonicalValue::Array(parts) = digest else {
        panic!("target configuration digest fixture must be an array");
    };
    let CanonicalValue::Bytes(bytes) = &mut parts[1] else {
        panic!("target configuration digest fixture must contain bytes");
    };
    bytes.pop();

    let bundle = bundle_with_adapter(&adapter);
    let bytes = encode_canonical_cbor(&adapter).expect("encode malformed adapter");
    let failures = decode_lawpack_adapter(&bundle, "echo.dpo@1", &bytes)
        .expect_err("malformed target configuration reference must reject");

    assert_eq!(
        adapter_failure_kinds(&failures),
        vec![LawpackAdapterFailureKind::InvalidTargetConfiguration]
    );
}

#[test]
fn lawpack_adapter_rejects_an_undeclared_write_class_at_the_effect_path() {
    let mut adapter = decode_canonical_cbor(ADAPTER_BYTES).expect("decode canonical adapter");
    let effect = first_map_value_mut(field_mut(&mut adapter, "effectImplementations"));
    replace_field(effect, "writeClass", text("modify"));
    let bundle = bundle_with_adapter(&adapter);
    let bytes = encode_canonical_cbor(&adapter).expect("encode adapter");

    let failures = decode_lawpack_adapter(&bundle, "echo.dpo@1", &bytes)
        .expect_err("undeclared write class must reject");

    assert_eq!(
        adapter_failure_kinds(&failures),
        vec![LawpackAdapterFailureKind::InvalidWriteClass]
    );
    assert_eq!(
        failures[0].path,
        "adapter.effectImplementations.hello.echo@1.createGreeting.writeClass"
    );
}

#[test]
fn lawpack_adapter_selection_requires_one_exact_target_profile() {
    let bundle =
        decode_lawpack_bundle(MANIFEST_BYTES, EXPORTS_BYTES).expect("load Hello Echo lawpack");
    let failures = decode_lawpack_adapter(&bundle, "echo.dpo@2", ADAPTER_BYTES)
        .expect_err("unselected target profile must reject");

    assert_eq!(
        adapter_failure_kinds(&failures),
        vec![LawpackAdapterFailureKind::MissingTargetAdapter]
    );
}

#[test]
fn lawpack_adapter_requires_complete_exported_effect_coverage() {
    let mut adapter = decode_canonical_cbor(ADAPTER_BYTES).expect("decode canonical adapter");
    map_mut(field_mut(&mut adapter, "effectImplementations")).clear();
    let bundle = bundle_with_adapter(&adapter);
    let bytes = encode_canonical_cbor(&adapter).expect("encode adapter");
    let failures = decode_lawpack_adapter(&bundle, "echo.dpo@1", &bytes)
        .expect_err("missing effect implementation must reject");

    assert_eq!(
        adapter_failure_kinds(&failures),
        vec![LawpackAdapterFailureKind::MissingEffectImplementation]
    );
}

#[test]
fn lawpack_adapter_rejects_profiles_that_collapse_to_one_core_coordinate() {
    let (mut exports, mut adapter) =
        request_only_adapter(Some("hello.echo@1.smallCreateBudget"), true);
    let exported_profiles = map_mut(field_mut(&mut exports, "operationProfiles"));
    let second_exported_profile = exported_profiles
        .first()
        .map(|(_coordinate, profile)| profile.clone())
        .expect("exported operation profile");
    exported_profiles.push((
        text("hello.echo@1.observeGreeting"),
        second_exported_profile,
    ));

    let adapter_profiles = map_mut(field_mut(&mut adapter, "operationProfiles"));
    let mut second_adapter_profile = adapter_profiles
        .first()
        .map(|(_coordinate, profile)| profile.clone())
        .expect("adapter operation profile");
    replace_field(
        field_mut(&mut second_adapter_profile, "targetConfiguration"),
        "digest",
        CanonicalValue::Array(vec![text("sha256"), CanonicalValue::Bytes(vec![0x42; 32])]),
    );
    adapter_profiles.push((text("hello.echo@1.observeGreeting"), second_adapter_profile));

    let bundle = bundle_with_exports_and_adapter(&exports, &adapter);
    let adapter_bytes = encode_canonical_cbor(&adapter).expect("encode ambiguous adapter");
    let failures = decode_lawpack_adapter(&bundle, "echo.dpo@1", &adapter_bytes)
        .expect_err("source profiles must not collapse to one Core profile");

    assert_eq!(
        adapter_failure_kinds(&failures),
        vec![LawpackAdapterFailureKind::DuplicateReference]
    );
    assert_eq!(
        failures[0].path,
        "adapter.operationProfiles.hello.echo@1.observeGreeting.core"
    );
}

#[test]
fn request_only_profile_supplies_budget_without_callable_effect_authority() {
    let (exports, adapter) = request_only_adapter(Some("hello.echo@1.smallCreateBudget"), true);
    let (bundle, adapter) = bundle_and_adapter(&exports, &adapter);
    let source = format!(
        r#"package examples.workspace_observer@1;

use lawpack hello.echo@1 digest "{}" as hello;
use capability workspace.snapshot.observe@1
  digest "sha256:{}"
  as snapshot;

type ObserveInput = {{
  payload: Bytes<max=1024>,
  scope: Bytes<max=32>,
  basis: Bytes<max=32>,
  maxSettlementBytes: U64,
  maxAttempts: U32,
}};

intent observe(input: ObserveInput)
  returns ExternalActionRequest<Bytes<max=65536>>
  profile hello.createGreeting
  basis input.basis
  budget <= hello.smallCreateBudget
{{
  request pending: ExternalActionRequest<Bytes<max=65536>> =
    snapshot(input.payload)
    input schema workspace.snapshot.input@1 digest "sha256:{}"
    settlement schema workspace.snapshot.settlement@1 digest "sha256:{}"
    authority input.scope
    basis input.basis
    budget maxSettlementBytes input.maxSettlementBytes maxAttempts input.maxAttempts
    reconcile workspace.snapshot.reconcile@1 digest "sha256:{}";
  return pending;
}}
"#,
        bundle.manifest_digest_review_string(),
        "a".repeat(64),
        "b".repeat(64),
        "c".repeat(64),
        "d".repeat(64),
    );
    let module = parse_module(&source).expect("parse request-only application");
    let preparation = prepare_lawpack_compilation(&module, &bundle, &adapter)
        .expect("prepare request-only application");
    let core = compile_to_core(&module, preparation.compiler_context())
        .expect("compile request-only application");
    let report = lower_to_target_ir(&core, preparation.target_ir_facts());
    let intent = report
        .artifact
        .expect("request-only Target IR")
        .intents
        .remove("observe")
        .expect("observe intent");

    assert_eq!(intent.external_action_requests.len(), 1);
    assert_eq!(intent.steps, Vec::<amblicit_syntax::TargetIrStep>::new());
    assert!(
        adapter.effects().is_empty(),
        "request-only profile must not grant target-call authority"
    );
}

#[test]
fn request_only_profile_rejects_another_profiles_budget() {
    let (mut exports, mut adapter) =
        request_only_adapter(Some("hello.echo@1.smallCreateBudget"), true);
    let exported_profiles = map_mut(field_mut(&mut exports, "operationProfiles"));
    let second_exported_profile = exported_profiles
        .first()
        .map(|(_coordinate, profile)| profile.clone())
        .expect("exported operation profile");
    exported_profiles.push((
        text("hello.echo@1.observeGreeting"),
        second_exported_profile,
    ));

    let adapter_profiles = map_mut(field_mut(&mut adapter, "operationProfiles"));
    let first_profile = adapter_profiles
        .first_mut()
        .map(|(_coordinate, profile)| profile)
        .expect("adapter operation profile");
    let mut second_profile = first_profile.clone();
    replace_field(
        &mut second_profile,
        "core",
        text("continuum.profile.observe/v1"),
    );
    replace_field(
        &mut second_profile,
        "budgetObligation",
        text("hello.echo@1.largeObservationBudget"),
    );
    adapter_profiles.push((text("hello.echo@1.observeGreeting"), second_profile));
    let budgets = map_mut(field_mut(&mut adapter, "budgets"));
    let large_budget = budgets
        .first()
        .map(|(_coordinate, budget)| budget.clone())
        .expect("adapter budget");
    budgets.push((text("hello.echo@1.largeObservationBudget"), large_budget));

    let (bundle, adapter) = bundle_and_adapter(&exports, &adapter);
    let source = format!(
        r#"package examples.workspace_observer@1;

use lawpack hello.echo@1 digest "{}" as hello;
use capability workspace.snapshot.observe@1
  digest "sha256:{}"
  as snapshot;

type ObserveInput = {{
  payload: Bytes<max=1024>,
  scope: Bytes<max=32>,
  basis: Bytes<max=32>,
  maxSettlementBytes: U64,
  maxAttempts: U32,
}};

intent observe(input: ObserveInput)
  returns ExternalActionRequest<Bytes<max=65536>>
  profile hello.createGreeting
  basis input.basis
  budget <= hello.largeObservationBudget
{{
  request pending: ExternalActionRequest<Bytes<max=65536>> =
    snapshot(input.payload)
    input schema workspace.snapshot.input@1 digest "sha256:{}"
    settlement schema workspace.snapshot.settlement@1 digest "sha256:{}"
    authority input.scope
    basis input.basis
    budget maxSettlementBytes input.maxSettlementBytes maxAttempts input.maxAttempts
    reconcile workspace.snapshot.reconcile@1 digest "sha256:{}";
  return pending;
}}
"#,
        bundle.manifest_digest_review_string(),
        "a".repeat(64),
        "b".repeat(64),
        "c".repeat(64),
        "d".repeat(64),
    );
    let module = parse_module(&source).expect("parse mismatched-budget application");
    let preparation = prepare_lawpack_compilation(&module, &bundle, &adapter)
        .expect("prepare mismatched-budget application");
    let failures = compile_to_core(&module, preparation.compiler_context())
        .expect_err("a profile must reject another profile's budget");

    assert_eq!(failures.len(), 1);
    assert_eq!(failures[0].stage, CompilerStage::Resolve);
    assert_eq!(failures[0].kind, CompilerErrorKind::MissingContextFact);
    assert!(
        failures[0]
            .message
            .contains("profile `hello.createGreeting` requires budget `hello.smallCreateBudget`"),
        "unexpected mismatch diagnostic: {}",
        failures[0].message
    );
}

#[test]
fn request_only_profile_requires_an_exact_budget_obligation() {
    let (exports, adapter) = request_only_adapter(Some("hello.echo@1.missingBudget"), true);
    let bundle = bundle_with_exports_and_adapter(&exports, &adapter);
    let bytes = encode_canonical_cbor(&adapter).expect("encode request-only adapter");

    let failures = decode_lawpack_adapter(&bundle, "echo.dpo@1", &bytes)
        .expect_err("unknown request-only budget must reject");

    assert_eq!(
        adapter_failure_kinds(&failures),
        vec![LawpackAdapterFailureKind::MissingBudget]
    );
}

#[test]
fn request_only_profile_requires_an_exact_target_configuration() {
    let (exports, adapter) = request_only_adapter(Some("hello.echo@1.smallCreateBudget"), false);
    let bundle = bundle_with_exports_and_adapter(&exports, &adapter);
    let bytes = encode_canonical_cbor(&adapter).expect("encode request-only adapter");

    let failures = decode_lawpack_adapter(&bundle, "echo.dpo@1", &bytes)
        .expect_err("unconfigured request-only profile must reject");

    assert_eq!(
        adapter_failure_kinds(&failures),
        vec![LawpackAdapterFailureKind::InvalidTargetConfiguration]
    );
}

#[test]
fn adapter_budget_closure_includes_dedicated_pure_helper_cost_templates() {
    let identity_body = map([
        (
            "params",
            CanonicalValue::Array(vec![local_ref("arg:0", "value", "U64")]),
        ),
        (
            "body",
            map([
                ("locals", CanonicalValue::Array(Vec::new())),
                ("bindings", CanonicalValue::Array(Vec::new())),
                (
                    "result",
                    map([
                        ("kind", text("local")),
                        ("ref", local_ref("arg:0", "value", "U64")),
                    ]),
                ),
            ]),
        ),
    ]);
    let mut exports = hello_echo_exports();
    let mut helper = pure_function_with_types(
        "hello.echo@1.identityU64",
        &["U64"],
        "U64",
        "edict",
        ("body", identity_body),
    );
    replace_field(
        &mut helper,
        "costTemplate",
        text("hello.echo@1.identityBudget"),
    );
    array_mut(field_mut(&mut exports, "pureFunctions")).push(helper);

    let mut adapter = decode_canonical_cbor(ADAPTER_BYTES).expect("decode canonical adapter");
    let helper_budget = map_mut(field_mut(&mut adapter, "budgets"))
        .first()
        .map(|(_coordinate, budget)| budget.clone())
        .expect("existing adapter budget");
    map_mut(field_mut(&mut adapter, "budgets"))
        .push((text("hello.echo@1.identityBudget"), helper_budget));

    let bundle = bundle_with_exports_and_adapter(&exports, &adapter);
    let bytes = encode_canonical_cbor(&adapter).expect("encode adapter");
    decode_lawpack_adapter(&bundle, "echo.dpo@1", &bytes)
        .expect("dedicated helper budget belongs to the exact adapter closure");
}

#[test]
fn lawpack_adapter_corroborates_footprint_cost_and_failure_obligations() {
    for (field, replacement, expected) in [
        (
            "footprintObligation",
            text("hello.echo@1.someOtherFootprint"),
            LawpackAdapterFailureKind::ObligationMismatch,
        ),
        (
            "costObligation",
            text("hello.echo@1.someOtherBudget"),
            LawpackAdapterFailureKind::ObligationMismatch,
        ),
    ] {
        let mut adapter = decode_canonical_cbor(ADAPTER_BYTES).expect("decode canonical adapter");
        let effect = first_map_value_mut(field_mut(&mut adapter, "effectImplementations"));
        replace_field(effect, field, replacement);
        let bundle = bundle_with_adapter(&adapter);
        let bytes = encode_canonical_cbor(&adapter).expect("encode adapter");
        let failures = decode_lawpack_adapter(&bundle, "echo.dpo@1", &bytes)
            .expect_err("mismatched obligation must reject");
        assert_eq!(adapter_failure_kinds(&failures), vec![expected]);
    }

    let mut adapter = decode_canonical_cbor(ADAPTER_BYTES).expect("decode canonical adapter");
    let effect = first_map_value_mut(field_mut(&mut adapter, "effectImplementations"));
    map_mut(field_mut(effect, "failureMappings")).clear();
    let bundle = bundle_with_adapter(&adapter);
    let bytes = encode_canonical_cbor(&adapter).expect("encode adapter");
    let failures = decode_lawpack_adapter(&bundle, "echo.dpo@1", &bytes)
        .expect_err("incomplete failure mapping must reject");
    assert_eq!(
        adapter_failure_kinds(&failures),
        vec![LawpackAdapterFailureKind::FailureMappingMismatch]
    );
}

#[test]
fn lawpack_import_rejects_raw_file_hash_with_manifest_domain_obligation() {
    let bundle =
        decode_lawpack_bundle(MANIFEST_BYTES, EXPORTS_BYTES).expect("load Hello Echo lawpack");
    let adapter =
        decode_lawpack_adapter(&bundle, "echo.dpo@1", ADAPTER_BYTES).expect("load exact adapter");
    let correct = parse_module(CREATE_GREETING_SOURCE).expect("parse exact source");
    prepare_lawpack_compilation(&correct, &bundle, &adapter)
        .expect("manifest sidecar digest is accepted");
    let raw_digest = format!("sha256:{:x}", Sha256::digest(MANIFEST_BYTES));
    assert_ne!(raw_digest, bundle.manifest_digest_review_string());
    let source = CREATE_GREETING_SOURCE.replace(MANIFEST_DIGEST.trim(), &raw_digest);
    let module = parse_module(&source).expect("file digest has valid review syntax");
    let failures = prepare_lawpack_compilation(&module, &bundle, &adapter)
        .expect_err("raw file hash does not identify the manifest domain");
    assert_eq!(
        adapter_failure_kinds(&failures),
        vec![LawpackAdapterFailureKind::SourceImportMismatch]
    );
    assert_eq!(failures[0].path, "module.imports.hello.digest");
    assert_eq!(
        failures[0].obligation,
        format!(
            "edict.lawpack/v1 domain-framed manifest digest `{}` (not the raw manifest.cbor file hash)",
            bundle.manifest_digest_review_string()
        )
    );
}

#[test]
fn malformed_lawpack_import_digest_retains_parser_kind_and_span() {
    let source = CREATE_GREETING_SOURCE.replace(MANIFEST_DIGEST.trim(), "sha256:not-a-digest");
    let failure = parse_module(&source).expect_err("malformed digest rejects before loading");
    assert_eq!(failure.kind, amblicit_syntax::ParseErrorKind::InvalidDigest);
    assert_eq!(
        &source[failure.span.start..failure.span.end],
        "\"sha256:not-a-digest\""
    );
}

#[test]
fn lawpack_compilation_requires_the_exact_digest_locked_source_import() {
    let bundle =
        decode_lawpack_bundle(MANIFEST_BYTES, EXPORTS_BYTES).expect("load Hello Echo lawpack");
    let adapter =
        decode_lawpack_adapter(&bundle, "echo.dpo@1", ADAPTER_BYTES).expect("load exact adapter");
    let source = CREATE_GREETING_SOURCE.replace(
        MANIFEST_DIGEST.trim(),
        &format!("sha256:{}", "0".repeat(64)),
    );
    let module = parse_module(&source).expect("parse source with substituted import");
    let failures = prepare_lawpack_compilation(&module, &bundle, &adapter)
        .expect_err("substituted source import must reject");

    assert_eq!(
        adapter_failure_kinds(&failures),
        vec![LawpackAdapterFailureKind::SourceImportMismatch]
    );
}

#[test]
fn noncanonical_manifest_bytes_reject_before_shape_validation() {
    let failures = decode_lawpack_bundle(&[0x18, 0x00], EXPORTS_BYTES)
        .expect_err("noncanonical manifest must reject");

    assert_eq!(
        failure_kinds(&failures),
        vec![LawpackValidationFailureKind::InvalidCanonicalCbor]
    );
}

#[test]
fn manifest_and_export_maps_are_closed() {
    let exports = hello_echo_exports();
    let exports_bytes = encode_canonical_cbor(&exports).expect("encode exports");
    let mut manifest = hello_echo_manifest(digest_value(EXPORTS_COORDINATE, &exports));
    insert_field(
        &mut manifest,
        "displayName",
        text("not hash-significant here"),
    );
    let manifest_bytes = encode_canonical_cbor(&manifest).expect("encode manifest");
    let failures = decode_lawpack_bundle(&manifest_bytes, &exports_bytes)
        .expect_err("unknown manifest field must reject");

    assert_eq!(
        failure_kinds(&failures),
        vec![LawpackValidationFailureKind::UnexpectedField]
    );
    assert_eq!(failures[0].path, "manifest.displayName");
}

#[test]
fn export_digest_substitution_rejects() {
    let exports = hello_echo_exports();
    let exports_bytes = encode_canonical_cbor(&exports).expect("encode exports");
    let manifest = hello_echo_manifest([0xff; 32]);
    let manifest_bytes = encode_canonical_cbor(&manifest).expect("encode manifest");
    let failures = decode_lawpack_bundle(&manifest_bytes, &exports_bytes)
        .expect_err("substituted export digest must reject");

    assert_eq!(
        failure_kinds(&failures),
        vec![LawpackValidationFailureKind::ExportsDigestMismatch]
    );
}

#[test]
fn runtime_effect_requires_at_least_one_target_adapter() {
    let exports = hello_echo_exports();
    let exports_bytes = encode_canonical_cbor(&exports).expect("encode exports");
    let mut manifest = hello_echo_manifest(digest_value(EXPORTS_COORDINATE, &exports));
    remove_field(&mut manifest, "targetAdapters");
    let manifest_bytes = encode_canonical_cbor(&manifest).expect("encode manifest");
    let failures = decode_lawpack_bundle(&manifest_bytes, &exports_bytes)
        .expect_err("runtime effect without an adapter must reject");

    assert_eq!(
        failure_kinds(&failures),
        vec![LawpackValidationFailureKind::RuntimeEffectWithoutTargetAdapter]
    );
}

#[test]
fn proof_only_effect_does_not_invent_a_runtime_adapter_requirement() {
    let mut exports = hello_echo_exports();
    replace_field(
        first_array_item_mut(field_mut(&mut exports, "effects")),
        "executionClass",
        text("proofOnly"),
    );
    let exports_bytes = encode_canonical_cbor(&exports).expect("encode exports");
    let mut manifest = hello_echo_manifest(digest_value(EXPORTS_COORDINATE, &exports));
    remove_field(&mut manifest, "targetAdapters");
    let manifest_bytes = encode_canonical_cbor(&manifest).expect("encode manifest");

    let bundle = decode_lawpack_bundle(&manifest_bytes, &exports_bytes)
        .expect("proof-only lawpack does not require an adapter");

    assert_eq!(
        bundle.exports().effects[0].execution_class,
        LawpackExecutionClass::ProofOnly
    );
}

#[test]
fn executable_verifier_must_carry_component_sandbox_and_fuel() {
    let exports = hello_echo_exports();
    let exports_bytes = encode_canonical_cbor(&exports).expect("encode exports");
    let mut manifest = hello_echo_manifest(digest_value(EXPORTS_COORDINATE, &exports));
    replace_field(
        &mut manifest,
        "verifier",
        map([
            ("class", text("executable")),
            (
                "component",
                resource_ref("hello.echo.verifier/v1", [0x77; 32]),
            ),
            (
                "sandbox",
                resource_ref("edict.wasm-component/v1", [0x88; 32]),
            ),
        ]),
    );
    let manifest_bytes = encode_canonical_cbor(&manifest).expect("encode manifest");
    let failures = decode_lawpack_bundle(&manifest_bytes, &exports_bytes)
        .expect_err("unbounded executable verifier must reject");

    assert_eq!(
        failure_kinds(&failures),
        vec![LawpackValidationFailureKind::MissingField]
    );
    assert_eq!(failures[0].path, "manifest.verifier.fuelModel");
}

#[test]
fn all_hash_bound_helper_and_verifier_variants_load() {
    let mut exports = hello_echo_exports();
    let identity_body = map([
        (
            "params",
            CanonicalValue::Array(vec![local_ref(
                "arg:0",
                "value",
                "hello.echo@1.GreetingKey",
            )]),
        ),
        (
            "body",
            map([
                ("locals", CanonicalValue::Array(Vec::new())),
                ("bindings", CanonicalValue::Array(Vec::new())),
                (
                    "result",
                    map([
                        ("kind", text("local")),
                        (
                            "ref",
                            local_ref("arg:0", "value", "hello.echo@1.GreetingKey"),
                        ),
                    ]),
                ),
            ]),
        ),
    ]);
    let pure_functions = array_mut(field_mut(&mut exports, "pureFunctions"));
    pure_functions.push(pure_function(
        "hello.echo@1.identity",
        "edict",
        ("body", identity_body),
    ));
    pure_functions.push(pure_function(
        "hello.echo@1.componentIdentity",
        "component",
        (
            "implementation",
            executable_component("hello.echo.component-identity/v1", 0x91),
        ),
    ));
    let exports_bytes = encode_canonical_cbor(&exports).expect("encode exports");
    let mut manifest = hello_echo_manifest(digest_value(EXPORTS_COORDINATE, &exports));
    insert_field(
        &mut manifest,
        "helperComponent",
        executable_component("hello.echo.helpers/v1", 0x92),
    );
    replace_field(
        &mut manifest,
        "verifier",
        map([
            ("class", text("executable")),
            (
                "component",
                resource_ref("hello.echo.verifier/v1", [0x93; 32]),
            ),
            (
                "sandbox",
                resource_ref("edict.wasm-component/v1", [0x94; 32]),
            ),
            ("fuelModel", resource_ref("edict.fuel/v1", [0x95; 32])),
        ]),
    );
    let manifest_bytes = encode_canonical_cbor(&manifest).expect("encode manifest");

    let bundle =
        decode_lawpack_bundle(&manifest_bytes, &exports_bytes).expect("full variant bundle");

    assert_eq!(
        bundle.manifest().verifier.class(),
        LawpackVerifierClass::Executable
    );
    assert!(bundle.manifest().helper_component.is_some());
    assert!(matches!(
        bundle.exports().pure_functions[0].implementation,
        LawpackPureFunctionImplementation::Edict { .. }
    ));
    assert!(matches!(
        bundle.exports().pure_functions[1].implementation,
        LawpackPureFunctionImplementation::Component { .. }
    ));
}

fn exact_u64_pure_helper_application() -> (
    amblicit_syntax::CoreModule,
    amblicit_syntax::TargetIrLoweringFacts,
) {
    exact_u64_pure_helper_application_with_source(
        "examples.pure_helper@1",
        "",
        "hello.identityU64(input.value)",
    )
}

fn exact_u64_pure_helper_exports() -> CanonicalValue {
    let identity_body = map([
        (
            "params",
            CanonicalValue::Array(vec![local_ref("arg:0", "value", "U64")]),
        ),
        (
            "body",
            map([
                ("locals", CanonicalValue::Array(Vec::new())),
                ("bindings", CanonicalValue::Array(Vec::new())),
                (
                    "result",
                    map([
                        ("kind", text("local")),
                        ("ref", local_ref("arg:0", "value", "U64")),
                    ]),
                ),
            ]),
        ),
    ]);
    let mut exports = hello_echo_exports();
    let mut helper = pure_function_with_types(
        "hello.echo@1.identityU64",
        &["U64"],
        "U64",
        "edict",
        ("body", identity_body),
    );
    replace_field(
        &mut helper,
        "costTemplate",
        text("hello.echo@1.identityBudget"),
    );
    array_mut(field_mut(&mut exports, "pureFunctions")).push(helper);
    exports
}

fn exact_u64_pure_helper_adapter() -> CanonicalValue {
    let mut adapter = decode_canonical_cbor(ADAPTER_BYTES).expect("decode fixture adapter");
    let budgets = field_mut(&mut adapter, "budgets");
    replace_field(
        budgets,
        "hello.echo@1.smallCreateBudget",
        map([
            ("maxSteps", CanonicalValue::Integer(4096)),
            ("maxAllocatedBytes", CanonicalValue::Integer(65536)),
            ("maxOutputBytes", CanonicalValue::Integer(4096)),
        ]),
    );
    insert_field(
        budgets,
        "hello.echo@1.identityBudget",
        map([
            ("maxSteps", CanonicalValue::Integer(16)),
            ("maxAllocatedBytes", CanonicalValue::Integer(512)),
            ("maxOutputBytes", CanonicalValue::Integer(64)),
        ]),
    );
    adapter
}

fn exact_u64_pure_helper_application_with_source(
    package: &str,
    functions: &str,
    expression: &str,
) -> (
    amblicit_syntax::CoreModule,
    amblicit_syntax::TargetIrLoweringFacts,
) {
    let exports = exact_u64_pure_helper_exports();
    let adapter = exact_u64_pure_helper_adapter();
    let (bundle, adapter) = bundle_and_adapter(&exports, &adapter);
    let source = format!(
        "package {package};\n\
         use lawpack hello.echo@1 digest \"{}\" as hello;\n\
         {functions}\n\
         type Input = {{ value: U64, }};\n\
         type Output = {{ value: U64, }};\n\
         intent apply(input: Input) returns Output\n\
           profile hello.createGreeting\n\
           basis none\n\
           budget <= hello.smallCreateBudget {{\n\
           let value: U64 = {expression};\n\
           return {{ value }};\n\
         }}",
        bundle.manifest_digest_review_string()
    );
    let module = parse_module(&source).expect("parse pure-helper application");
    let preparation = prepare_lawpack_compilation(&module, &bundle, &adapter)
        .expect("derive pure-helper compiler fact");
    let core = compile_to_core(&module, preparation.compiler_context())
        .expect("compile exact lawpack helper call");
    (core, preparation.target_ir_facts().clone())
}

fn pure_helper_call_mut(
    core: &mut amblicit_syntax::CoreModule,
) -> (&mut String, &mut Vec<String>, &mut Vec<CoreExpr>) {
    let intent = core.intents.get_mut("apply").expect("lowered apply intent");
    let CoreNode::Let { value, .. } = &mut intent.body.nodes[0] else {
        panic!("pure helper is a Core let");
    };
    let CoreExpr::Call {
        callee,
        type_args,
        args,
    } = value
    else {
        panic!("pure helper binding is a call");
    };
    (callee, type_args, args)
}

#[test]
fn exact_lawpack_pure_helper_signature_enters_source_compilation() {
    let (core, _) = exact_u64_pure_helper_application();
    let intent = core.intents.get("apply").expect("lowered apply intent");
    let CoreNode::Let { value, .. } = &intent.body.nodes[0] else {
        panic!("pure helper is a Core let");
    };

    assert!(matches!(
        value,
        CoreExpr::Call { callee, args, .. }
            if callee == "hello.echo@1.identityU64" && args.len() == 1
    ));
}

#[test]
fn target_lowering_requires_exact_lawpack_pure_helper_authority() {
    let (core, facts) = exact_u64_pure_helper_application();
    let control = lower_to_target_ir(&core, &facts);

    assert_eq!(control.status, TargetLoweringStatus::Lowered);
    assert_eq!(
        control.failures,
        Vec::<amblicit_syntax::TargetLoweringFailure>::new()
    );
    assert!(control.artifact.is_some());
    assert_eq!(facts.pure_functions.len(), 1);
    let helper_fact = &facts.pure_functions[0];
    assert_eq!(helper_fact.coordinate(), "hello.echo@1.identityU64");
    assert_eq!(helper_fact.parameter_types(), ["U64"]);
    assert_eq!(helper_fact.return_type(), "U64");
    assert_eq!(helper_fact.type_parameters(), Vec::<String>::new());
    assert_eq!(helper_fact.lawpack(), &core.imports[0].resource);

    let assert_rejects = |case: &str,
                          core: &amblicit_syntax::CoreModule,
                          facts: &amblicit_syntax::TargetIrLoweringFacts| {
        let report = lower_to_target_ir(core, facts);

        assert_eq!(
            report.status,
            TargetLoweringStatus::Unsupported,
            "{case}: {:?}",
            report.failures
        );
        assert!(report.artifact.is_none(), "{case}");
        assert_eq!(report.failures.len(), 1, "{case}");
        assert_eq!(
            report.failures[0].kind,
            amblicit_syntax::TargetLoweringFailureKind::InvalidCoreIdentity,
            "{case}"
        );
    };

    let mut unknown_callee = core.clone();
    *pure_helper_call_mut(&mut unknown_callee).0 = "hello.echo@1.unboundIdentityU64".to_owned();
    assert_rejects("unknown callee", &unknown_callee, &facts);

    let mut foreign_coordinate = core.clone();
    *pure_helper_call_mut(&mut foreign_coordinate).0 = "foreign.echo@1.identityU64".to_owned();
    assert_rejects(
        "coordinate outside owning lawpack",
        &foreign_coordinate,
        &facts,
    );

    let mut type_arguments = core.clone();
    pure_helper_call_mut(&mut type_arguments)
        .1
        .push("U64".to_owned());
    assert_rejects("unexpected type arguments", &type_arguments, &facts);

    let mut wrong_arity = core.clone();
    pure_helper_call_mut(&mut wrong_arity).2.clear();
    assert_rejects("wrong argument count", &wrong_arity, &facts);

    let mut wrong_argument = core.clone();
    pure_helper_call_mut(&mut wrong_argument).2[0] =
        CoreExpr::Const(amblicit_syntax::CoreValue::Bool(true));
    assert_rejects("wrong argument type", &wrong_argument, &facts);

    let mut duplicate_facts = facts.clone();
    duplicate_facts
        .pure_functions
        .push(duplicate_facts.pure_functions[0].clone());
    assert_rejects("duplicate helper fact", &core, &duplicate_facts);

    let mut missing_facts = facts.clone();
    missing_facts.pure_functions.clear();
    assert_rejects("missing helper fact", &core, &missing_facts);
}

#[test]
fn source_function_can_call_disjoint_import_in_its_own_package_namespace() {
    for package in ["examples.pure_helper@1", "hello.echo@1"] {
        let (core, facts) = exact_u64_pure_helper_application_with_source(
            package,
            "fn local(value: U64) -> U64 { return hello.identityU64(value); }",
            "local(input.value)",
        );
        let target = lower_to_target_ir(&core, &facts);
        assert_eq!(
            target.status,
            TargetLoweringStatus::Lowered,
            "{package}: {target:?}"
        );
        assert!(target.artifact.is_some());
        assert!(matches!(
            &core.functions["local"].body.result,
            CoreExpr::Call { callee, .. } if callee == "hello.echo@1.identityU64"
        ));

        let mut missing_facts = facts.clone();
        missing_facts.pure_functions.clear();
        let missing = lower_to_target_ir(&core, &missing_facts);
        assert_eq!(
            missing.status,
            TargetLoweringStatus::Unsupported,
            "{missing:?}"
        );
        assert!(missing.artifact.is_none());
        assert!(missing.failures.iter().any(|failure| {
            failure.kind == amblicit_syntax::TargetLoweringFailureKind::InvalidCoreIdentity
        }));

        let mut unknown = core;
        let CoreExpr::Call { callee, .. } =
            &mut unknown.functions.get_mut("local").unwrap().body.result
        else {
            panic!("imported helper call");
        };
        *callee = "hello.echo@1.unknownExport".to_owned();
        let report = lower_to_target_ir(&unknown, &facts);
        assert_eq!(
            report.status,
            TargetLoweringStatus::Unsupported,
            "{report:?}"
        );
        assert!(report.artifact.is_none());
        assert!(report.failures.iter().any(|failure| {
            failure.kind == amblicit_syntax::TargetLoweringFailureKind::InvalidCoreIdentity
        }));
    }
}

#[test]
fn target_rejects_unused_source_function_colliding_with_imported_pure_fact() {
    let (mut core, facts) =
        exact_u64_pure_helper_application_with_source("hello.echo@1", "", "input.value");
    assert_eq!(facts.pure_functions.len(), 1);
    assert_eq!(
        facts.pure_functions[0].coordinate(),
        "hello.echo@1.identityU64"
    );
    let control = lower_to_target_ir(&core, &facts);
    assert_eq!(control.status, TargetLoweringStatus::Lowered, "{control:?}");

    let source = parse_module(
        "package source.functions@1; fn identityU64(value: U64) -> U64 { return value; }",
    )
    .expect("parse an unused source-owned function");
    let functions = compile_to_core(&source, &amblicit_syntax::CompilerContext::new())
        .expect("compile the independent source-owned identity");
    core.functions = functions.functions;
    amblicit_syntax::validate_core_module_type_integrity(&core)
        .expect("the collision is at the imported-authority boundary, not Core scope");

    let report = lower_to_target_ir(&core, &facts);
    assert_eq!(
        report.status,
        TargetLoweringStatus::Unsupported,
        "{report:?}"
    );
    assert!(report.artifact.is_none());
    assert_eq!(report.failures.len(), 1);
    assert_eq!(
        report.failures[0].kind,
        amblicit_syntax::TargetLoweringFailureKind::InvalidCoreIdentity
    );
    assert_eq!(report.failures[0].intent, None);
}

#[test]
fn exact_lawpack_exported_type_enters_pure_helper_signature_closure() {
    let exports = typed_helper_exports(true);
    let exports_bytes = encode_canonical_cbor(&exports).expect("encode typed helper exports");
    let manifest = hello_echo_manifest(digest_value(EXPORTS_COORDINATE, &exports));
    let manifest_bytes = encode_canonical_cbor(&manifest).expect("encode typed helper manifest");
    let bundle =
        decode_lawpack_bundle(&manifest_bytes, &exports_bytes).expect("load typed helper lawpack");
    let source = typed_helper_source(&bundle);
    let module = parse_module(&source).expect("parse typed helper application");
    let adapter =
        decode_lawpack_adapter(&bundle, "echo.dpo@1", ADAPTER_BYTES).expect("load adapter");
    let preparation = prepare_lawpack_compilation(&module, &bundle, &adapter)
        .expect("derive typed helper compiler facts");

    let core = compile_to_core(&module, preparation.compiler_context())
        .expect("compile helper over an exact exported bounded type");
    assert_eq!(
        core.types.get("hello.echo@1.GreetingKey"),
        Some(&amblicit_syntax::CoreType::String {
            max: 32,
            canonical: "raw-utf8".to_owned(),
        }),
        "a helper-only exported return type enters the Core closure"
    );
    let target = lower_to_target_ir(&core, preparation.target_ir_facts());
    assert_eq!(target.status, TargetLoweringStatus::Lowered);
    assert_eq!(
        target.failures,
        Vec::<amblicit_syntax::TargetLoweringFailure>::new()
    );
    assert!(target.artifact.is_some());

    let missing_type_exports = typed_helper_exports(false);
    let missing_type_exports_bytes = encode_canonical_cbor(&missing_type_exports)
        .expect("encode helper exports without its type closure");
    let missing_type_manifest =
        hello_echo_manifest(digest_value(EXPORTS_COORDINATE, &missing_type_exports));
    let missing_type_manifest_bytes = encode_canonical_cbor(&missing_type_manifest)
        .expect("encode helper manifest without its type closure");
    let missing_type_bundle =
        decode_lawpack_bundle(&missing_type_manifest_bytes, &missing_type_exports_bytes)
            .expect("load helper lawpack without its type closure");
    let missing_type_source = typed_helper_source(&missing_type_bundle);
    let missing_type_module =
        parse_module(&missing_type_source).expect("parse missing-type helper application");
    let missing_type_adapter =
        decode_lawpack_adapter(&missing_type_bundle, "echo.dpo@1", ADAPTER_BYTES)
            .expect("load missing-type adapter");
    let failures = prepare_lawpack_compilation(
        &missing_type_module,
        &missing_type_bundle,
        &missing_type_adapter,
    )
    .expect_err("helper without its exported type closure rejects during preparation");
    assert_eq!(
        adapter_failure_kinds(&failures),
        vec![LawpackAdapterFailureKind::InvalidShape]
    );
}

#[test]
fn lawpack_pure_helper_signature_rejects_type_definition_substitution() {
    let exports = typed_helper_exports(true);
    let exports_bytes = encode_canonical_cbor(&exports).expect("encode typed helper exports");
    let manifest = hello_echo_manifest(digest_value(EXPORTS_COORDINATE, &exports));
    let manifest_bytes = encode_canonical_cbor(&manifest).expect("encode typed helper manifest");
    let bundle =
        decode_lawpack_bundle(&manifest_bytes, &exports_bytes).expect("load typed helper lawpack");
    let source = typed_helper_source(&bundle);
    let module = parse_module(&source).expect("parse typed helper application");
    let adapter =
        decode_lawpack_adapter(&bundle, "echo.dpo@1", ADAPTER_BYTES).expect("load adapter");
    let preparation = prepare_lawpack_compilation(&module, &bundle, &adapter)
        .expect("derive typed helper compiler facts");
    let mut core = compile_to_core(&module, preparation.compiler_context())
        .expect("compile helper over an exact exported bounded type");

    for parent in ["Input", "Output"] {
        let Some(amblicit_syntax::CoreType::Record { fields }) = core.types.get(parent) else {
            panic!("{parent} starts as a named record");
        };
        assert_eq!(
            fields.get("value").map(String::as_str),
            Some("String<max=32,canonical=raw-utf8>")
        );
    }
    let prior = core.types.insert(
        "hello.echo@1.GreetingKey".to_owned(),
        amblicit_syntax::CoreType::Bool,
    );
    assert!(matches!(
        prior,
        Some(amblicit_syntax::CoreType::String { .. })
    ));

    let target = lower_to_target_ir(&core, preparation.target_ir_facts());

    assert_eq!(target.status, TargetLoweringStatus::Unsupported);
    assert!(target.artifact.is_none());
    assert_eq!(target.failures.len(), 1);
    assert_eq!(
        target.failures[0].kind,
        amblicit_syntax::TargetLoweringFailureKind::InvalidCoreIdentity
    );
}

#[test]
fn nested_exported_type_closure_enters_pure_helper_compilation() {
    let mut exports = typed_helper_exports(true);
    let types = array_mut(field_mut(&mut exports, "types"));
    replace_field(
        types.last_mut().expect("GreetingKey exported type"),
        "definition",
        text("List<hello.echo@1.GreetingAtom,max=4>"),
    );
    types.push(map([
        ("coordinate", text("hello.echo@1.GreetingAtom")),
        ("definition", text("String<max=32,canonical=raw-utf8>")),
    ]));
    let exports_bytes = encode_canonical_cbor(&exports).expect("encode nested typed exports");
    let manifest = hello_echo_manifest(digest_value(EXPORTS_COORDINATE, &exports));
    let manifest_bytes = encode_canonical_cbor(&manifest).expect("encode nested typed manifest");
    let bundle = decode_lawpack_bundle(&manifest_bytes, &exports_bytes)
        .expect("load nested typed helper lawpack");
    let source = typed_helper_list_source(&bundle.manifest_digest_review_string());
    let module = parse_module(&source).expect("parse nested typed helper application");
    let adapter =
        decode_lawpack_adapter(&bundle, "echo.dpo@1", ADAPTER_BYTES).expect("load adapter");
    let preparation = prepare_lawpack_compilation(&module, &bundle, &adapter)
        .expect("derive nested typed helper compiler facts");

    compile_to_core(&module, preparation.compiler_context())
        .expect("compile helper through nested exported type closure");

    let mut missing_nested_exports = exports;
    let mut removed = array_mut(field_mut(&mut missing_nested_exports, "types"))
        .pop()
        .expect("remove GreetingAtom while retaining nested GreetingKey");
    assert_eq!(
        field_mut(&mut removed, "coordinate"),
        &text("hello.echo@1.GreetingAtom")
    );
    let missing_nested_exports_bytes = encode_canonical_cbor(&missing_nested_exports)
        .expect("encode exports without nested type dependency");
    let missing_nested_manifest =
        hello_echo_manifest(digest_value(EXPORTS_COORDINATE, &missing_nested_exports));
    let missing_nested_manifest_bytes = encode_canonical_cbor(&missing_nested_manifest)
        .expect("encode manifest without nested type dependency");
    let missing_nested_bundle = decode_lawpack_bundle(
        &missing_nested_manifest_bytes,
        &missing_nested_exports_bytes,
    )
    .expect("load helper lawpack without nested type dependency");
    let missing_nested_source =
        typed_helper_list_source(&missing_nested_bundle.manifest_digest_review_string());
    let missing_nested_module =
        parse_module(&missing_nested_source).expect("parse missing nested type application");
    let missing_nested_adapter =
        decode_lawpack_adapter(&missing_nested_bundle, "echo.dpo@1", ADAPTER_BYTES)
            .expect("load missing nested type adapter");
    let failures = prepare_lawpack_compilation(
        &missing_nested_module,
        &missing_nested_bundle,
        &missing_nested_adapter,
    )
    .expect_err("missing nested exported type rejects during preparation");
    assert_eq!(
        adapter_failure_kinds(&failures),
        vec![LawpackAdapterFailureKind::InvalidShape]
    );
}

#[test]
fn inline_structural_record_pure_helper_signature_lowers() {
    let (core, facts) = structural_pure_helper_application();
    let target = lower_to_target_ir(&core, &facts);

    assert_eq!(
        target.status,
        TargetLoweringStatus::Lowered,
        "{:?}",
        target.failures
    );
    assert_eq!(
        target.failures,
        Vec::<amblicit_syntax::TargetLoweringFailure>::new()
    );
    assert!(target.artifact.is_some());
    assert!(
        !core
            .types
            .keys()
            .any(|coordinate| coordinate.starts_with("Record<")),
        "structural syntax must not become named Core authority"
    );
}

#[test]
fn inline_structural_record_effect_signature_lowers() {
    let (core, facts) = structural_effect_application();
    let target = lower_to_target_ir(&core, &facts);

    assert_eq!(
        target.status,
        TargetLoweringStatus::Lowered,
        "{:?}",
        target.failures
    );
    assert_eq!(
        target.failures,
        Vec::<amblicit_syntax::TargetLoweringFailure>::new()
    );
    assert!(target.artifact.is_some());
    assert!(
        !core
            .types
            .keys()
            .any(|coordinate| coordinate.starts_with("Record<")),
        "structural syntax must not become named Core authority"
    );
}

#[test]
fn named_signature_closure_rejects_parent_and_leaf_tampering() {
    let applications = [
        (
            "pure helper",
            structural_pure_helper_application(),
            "hello.echo@1.StructuralEnvelope",
        ),
        (
            "effect",
            structural_effect_application(),
            "hello.echo@1.CreateGreetingInput",
        ),
    ];

    for (producer, (core, facts), parent) in applications {
        let control = lower_to_target_ir(&core, &facts);
        assert_eq!(
            control.status,
            TargetLoweringStatus::Lowered,
            "{producer}: {:?}",
            control.failures
        );
        assert!(control.artifact.is_some(), "{producer}");

        for (case, coordinate, replacement) in [
            ("missing parent", parent, None),
            (
                "substituted parent",
                parent,
                Some(amblicit_syntax::CoreType::Bool),
            ),
            ("missing leaf", "hello.echo@1.StructuralLeaf", None),
            (
                "substituted leaf",
                "hello.echo@1.StructuralLeaf",
                Some(amblicit_syntax::CoreType::Bool),
            ),
        ] {
            let mut tampered = core.clone();
            match replacement {
                Some(replacement) => {
                    tampered.types.insert(coordinate.to_owned(), replacement);
                }
                None => {
                    tampered.types.remove(coordinate);
                }
            }

            let target = lower_to_target_ir(&tampered, &facts);
            assert_eq!(
                target.status,
                TargetLoweringStatus::Unsupported,
                "{producer} {case}: {:?}",
                target.failures
            );
            assert!(target.artifact.is_none(), "{producer} {case}");
            assert_eq!(target.failures.len(), 1, "{producer} {case}");
            assert_eq!(
                target.failures[0].kind,
                amblicit_syntax::TargetLoweringFailureKind::InvalidCoreIdentity,
                "{producer} {case}"
            );
        }
    }
}

#[test]
fn exported_type_outside_lawpack_namespace_rejects_before_compiler_facts() {
    let mut exports = typed_helper_exports(true);
    let exported_type = array_mut(field_mut(&mut exports, "types"))
        .last_mut()
        .expect("GreetingKey exported type");
    replace_field(
        exported_type,
        "coordinate",
        text("other.package@1.GreetingKey"),
    );
    let exports_bytes = encode_canonical_cbor(&exports).expect("encode foreign typed exports");
    let manifest = hello_echo_manifest(digest_value(EXPORTS_COORDINATE, &exports));
    let manifest_bytes = encode_canonical_cbor(&manifest).expect("encode foreign typed manifest");
    let bundle = decode_lawpack_bundle(&manifest_bytes, &exports_bytes)
        .expect("load bundle before adapter namespace validation");
    let source = typed_helper_source(&bundle);
    let module = parse_module(&source).expect("parse foreign typed helper application");
    let adapter =
        decode_lawpack_adapter(&bundle, "echo.dpo@1", ADAPTER_BYTES).expect("load adapter");

    let failures = prepare_lawpack_compilation(&module, &bundle, &adapter)
        .expect_err("foreign exported type namespace rejects before compiler facts");

    assert_eq!(
        failures
            .iter()
            .map(|failure| failure.kind)
            .collect::<Vec<_>>(),
        vec![LawpackAdapterFailureKind::InvalidShape]
    );
}

#[test]
fn generic_lawpack_helper_rejects_before_core() {
    let mut exports = typed_helper_exports(true);
    let helper = array_mut(field_mut(&mut exports, "pureFunctions"))
        .last_mut()
        .expect("typed helper export");
    replace_field(
        helper,
        "typeParameters",
        CanonicalValue::Array(vec![text("T")]),
    );
    let exports_bytes = encode_canonical_cbor(&exports).expect("encode generic helper exports");
    let manifest = hello_echo_manifest(digest_value(EXPORTS_COORDINATE, &exports));
    let manifest_bytes = encode_canonical_cbor(&manifest).expect("encode generic helper manifest");
    let bundle = decode_lawpack_bundle(&manifest_bytes, &exports_bytes)
        .expect("load generic helper lawpack");
    let source = typed_helper_source(&bundle);
    let module = parse_module(&source).expect("parse generic helper application");
    let adapter =
        decode_lawpack_adapter(&bundle, "echo.dpo@1", ADAPTER_BYTES).expect("load adapter");
    let preparation = prepare_lawpack_compilation(&module, &bundle, &adapter)
        .expect("derive generic helper compiler facts");

    let errors = compile_to_core(&module, preparation.compiler_context())
        .expect_err("generic helper declaration rejects before Core");

    assert!(errors
        .iter()
        .all(|error| error.stage == CompilerStage::TypeCheck));
    assert!(errors
        .iter()
        .any(|error| error.kind == CompilerErrorKind::UnsupportedSourceShape));
}

#[test]
fn exact_lawpack_constant_enters_loop_bound_compilation() {
    let mut exports = hello_echo_exports();
    array_mut(field_mut(&mut exports, "constants")).push(map([
        ("coordinate", text("hello.echo@1.maxItems")),
        ("type", text("U64")),
        ("value", CanonicalValue::Integer(4)),
    ]));
    let exports_bytes = encode_canonical_cbor(&exports).expect("encode bound exports");
    let manifest = hello_echo_manifest(digest_value(EXPORTS_COORDINATE, &exports));
    let manifest_bytes = encode_canonical_cbor(&manifest).expect("encode bound manifest");
    let bundle =
        decode_lawpack_bundle(&manifest_bytes, &exports_bytes).expect("load bound lawpack");
    let source = format!(
        "package examples.bound_loop@1;\n\
         use lawpack hello.echo@1 digest \"{}\" as hello;\n\
         type Input = {{ items: List<U64, max=4>, }};\n\
         type Output = {{ value: U64, }};\n\
         intent apply(input: Input) returns Output\n\
           profile hello.createGreeting\n\
           basis none\n\
           budget <= hello.smallCreateBudget {{\n\
           for item in input.items bounded hello.maxItems {{\n\
             require item <= 10u64 else example.ItemTooLarge;\n\
           }}\n\
           return {{ value: 0u64 }};\n\
         }}",
        bundle.manifest_digest_review_string()
    );
    let module = parse_module(&source).expect("parse coordinate-bounded application");
    let adapter =
        decode_lawpack_adapter(&bundle, "echo.dpo@1", ADAPTER_BYTES).expect("load adapter");
    let preparation = prepare_lawpack_compilation(&module, &bundle, &adapter)
        .expect("derive digest-bound constant fact");
    let core = compile_to_core(&module, preparation.compiler_context())
        .expect("compile coordinate-bounded loop");
    let intent = core.intents.get("apply").expect("lowered apply intent");
    let CoreNode::For { bound, .. } = &intent.body.nodes[0] else {
        panic!("first node is a Core for");
    };

    assert_eq!(
        bound,
        &CoreBound::Coordinate("hello.echo@1.maxItems".to_owned())
    );
}

#[test]
fn exact_lawpack_u32_constant_enters_loop_bound_compilation() {
    let mut exports = hello_echo_exports();
    array_mut(field_mut(&mut exports, "constants")).push(map([
        ("coordinate", text("hello.echo@1.maxItems")),
        ("type", text("U32")),
        ("value", CanonicalValue::Integer(4)),
    ]));
    let exports_bytes = encode_canonical_cbor(&exports).expect("encode U32 bound exports");
    let manifest = hello_echo_manifest(digest_value(EXPORTS_COORDINATE, &exports));
    let manifest_bytes = encode_canonical_cbor(&manifest).expect("encode U32 bound manifest");
    let bundle =
        decode_lawpack_bundle(&manifest_bytes, &exports_bytes).expect("load U32 bound lawpack");
    let source = format!(
        "package examples.bound_loop@1;\n\
         use lawpack hello.echo@1 digest \"{}\" as hello;\n\
         type Input = {{ items: List<U64, max=4>, }};\n\
         type Output = {{ value: U64, }};\n\
         intent apply(input: Input) returns Output\n\
           profile hello.createGreeting\n\
           basis none\n\
           budget <= hello.smallCreateBudget {{\n\
           for item in input.items bounded hello.maxItems {{\n\
             require item <= 10u64 else example.ItemTooLarge;\n\
           }}\n\
           return {{ value: 0u64 }};\n\
         }}",
        bundle.manifest_digest_review_string()
    );
    let module = parse_module(&source).expect("parse U32 coordinate-bounded application");
    let adapter =
        decode_lawpack_adapter(&bundle, "echo.dpo@1", ADAPTER_BYTES).expect("load adapter");
    let preparation =
        prepare_lawpack_compilation(&module, &bundle, &adapter).expect("derive U32 constant fact");

    compile_to_core(&module, preparation.compiler_context())
        .expect("compile U32 coordinate-bounded loop");
}

#[test]
fn exact_lawpack_numeric_constants_reject_type_and_budget_violations() {
    let mut oversized_u32_exports = hello_echo_exports();
    array_mut(field_mut(&mut oversized_u32_exports, "constants")).push(map([
        ("coordinate", text("hello.echo@1.maxItems")),
        ("type", text("U32")),
        ("value", CanonicalValue::Integer(i128::from(u32::MAX) + 1)),
    ]));
    let exports_bytes =
        encode_canonical_cbor(&oversized_u32_exports).expect("encode oversized U32 exports");
    let manifest = hello_echo_manifest(digest_value(EXPORTS_COORDINATE, &oversized_u32_exports));
    let manifest_bytes = encode_canonical_cbor(&manifest).expect("encode oversized U32 manifest");
    let bundle = decode_lawpack_bundle(&manifest_bytes, &exports_bytes)
        .expect("load syntactically canonical oversized U32 lawpack");
    let source = format!(
        "package examples.bound_loop@1;\n\
         use lawpack hello.echo@1 digest \"{}\" as hello;",
        bundle.manifest_digest_review_string()
    );
    let module = parse_module(&source).expect("parse exact lawpack import");
    let adapter =
        decode_lawpack_adapter(&bundle, "echo.dpo@1", ADAPTER_BYTES).expect("load adapter");
    let failures = prepare_lawpack_compilation(&module, &bundle, &adapter)
        .expect_err("oversized U32 bound must reject preparation");
    assert_eq!(failures[0].kind, LawpackAdapterFailureKind::InvalidShape);

    let mut over_budget_exports = hello_echo_exports();
    array_mut(field_mut(&mut over_budget_exports, "constants")).push(map([
        ("coordinate", text("hello.echo@1.maxItems")),
        ("type", text("U64")),
        ("value", CanonicalValue::Integer(65)),
    ]));
    let exports_bytes =
        encode_canonical_cbor(&over_budget_exports).expect("encode over-budget exports");
    let manifest = hello_echo_manifest(digest_value(EXPORTS_COORDINATE, &over_budget_exports));
    let manifest_bytes = encode_canonical_cbor(&manifest).expect("encode over-budget manifest");
    let bundle =
        decode_lawpack_bundle(&manifest_bytes, &exports_bytes).expect("load over-budget lawpack");
    let source = format!(
        "package examples.bound_loop@1;\n\
         use lawpack hello.echo@1 digest \"{}\" as hello;\n\
         type Input = {{ items: List<U64, max=4>, }};\n\
         type Output = {{ value: U64, }};\n\
         intent apply(input: Input) returns Output\n\
           profile hello.createGreeting\n\
           basis none\n\
           budget <= hello.smallCreateBudget {{\n\
           for item in input.items bounded hello.maxItems {{\n\
             require item <= 10u64 else example.ItemTooLarge;\n\
           }}\n\
           return {{ value: 0u64 }};\n\
         }}",
        bundle.manifest_digest_review_string()
    );
    let module = parse_module(&source).expect("parse over-budget application");
    let adapter =
        decode_lawpack_adapter(&bundle, "echo.dpo@1", ADAPTER_BYTES).expect("load adapter");
    let preparation = prepare_lawpack_compilation(&module, &bundle, &adapter)
        .expect("derive over-budget constant fact");
    let errors = compile_to_core(&module, preparation.compiler_context())
        .expect_err("over-budget U64 bound must reject compilation");
    assert!(errors
        .iter()
        .any(|error| error.kind == CompilerErrorKind::InvalidBound));
}

#[test]
fn edict_pure_helpers_reject_effectful_and_unresolved_callees() {
    for callee in ["hello.echo@1.createGreeting", "hello.echo@1.notExported"] {
        let mut exports = hello_echo_exports();
        let body = map([
            (
                "params",
                CanonicalValue::Array(vec![local_ref(
                    "arg:0",
                    "value",
                    "hello.echo@1.GreetingKey",
                )]),
            ),
            (
                "body",
                map([
                    ("locals", CanonicalValue::Array(Vec::new())),
                    ("bindings", CanonicalValue::Array(Vec::new())),
                    (
                        "result",
                        map([
                            ("kind", text("call")),
                            ("callee", text(callee)),
                            ("typeArgs", CanonicalValue::Array(Vec::new())),
                            (
                                "args",
                                CanonicalValue::Array(vec![map([
                                    ("kind", text("local")),
                                    (
                                        "ref",
                                        local_ref("arg:0", "value", "hello.echo@1.GreetingKey"),
                                    ),
                                ])]),
                            ),
                        ]),
                    ),
                ]),
            ),
        ]);
        array_mut(field_mut(&mut exports, "pureFunctions")).push(pure_function(
            "hello.echo@1.invalidCaller",
            "edict",
            ("body", body),
        ));
        let exports_bytes = encode_canonical_cbor(&exports).expect("encode exports");
        let manifest = hello_echo_manifest(digest_value(EXPORTS_COORDINATE, &exports));
        let manifest_bytes = encode_canonical_cbor(&manifest).expect("encode manifest");

        let failures = decode_lawpack_bundle(&manifest_bytes, &exports_bytes)
            .expect_err("effectful or unresolved pure callee must reject");

        assert_eq!(
            failure_kinds(&failures),
            vec![LawpackValidationFailureKind::InvalidPureFunctionBody],
            "callee {callee}"
        );
    }
}

#[test]
fn edict_pure_helper_body_must_match_its_exported_signature() {
    let body = map([
        (
            "params",
            CanonicalValue::Array(vec![local_ref("arg:0", "value", "U64")]),
        ),
        (
            "body",
            map([
                ("locals", CanonicalValue::Array(Vec::new())),
                ("bindings", CanonicalValue::Array(Vec::new())),
                (
                    "result",
                    map([
                        ("kind", text("const")),
                        (
                            "value",
                            map([("kind", text("string")), ("value", text("wrong"))]),
                        ),
                    ]),
                ),
            ]),
        ),
    ]);
    let mut exports = hello_echo_exports();
    array_mut(field_mut(&mut exports, "pureFunctions")).push(pure_function_with_types(
        "hello.echo@1.invalidIdentity",
        &["U64"],
        "U64",
        "edict",
        ("body", body),
    ));
    let exports_bytes = encode_canonical_cbor(&exports).expect("encode exports");
    let manifest = hello_echo_manifest(digest_value(EXPORTS_COORDINATE, &exports));
    let manifest_bytes = encode_canonical_cbor(&manifest).expect("encode manifest");

    let failures = decode_lawpack_bundle(&manifest_bytes, &exports_bytes)
        .expect_err("helper body return type must match its exported signature");

    assert_eq!(
        failure_kinds(&failures),
        vec![LawpackValidationFailureKind::InvalidPureFunctionBody]
    );
}

#[allow(clippy::too_many_lines)]
#[test]
fn edict_pure_helper_signature_checks_params_locals_bindings_and_calls() {
    let cases = [
        (
            "parameter",
            map([
                (
                    "params",
                    CanonicalValue::Array(vec![local_ref("arg:0", "value", "Bool")]),
                ),
                (
                    "body",
                    map([
                        ("locals", CanonicalValue::Array(Vec::new())),
                        ("bindings", CanonicalValue::Array(Vec::new())),
                        (
                            "result",
                            map([
                                ("kind", text("local")),
                                ("ref", local_ref("arg:0", "value", "Bool")),
                            ]),
                        ),
                    ]),
                ),
            ]),
        ),
        (
            "local",
            map([
                (
                    "params",
                    CanonicalValue::Array(vec![local_ref("arg:0", "value", "U64")]),
                ),
                (
                    "body",
                    map([
                        ("locals", CanonicalValue::Array(Vec::new())),
                        ("bindings", CanonicalValue::Array(Vec::new())),
                        (
                            "result",
                            map([
                                ("kind", text("local")),
                                ("ref", local_ref("arg:missing", "value", "U64")),
                            ]),
                        ),
                    ]),
                ),
            ]),
        ),
        (
            "binding",
            map([
                ("params", CanonicalValue::Array(Vec::new())),
                (
                    "body",
                    map([
                        (
                            "locals",
                            CanonicalValue::Array(vec![local_ref("local:0", "value", "U64")]),
                        ),
                        (
                            "bindings",
                            CanonicalValue::Array(vec![map([
                                ("kind", text("let")),
                                ("binding", local_ref("local:0", "value", "U64")),
                                (
                                    "value",
                                    map([
                                        ("kind", text("const")),
                                        (
                                            "value",
                                            map([
                                                ("kind", text("bool")),
                                                ("value", CanonicalValue::Bool(true)),
                                            ]),
                                        ),
                                    ]),
                                ),
                            ])]),
                        ),
                        (
                            "result",
                            map([
                                ("kind", text("local")),
                                ("ref", local_ref("local:0", "value", "U64")),
                            ]),
                        ),
                    ]),
                ),
            ]),
        ),
    ];

    for (label, body) in cases {
        let mut exports = hello_echo_exports();
        let parameter_types: &[&str] = if label == "binding" { &[] } else { &["U64"] };
        array_mut(field_mut(&mut exports, "pureFunctions")).push(pure_function_with_types(
            "hello.echo@1.invalidBody",
            parameter_types,
            "U64",
            "edict",
            ("body", body),
        ));
        let exports_bytes = encode_canonical_cbor(&exports).expect("encode exports");
        let manifest = hello_echo_manifest(digest_value(EXPORTS_COORDINATE, &exports));
        let manifest_bytes = encode_canonical_cbor(&manifest).expect("encode manifest");

        let failures = decode_lawpack_bundle(&manifest_bytes, &exports_bytes)
            .expect_err("invalid helper signature closure must reject");
        assert_eq!(
            failure_kinds(&failures),
            vec![LawpackValidationFailureKind::InvalidPureFunctionBody],
            "{label}"
        );
    }

    let identity_body = map([
        (
            "params",
            CanonicalValue::Array(vec![local_ref("arg:0", "value", "U64")]),
        ),
        (
            "body",
            map([
                ("locals", CanonicalValue::Array(Vec::new())),
                ("bindings", CanonicalValue::Array(Vec::new())),
                (
                    "result",
                    map([
                        ("kind", text("local")),
                        ("ref", local_ref("arg:0", "value", "U64")),
                    ]),
                ),
            ]),
        ),
    ]);
    let caller_body = map([
        (
            "params",
            CanonicalValue::Array(vec![local_ref("arg:0", "value", "Bool")]),
        ),
        (
            "body",
            map([
                ("locals", CanonicalValue::Array(Vec::new())),
                ("bindings", CanonicalValue::Array(Vec::new())),
                (
                    "result",
                    map([
                        ("kind", text("call")),
                        ("callee", text("hello.echo@1.identityU64")),
                        ("typeArgs", CanonicalValue::Array(Vec::new())),
                        (
                            "args",
                            CanonicalValue::Array(vec![map([
                                ("kind", text("local")),
                                ("ref", local_ref("arg:0", "value", "Bool")),
                            ])]),
                        ),
                    ]),
                ),
            ]),
        ),
    ]);
    let mut exports = hello_echo_exports();
    let functions = array_mut(field_mut(&mut exports, "pureFunctions"));
    functions.push(pure_function_with_types(
        "hello.echo@1.identityU64",
        &["U64"],
        "U64",
        "edict",
        ("body", identity_body),
    ));
    functions.push(pure_function_with_types(
        "hello.echo@1.invalidCaller",
        &["Bool"],
        "U64",
        "edict",
        ("body", caller_body),
    ));
    let exports_bytes = encode_canonical_cbor(&exports).expect("encode exports");
    let manifest = hello_echo_manifest(digest_value(EXPORTS_COORDINATE, &exports));
    let manifest_bytes = encode_canonical_cbor(&manifest).expect("encode manifest");
    let failures = decode_lawpack_bundle(&manifest_bytes, &exports_bytes)
        .expect_err("call arguments must match the callee signature");
    assert_eq!(
        failure_kinds(&failures),
        vec![LawpackValidationFailureKind::InvalidPureFunctionBody]
    );
}

#[test]
fn edict_pure_helper_collection_constants_obey_bounded_item_types() {
    let list_body = map([
        ("params", CanonicalValue::Array(Vec::new())),
        (
            "body",
            map([
                ("locals", CanonicalValue::Array(Vec::new())),
                ("bindings", CanonicalValue::Array(Vec::new())),
                (
                    "result",
                    map([
                        ("kind", text("const")),
                        (
                            "value",
                            map([
                                ("kind", text("list")),
                                (
                                    "values",
                                    CanonicalValue::Array(vec![
                                        map([
                                            ("kind", text("int")),
                                            ("width", text("U64")),
                                            ("value", CanonicalValue::Integer(1)),
                                        ]),
                                        map([
                                            ("kind", text("int")),
                                            ("width", text("U64")),
                                            ("value", CanonicalValue::Integer(2)),
                                        ]),
                                    ]),
                                ),
                            ]),
                        ),
                    ]),
                ),
            ]),
        ),
    ]);
    let mut exports = hello_echo_exports();
    array_mut(field_mut(&mut exports, "types")).push(map([
        ("coordinate", text("hello.echo@1.OneU64")),
        ("definition", text("List<U64,max=1>")),
    ]));
    array_mut(field_mut(&mut exports, "pureFunctions")).push(pure_function_with_types(
        "hello.echo@1.invalidList",
        &[],
        "hello.echo@1.OneU64",
        "edict",
        ("body", list_body),
    ));
    let exports_bytes = encode_canonical_cbor(&exports).expect("encode list exports");
    let manifest = hello_echo_manifest(digest_value(EXPORTS_COORDINATE, &exports));
    let manifest_bytes = encode_canonical_cbor(&manifest).expect("encode list manifest");

    let failures = decode_lawpack_bundle(&manifest_bytes, &exports_bytes)
        .expect_err("over-bound list constant must reject");
    assert_eq!(
        failure_kinds(&failures),
        vec![LawpackValidationFailureKind::InvalidPureFunctionBody]
    );
}

#[test]
fn edict_pure_helper_field_results_come_from_the_base_record() {
    let field_body = map([
        (
            "params",
            CanonicalValue::Array(vec![local_ref("arg:0", "value", "hello.echo@1.Counter")]),
        ),
        (
            "body",
            map([
                ("locals", CanonicalValue::Array(Vec::new())),
                ("bindings", CanonicalValue::Array(Vec::new())),
                (
                    "result",
                    map([
                        ("kind", text("field")),
                        (
                            "base",
                            map([
                                ("kind", text("local")),
                                ("ref", local_ref("arg:0", "value", "hello.echo@1.Counter")),
                            ]),
                        ),
                        ("field", text("missing")),
                    ]),
                ),
            ]),
        ),
    ]);
    let mut exports = hello_echo_exports();
    array_mut(field_mut(&mut exports, "types")).push(map([
        ("coordinate", text("hello.echo@1.Counter")),
        ("definition", text("{ count: U64, }")),
    ]));
    array_mut(field_mut(&mut exports, "pureFunctions")).push(pure_function_with_types(
        "hello.echo@1.invalidField",
        &["hello.echo@1.Counter"],
        "U64",
        "edict",
        ("body", field_body),
    ));
    let exports_bytes = encode_canonical_cbor(&exports).expect("encode field exports");
    let manifest = hello_echo_manifest(digest_value(EXPORTS_COORDINATE, &exports));
    let manifest_bytes = encode_canonical_cbor(&manifest).expect("encode field manifest");

    let failures = decode_lawpack_bundle(&manifest_bytes, &exports_bytes)
        .expect_err("unknown field must reject");
    assert_eq!(
        failure_kinds(&failures),
        vec![LawpackValidationFailureKind::InvalidPureFunctionBody]
    );
}

#[test]
fn edict_pure_helper_constants_obey_scalar_map_and_variant_types() {
    let cases = [
        (
            "integer",
            "U32",
            None,
            map([
                ("kind", text("int")),
                ("width", text("U32")),
                ("value", CanonicalValue::Integer(-1)),
            ]),
        ),
        (
            "string",
            "hello.echo@1.OneByte",
            Some("String<max=1,canonical=raw-utf8>"),
            map([("kind", text("string")), ("value", text("too long"))]),
        ),
        (
            "map",
            "hello.echo@1.OneEntry",
            Some("Map<U64,U64,max=1>"),
            map([
                ("kind", text("map")),
                (
                    "entries",
                    CanonicalValue::Array(vec![
                        CanonicalValue::Array(vec![core_u64(1), core_u64(10)]),
                        CanonicalValue::Array(vec![core_u64(2), core_u64(20)]),
                    ]),
                ),
            ]),
        ),
    ];

    for (label, return_type, definition, value) in cases {
        let mut exports = hello_echo_exports();
        if let Some(definition) = definition {
            array_mut(field_mut(&mut exports, "types")).push(map([
                ("coordinate", text(return_type)),
                ("definition", text(definition)),
            ]));
        }
        array_mut(field_mut(&mut exports, "pureFunctions")).push(pure_function_with_types(
            &format!("hello.echo@1.invalid{label}"),
            &[],
            return_type,
            "edict",
            (
                "body",
                pure_body(Vec::new(), map([("kind", text("const")), ("value", value)])),
            ),
        ));
        let exports_bytes = encode_canonical_cbor(&exports).expect("encode invalid exports");
        let manifest = hello_echo_manifest(digest_value(EXPORTS_COORDINATE, &exports));
        let manifest_bytes = encode_canonical_cbor(&manifest).expect("encode invalid manifest");

        let failures = decode_lawpack_bundle(&manifest_bytes, &exports_bytes)
            .expect_err("out-of-domain helper constant must reject");
        assert_eq!(
            failure_kinds(&failures),
            vec![LawpackValidationFailureKind::InvalidPureFunctionBody],
            "{label}"
        );
    }

    let mut exports = hello_echo_exports();
    array_mut(field_mut(&mut exports, "types")).push(map([
        ("coordinate", text("hello.echo@1.Choice")),
        ("definition", text("variant { Present(U64), Empty, }")),
    ]));
    let invalid_variant = map([
        ("kind", text("variant")),
        ("type", text("hello.echo@1.Choice")),
        ("case", text("Missing")),
    ]);
    array_mut(field_mut(&mut exports, "pureFunctions")).push(pure_function_with_types(
        "hello.echo@1.invalidVariant",
        &[],
        "hello.echo@1.Choice",
        "edict",
        ("body", pure_body(Vec::new(), invalid_variant)),
    ));
    let exports_bytes = encode_canonical_cbor(&exports).expect("encode variant exports");
    let manifest = hello_echo_manifest(digest_value(EXPORTS_COORDINATE, &exports));
    let manifest_bytes = encode_canonical_cbor(&manifest).expect("encode variant manifest");
    let failures = decode_lawpack_bundle(&manifest_bytes, &exports_bytes)
        .expect_err("unknown variant case must reject");
    assert_eq!(
        failure_kinds(&failures),
        vec![LawpackValidationFailureKind::InvalidPureFunctionBody]
    );
}

#[test]
fn edict_pure_helper_byte_constants_obey_complete_intervals() {
    fn validation_outcome(definition: &str, byte_len: usize) -> &'static str {
        let mut exports = hello_echo_exports();
        array_mut(field_mut(&mut exports, "types")).push(map([
            ("coordinate", text("hello.echo@1.ByteInterval")),
            ("definition", text(definition)),
        ]));
        array_mut(field_mut(&mut exports, "pureFunctions")).push(pure_function_with_types(
            "hello.echo@1.byteInterval",
            &[],
            "hello.echo@1.ByteInterval",
            "edict",
            (
                "body",
                pure_body(
                    Vec::new(),
                    map([
                        ("kind", text("const")),
                        (
                            "value",
                            map([
                                ("kind", text("bytes")),
                                ("value", CanonicalValue::Bytes(vec![0; byte_len])),
                            ]),
                        ),
                    ]),
                ),
            ),
        ));
        let exports_bytes = encode_canonical_cbor(&exports).expect("encode byte exports");
        let manifest = hello_echo_manifest(digest_value(EXPORTS_COORDINATE, &exports));
        let manifest_bytes = encode_canonical_cbor(&manifest).expect("encode byte manifest");

        match decode_lawpack_bundle(&manifest_bytes, &exports_bytes) {
            Ok(_) => "accepted",
            Err(failures)
                if failure_kinds(&failures)
                    == vec![LawpackValidationFailureKind::InvalidPureFunctionBody] =>
            {
                "invalid-pure-body"
            }
            Err(_) => "other-failure",
        }
    }

    assert_eq!(
        [
            validation_outcome("Bytes<exact=4>", 1),
            validation_outcome("Bytes<min=2,max=4>", 1),
            validation_outcome("Bytes<min=2,max=4>", 3),
            validation_outcome("Bytes<min=2,max=4>", 5),
        ],
        [
            "invalid-pure-body",
            "invalid-pure-body",
            "accepted",
            "invalid-pure-body",
        ]
    );
}

#[test]
fn edict_pure_helper_call_graph_depth_is_bounded() {
    for count in [128, 129] {
        let mut exports = hello_echo_exports();
        let functions = array_mut(field_mut(&mut exports, "pureFunctions"));
        for index in 0..count {
            let result = if index + 1 == count {
                map([("kind", text("const")), ("value", core_u64(0))])
            } else {
                map([
                    ("kind", text("call")),
                    ("callee", text(&format!("hello.echo@1.chain{}", index + 1))),
                    ("typeArgs", CanonicalValue::Array(Vec::new())),
                    ("args", CanonicalValue::Array(Vec::new())),
                ])
            };
            functions.push(pure_function_with_types(
                &format!("hello.echo@1.chain{index}"),
                &[],
                "U64",
                "edict",
                ("body", pure_body(Vec::new(), result)),
            ));
        }
        let exports_bytes = encode_canonical_cbor(&exports).expect("encode deep helper graph");
        let manifest = hello_echo_manifest(digest_value(EXPORTS_COORDINATE, &exports));
        let manifest_bytes = encode_canonical_cbor(&manifest).expect("encode deep graph manifest");

        if count == 128 {
            decode_lawpack_bundle(&manifest_bytes, &exports_bytes)
                .expect("helper graph at the depth boundary is accepted");
        } else {
            let failures = decode_lawpack_bundle(&manifest_bytes, &exports_bytes)
                .expect_err("over-depth helper graph must reject without recursion failure");
            assert_eq!(
                failure_kinds(&failures),
                vec![LawpackValidationFailureKind::InvalidPureFunctionBody]
            );
        }
    }
}

#[test]
fn edict_pure_helper_call_graph_depth_is_independent_of_root_order() {
    let mut observed = Vec::new();
    let mut expected = Vec::new();
    for count in [128, 129] {
        for shallow_first in [false, true] {
            for reverse_exports in [false, true] {
                // Coordinate order, not export insertion order, selects DFS roots.
                let coordinates = (0..count)
                    .map(|index| {
                        let key = if shallow_first {
                            count - 1 - index
                        } else {
                            index
                        };
                        format!("hello.echo@1.depth{key:03}")
                    })
                    .collect::<Vec<_>>();
                let mut functions = pure_helper_chain(&coordinates);
                if reverse_exports {
                    functions.reverse();
                }
                observed.push((
                    count,
                    shallow_first,
                    reverse_exports,
                    pure_helper_graph_outcome(functions),
                ));
                expected.push((
                    count,
                    shallow_first,
                    reverse_exports,
                    if count == 128 {
                        Ok(())
                    } else {
                        Err(vec![LawpackValidationFailureKind::InvalidPureFunctionBody])
                    },
                ));
            }
        }
    }
    assert_eq!(observed, expected);
}

#[test]
fn edict_pure_helper_call_graph_depth_includes_shared_suffixes() {
    let mut observed = Vec::new();
    let mut expected = Vec::new();
    for height in [128, 129] {
        for shallow_first in [false, true] {
            for reverse_exports in [false, true] {
                // The root calls a two-helper suffix directly and through a long
                // prefix. Rename the same graph to visit either edge first.
                let coordinates = (0..height)
                    .map(|index| {
                        let key = if !shallow_first || index == 0 {
                            index
                        } else if index >= height - 2 {
                            index - (height - 3)
                        } else {
                            index + 2
                        };
                        format!("hello.echo@1.shared{key:03}")
                    })
                    .collect::<Vec<_>>();
                let mut functions = pure_helper_chain(&coordinates);
                let block = field_mut(field_mut(&mut functions[0], "body"), "body");
                let binding = local_ref("local:0", "shortPath", "U64");
                *field_mut(block, "locals") = CanonicalValue::Array(vec![binding.clone()]);
                *field_mut(block, "bindings") = CanonicalValue::Array(vec![map([
                    ("kind", text("let")),
                    ("binding", binding),
                    ("value", pure_helper_call(&coordinates[height - 2])),
                ])]);
                if reverse_exports {
                    functions.reverse();
                }
                observed.push((
                    height,
                    shallow_first,
                    reverse_exports,
                    pure_helper_graph_outcome(functions),
                ));
                expected.push((
                    height,
                    shallow_first,
                    reverse_exports,
                    if height == 128 {
                        Ok(())
                    } else {
                        Err(vec![LawpackValidationFailureKind::InvalidPureFunctionBody])
                    },
                ));
            }
        }
    }
    assert_eq!(observed, expected);
}

#[test]
fn edict_pure_helper_call_graph_depth_is_not_total_helper_count() {
    let functions = (0..129)
        .map(|index| {
            pure_function_with_types(
                &format!("hello.echo@1.independent{index:03}"),
                &[],
                "U64",
                "edict",
                (
                    "body",
                    pure_body(
                        Vec::new(),
                        map([("kind", text("const")), ("value", core_u64(0))]),
                    ),
                ),
            )
        })
        .collect();
    assert_eq!(pure_helper_graph_outcome(functions), Ok(()));
}

#[test]
fn edict_pure_helper_call_graph_must_be_acyclic() {
    for (coordinates, callees) in [
        (
            vec!["hello.echo@1.selfRecursive"],
            vec!["hello.echo@1.selfRecursive"],
        ),
        (
            vec!["hello.echo@1.mutualLeft", "hello.echo@1.mutualRight"],
            vec!["hello.echo@1.mutualRight", "hello.echo@1.mutualLeft"],
        ),
    ] {
        let mut exports = hello_echo_exports();
        for (coordinate, callee) in coordinates.iter().zip(&callees) {
            let body = map([
                ("params", CanonicalValue::Array(Vec::new())),
                (
                    "body",
                    map([
                        ("locals", CanonicalValue::Array(Vec::new())),
                        ("bindings", CanonicalValue::Array(Vec::new())),
                        (
                            "result",
                            map([
                                ("kind", text("call")),
                                ("callee", text(callee)),
                                ("typeArgs", CanonicalValue::Array(Vec::new())),
                                ("args", CanonicalValue::Array(Vec::new())),
                            ]),
                        ),
                    ]),
                ),
            ]);
            array_mut(field_mut(&mut exports, "pureFunctions")).push(pure_function_with_types(
                coordinate,
                &[],
                "U64",
                "edict",
                ("body", body),
            ));
        }
        let exports_bytes = encode_canonical_cbor(&exports).expect("encode exports");
        let manifest = hello_echo_manifest(digest_value(EXPORTS_COORDINATE, &exports));
        let manifest_bytes = encode_canonical_cbor(&manifest).expect("encode manifest");

        let failures = decode_lawpack_bundle(&manifest_bytes, &exports_bytes)
            .expect_err("recursive helper call graph must reject");

        assert_eq!(
            failure_kinds(&failures),
            vec![LawpackValidationFailureKind::InvalidPureFunctionBody]
        );
    }
}

#[test]
fn typed_digests_and_target_adapter_selectors_are_exact() {
    let exports = hello_echo_exports();
    let exports_bytes = encode_canonical_cbor(&exports).expect("encode exports");
    let mut malformed = hello_echo_manifest(digest_value(EXPORTS_COORDINATE, &exports));
    let digest = field_mut(field_mut(&mut malformed, "exports"), "digest");
    *digest = CanonicalValue::Array(vec![text("sha256"), CanonicalValue::Bytes(vec![0x01; 31])]);
    let failures = decode_lawpack_bundle(
        &encode_canonical_cbor(&malformed).expect("encode malformed manifest"),
        &exports_bytes,
    )
    .expect_err("short digest must reject");
    assert_eq!(
        failure_kinds(&failures),
        vec![LawpackValidationFailureKind::InvalidDigest]
    );

    let mut duplicate = hello_echo_manifest(digest_value(EXPORTS_COORDINATE, &exports));
    let adapters = array_mut(field_mut(&mut duplicate, "targetAdapters"));
    adapters.push(adapters[0].clone());
    let failures = decode_lawpack_bundle(
        &encode_canonical_cbor(&duplicate).expect("encode duplicate adapter manifest"),
        &exports_bytes,
    )
    .expect_err("duplicate exact target selector must reject");
    assert_eq!(
        failure_kinds(&failures),
        vec![LawpackValidationFailureKind::DuplicateIdentity]
    );
}

#[test]
fn operation_profile_optic_template_is_a_closed_typed_contract() {
    let mut exports = hello_echo_exports();
    let profiles = field_mut(&mut exports, "operationProfiles");
    let profile = first_map_value_mut(profiles);
    let optic = field_mut(profile, "opticTemplate");
    replace_field(
        field_mut(optic, "apertureRequirement"),
        "kind",
        text("whatever"),
    );
    let exports_bytes = encode_canonical_cbor(&exports).expect("encode exports");
    let manifest = hello_echo_manifest(digest_value(EXPORTS_COORDINATE, &exports));
    let failures = decode_lawpack_bundle(
        &encode_canonical_cbor(&manifest).expect("encode manifest"),
        &exports_bytes,
    )
    .expect_err("unknown aperture kind must reject");

    assert_eq!(
        failure_kinds(&failures),
        vec![LawpackValidationFailureKind::InvalidDiscriminant]
    );
}

#[test]
fn manifest_must_accept_the_supported_core_abi() {
    let exports = hello_echo_exports();
    let exports_bytes = encode_canonical_cbor(&exports).expect("encode exports");
    let mut manifest = hello_echo_manifest(digest_value(EXPORTS_COORDINATE, &exports));
    replace_field(
        &mut manifest,
        "acceptedCoreAbi",
        CanonicalValue::Array(vec![text("edict.core/v2")]),
    );
    let failures = decode_lawpack_bundle(
        &encode_canonical_cbor(&manifest).expect("encode manifest"),
        &exports_bytes,
    )
    .expect_err("unsupported Core ABI set must reject");

    assert_eq!(
        failure_kinds(&failures),
        vec![LawpackValidationFailureKind::MissingAcceptedCoreAbi]
    );
}

#[test]
fn effect_failure_names_must_be_source_mappable_identifiers() {
    for (identifier, expected) in [
        (
            "not-source-mappable",
            LawpackValidationFailureKind::InvalidFailureIdentifier,
        ),
        (
            "else",
            LawpackValidationFailureKind::ReservedFailureIdentifier,
        ),
    ] {
        let mut exports = hello_echo_exports();
        let effect = first_array_item_mut(field_mut(&mut exports, "effects"));
        let failures = field_mut(effect, "effectFailures");
        rename_only_map_key(failures, identifier);
        let exports_bytes = encode_canonical_cbor(&exports).expect("encode exports");
        let manifest = hello_echo_manifest(digest_value(EXPORTS_COORDINATE, &exports));
        let manifest_bytes = encode_canonical_cbor(&manifest).expect("encode manifest");
        let actual = decode_lawpack_bundle(&manifest_bytes, &exports_bytes)
            .expect_err("unmappable failure identifier must reject");

        assert_eq!(failure_kinds(&actual), vec![expected]);
    }
}

#[test]
fn duplicate_export_coordinates_reject_within_their_category() {
    let mut exports = hello_echo_exports();
    let effects = array_mut(field_mut(&mut exports, "effects"));
    effects.push(effects[0].clone());
    let exports_bytes = encode_canonical_cbor(&exports).expect("encode exports");
    let manifest = hello_echo_manifest(digest_value(EXPORTS_COORDINATE, &exports));
    let manifest_bytes = encode_canonical_cbor(&manifest).expect("encode manifest");
    let failures = decode_lawpack_bundle(&manifest_bytes, &exports_bytes)
        .expect_err("duplicate effect coordinate must reject");

    assert_eq!(
        failure_kinds(&failures),
        vec![LawpackValidationFailureKind::DuplicateIdentity]
    );
}

#[test]
fn callable_export_coordinates_must_be_disjoint() {
    let mut exports = hello_echo_exports();
    array_mut(field_mut(&mut exports, "pureFunctions")).push(pure_function(
        "hello.echo@1.createGreeting",
        "component",
        (
            "implementation",
            executable_component("hello.echo.colliding-helper/v1", 0x96),
        ),
    ));
    let exports_bytes = encode_canonical_cbor(&exports).expect("encode colliding exports");
    let manifest = hello_echo_manifest(digest_value(EXPORTS_COORDINATE, &exports));
    let manifest_bytes = encode_canonical_cbor(&manifest).expect("encode colliding manifest");

    let failures = decode_lawpack_bundle(&manifest_bytes, &exports_bytes)
        .expect_err("pure helper and semantic effect coordinates must be disjoint");

    assert_eq!(
        failure_kinds(&failures),
        vec![LawpackValidationFailureKind::DuplicateIdentity]
    );
}

#[test]
fn edict_pure_helper_body_must_match_the_closed_pure_core_schema() {
    let mut exports = hello_echo_exports();
    array_mut(field_mut(&mut exports, "pureFunctions")).push(map([
        ("coordinate", text("hello.echo@1.identity")),
        ("typeParameters", CanonicalValue::Array(Vec::new())),
        (
            "parameterTypes",
            CanonicalValue::Array(vec![text("hello.echo@1.GreetingKey")]),
        ),
        ("returnType", text("hello.echo@1.GreetingKey")),
        ("costTemplate", text("hello.echo@1.smallCreateBudget")),
        ("determinismClass", text("total")),
        ("source", text("edict")),
        ("body", map([("opaque", text("not Core"))])),
    ]));
    let exports_bytes = encode_canonical_cbor(&exports).expect("encode exports");
    let manifest = hello_echo_manifest(digest_value(EXPORTS_COORDINATE, &exports));
    let manifest_bytes = encode_canonical_cbor(&manifest).expect("encode manifest");
    let failures = decode_lawpack_bundle(&manifest_bytes, &exports_bytes)
        .expect_err("opaque pure helper body must reject");

    assert_eq!(
        failure_kinds(&failures),
        vec![LawpackValidationFailureKind::InvalidPureFunctionBody]
    );
}

#[test]
fn dependency_graph_requires_the_complete_exact_set_independent_of_input_order() {
    let dependency = bundle("hello.base", "1", &[]);
    let dependent = bundle(
        "hello.app",
        "1",
        &[("hello.base", "1", *dependency.manifest_digest())],
    );

    validate_lawpack_dependency_graph(&[dependent.clone(), dependency.clone()])
        .expect("reverse input order validates");
    validate_lawpack_dependency_graph(&[dependency, dependent]).expect("forward order validates");
}

#[test]
fn dependency_graph_rejects_missing_and_substituted_manifests() {
    let missing = bundle("hello.app", "1", &[("hello.base", "1", [0x99; 32])]);
    let failures =
        validate_lawpack_dependency_graph(&[missing]).expect_err("missing dependency must reject");
    assert_eq!(
        failure_kinds(&failures),
        vec![LawpackValidationFailureKind::MissingDependency]
    );

    let dependency = bundle("hello.base", "1", &[]);
    let substituted = bundle("hello.app", "1", &[("hello.base", "1", [0xaa; 32])]);
    let failures = validate_lawpack_dependency_graph(&[dependency, substituted])
        .expect_err("dependency digest substitution must reject");
    assert_eq!(
        failure_kinds(&failures),
        vec![LawpackValidationFailureKind::DependencyDigestMismatch]
    );
}

#[test]
fn dependency_graph_rejects_cycles_before_digest_corroboration() {
    let left = bundle("hello.left", "1", &[("hello.right", "1", [0x01; 32])]);
    let right = bundle("hello.right", "1", &[("hello.left", "1", [0x02; 32])]);
    let failures =
        validate_lawpack_dependency_graph(&[right, left]).expect_err("cycle must reject");

    assert_eq!(
        failure_kinds(&failures),
        vec![LawpackValidationFailureKind::DependencyCycle]
    );
}

fn hello_echo_manifest(exports_digest: [u8; 32]) -> CanonicalValue {
    manifest("hello.echo", "1", &[], exports_digest)
}

fn manifest(
    id: &str,
    version: &str,
    dependencies: &[(&str, &str, [u8; 32])],
    exports_digest: [u8; 32],
) -> CanonicalValue {
    let mut value = decode_canonical_cbor(MANIFEST_BYTES).expect("decode fixture manifest");
    replace_field(&mut value, "id", text(id));
    replace_field(&mut value, "version", text(version));
    replace_field(
        &mut value,
        "dependencies",
        CanonicalValue::Array(
            dependencies
                .iter()
                .map(|(id, version, digest)| {
                    map([
                        ("id", text(id)),
                        ("version", text(version)),
                        (
                            "digest",
                            CanonicalValue::Array(vec![
                                text("sha256"),
                                CanonicalValue::Bytes(digest.to_vec()),
                            ]),
                        ),
                    ])
                })
                .collect(),
        ),
    );
    let exports = field_mut(&mut value, "exports");
    replace_field(
        exports,
        "digest",
        CanonicalValue::Array(vec![
            text("sha256"),
            CanonicalValue::Bytes(exports_digest.to_vec()),
        ]),
    );
    value
}

fn bundle(
    id: &str,
    version: &str,
    dependencies: &[(&str, &str, [u8; 32])],
) -> ValidatedLawpackBundle {
    let exports = hello_echo_exports();
    let exports_bytes = encode_canonical_cbor(&exports).expect("encode exports");
    let manifest = manifest(
        id,
        version,
        dependencies,
        digest_value(EXPORTS_COORDINATE, &exports),
    );
    let manifest_bytes = encode_canonical_cbor(&manifest).expect("encode manifest");
    decode_lawpack_bundle(&manifest_bytes, &exports_bytes).expect("decode test bundle")
}

fn hello_echo_exports() -> CanonicalValue {
    decode_canonical_cbor(EXPORTS_BYTES).expect("decode fixture exports")
}

fn typed_helper_exports(include_type: bool) -> CanonicalValue {
    let identity_body = map([
        (
            "params",
            CanonicalValue::Array(vec![local_ref(
                "arg:0",
                "value",
                "hello.echo@1.GreetingKey",
            )]),
        ),
        (
            "body",
            map([
                ("locals", CanonicalValue::Array(Vec::new())),
                ("bindings", CanonicalValue::Array(Vec::new())),
                (
                    "result",
                    map([
                        ("kind", text("local")),
                        (
                            "ref",
                            local_ref("arg:0", "value", "hello.echo@1.GreetingKey"),
                        ),
                    ]),
                ),
            ]),
        ),
    ]);
    let mut exports = hello_echo_exports();
    if include_type {
        array_mut(field_mut(&mut exports, "types")).push(map([
            ("coordinate", text("hello.echo@1.GreetingKey")),
            ("definition", text("String<max=32,canonical=raw-utf8>")),
        ]));
    }
    array_mut(field_mut(&mut exports, "pureFunctions")).push(pure_function(
        "hello.echo@1.identityKey",
        "edict",
        ("body", identity_body),
    ));
    exports
}

fn structural_signature_types(exports: &mut CanonicalValue) {
    let types = array_mut(field_mut(exports, "types"));
    types.push(map([
        (
            "coordinate",
            text("hello.echo@1.StructuralEnvelope"),
        ),
        (
            "definition",
            text(
                "Record<metadata:Record<value:hello.echo@1.StructuralLeaf>,payload:List<hello.echo@1.StructuralLeaf,max=2>>",
            ),
        ),
    ]));
    types.push(map([
        ("coordinate", text("hello.echo@1.StructuralLeaf")),
        ("definition", text("U64")),
    ]));
}

fn structural_pure_helper_application() -> (
    amblicit_syntax::CoreModule,
    amblicit_syntax::TargetIrLoweringFacts,
) {
    let root = "hello.echo@1.StructuralEnvelope";
    let identity_body = pure_body(
        vec![local_ref("arg:0", "value", root)],
        map([
            ("kind", text("local")),
            ("ref", local_ref("arg:0", "value", root)),
        ]),
    );
    let mut exports = hello_echo_exports();
    structural_signature_types(&mut exports);
    array_mut(field_mut(&mut exports, "pureFunctions")).push(pure_function_with_types(
        "hello.echo@1.identityStructural",
        &[root],
        root,
        "edict",
        ("body", identity_body),
    ));
    let exports_bytes = encode_canonical_cbor(&exports).expect("encode structural helper exports");
    let manifest = hello_echo_manifest(digest_value(EXPORTS_COORDINATE, &exports));
    let manifest_bytes =
        encode_canonical_cbor(&manifest).expect("encode structural helper manifest");
    let bundle = decode_lawpack_bundle(&manifest_bytes, &exports_bytes)
        .expect("load structural helper lawpack");
    let source = format!(
        "package examples.structural_helper@1;\n\
         use lawpack hello.echo@1 digest \"{}\" as hello;\n\
         type Input = {{ value: hello.StructuralEnvelope, }};\n\
         type Output = {{ value: hello.StructuralEnvelope, }};\n\
         intent apply(input: Input) returns Output\n\
           profile hello.createGreeting\n\
           basis none\n\
           budget <= hello.smallCreateBudget {{\n\
           let value = hello.identityStructural(input.value);\n\
           return {{ value }};\n\
         }}",
        bundle.manifest_digest_review_string()
    );
    let module = parse_module(&source).expect("parse structural helper application");
    let adapter =
        decode_lawpack_adapter(&bundle, "echo.dpo@1", ADAPTER_BYTES).expect("load adapter");
    let preparation = prepare_lawpack_compilation(&module, &bundle, &adapter)
        .expect("derive structural helper facts");
    let core = compile_to_core(&module, preparation.compiler_context())
        .expect("compile structural helper application");
    (core, preparation.target_ir_facts().clone())
}

fn structural_effect_application() -> (
    amblicit_syntax::CoreModule,
    amblicit_syntax::TargetIrLoweringFacts,
) {
    let mut exports = hello_echo_exports();
    let types = array_mut(field_mut(&mut exports, "types"));
    replace_field(
        &mut types[0],
        "definition",
        text(
            "Record<metadata:Record<value:hello.echo@1.StructuralLeaf>,payload:List<hello.echo@1.StructuralLeaf,max=2>>",
        ),
    );
    replace_field(
        &mut types[1],
        "definition",
        text("Record<metadata:Record<value:hello.echo@1.StructuralLeaf>>"),
    );
    types.push(map([
        ("coordinate", text("hello.echo@1.StructuralLeaf")),
        ("definition", text("U64")),
    ]));
    let exports_bytes = encode_canonical_cbor(&exports).expect("encode structural effect exports");
    let manifest = hello_echo_manifest(digest_value(EXPORTS_COORDINATE, &exports));
    let manifest_bytes =
        encode_canonical_cbor(&manifest).expect("encode structural effect manifest");
    let bundle = decode_lawpack_bundle(&manifest_bytes, &exports_bytes)
        .expect("load structural effect lawpack");
    let source = format!(
        "package examples.structural_effect@1;\n\
         use lawpack hello.echo@1 digest \"{}\" as hello;\n\
         type Output = {{ value: hello.GreetingReceipt, }};\n\
         intent apply(input: hello.CreateGreetingInput) returns Output\n\
           profile hello.createGreeting\n\
           basis none\n\
           budget <= hello.smallCreateBudget {{\n\
           let value: hello.GreetingReceipt = hello.createGreeting(input)\n\
             else {{ alreadyExists(existing) => hello.AlreadyExists }};\n\
           return {{ value }};\n\
         }}",
        bundle.manifest_digest_review_string()
    );
    let module = parse_module(&source).expect("parse structural effect application");
    let adapter =
        decode_lawpack_adapter(&bundle, "echo.dpo@1", ADAPTER_BYTES).expect("load adapter");
    let preparation = prepare_lawpack_compilation(&module, &bundle, &adapter)
        .expect("derive structural effect facts");
    let core = compile_to_core(&module, preparation.compiler_context())
        .expect("compile structural effect application");
    (core, preparation.target_ir_facts().clone())
}

fn typed_helper_source(bundle: &ValidatedLawpackBundle) -> String {
    format!(
        "package examples.typed_helper@1;\n\
         use lawpack hello.echo@1 digest \"{}\" as hello;\n\
         type Input = {{ value: String<max=32>, }};\n\
         type Output = {{ value: String<max=32>, }};\n\
         intent apply(input: Input) returns Output\n\
           profile hello.createGreeting\n\
           basis none\n\
           budget <= hello.smallCreateBudget {{\n\
           let value = hello.identityKey(input.value);\n\
           return {{ value }};\n\
         }}",
        bundle.manifest_digest_review_string()
    )
}

fn typed_helper_list_source(manifest_digest: &str) -> String {
    format!(
        "package examples.typed_helper@1;\n\
         use lawpack hello.echo@1 digest \"{manifest_digest}\" as hello;\n\
         type Input = {{ values: List<String<max=32>, max=4>, }};\n\
         type Output = {{ values: List<String<max=32>, max=4>, }};\n\
         intent apply(input: Input) returns Output\n\
           profile hello.createGreeting\n\
           basis none\n\
           budget <= hello.smallCreateBudget {{\n\
           let values = hello.identityKey(input.values);\n\
           return {{ values }};\n\
         }}"
    )
}

fn request_only_adapter(
    budget_obligation: Option<&str>,
    include_target_configuration: bool,
) -> (CanonicalValue, CanonicalValue) {
    let mut exports = hello_echo_exports();
    array_mut(field_mut(&mut exports, "effects")).clear();
    let mut adapter = decode_canonical_cbor(ADAPTER_BYTES).expect("decode canonical adapter");
    let target_configuration = field_mut(
        first_map_value_mut(field_mut(&mut adapter, "effectImplementations")),
        "targetConfiguration",
    )
    .clone();
    map_mut(field_mut(&mut adapter, "effectImplementations")).clear();
    let profile = first_map_value_mut(field_mut(&mut adapter, "operationProfiles"));
    array_mut(field_mut(profile, "semanticEffects")).clear();
    if let Some(budget) = budget_obligation {
        insert_field(profile, "budgetObligation", text(budget));
    }
    if include_target_configuration {
        insert_field(profile, "targetConfiguration", target_configuration);
    }
    (exports, adapter)
}

fn resource_ref(id: &str, digest: [u8; 32]) -> CanonicalValue {
    map([
        ("id", text(id)),
        (
            "digest",
            CanonicalValue::Array(vec![text("sha256"), CanonicalValue::Bytes(digest.to_vec())]),
        ),
    ])
}

fn executable_component(id: &str, digest_byte: u8) -> CanonicalValue {
    map([
        ("component", resource_ref(id, [digest_byte; 32])),
        (
            "sandbox",
            resource_ref("edict.wasm-component/v1", [digest_byte.wrapping_add(1); 32]),
        ),
        (
            "fuelModel",
            resource_ref("edict.fuel/v1", [digest_byte.wrapping_add(2); 32]),
        ),
    ])
}

fn pure_function(
    coordinate: &str,
    source: &str,
    implementation: (&str, CanonicalValue),
) -> CanonicalValue {
    pure_function_with_types(
        coordinate,
        &["hello.echo@1.GreetingKey"],
        "hello.echo@1.GreetingKey",
        source,
        implementation,
    )
}

fn pure_function_with_types(
    coordinate: &str,
    parameter_types: &[&str],
    return_type: &str,
    source: &str,
    implementation: (&str, CanonicalValue),
) -> CanonicalValue {
    map([
        ("coordinate", text(coordinate)),
        ("typeParameters", CanonicalValue::Array(Vec::new())),
        (
            "parameterTypes",
            CanonicalValue::Array(parameter_types.iter().copied().map(text).collect()),
        ),
        ("returnType", text(return_type)),
        ("costTemplate", text("hello.echo@1.smallCreateBudget")),
        ("determinismClass", text("total")),
        ("source", text(source)),
        implementation,
    ])
}

fn pure_body(params: Vec<CanonicalValue>, result: CanonicalValue) -> CanonicalValue {
    map([
        ("params", CanonicalValue::Array(params)),
        (
            "body",
            map([
                ("locals", CanonicalValue::Array(Vec::new())),
                ("bindings", CanonicalValue::Array(Vec::new())),
                ("result", result),
            ]),
        ),
    ])
}

fn pure_helper_call(coordinate: &str) -> CanonicalValue {
    map([
        ("kind", text("call")),
        ("callee", text(coordinate)),
        ("typeArgs", CanonicalValue::Array(Vec::new())),
        ("args", CanonicalValue::Array(Vec::new())),
    ])
}

fn pure_helper_chain(coordinates: &[String]) -> Vec<CanonicalValue> {
    coordinates
        .iter()
        .enumerate()
        .map(|(index, coordinate)| {
            let result = coordinates.get(index + 1).map_or_else(
                || map([("kind", text("const")), ("value", core_u64(0))]),
                |callee| pure_helper_call(callee),
            );
            pure_function_with_types(
                coordinate,
                &[],
                "U64",
                "edict",
                ("body", pure_body(Vec::new(), result)),
            )
        })
        .collect()
}

fn pure_helper_graph_outcome(
    functions: Vec<CanonicalValue>,
) -> Result<(), Vec<LawpackValidationFailureKind>> {
    let mut exports = hello_echo_exports();
    array_mut(field_mut(&mut exports, "pureFunctions")).extend(functions);
    let exports_bytes = encode_canonical_cbor(&exports).expect("encode helper graph");
    let manifest = hello_echo_manifest(digest_value(EXPORTS_COORDINATE, &exports));
    let manifest_bytes = encode_canonical_cbor(&manifest).expect("encode helper graph manifest");
    decode_lawpack_bundle(&manifest_bytes, &exports_bytes)
        .map(|_| ())
        .map_err(|failures| failure_kinds(&failures))
}

fn core_u64(value: i128) -> CanonicalValue {
    map([
        ("kind", text("int")),
        ("width", text("U64")),
        ("value", CanonicalValue::Integer(value)),
    ])
}

fn local_ref(id: &str, alpha_name: &str, ty: &str) -> CanonicalValue {
    map([
        ("id", text(id)),
        ("alphaName", text(alpha_name)),
        ("type", text(ty)),
    ])
}

fn digest_value(domain: &str, value: &CanonicalValue) -> [u8; 32] {
    let framed = CanonicalValue::Array(vec![text(DIGEST_FRAME), text(domain), value.clone()]);
    let bytes = encode_canonical_cbor(&framed).expect("encode digest frame");
    Sha256::digest(bytes).into()
}

fn map<const N: usize>(entries: [(&str, CanonicalValue); N]) -> CanonicalValue {
    CanonicalValue::Map(
        entries
            .into_iter()
            .map(|(key, value)| (text(key), value))
            .collect(),
    )
}

fn text(value: &str) -> CanonicalValue {
    CanonicalValue::Text(value.to_owned())
}

fn failure_kinds(
    failures: &[amblicit_syntax::LawpackValidationFailure],
) -> Vec<LawpackValidationFailureKind> {
    failures.iter().map(|failure| failure.kind).collect()
}

fn adapter_failure_kinds(
    failures: &[amblicit_syntax::LawpackAdapterFailure],
) -> Vec<LawpackAdapterFailureKind> {
    failures.iter().map(|failure| failure.kind).collect()
}

fn bundle_with_adapter(adapter: &CanonicalValue) -> ValidatedLawpackBundle {
    let mut manifest = decode_canonical_cbor(MANIFEST_BYTES).expect("decode canonical manifest");
    let descriptor = first_array_item_mut(field_mut(&mut manifest, "targetAdapters"));
    replace_field(
        descriptor,
        "adapter",
        resource_ref(
            ADAPTER_COORDINATE,
            digest_value(ADAPTER_COORDINATE, adapter),
        ),
    );
    let manifest_bytes = encode_canonical_cbor(&manifest).expect("encode rebound manifest");
    decode_lawpack_bundle(&manifest_bytes, EXPORTS_BYTES).expect("load rebound lawpack")
}

fn bundle_with_exports_and_adapter(
    exports: &CanonicalValue,
    adapter: &CanonicalValue,
) -> ValidatedLawpackBundle {
    let exports_bytes = encode_canonical_cbor(exports).expect("encode rebound exports");
    let mut manifest = decode_canonical_cbor(MANIFEST_BYTES).expect("decode canonical manifest");
    replace_field(
        field_mut(&mut manifest, "exports"),
        "digest",
        CanonicalValue::Array(vec![
            text("sha256"),
            CanonicalValue::Bytes(digest_value(EXPORTS_COORDINATE, exports).to_vec()),
        ]),
    );
    let descriptor = first_array_item_mut(field_mut(&mut manifest, "targetAdapters"));
    replace_field(
        descriptor,
        "adapter",
        resource_ref(
            ADAPTER_COORDINATE,
            digest_value(ADAPTER_COORDINATE, adapter),
        ),
    );
    let manifest_bytes = encode_canonical_cbor(&manifest).expect("encode rebound manifest");
    decode_lawpack_bundle(&manifest_bytes, &exports_bytes).expect("load rebound lawpack")
}

fn bundle_and_adapter(
    exports: &CanonicalValue,
    adapter: &CanonicalValue,
) -> (
    ValidatedLawpackBundle,
    amblicit_syntax::ValidatedLawpackAdapter,
) {
    let bundle = bundle_with_exports_and_adapter(exports, adapter);
    let adapter_bytes = encode_canonical_cbor(adapter).expect("encode rebound adapter");
    let validated = decode_lawpack_adapter(&bundle, "echo.dpo@1", &adapter_bytes)
        .expect("decode rebound adapter");
    (bundle, validated)
}

fn insert_field(value: &mut CanonicalValue, field: &str, replacement: CanonicalValue) {
    map_mut(value).push((text(field), replacement));
}

fn replace_field(value: &mut CanonicalValue, field: &str, replacement: CanonicalValue) {
    let target = field_mut(value, field);
    *target = replacement;
}

fn remove_field(value: &mut CanonicalValue, field: &str) {
    let entries = map_mut(value);
    let index = entries
        .iter()
        .position(|(key, _value)| key == &text(field))
        .expect("fixture field");
    entries.remove(index);
}

fn field_mut<'a>(value: &'a mut CanonicalValue, field: &str) -> &'a mut CanonicalValue {
    map_mut(value)
        .iter_mut()
        .find_map(|(key, value)| (key == &text(field)).then_some(value))
        .expect("fixture field")
}

fn rename_only_map_key(value: &mut CanonicalValue, replacement: &str) {
    let entries = map_mut(value);
    assert_eq!(entries.len(), 1);
    entries[0].0 = text(replacement);
}

fn first_array_item_mut(value: &mut CanonicalValue) -> &mut CanonicalValue {
    array_mut(value).first_mut().expect("fixture array item")
}

fn first_map_value_mut(value: &mut CanonicalValue) -> &mut CanonicalValue {
    map_mut(value)
        .first_mut()
        .map(|(_key, value)| value)
        .expect("fixture map entry")
}

fn array_mut(value: &mut CanonicalValue) -> &mut Vec<CanonicalValue> {
    let CanonicalValue::Array(values) = value else {
        panic!("fixture array");
    };
    values
}

fn map_mut(value: &mut CanonicalValue) -> &mut Vec<(CanonicalValue, CanonicalValue)> {
    let CanonicalValue::Map(entries) = value else {
        panic!("fixture map");
    };
    entries
}

#[test]
fn effect_inputs_use_authenticated_contextual_types() {
    for (input_type, control_arg, literal_arg) in [
        ("U64", "input", "1"),
        ("Record<value:U64>", "{ value: input, }", "{ value: 1, }"),
    ] {
        let mut exports = hello_echo_exports();
        let effect = first_array_item_mut(field_mut(&mut exports, "effects"));
        replace_field(effect, "inputType", text(input_type));
        replace_field(effect, "outputType", text("U64"));
        let (bundle, adapter) = bundle_and_adapter(
            &exports,
            &decode_canonical_cbor(ADAPTER_BYTES).expect("decode adapter"),
        );
        let source = format!(
            "package examples.literal_effect@1;\nuse lawpack hello.echo@1 digest \"{}\" as hello;\nintent apply(input: U64) returns U64\n profile hello.createGreeting\n basis none\n budget <= hello.smallCreateBudget {{\n let receipt: U64 = hello.createGreeting({control_arg})\n else {{ alreadyExists(existing) => hello.AlreadyExists }};\n return receipt;\n}}",
            bundle.manifest_digest_review_string()
        );
        let compile = |source: &str| {
            let module = parse_module(source).expect("contextual source parses");
            let prepared = prepare_lawpack_compilation(&module, &bundle, &adapter)
                .expect("authenticate contextual signature");
            compile_to_core(&module, prepared.compiler_context())
        };
        compile(&source).expect("typed input control compiles");
        let literal = source.replace(
            &format!("hello.createGreeting({control_arg})"),
            &format!("hello.createGreeting({literal_arg})"),
        );
        compile(&literal).expect("signature supplies integer literal context");
        let mapping = "\n else { alreadyExists(existing) => hello.AlreadyExists }";
        assert_eq!(literal.matches(mapping).count(), 1);
        let bare = literal.replace(mapping, "");
        let errors = compile(&bare).expect_err("valid bare effect still needs mapping");
        assert_eq!(
            errors[0].kind,
            CompilerErrorKind::EffectWithoutFailureMapping
        );
    }
}

#[test]
fn effect_comparison_literals_follow_exported_numeric_output() {
    for (width, valid, invalid) in [("U64", "1", "-1"), ("I64", "-1", "9223372036854775808")] {
        let mut exports = hello_echo_exports();
        let effect = first_array_item_mut(field_mut(&mut exports, "effects"));
        replace_field(effect, "inputType", text("U64"));
        replace_field(effect, "outputType", text(width));
        let (bundle, adapter) = bundle_and_adapter(
            &exports,
            &decode_canonical_cbor(ADAPTER_BYTES).expect("adapter"),
        );
        let base = format!("package examples.numeric_compare@1;\nuse lawpack hello.echo@1 digest \"{}\" as hello;\nintent apply(input: U64) returns {width}\n profile hello.createGreeting\n basis none\n budget <= hello.smallCreateBudget {{\n let receipt: {width} = hello.createGreeting(input)\n else {{ alreadyExists(existing) => hello.AlreadyExists }};\n require receipt == {valid} else hello.AlreadyExists;\n return receipt;\n}}", bundle.manifest_digest_review_string());
        let compile = |source: &str| {
            let module = parse_module(source).expect("numeric comparison source");
            let prepared =
                prepare_lawpack_compilation(&module, &bundle, &adapter).expect("numeric signature");
            compile_to_core(&module, prepared.compiler_context())
        };
        compile(&base).expect("mapped numeric comparison control compiles");
        for (literal, expected) in [
            (valid, CompilerErrorKind::EffectWithoutFailureMapping),
            (invalid, CompilerErrorKind::TypeMismatch),
        ] {
            for comparison in [
                format!("hello.createGreeting(input) == {literal}"),
                format!("{literal} == hello.createGreeting(input)"),
            ] {
                let source = base.replace(&format!("receipt == {valid}"), &comparison);
                let errors = compile(&source).expect_err("bare effect or bad literal rejects");
                assert_eq!(
                    errors[0].kind, expected,
                    "{width}: {comparison}: {errors:?}"
                );
            }
        }
    }
}

#[test]
fn authenticated_empty_effect_surface_is_not_missing_information() {
    let (exports, adapter) = request_only_adapter(Some("hello.echo@1.smallCreateBudget"), true);
    let (bundle, adapter) = bundle_and_adapter(&exports, &adapter);
    let source = CREATE_GREETING_SOURCE.replace(
        MANIFEST_DIGEST.trim(),
        &bundle.manifest_digest_review_string(),
    );
    let module = parse_module(&source).expect("empty-surface fixture parses");
    let prepared = prepare_lawpack_compilation(&module, &bundle, &adapter)
        .expect("authenticate empty effect surface");
    let errors = compile_to_core(&module, prepared.compiler_context())
        .expect_err("requested effect is not exported");
    assert_eq!(errors[0].kind, CompilerErrorKind::MissingContextFact);
    assert!(errors[0]
        .message
        .contains("no authenticated effect exports"));
    assert!(!errors[0]
        .message
        .contains("missing authenticated effect export information"));
}

#[test]
fn failure_guidance_names_only_domain_mappable_coordinates() {
    for class in [
        "participantOwned",
        "integrityFault",
        "resourceFault",
        "internalFault",
    ] {
        let mut exports = hello_echo_exports();
        let effect = first_array_item_mut(field_mut(&mut exports, "effects"));
        let failure = first_map_value_mut(field_mut(effect, "effectFailures"));
        replace_field(failure, "authorityClass", text(class));
        let obstruction = first_array_item_mut(field_mut(&mut exports, "obstructions"));
        replace_field(obstruction, "authorityClass", text(class));
        let (bundle, adapter) = bundle_and_adapter(
            &exports,
            &decode_canonical_cbor(ADAPTER_BYTES).expect("adapter bytes"),
        );
        let source = CREATE_GREETING_SOURCE.replace(
            MANIFEST_DIGEST.trim(),
            &bundle.manifest_digest_review_string(),
        );
        let mapping = "\n    else { alreadyExists(existing) => hello.AlreadyExists }";
        assert_eq!(source.matches(mapping).count(), 1);
        let bare = source.replace(mapping, "");
        let module = parse_module(&bare).expect("non-domain source parses");
        let prepared = prepare_lawpack_compilation(&module, &bundle, &adapter)
            .expect("authenticate failure authority");
        let errors = compile_to_core(&module, prepared.compiler_context())
            .expect_err("no-map bindings remain unsupported");
        assert_eq!(
            errors[0].kind,
            CompilerErrorKind::EffectWithoutFailureMapping
        );
        assert!(
            errors[0].message.contains("no domain-mappable failures"),
            "{class}: {errors:?}"
        );
        assert!(!errors[0]
            .message
            .contains("declared failures: alreadyExists"));
        assert!(!errors[0].message.contains("with `else`"));
    }
}

#[test]
fn mixed_authority_failure_guidance_labels_only_mappable_names() {
    for class in [
        "participantOwned",
        "integrityFault",
        "resourceFault",
        "internalFault",
    ] {
        let mut exports = hello_echo_exports();
        let effect = first_array_item_mut(field_mut(&mut exports, "effects"));
        let failures = field_mut(effect, "effectFailures");
        let mut extra = first_map_value_mut(failures).clone();
        replace_field(&mut extra, "authorityClass", text(class));
        map_mut(failures).push((text("platformFailure"), extra));
        let mut adapter = decode_canonical_cbor(ADAPTER_BYTES).expect("adapter");
        let implementation = first_map_value_mut(field_mut(&mut adapter, "effectImplementations"));
        let mappings = field_mut(implementation, "failureMappings");
        let CanonicalValue::Text(original_target) = first_map_value_mut(mappings) else {
            panic!("target coordinate")
        };
        let target = format!("{original_target}.platform");
        map_mut(mappings).push((text("platformFailure"), text(&target)));
        let (bundle, adapter) = bundle_and_adapter(&exports, &adapter);
        let source = CREATE_GREETING_SOURCE.replace(
            MANIFEST_DIGEST.trim(),
            &bundle.manifest_digest_review_string(),
        );
        let mapping = "\n    else { alreadyExists(existing) => hello.AlreadyExists }";
        assert_eq!(source.matches(mapping).count(), 1);
        let bare = source.replace(mapping, "");
        let module = parse_module(&bare).expect("mixed-authority source");
        let prepared = prepare_lawpack_compilation(&module, &bundle, &adapter)
            .expect("authenticate mixed failures");
        let errors = compile_to_core(&module, prepared.compiler_context())
            .expect_err("bare binding needs map");
        assert_eq!(
            errors[0].kind,
            CompilerErrorKind::EffectWithoutFailureMapping
        );
        assert!(errors[0].message.contains("alreadyExists"));
        assert!(!errors[0].message.contains("platformFailure"));
        assert!(errors[0].message.contains("domain-mappable failures:"));
    }
}

#[test]
fn linked_modules_prepare_each_authenticated_lawpack_alias() {
    use amblicit_syntax::source_closure::{
        resolve_source_closure, source_module_digest, SourceClosureLimits,
    };
    use amblicit_syntax::source_linker::link_source_closure;
    let mut exports = typed_helper_exports(true);
    // The operation budget must contain the helper's separate cost allowance.
    for helper in array_mut(field_mut(&mut exports, "pureFunctions")) {
        replace_field(helper, "costTemplate", text("hello.echo@1.identityBudget"));
    }
    let adapter_value = exact_u64_pure_helper_adapter();
    let (bundle, adapter) = bundle_and_adapter(&exports, &adapter_value);
    let digest = bundle.manifest_digest_review_string();
    let library = format!("package library.keys@1; use lawpack hello.echo@1 digest \"{digest}\" as local; fn identity(value: local.GreetingKey) -> local.GreetingKey {{ return local.identityKey(value); }}");
    let pin = source_module_digest(&library).expect("module pin");
    let source = typed_helper_source(&bundle)
        .replacen(
            "type Input",
            &format!("use module library.keys@1 digest \"{pin}\" as keys; type Input"),
            1,
        )
        .replace(
            "hello.identityKey(input.value)",
            "keys.identity(input.value)",
        );
    let closure = resolve_source_closure(&source, &[&library], SourceClosureLimits::default())
        .expect("closure");
    let linked = link_source_closure(&closure).expect("link");
    let preparation = prepare_lawpack_compilation(linked.module(), &bundle, &adapter)
        .expect("all linked aliases prepare");
    let core = linked
        .compile(preparation.compiler_context())
        .expect("compile imported helper and type");
    let target = lower_to_target_ir(&core, preparation.target_ir_facts());
    assert_eq!(target.status, TargetLoweringStatus::Lowered, "{target:?}");
    assert_eq!(
        preparation.target_ir_facts().pure_functions.len(),
        bundle.exports().pure_functions.len(),
        "canonical helper facts must not duplicate per alias"
    );
    // A matching first import must never hide a substituted later import.
    let mut substituted = linked.module().clone();
    substituted
        .imports
        .last_mut()
        .expect("library import")
        .digest = Some(format!("sha256:{}", "0".repeat(64)));
    let errors = prepare_lawpack_compilation(&substituted, &bundle, &adapter)
        .expect_err("authenticate every alias");
    assert_eq!(
        adapter_failure_kinds(&errors),
        vec![LawpackAdapterFailureKind::SourceImportMismatch]
    );
}

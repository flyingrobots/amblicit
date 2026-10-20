//! RED contract for typed external-action request values.
//!
//! The tests use only public compiler surfaces so the first failure is the
//! absence of request syntax/semantics, not a test compile error.

use std::{collections::BTreeSet, fmt::Write as _};

use edict_syntax::{
    compile_to_core, decode_canonical_cbor, digest_core_module, encode_core_module,
    encode_target_ir_artifact, lower_to_target_ir, parse_module,
    validate_core_module_type_integrity, CanonicalValue, CompilerContext, CompilerErrorKind,
    CoreBlock, CoreBound, CoreBudget, CoreExpr, CoreNode, CoreTypeIntegrityFailureKind, CoreValue,
    LocalRef, ResourceRef, TargetIrLoweringFacts, TargetLoweringFailureKind, TargetLoweringStatus,
    WriteClass, ECHO_DPO_TARGET_PROFILE, ECHO_SPAN_IR_DOMAIN, MAX_CANONICAL_NESTING_DEPTH,
};

const OPERATION_DIGEST: char = 'a';
const INPUT_SCHEMA_DIGEST: char = 'b';
const SETTLEMENT_SCHEMA_DIGEST: char = 'c';
const RECONCILIATION_DIGEST: char = 'd';
const TARGET_PROFILE_DIGEST: char = 'e';
const PROPERTY_SEED: u64 = 0x4558_5452_4551_0001;

fn digest(hex: char) -> String {
    format!("sha256:{}", hex.to_string().repeat(64))
}

fn context() -> CompilerContext {
    CompilerContext::new()
        .with_operation_profile("workspace.read", "continuum.profile.read-only/v1")
        .with_budget(
            "workspace.tiny",
            CoreBudget {
                max_steps: 512,
                max_allocated_bytes: 256 * 1024,
                max_output_bytes: 128 * 1024,
            },
        )
}

fn target_facts() -> TargetIrLoweringFacts {
    TargetIrLoweringFacts {
        target_profile: ResourceRef {
            coordinate: ECHO_DPO_TARGET_PROFILE.to_owned(),
            digest: Some(digest(TARGET_PROFILE_DIGEST)),
        },
        target_ir_domain: ECHO_SPAN_IR_DOMAIN.to_owned(),
        operation_profiles: vec!["continuum.profile.read-only/v1".to_owned()],
        obstruction_coordinates: Vec::new(),
        effect_lowerings: Vec::new(),
        effect_signatures: Vec::new(),
        pure_functions: Vec::new(),
    }
}

struct RequestSource<'a> {
    capability_coordinate: &'a str,
    operation_alias: &'a str,
    operation_digest: &'a str,
    input_schema_coordinate: &'a str,
    input_schema_digest: &'a str,
    settlement_schema_coordinate: &'a str,
    settlement_schema_digest: &'a str,
    input_expr: &'a str,
    authority_expr: &'a str,
    basis_expr: &'a str,
    max_settlement_bytes: &'a str,
    max_attempts: &'a str,
    reconciliation_coordinate: &'a str,
    reconciliation_digest: &'a str,
}

fn request_source(request: &RequestSource<'_>) -> String {
    let RequestSource {
        capability_coordinate,
        operation_alias,
        operation_digest,
        input_schema_coordinate,
        input_schema_digest,
        settlement_schema_coordinate,
        settlement_schema_digest,
        input_expr,
        authority_expr,
        basis_expr,
        max_settlement_bytes,
        max_attempts,
        reconciliation_coordinate,
        reconciliation_digest,
    } = request;
    format!(
        r#"package examples.workspace_observer@1;

use capability {capability_coordinate} digest "{operation_digest}" as {operation_alias};

type ObserveInput = {{
  payload: Bytes<max=1024>,
  alternatePayload: Bytes<max=1024>,
  scope: Bytes<max=32>,
  alternateScope: Bytes<max=32>,
  basis: Bytes<max=32>,
  alternateBasis: Bytes<max=32>,
  maxSettlementBytes: U64,
  alternateMaxSettlementBytes: U64,
  maxAttempts: U32,
  alternateMaxAttempts: U32,
}};

intent observe(input: ObserveInput) returns ExternalActionRequest<Bytes<max=65536>>
  profile workspace.read
  basis input.basis
  budget <= workspace.tiny
{{
  request pending: ExternalActionRequest<Bytes<max=65536>> =
    {operation_alias}({input_expr})
    input schema {input_schema_coordinate} digest "{input_schema_digest}"
    settlement schema {settlement_schema_coordinate} digest "{settlement_schema_digest}"
    authority {authority_expr}
    basis {basis_expr}
    budget maxSettlementBytes {max_settlement_bytes} maxAttempts {max_attempts}
    reconcile {reconciliation_coordinate} digest "{reconciliation_digest}";
  return pending;
}}
"#
    )
}

fn baseline_source() -> String {
    request_source(&RequestSource {
        capability_coordinate: "workspace.snapshot.observe@1",
        operation_alias: "snapshot",
        operation_digest: &digest(OPERATION_DIGEST),
        input_schema_coordinate: "workspace.snapshot.input@1",
        input_schema_digest: &digest(INPUT_SCHEMA_DIGEST),
        settlement_schema_coordinate: "workspace.snapshot.settlement@1",
        settlement_schema_digest: &digest(SETTLEMENT_SCHEMA_DIGEST),
        input_expr: "input.payload",
        authority_expr: "input.scope",
        basis_expr: "input.basis",
        max_settlement_bytes: "input.maxSettlementBytes",
        max_attempts: "input.maxAttempts",
        reconciliation_coordinate: "workspace.snapshot.reconcile@1",
        reconciliation_digest: &digest(RECONCILIATION_DIGEST),
    })
}

fn validated_patch_source() -> String {
    request_source(&RequestSource {
        capability_coordinate: "workspace.patch.applyValidated@1",
        operation_alias: "patch",
        operation_digest: &digest('9'),
        input_schema_coordinate: "workspace.patch.input@1",
        input_schema_digest: &digest('8'),
        settlement_schema_coordinate: "workspace.patch.settlement@1",
        settlement_schema_digest: &digest('7'),
        input_expr: "input.payload",
        authority_expr: "input.scope",
        basis_expr: "input.basis",
        max_settlement_bytes: "input.maxSettlementBytes",
        max_attempts: "input.maxAttempts",
        reconciliation_coordinate: "workspace.patch.reconcile@1",
        reconciliation_digest: &digest('6'),
    })
}

fn compile_source(source: &str) -> edict_syntax::CoreModule {
    let module = parse_module(source).expect("external-action source parses");
    compile_to_core(&module, &context()).expect("external-action source compiles")
}

fn lower_source(source: &str) -> (edict_syntax::CoreModule, edict_syntax::TargetIrArtifact) {
    let core = compile_source(source);
    let report = lower_to_target_ir(&core, &target_facts());
    assert_eq!(report.status, TargetLoweringStatus::Lowered);
    assert_eq!(report.failures, Vec::new());
    (core, report.artifact.expect("external-action Target IR"))
}

#[test]
fn external_request_expressions_require_closed_helper_authority() {
    let core = compile_source(&baseline_source());
    let control = lower_to_target_ir(&core, &target_facts());
    assert_eq!(control.status, TargetLoweringStatus::Lowered);
    assert_eq!(
        control.failures,
        [] as [edict_syntax::TargetLoweringFailure; 0]
    );
    assert!(control.artifact.is_some());

    let unbound_call = || CoreExpr::Call {
        callee: "workspace.snapshot@1.notExported".to_owned(),
        type_args: Vec::new(),
        args: Vec::new(),
    };

    for field in [
        "input",
        "authorityScope",
        "basis",
        "maxSettlementBytes",
        "maxAttempts",
    ] {
        let mut changed = core.clone();
        let request = changed
            .intents
            .get_mut("observe")
            .expect("observe intent")
            .body
            .nodes
            .first_mut()
            .expect("request node");
        let CoreNode::ExternalActionRequest {
            input,
            authority_scope,
            basis,
            budget,
            ..
        } = request
        else {
            panic!("first node is an external-action request");
        };
        match field {
            "input" => *input = unbound_call(),
            "authorityScope" => **authority_scope = unbound_call(),
            "basis" => **basis = unbound_call(),
            "maxSettlementBytes" => budget.max_settlement_bytes = unbound_call(),
            "maxAttempts" => budget.max_attempts = unbound_call(),
            _ => unreachable!("bounded request-expression corpus"),
        }

        let report = lower_to_target_ir(&changed, &target_facts());

        assert_eq!(report.status, TargetLoweringStatus::Unsupported, "{field}");
        assert!(report.artifact.is_none(), "{field}");
        let [failure] = report.failures.as_slice() else {
            panic!("{field} must reject with one structured failure");
        };
        assert_eq!(
            failure.kind,
            TargetLoweringFailureKind::InvalidCoreIdentity,
            "{field}"
        );
    }
}

#[test]
fn external_request_settlement_type_must_match_binding() {
    let mut core = compile_source(&baseline_source());
    let request = core
        .intents
        .get_mut("observe")
        .expect("observe intent")
        .body
        .nodes
        .first_mut()
        .expect("request node");
    let CoreNode::ExternalActionRequest {
        binding,
        settlement_type,
        ..
    } = request
    else {
        panic!("first node is an external-action request");
    };
    assert_ne!(binding.ty, "ExternalActionRequest<U64>");
    *settlement_type = "U64".to_owned();

    let report = lower_to_target_ir(&core, &target_facts());

    assert_eq!(report.status, TargetLoweringStatus::Unsupported);
    assert!(report.artifact.is_none());
    let [failure] = report.failures.as_slice() else {
        panic!("settlement mismatch must reject with one structured failure");
    };
    assert_eq!(failure.kind, TargetLoweringFailureKind::InvalidCoreIdentity);
}

fn map_field<'a>(value: &'a CanonicalValue, field: &str) -> &'a CanonicalValue {
    let CanonicalValue::Map(entries) = value else {
        panic!("expected map while looking up {field:?}, got {value:?}");
    };
    entries
        .iter()
        .find_map(|(key, value)| match key {
            CanonicalValue::Text(key) if key == field => Some(value),
            _ => None,
        })
        .unwrap_or_else(|| panic!("missing canonical field {field:?}"))
}

fn text_field<'a>(value: &'a CanonicalValue, field: &str) -> &'a str {
    let CanonicalValue::Text(value) = map_field(value, field) else {
        panic!("canonical field {field:?} is not text");
    };
    value
}

fn array_field<'a>(value: &'a CanonicalValue, field: &str) -> &'a [CanonicalValue] {
    let CanonicalValue::Array(values) = map_field(value, field) else {
        panic!("canonical field {field:?} is not an array");
    };
    values
}

fn core_request_node(core: &edict_syntax::CoreModule) -> CanonicalValue {
    let encoded = encode_core_module(core).expect("Core request encodes");
    let value = decode_canonical_cbor(&encoded).expect("Core request decodes");
    let intent = map_field(map_field(&value, "intents"), "observe");
    array_field(map_field(intent, "body"), "nodes")[0].clone()
}

fn target_request_values(target: &edict_syntax::TargetIrArtifact) -> Vec<CanonicalValue> {
    let encoded = encode_target_ir_artifact(target).expect("Target IR request encodes");
    let value = decode_canonical_cbor(&encoded).expect("Target IR request decodes");
    let intent = map_field(map_field(&value, "intents"), "observe");
    array_field(intent, "externalActionRequests").to_vec()
}

fn assert_application_input_field(expr: &CoreExpr, expected_field: &str) {
    let CoreExpr::Field { base, field } = expr else {
        panic!("expected application-input field expression, got {expr:?}");
    };
    assert_eq!(field, expected_field);
    let CoreExpr::Local { reference } = base.as_ref() else {
        panic!("expected application-input local base, got {base:?}");
    };
    assert_eq!(reference.id, "arg.0");
}

#[test]
fn workspace_observation_request_compiles_as_non_callable_data() {
    let (core, target) = lower_source(&baseline_source());
    let core_value = decode_canonical_cbor(&encode_core_module(&core).expect("Core encodes"))
        .expect("Core CBOR");
    let imports = array_field(&core_value, "imports");
    assert_eq!(imports.len(), 1);
    assert_eq!(text_field(&imports[0], "kind"), "capability");
    assert_eq!(
        text_field(map_field(&imports[0], "ref"), "id"),
        "workspace.snapshot.observe@1"
    );

    let request = core_request_node(&core);
    assert_eq!(text_field(&request, "kind"), "externalActionRequest");
    assert_eq!(text_field(&request, "inputType"), "Bytes<max=1024>");
    assert_eq!(text_field(&request, "settlementType"), "Bytes<max=65536>");
    assert_eq!(text_field(&request, "state"), "awaitingSettlement");
    assert_eq!(
        text_field(&request, "settlementAdmission"),
        "schemaRequired"
    );

    let target_bytes = encode_target_ir_artifact(&target).expect("Target IR encodes");
    let target_value = decode_canonical_cbor(&target_bytes).expect("Target IR CBOR");
    let closure = map_field(&target_value, "semanticClosure");
    let capabilities = array_field(closure, "capabilities");
    assert_eq!(capabilities.len(), 1);
    assert_eq!(
        text_field(&capabilities[0], "id"),
        "workspace.snapshot.observe@1"
    );
    let intent = map_field(map_field(&target_value, "intents"), "observe");
    assert_eq!(array_field(intent, "steps"), []);
    let requests = array_field(intent, "externalActionRequests");
    assert_eq!(requests.len(), 1);
    assert_eq!(
        text_field(map_field(&requests[0], "operation"), "id"),
        "workspace.snapshot.observe@1"
    );
    assert_eq!(
        text_field(&requests[0], "settlementAdmission"),
        "schemaRequired"
    );
    assert!(!target_bytes
        .windows("targetIntrinsic".len())
        .any(|window| window == b"targetIntrinsic"));
}

#[test]
fn validated_patch_request_compiles_as_non_callable_data() {
    let (core, target) = lower_source(&validated_patch_source());
    let request = core
        .intents
        .get("observe")
        .expect("patch intent exists")
        .body
        .nodes
        .first()
        .expect("patch request exists");
    let CoreNode::ExternalActionRequest {
        binding,
        operation,
        input_type,
        settlement_type,
        input_schema,
        settlement_schema,
        input,
        authority_scope,
        basis,
        budget,
        reconciliation_law,
    } = request
    else {
        panic!("first patch node is an external-action request");
    };
    assert_eq!(
        operation,
        &ResourceRef {
            coordinate: "workspace.patch.applyValidated@1".to_owned(),
            digest: Some(digest('9')),
        }
    );
    assert_eq!(input_type, "Bytes<max=1024>");
    assert_eq!(settlement_type, "Bytes<max=65536>");
    assert_eq!(
        input_schema,
        &ResourceRef {
            coordinate: "workspace.patch.input@1".to_owned(),
            digest: Some(digest('8')),
        }
    );
    assert_eq!(
        settlement_schema,
        &ResourceRef {
            coordinate: "workspace.patch.settlement@1".to_owned(),
            digest: Some(digest('7')),
        }
    );
    assert_eq!(
        reconciliation_law,
        &ResourceRef {
            coordinate: "workspace.patch.reconcile@1".to_owned(),
            digest: Some(digest('6')),
        }
    );
    assert_application_input_field(input, "payload");
    assert_application_input_field(authority_scope, "scope");
    assert_application_input_field(basis, "basis");
    assert_application_input_field(&budget.max_settlement_bytes, "maxSettlementBytes");
    assert_application_input_field(&budget.max_attempts, "maxAttempts");

    let intent = target.intents.get("observe").expect("patch intent lowers");
    assert_eq!(intent.steps, [] as [edict_syntax::TargetIrStep; 0]);
    assert_eq!(intent.external_action_requests.len(), 1);
    let target_request = &intent.external_action_requests[0];
    assert_eq!(target_request.id, "observe.request.0");
    assert_eq!(&target_request.binding, binding);
    assert_eq!(&target_request.operation, operation);
    assert_eq!(&target_request.input_type, input_type);
    assert_eq!(&target_request.settlement_type, settlement_type);
    assert_eq!(&target_request.input_schema, input_schema);
    assert_eq!(&target_request.settlement_schema, settlement_schema);
    assert_eq!(&target_request.input, input);
    assert_eq!(&target_request.authority_scope, authority_scope.as_ref());
    assert_eq!(&target_request.basis, basis.as_ref());
    assert_eq!(&target_request.budget, budget.as_ref());
    assert_eq!(&target_request.reconciliation_law, reconciliation_law);
}

#[test]
fn undeclared_or_floating_operation_families_fail_closed() {
    let undeclared = baseline_source().replace("snapshot(input.payload)", "missing(input.payload)");
    let module = parse_module(&undeclared).expect("undeclared operation source parses");
    let errors = compile_to_core(&module, &context()).expect_err("undeclared operation rejects");
    assert_eq!(errors[0].kind, CompilerErrorKind::MissingContextFact);

    let floating =
        baseline_source().replace(&format!(" digest \"{}\"", digest(OPERATION_DIGEST)), "");
    let module = parse_module(&floating).expect("floating capability source parses");
    assert_eq!(
        compile_to_core(&module, &context())
            .expect_err("floating operation rejects during compilation")[0]
            .kind,
        CompilerErrorKind::MissingContextFact
    );
}

#[test]
fn request_operation_must_remain_in_core_and_target_capability_closure() {
    let (mut core, mut target) = lower_source(&baseline_source());
    core.imports.clear();
    assert_eq!(
        encode_core_module(&core)
            .expect_err("Core request without capability import rejects")
            .kind(),
        edict_syntax::CanonicalErrorKind::UnsupportedValue
    );

    target
        .semantic_closure
        .as_mut()
        .expect("request-bearing Target IR has a closure")
        .capabilities
        .clear();
    assert_eq!(
        encode_target_ir_artifact(&target)
            .expect_err("Target IR request without capability closure rejects")
            .kind(),
        edict_syntax::CanonicalErrorKind::UnsupportedValue
    );
}

#[test]
fn nested_request_validation_distinguishes_graph_depth_and_closure_failures() {
    let nested_core = |loop_count: usize| {
        let mut core = compile_source(&baseline_source());
        core.imports.clear();
        let intent = core
            .intents
            .get_mut("observe")
            .expect("observe intent exists");
        let mut request = intent.body.nodes.remove(0);
        let CoreNode::ExternalActionRequest {
            input,
            authority_scope,
            basis,
            budget,
            ..
        } = &mut request
        else {
            panic!("request node");
        };
        // Atomic operands isolate block depth from expression nesting. This
        // fixture exercises integrity and closure, not request operand typing.
        for operand in [
            input,
            authority_scope,
            basis,
            &mut budget.max_settlement_bytes,
            &mut budget.max_attempts,
        ] {
            *operand = CoreExpr::Const(CoreValue::Null);
        }
        let mut nested = CoreBlock {
            locals: Vec::new(),
            nodes: vec![request],
            result: CoreExpr::Const(CoreValue::Null),
        };
        for depth in 0..loop_count {
            nested = CoreBlock {
                locals: Vec::new(),
                nodes: vec![CoreNode::For {
                    binder: LocalRef {
                        id: format!("nested.{depth}"),
                        alpha_name: format!("$nested{depth}"),
                        ty: "U64".to_owned(),
                    },
                    iter: CoreExpr::Const(CoreValue::Null),
                    bound: CoreBound::Literal(0),
                    body: nested,
                }],
                result: CoreExpr::Const(CoreValue::Null),
            };
        }
        intent.body = nested;
        core
    };

    let at_limit = nested_core(MAX_CANONICAL_NESTING_DEPTH);
    validate_core_module_type_integrity(&at_limit).expect("at-limit graph passes Core integrity");
    assert_eq!(
        encode_core_module(&at_limit)
            .expect_err("at-limit traversal reaches request closure validation")
            .kind(),
        edict_syntax::CanonicalErrorKind::UnsupportedValue
    );

    let beyond = nested_core(MAX_CANONICAL_NESTING_DEPTH + 1);
    let failure = validate_core_module_type_integrity(&beyond).unwrap_err();
    assert_eq!(failure.kind(), CoreTypeIntegrityFailureKind::DepthExceeded);
    assert_eq!(
        failure.path(),
        format!(
            "intents.observe.body{}",
            ".nodes[0].for.body".repeat(MAX_CANONICAL_NESTING_DEPTH + 1)
        )
    );
    assert_eq!(
        encode_core_module(&beyond)
            .expect_err("excessive nested request traversal rejects before closure checks")
            .kind(),
        edict_syntax::CanonicalErrorKind::UnsupportedValue
    );
}

#[test]
fn request_resource_coordinates_must_be_nonempty() {
    let core = compile_source(&baseline_source());

    for field in ["inputSchema", "settlementSchema", "reconciliationLaw"] {
        let mut changed = core.clone();
        let request = changed
            .intents
            .get_mut("observe")
            .expect("observe intent exists")
            .body
            .nodes
            .first_mut()
            .expect("request node exists");
        let CoreNode::ExternalActionRequest {
            input_schema,
            settlement_schema,
            reconciliation_law,
            ..
        } = request
        else {
            panic!("first node is an external-action request");
        };
        match field {
            "inputSchema" => input_schema.coordinate.clear(),
            "settlementSchema" => settlement_schema.coordinate.clear(),
            "reconciliationLaw" => reconciliation_law.coordinate.clear(),
            _ => unreachable!("bounded field corpus"),
        }

        assert_eq!(
            encode_core_module(&changed)
                .expect_err("empty request resource coordinate rejects")
                .kind(),
            edict_syntax::CanonicalErrorKind::UnsupportedValue,
            "{field}"
        );
    }
}

#[test]
fn duplicate_target_request_ids_reject_before_identity() {
    let (_, mut target) = lower_source(&baseline_source());
    let intent = target
        .intents
        .get_mut("observe")
        .expect("observe intent exists");
    intent
        .external_action_requests
        .push(intent.external_action_requests[0].clone());

    assert_eq!(
        encode_target_ir_artifact(&target)
            .expect_err("duplicate target request ids reject")
            .kind(),
        edict_syntax::CanonicalErrorKind::UnsupportedValue
    );
}

#[test]
fn capability_import_cannot_be_called_as_an_effect() {
    let source = format!(
        r#"package examples.direct_call@1;
use capability workspace.snapshot.observe@1 digest "{}" as snapshot;
type Input = {{ payload: Bytes<max=1024>, }};
intent observe(input: Input) returns Bytes<max=65536>
  profile workspace.read
  basis none
  budget <= workspace.tiny {{
  let output: Bytes<max=65536> = snapshot(input.payload)
    else {{ rejected(reason) => workspace.Rejected }};
  return output;
}}
"#,
        digest(OPERATION_DIGEST)
    );
    let module = parse_module(&source).expect("direct call source parses");
    let permissive_effect_facts = context()
        .with_operation_profile_write_classes("workspace.read", [WriteClass::Read])
        .with_effect_write_class("snapshot", WriteClass::Read);
    let errors = compile_to_core(&module, &permissive_effect_facts)
        .expect_err("capability alias rejects despite matching effect facts");
    assert_eq!(errors[0].kind, CompilerErrorKind::UnsupportedSourceShape);
}

#[test]
fn capability_import_cannot_be_used_as_an_obstruction_coordinate() {
    let require_source = baseline_source().replace(
        "{\n  request pending:",
        "{\n  require false else snapshot;\n  request pending:",
    );
    let module = parse_module(&require_source).expect("capability-require source parses");
    let errors = compile_to_core(&module, &context())
        .expect_err("capability alias in obstruction position rejects");
    assert_eq!(errors[0].kind, CompilerErrorKind::UnsupportedSourceShape);

    let effect_source = format!(
        r#"package examples.obstruction_alias@1;
use capability workspace.snapshot.observe@1 digest "{}" as snapshot;
type Input = {{ payload: Bytes<max=1024>, }};
type Output = {{ payload: Bytes<max=1024>, }};
intent observe(input: Input) returns Output
  profile workspace.read
  basis none
  budget <= workspace.tiny {{
  let output: Output = target.read(input.payload)
    else {{ rejected(reason) => snapshot }};
  return output;
}}
"#,
        digest(OPERATION_DIGEST)
    );
    let module = parse_module(&effect_source).expect("capability-effect-obstruction source parses");
    let effect_context = context()
        .with_operation_profile_write_classes("workspace.read", [WriteClass::Read])
        .with_effect_write_class("target.read", WriteClass::Read);
    let errors = compile_to_core(&module, &effect_context)
        .expect_err("capability alias in effect obstruction position rejects");
    assert_eq!(errors[0].kind, CompilerErrorKind::UnsupportedSourceShape);
}

#[test]
fn ambient_operation_families_are_not_requestable() {
    for coordinate in [
        "filesystem.read@1",
        "Filesystem.read@1",
        "process.spawn@1",
        "network.fetch@1",
        "NETWORK.fetch@1",
        "git.push@1",
        "github.open_pull_request@1",
        "model.invoke@1",
        "shell.command@1",
        "fs.read@1",
        "net.fetch@1",
        "http.get@1",
        "exec.command@1",
        "gh.open_pull_request@1",
        "calendar.observe@1",
    ] {
        let source = request_source(&RequestSource {
            capability_coordinate: coordinate,
            operation_alias: "snapshot",
            operation_digest: &digest(OPERATION_DIGEST),
            input_schema_coordinate: "workspace.snapshot.input@1",
            input_schema_digest: &digest(INPUT_SCHEMA_DIGEST),
            settlement_schema_coordinate: "workspace.snapshot.settlement@1",
            settlement_schema_digest: &digest(SETTLEMENT_SCHEMA_DIGEST),
            input_expr: "input.payload",
            authority_expr: "input.scope",
            basis_expr: "input.basis",
            max_settlement_bytes: "input.maxSettlementBytes",
            max_attempts: "input.maxAttempts",
            reconciliation_coordinate: "workspace.snapshot.reconcile@1",
            reconciliation_digest: &digest(RECONCILIATION_DIGEST),
        });
        let module = parse_module(&source).expect("ambient-family source parses structurally");
        let errors =
            compile_to_core(&module, &context()).expect_err("ambient operation family rejects");
        assert_eq!(
            errors[0].kind,
            CompilerErrorKind::UnrequestableExternalOperation,
            "{coordinate}"
        );
    }
}

#[test]
fn non_call_request_operation_has_a_request_specific_parse_kind() {
    let source = baseline_source().replace("snapshot(input.payload)", "input.payload");
    let error = parse_module(&source).expect_err("non-call request operation rejects");
    assert_eq!(error.kind.code(), "NonCallExternalActionOperation");
}

#[test]
fn dynamic_admission_values_survive_without_compile_time_execution() {
    let core = compile_source(&baseline_source());
    let request = core_request_node(&core);
    assert_eq!(
        text_field(map_field(&request, "authorityScope"), "field"),
        "scope"
    );
    assert_eq!(text_field(map_field(&request, "basis"), "field"), "basis");
    let budget = map_field(&request, "budget");
    assert_eq!(
        text_field(map_field(budget, "maxSettlementBytes"), "field"),
        "maxSettlementBytes"
    );
    assert_eq!(
        text_field(map_field(budget, "maxAttempts"), "field"),
        "maxAttempts"
    );
}

#[test]
fn request_artifacts_are_reproducible() {
    let source = baseline_source();
    let (left_core, left_target) = lower_source(&source);
    let (right_core, right_target) = lower_source(&source);
    assert_eq!(
        encode_core_module(&left_core).expect("left Core"),
        encode_core_module(&right_core).expect("right Core")
    );
    assert_eq!(
        encode_target_ir_artifact(&left_target).expect("left Target IR"),
        encode_target_ir_artifact(&right_target).expect("right Target IR")
    );
}

#[test]
fn every_request_authority_field_moves_core_and_target_identity() {
    let baseline = baseline_source();
    let (baseline_core, baseline_target) = lower_source(&baseline);
    let baseline_core_digest = digest_core_module(&baseline_core).expect("baseline Core digest");
    let baseline_target_bytes =
        encode_target_ir_artifact(&baseline_target).expect("baseline Target IR");
    let mutations = [
        baseline.replace(&digest(OPERATION_DIGEST), &digest('1')),
        baseline.replace(&digest(INPUT_SCHEMA_DIGEST), &digest('2')),
        baseline.replace(&digest(SETTLEMENT_SCHEMA_DIGEST), &digest('3')),
        baseline.replace("authority input.scope", "authority input.alternateScope"),
        baseline.replace("basis input.basis", "basis input.alternateBasis"),
        baseline.replace(
            "maxSettlementBytes input.maxSettlementBytes",
            "maxSettlementBytes input.alternateMaxSettlementBytes",
        ),
        baseline.replace(
            "maxAttempts input.maxAttempts",
            "maxAttempts input.alternateMaxAttempts",
        ),
        baseline.replace(
            "snapshot(input.payload)",
            "snapshot(input.alternatePayload)",
        ),
        baseline.replace(&digest(RECONCILIATION_DIGEST), &digest('4')),
    ];
    for mutation in mutations {
        let (mutated_core, mutated_target) = lower_source(&mutation);
        assert_ne!(
            digest_core_module(&mutated_core).expect("mutated Core digest"),
            baseline_core_digest
        );
        assert_ne!(
            encode_target_ir_artifact(&mutated_target).expect("mutated Target IR"),
            baseline_target_bytes
        );
    }
}

#[test]
fn fixed_seed_request_identity_corpus_is_deterministic() {
    let mut state = PROPERTY_SEED;
    let mut observed = BTreeSet::new();
    for _ in 0..32 {
        state = state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        let mut hex = format!("{state:016x}");
        hex = hex.repeat(4);
        let operation_digest = format!("sha256:{hex}");
        let source = request_source(&RequestSource {
            capability_coordinate: "workspace.snapshot.observe@1",
            operation_alias: "snapshot",
            operation_digest: &operation_digest,
            input_schema_coordinate: "workspace.snapshot.input@1",
            input_schema_digest: &digest(INPUT_SCHEMA_DIGEST),
            settlement_schema_coordinate: "workspace.snapshot.settlement@1",
            settlement_schema_digest: &digest(SETTLEMENT_SCHEMA_DIGEST),
            input_expr: "input.payload",
            authority_expr: "input.scope",
            basis_expr: "input.basis",
            max_settlement_bytes: "input.maxSettlementBytes",
            max_attempts: "input.maxAttempts",
            reconciliation_coordinate: "workspace.snapshot.reconcile@1",
            reconciliation_digest: &digest(RECONCILIATION_DIGEST),
        });
        let first = digest_core_module(&compile_source(&source)).expect("first property digest");
        let second = digest_core_module(&compile_source(&source)).expect("second property digest");
        assert_eq!(first, second);
        assert!(observed.insert(first));
    }
    assert_eq!(observed.len(), 32);
}

#[test]
fn sixty_four_requests_remain_bounded_non_callable_data() {
    let mut requests = String::new();
    for index in 0..64 {
        write!(
            requests,
            r#"  request pending{index}: ExternalActionRequest<Bytes<max=65536>> =
    snapshot(input.payload)
    input schema workspace.snapshot.input@1 digest "{}"
    settlement schema workspace.snapshot.settlement@1 digest "{}"
    authority input.scope
    basis input.basis
    budget maxSettlementBytes input.maxSettlementBytes maxAttempts input.maxAttempts
    reconcile workspace.snapshot.reconcile@1 digest "{}";
"#,
            digest(INPUT_SCHEMA_DIGEST),
            digest(SETTLEMENT_SCHEMA_DIGEST),
            digest(RECONCILIATION_DIGEST),
        )
        .expect("writing to a String cannot fail");
    }
    let source = format!(
        r#"package examples.workspace_stress@1;
use capability workspace.snapshot.observe@1 digest "{}" as snapshot;
type ObserveInput = {{
  payload: Bytes<max=1024>,
  scope: Bytes<max=32>,
  basis: Bytes<max=32>,
  maxSettlementBytes: U64,
  maxAttempts: U32,
}};
intent observe(input: ObserveInput) returns ExternalActionRequest<Bytes<max=65536>>
  profile workspace.read
  basis input.basis
  budget <= workspace.tiny
{{
{requests}  return pending63;
}}
"#,
        digest(OPERATION_DIGEST),
    );
    let (core, target) = lower_source(&source);
    let core_value =
        decode_canonical_cbor(&encode_core_module(&core).expect("stress Core encodes"))
            .expect("stress Core decodes");
    let core_intent = map_field(map_field(&core_value, "intents"), "observe");
    assert_eq!(
        array_field(map_field(core_intent, "body"), "nodes").len(),
        64
    );
    assert_eq!(target_request_values(&target).len(), 64);

    let target_value = decode_canonical_cbor(
        &encode_target_ir_artifact(&target).expect("stress Target IR encodes"),
    )
    .expect("stress Target IR decodes");
    let target_intent = map_field(map_field(&target_value, "intents"), "observe");
    assert_eq!(array_field(target_intent, "steps"), []);
}

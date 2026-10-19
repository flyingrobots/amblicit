//! Target IR generation tests for the first Echo lowering slice.
//!
//! These tests assert public behavior: structured Target IR artifacts and
//! stable failure kinds. They do not inspect diagnostic prose, repository
//! layout, or implementation-private lowering helpers.

use std::collections::BTreeMap;

use edict_syntax::{
    check_lowerability, compile_to_core, decode_canonical_cbor, digest_target_ir_artifact,
    encode_core_module, encode_target_ir_artifact, lower_to_target_ir, AtomicityRequirement,
    CanonicalErrorKind, CanonicalValue, CompareOp, CompilerContext, CoreBlock, CoreBound,
    CoreBudget, CoreExpr, CoreExternalActionBudget, CoreImport, CoreImportKind, CoreModule,
    CoreNode, CorePredicate, CoreType, CoreValue, GuardKind, InputConstraint,
    InputConstraintSource, LocalRef, LowerabilityStatus, LoweringRequirements, NativeEffectSupport,
    ResourceRef, SemanticEffectRequirement, TargetEffectLowering, TargetIrArtifact,
    TargetIrExternalActionRequest, TargetIrLoweringFacts, TargetIrRequireFailure, TargetIrStep,
    TargetLoweringFailureKind, TargetLoweringStatus, TargetProfileFacts, TypeShapeFact, WriteClass,
    ECHO_DPO_TARGET_PROFILE, ECHO_SPAN_IR_DOMAIN, GITWARP_COMMIT_REDUCER_IR_DOMAIN,
    GITWARP_REF_CRDT_TARGET_PROFILE, TARGET_IR_ARTIFACT_DIGEST_DOMAIN,
};

const EFFECTFUL_REPLACE: &str = "package a.b@1;\n\
    type Input = { id: String<max=16>, };\n\
    type Receipt = { id: String<max=16>, };\n\
    type Output = { id: String<max=16>, };\n\
    intent t(input: Input) returns Output\n\
      profile p.effectful\n\
      basis none\n\
      budget <= p.tiny {\n\
      let receipt: Receipt = target.replace(input.id)\n\
        else { rejected(reason) => domain.WriteRejected };\n\
      return { id: input.id };\n\
    }";
const CHAINED_EFFECT_RESULTS: &str = "package a.b@1;\n\
    type Input = { id: String<max=16>, };\n\
    type Receipt = { id: String<max=16>, };\n\
    type Output = { id: String<max=16>, };\n\
    intent t(input: Input) returns Output\n\
      profile p.effectful\n\
      basis none\n\
      budget <= p.tiny {\n\
      let first: Receipt = target.replace(input.id)\n\
        else { rejected(reason) => domain.WriteRejected };\n\
      let second: Receipt = target.replace(first.id)\n\
        else { rejected(reason) => domain.WriteRejected };\n\
      return { id: second.id };\n\
    }";
const GITWARP_APPEND_EVENT: &str = "package a.git@1;\n\
    type Input = { id: String<max=16>, };\n\
    type Receipt = { id: String<max=16>, };\n\
    type Output = { id: String<max=16>, };\n\
    intent t(input: Input) returns Output\n\
      profile p.gitwarp\n\
      basis none\n\
      budget <= p.tiny\n\
      where input.id != \"\" {\n\
      let receipt: Receipt = gitwarp.appendEvent(input.id)\n\
        else { conflict(reason) => domain.MergeConflict };\n\
      return { id: receipt.id };\n\
    }";
const ECHO_CONTINUE_OBSTRUCTED_REQUIRE: &str = "package a.b@1;\n\
    type Input = { id: String<max=16>, };\n\
    type Output = { id: String<max=16>, };\n\
    intent t(input: Input) returns Output\n\
      profile p.effectful\n\
      basis none\n\
      budget <= p.tiny {\n\
      require true else continue obstructed {\n\
        reason: jim.EditObstruction.StaleBase,\n\
        provided: input.id,\n\
      };\n\
      return { id: input.id };\n\
    }";
const ECHO_TERMINAL_REQUIRE: &str = "package a.b@1;\n\
    type Input = { id: String<max=16>, };\n\
    type Output = { id: String<max=16>, };\n\
    intent t(input: Input) returns Output\n\
      profile p.effectful\n\
      basis none\n\
      budget <= p.tiny {\n\
      require true else jim.EditObstruction.StaleBase({ reason: input.id });\n\
      return { id: input.id };\n\
    }";
const ECHO_EFFECT_OUTPUT_DEPENDENT_REQUIRE: &str = "package a.b@1;\n\
    type Input = { id: String<max=16>, };\n\
    type Receipt = { id: String<max=16>, };\n\
    type Output = { id: String<max=16>, };\n\
    intent t(input: Input) returns Output\n\
      profile p.effectful\n\
      basis none\n\
      budget <= p.tiny {\n\
      let receipt: Receipt = target.replace(input.id)\n\
        else { rejected(reason) => domain.WriteRejected };\n\
      require receipt.id != \"\" else continue obstructed {\n\
        reason: jim.EditObstruction.StaleBase,\n\
        provided: receipt.id,\n\
      };\n\
      return { id: receipt.id };\n\
    }";
const ECHO_POST_STEP_INPUT_REQUIRE: &str = "package a.b@1;\n\
    type Input = { id: String<max=16>, };\n\
    type Receipt = { id: String<max=16>, };\n\
    type Output = { id: String<max=16>, };\n\
    intent t(input: Input) returns Output\n\
      profile p.effectful\n\
      basis none\n\
      budget <= p.tiny {\n\
      let receipt: Receipt = target.replace(input.id)\n\
        else { rejected(reason) => domain.WriteRejected };\n\
      require input.id != \"\" else continue obstructed {\n\
        reason: jim.EditObstruction.StaleBase,\n\
        provided: input.id,\n\
      };\n\
      return { id: receipt.id };\n\
    }";
const GITWARP_CONTINUE_OBSTRUCTED_REQUIRE: &str = "package a.git@1;\n\
    type Input = { id: String<max=16>, };\n\
    type Output = { id: String<max=16>, };\n\
    intent t(input: Input) returns Output\n\
      profile p.gitwarp\n\
      basis none\n\
      budget <= p.tiny {\n\
      require true else continue obstructed {\n\
        reason: jim.EditObstruction.StaleBase,\n\
        provided: input.id,\n\
      };\n\
      return { id: input.id };\n\
    }";

const PURE_LOCAL_RECORD: &str = include_str!("../../../fixtures/lang/bounds/bounded-hello.edict");
const ECHO_PROFILE_DIGEST: &str =
    "sha256:1111111111111111111111111111111111111111111111111111111111111111";
const GITWARP_PROFILE_DIGEST: &str =
    "sha256:2222222222222222222222222222222222222222222222222222222222222222";

fn effectful_core() -> edict_syntax::CoreModule {
    let module = edict_syntax::parse_module(EFFECTFUL_REPLACE).expect("effectful source parses");
    compile_to_core(&module, &effectful_context()).expect("effectful source compiles to Core")
}

fn effectful_artifact(source: &str) -> edict_syntax::TargetIrArtifact {
    let module = edict_syntax::parse_module(source).expect("effectful source parses");
    let core =
        compile_to_core(&module, &effectful_context()).expect("effectful source compiles to Core");
    lower_to_target_ir(&core, &echo_facts())
        .artifact
        .expect("supported source lowers to Target IR")
}

fn gitwarp_artifact() -> edict_syntax::TargetIrArtifact {
    lower_to_target_ir(&gitwarp_core(), &gitwarp_facts())
        .artifact
        .expect("supported git-warp source lowers to Target IR")
}

fn pure_core() -> edict_syntax::CoreModule {
    let module = edict_syntax::parse_module(PURE_LOCAL_RECORD).expect("pure source parses");
    compile_to_core(&module, &pure_context()).expect("pure source compiles to Core")
}

fn pure_target_facts() -> TargetIrLoweringFacts {
    let mut facts = echo_facts();
    facts
        .operation_profiles
        .push("continuum.profile.read-only/v1".to_owned());
    facts
}

fn pure_artifact() -> TargetIrArtifact {
    lower_to_target_ir(&pure_core(), &pure_target_facts())
        .artifact
        .expect("pure source lowers to Target IR")
}

fn gitwarp_core() -> edict_syntax::CoreModule {
    let module = edict_syntax::parse_module(GITWARP_APPEND_EVENT).expect("git-warp source parses");
    compile_to_core(&module, &gitwarp_context()).expect("git-warp source compiles to Core")
}

fn gitwarp_obstruction_core() -> edict_syntax::CoreModule {
    let module = edict_syntax::parse_module(GITWARP_CONTINUE_OBSTRUCTED_REQUIRE)
        .expect("git-warp obstruction source parses");
    compile_to_core(&module, &gitwarp_context())
        .expect("git-warp obstruction source compiles to Core")
}

fn effectful_context() -> CompilerContext {
    effectful_context_with_profile("continuum.profile.write/v1")
}

fn effectful_context_with_profile(profile: &str) -> CompilerContext {
    CompilerContext::new()
        .with_operation_profile("p.effectful", profile)
        .with_operation_profile_write_classes("p.effectful", [WriteClass::Replace])
        .with_effect_write_class("target.replace", WriteClass::Replace)
        .with_budget(
            "p.tiny",
            CoreBudget {
                max_steps: 8,
                max_allocated_bytes: 1024,
                max_output_bytes: 256,
            },
        )
}

fn pure_context() -> CompilerContext {
    CompilerContext::new()
        .with_operation_profile("hello.readOnly", "continuum.profile.read-only/v1")
        .with_budget(
            "hello.tinyBudget",
            CoreBudget {
                max_steps: 64,
                max_allocated_bytes: 4096,
                max_output_bytes: 1024,
            },
        )
}

fn gitwarp_context() -> CompilerContext {
    CompilerContext::new()
        .with_operation_profile("p.gitwarp", "continuum.profile.append/v1")
        .with_operation_profile_write_classes("p.gitwarp", [WriteClass::Append])
        .with_effect_write_class("gitwarp.appendEvent", WriteClass::Append)
        .with_budget(
            "p.tiny",
            CoreBudget {
                max_steps: 13,
                max_allocated_bytes: 2048,
                max_output_bytes: 512,
            },
        )
}

fn echo_facts() -> TargetIrLoweringFacts {
    TargetIrLoweringFacts {
        target_profile: ResourceRef {
            coordinate: ECHO_DPO_TARGET_PROFILE.to_owned(),
            digest: Some(echo_profile_digest()),
        },
        target_ir_domain: ECHO_SPAN_IR_DOMAIN.to_owned(),
        operation_profiles: vec!["continuum.profile.write/v1".to_owned()],
        obstruction_coordinates: vec!["rejected".to_owned()],
        effect_lowerings: vec![TargetEffectLowering {
            effect: "target.replace".to_owned(),
            target_intrinsic: "echo.dpo@1.replace".to_owned(),
            failure_mappings: BTreeMap::new(),
        }],
        effect_signatures: Vec::new(),
        pure_functions: Vec::new(),
    }
}

fn gitwarp_facts() -> TargetIrLoweringFacts {
    TargetIrLoweringFacts {
        target_profile: ResourceRef {
            coordinate: GITWARP_REF_CRDT_TARGET_PROFILE.to_owned(),
            digest: Some(gitwarp_profile_digest()),
        },
        target_ir_domain: GITWARP_COMMIT_REDUCER_IR_DOMAIN.to_owned(),
        operation_profiles: vec!["continuum.profile.append/v1".to_owned()],
        obstruction_coordinates: vec!["conflict".to_owned()],
        effect_lowerings: vec![TargetEffectLowering {
            effect: "gitwarp.appendEvent".to_owned(),
            target_intrinsic: "gitwarp.ref_crdt@1.appendEvent".to_owned(),
            failure_mappings: BTreeMap::new(),
        }],
        effect_signatures: Vec::new(),
        pure_functions: Vec::new(),
    }
}

fn echo_profile_digest() -> String {
    ECHO_PROFILE_DIGEST.to_owned()
}

fn gitwarp_profile_digest() -> String {
    GITWARP_PROFILE_DIGEST.to_owned()
}

fn echo_profile_ref() -> ResourceRef {
    ResourceRef {
        coordinate: ECHO_DPO_TARGET_PROFILE.to_owned(),
        digest: Some(echo_profile_digest()),
    }
}

fn gitwarp_profile_ref() -> ResourceRef {
    ResourceRef {
        coordinate: GITWARP_REF_CRDT_TARGET_PROFILE.to_owned(),
        digest: Some(gitwarp_profile_digest()),
    }
}

fn echo_profile_facts() -> TargetProfileFacts {
    TargetProfileFacts {
        coordinate: ECHO_DPO_TARGET_PROFILE.to_owned(),
        operation_profiles: vec!["continuum.profile.write/v1".to_owned()],
        native_effects: vec![NativeEffectSupport {
            coordinate: "target.replace".to_owned(),
            target_intrinsic: "echo.dpo@1.replace".to_owned(),
            write_class: WriteClass::Replace,
            guard_kinds: vec![GuardKind::PrecommitAtomic],
        }],
        direct_adapters: Vec::new(),
        write_classes: vec![WriteClass::Replace],
        guard_kinds: vec![GuardKind::PrecommitAtomic],
        atomicity: vec![AtomicityRequirement::Atomic],
        postcondition_support: true,
        obstruction_coordinates: vec!["rejected".to_owned()],
        footprint_obligations: vec!["target.replace.footprint".to_owned()],
        cost_obligations: vec!["target.replace.cost".to_owned()],
        optic_contracts: vec!["replace-point".to_owned()],
    }
}

fn gitwarp_profile_facts() -> TargetProfileFacts {
    TargetProfileFacts {
        coordinate: GITWARP_REF_CRDT_TARGET_PROFILE.to_owned(),
        operation_profiles: vec!["continuum.profile.append/v1".to_owned()],
        native_effects: vec![NativeEffectSupport {
            coordinate: "gitwarp.appendEvent".to_owned(),
            target_intrinsic: "gitwarp.ref_crdt@1.appendEvent".to_owned(),
            write_class: WriteClass::Append,
            guard_kinds: vec![GuardKind::PrecommitAtomic],
        }],
        direct_adapters: Vec::new(),
        write_classes: vec![WriteClass::Append],
        guard_kinds: vec![GuardKind::PrecommitAtomic],
        atomicity: vec![AtomicityRequirement::Atomic],
        postcondition_support: true,
        obstruction_coordinates: vec!["conflict".to_owned()],
        footprint_obligations: vec!["gitwarp.appendEvent.footprint".to_owned()],
        cost_obligations: vec!["gitwarp.appendEvent.cost".to_owned()],
        optic_contracts: vec!["append-event".to_owned()],
    }
}

fn echo_requirements() -> LoweringRequirements {
    LoweringRequirements {
        operation_profile: "continuum.profile.write/v1".to_owned(),
        semantic_effects: vec![SemanticEffectRequirement {
            coordinate: "target.replace".to_owned(),
            write_class: WriteClass::Replace,
            guard_kinds: vec![GuardKind::PrecommitAtomic],
            obstruction_coordinates: vec!["rejected".to_owned()],
            footprint_obligations: vec!["target.replace.footprint".to_owned()],
            cost_obligations: vec!["target.replace.cost".to_owned()],
        }],
        required_write_classes: vec![WriteClass::Replace],
        guard_kinds: vec![GuardKind::PrecommitAtomic],
        atomicity: AtomicityRequirement::Atomic,
        postcondition_support: true,
        obstruction_coordinates: vec!["rejected".to_owned()],
        footprint_obligations: vec!["target.replace.footprint".to_owned()],
        cost_obligations: vec!["target.replace.cost".to_owned()],
        optic_contract: "replace-point".to_owned(),
    }
}

fn gitwarp_requirements() -> LoweringRequirements {
    LoweringRequirements {
        operation_profile: "continuum.profile.append/v1".to_owned(),
        semantic_effects: vec![SemanticEffectRequirement {
            coordinate: "gitwarp.appendEvent".to_owned(),
            write_class: WriteClass::Append,
            guard_kinds: vec![GuardKind::PrecommitAtomic],
            obstruction_coordinates: vec!["conflict".to_owned()],
            footprint_obligations: vec!["gitwarp.appendEvent.footprint".to_owned()],
            cost_obligations: vec!["gitwarp.appendEvent.cost".to_owned()],
        }],
        required_write_classes: vec![WriteClass::Append],
        guard_kinds: vec![GuardKind::PrecommitAtomic],
        atomicity: AtomicityRequirement::Atomic,
        postcondition_support: true,
        obstruction_coordinates: vec!["conflict".to_owned()],
        footprint_obligations: vec!["gitwarp.appendEvent.footprint".to_owned()],
        cost_obligations: vec!["gitwarp.appendEvent.cost".to_owned()],
        optic_contract: "append-event".to_owned(),
    }
}

fn failure_kinds(report: &edict_syntax::TargetLoweringReport) -> Vec<TargetLoweringFailureKind> {
    report.failures.iter().map(|failure| failure.kind).collect()
}

fn assert_invalid_core_identity(report: &edict_syntax::TargetLoweringReport, case: &str) {
    assert_eq!(report.status, TargetLoweringStatus::Unsupported, "{case}");
    assert!(report.artifact.is_none(), "{case}");
    assert_eq!(
        failure_kinds(report),
        vec![TargetLoweringFailureKind::InvalidCoreIdentity],
        "{case}"
    );
}

fn mutate_first_local_reference_type(expression: &mut CoreExpr, ty: &str) -> bool {
    match expression {
        CoreExpr::Local { reference } => {
            ty.clone_into(&mut reference.ty);
            true
        }
        CoreExpr::Const(_) => false,
        CoreExpr::Record { fields } => fields
            .values_mut()
            .any(|value| mutate_first_local_reference_type(value, ty)),
        CoreExpr::Field { base, .. } => mutate_first_local_reference_type(base, ty),
        CoreExpr::Call { args, .. } => args
            .iter_mut()
            .any(|value| mutate_first_local_reference_type(value, ty)),
        CoreExpr::If {
            predicate,
            then_value,
            else_value,
        } => {
            mutate_first_predicate_local_type(predicate, ty)
                || mutate_first_local_reference_type(then_value, ty)
                || mutate_first_local_reference_type(else_value, ty)
        }
    }
}

fn mutate_first_predicate_local_type(predicate: &mut CorePredicate, ty: &str) -> bool {
    match predicate {
        CorePredicate::True | CorePredicate::False => false,
        CorePredicate::Not(value) => mutate_first_predicate_local_type(value, ty),
        CorePredicate::All(values) | CorePredicate::Any(values) => values
            .iter_mut()
            .any(|value| mutate_first_predicate_local_type(value, ty)),
        CorePredicate::Compare { left, right, .. } => {
            mutate_first_local_reference_type(left, ty)
                || mutate_first_local_reference_type(right, ty)
        }
    }
}

fn replace_first_let_binding_type(core: &mut CoreModule, from: &str, to: &str) {
    let intent = core.intents.get_mut("t").expect("intent t");
    let binding_id = {
        let CoreNode::Let { binding, .. } = &mut intent.body.nodes[0] else {
            panic!("bounded-list fixture starts with a let");
        };
        let replacement = binding.ty.replace(from, to);
        assert_ne!(replacement, binding.ty);
        binding.ty = replacement;
        binding.id.clone()
    };
    let local = intent
        .body
        .locals
        .iter_mut()
        .find(|local| local.id == binding_id)
        .expect("list binding remains in the Core local table");
    local.ty = match &intent.body.nodes[0] {
        CoreNode::Let { binding, .. } => binding.ty.clone(),
        _ => unreachable!("bounded-list fixture starts with a let"),
    };
}

#[test]
fn supported_effectful_core_lowers_to_echo_span_ir() {
    let core = effectful_core();
    let report = lower_to_target_ir(&core, &echo_facts());

    assert_eq!(report.status, TargetLoweringStatus::Lowered);
    assert_eq!(
        report.failures,
        Vec::<edict_syntax::TargetLoweringFailure>::new()
    );

    let artifact = report.artifact.expect("supported Core emits Target IR");
    assert_eq!(artifact.domain, ECHO_SPAN_IR_DOMAIN);
    assert_eq!(artifact.target_profile.coordinate, ECHO_DPO_TARGET_PROFILE);
    assert_eq!(artifact.source_core_coordinate, "a.b@1");
    assert_eq!(artifact.intents.len(), 1);

    let intent = artifact.intents.get("t").expect("lowered intent t");
    assert_eq!(intent.operation_profile, "continuum.profile.write/v1");
    assert_eq!(intent.steps.len(), 1);

    let step = &intent.steps[0];
    assert_eq!(step.id, "t.step.0");
    assert_eq!(step.effect, "target.replace");
    assert_eq!(step.target_intrinsic, "echo.dpo@1.replace");
    assert_eq!(step.obstruction_failures, vec!["rejected".to_owned()]);

    let CoreExpr::Field { field, .. } = &step.input else {
        panic!("effect input is preserved structurally");
    };
    assert_eq!(field, "id");
}

#[test]
fn lowerability_native_support_feeds_echo_target_lowering() {
    let profile_facts = echo_profile_facts();
    let lowerability = check_lowerability(&echo_requirements(), &profile_facts);
    assert_eq!(lowerability.status, LowerabilityStatus::Native);
    assert_eq!(
        lowerability.failures,
        Vec::<edict_syntax::LowerabilityFailure>::new()
    );

    let target_facts = TargetIrLoweringFacts::from_lowerability_report(
        echo_profile_ref(),
        ECHO_SPAN_IR_DOMAIN,
        &lowerability,
    )
    .expect("native lowerability builds target facts");
    let report = lower_to_target_ir(&effectful_core(), &target_facts);

    assert_eq!(report.status, TargetLoweringStatus::Lowered);
    let artifact = report
        .artifact
        .expect("native lowerability feeds target IR");
    let step = &artifact.intents.get("t").expect("intent t").steps[0];
    assert_eq!(step.effect, "target.replace");
    assert_eq!(step.target_intrinsic, "echo.dpo@1.replace");
}

#[test]
fn supported_gitwarp_core_lowers_to_commit_reducer_ir() {
    let core = gitwarp_core();
    let report = lower_to_target_ir(&core, &gitwarp_facts());

    assert_eq!(report.status, TargetLoweringStatus::Lowered);
    assert_eq!(
        report.failures,
        Vec::<edict_syntax::TargetLoweringFailure>::new()
    );

    let artifact = report.artifact.expect("supported Core emits Target IR");
    assert_eq!(artifact.domain, GITWARP_COMMIT_REDUCER_IR_DOMAIN);
    assert_eq!(
        artifact.target_profile.coordinate,
        GITWARP_REF_CRDT_TARGET_PROFILE
    );
    assert_eq!(artifact.source_core_coordinate, "a.git@1");

    let intent = artifact.intents.get("t").expect("lowered intent t");
    assert_eq!(intent.operation_profile, "continuum.profile.append/v1");
    assert_eq!(
        intent.core_evaluation_budget,
        CoreBudget {
            max_steps: 13,
            max_allocated_bytes: 2048,
            max_output_bytes: 512,
        }
    );
    assert_eq!(intent.input_constraints.len(), 1);
    assert!(matches!(
        intent.input_constraints[0].predicate,
        CorePredicate::Compare { .. }
    ));
    assert_eq!(intent.steps.len(), 1);

    let step = &intent.steps[0];
    assert_eq!(step.effect, "gitwarp.appendEvent");
    assert_eq!(step.target_intrinsic, "gitwarp.ref_crdt@1.appendEvent");
    assert_eq!(step.obstruction_failures, vec!["conflict".to_owned()]);
    assert!(step.obstruction_arms.contains_key("conflict"));

    let CoreExpr::Field { field, .. } = &step.input else {
        panic!("git-warp effect input is preserved structurally");
    };
    assert_eq!(field, "id");

    let CoreExpr::Record { fields } = &intent.result else {
        panic!("git-warp intent result is preserved structurally");
    };
    assert!(fields.contains_key("id"));
}

#[test]
fn lowerability_native_support_feeds_gitwarp_target_lowering() {
    let lowerability = check_lowerability(&gitwarp_requirements(), &gitwarp_profile_facts());
    assert_eq!(lowerability.status, LowerabilityStatus::Native);
    assert_eq!(
        lowerability.failures,
        Vec::<edict_syntax::LowerabilityFailure>::new()
    );

    let target_facts = TargetIrLoweringFacts::from_lowerability_report(
        gitwarp_profile_ref(),
        GITWARP_COMMIT_REDUCER_IR_DOMAIN,
        &lowerability,
    )
    .expect("native git-warp lowerability builds target facts");
    let report = lower_to_target_ir(&gitwarp_core(), &target_facts);

    assert_eq!(report.status, TargetLoweringStatus::Lowered);
    let artifact = report
        .artifact
        .expect("native git-warp lowerability feeds target IR");
    let step = &artifact.intents.get("t").expect("intent t").steps[0];
    assert_eq!(step.effect, "gitwarp.appendEvent");
    assert_eq!(step.target_intrinsic, "gitwarp.ref_crdt@1.appendEvent");
}

#[test]
fn echo_target_ir_contains_obstruction_requirement_payload() {
    let artifact = effectful_artifact(ECHO_CONTINUE_OBSTRUCTED_REQUIRE);
    let intent = artifact.intents.get("t").expect("lowered intent t");

    assert_eq!(intent.steps, Vec::<edict_syntax::TargetIrStep>::new());
    assert_eq!(intent.requirements.len(), 1);
    let requirement = &intent.requirements[0];
    assert_eq!(requirement.id, "t.require.0");
    assert_eq!(requirement.predicate, CorePredicate::True);

    let TargetIrRequireFailure::ContinueObstructed { reason } = &requirement.on_failure else {
        panic!("continue obstructed require remains preserved in Target IR");
    };
    assert_eq!(reason.kind, "jim.EditObstruction.StaleBase");
    assert_eq!(reason.payload.keys().collect::<Vec<_>>(), vec!["provided"]);
    assert!(matches!(
        &reason.payload["provided"],
        CoreExpr::Field { field, .. } if field == "id"
    ));
}

#[test]
fn terminal_and_preserved_requirements_are_target_ir_distinct() {
    let terminal = effectful_artifact(ECHO_TERMINAL_REQUIRE);
    let preserved_source = replace_required(
        ECHO_CONTINUE_OBSTRUCTED_REQUIRE,
        "provided: input.id,\n",
        "",
    );
    let preserved = effectful_artifact(&preserved_source);

    let terminal_requirement = &terminal.intents.get("t").expect("intent t").requirements[0];
    let preserved_requirement = &preserved.intents.get("t").expect("intent t").requirements[0];
    assert!(matches!(
        terminal_requirement.on_failure,
        TargetIrRequireFailure::Terminal { .. }
    ));
    assert!(matches!(
        preserved_requirement.on_failure,
        TargetIrRequireFailure::ContinueObstructed { .. }
    ));
    assert_ne!(
        digest_target_ir_artifact(&terminal).expect("terminal Target IR digests"),
        digest_target_ir_artifact(&preserved).expect("preserved Target IR digests")
    );
}

#[test]
fn target_ir_requirement_mutations_move_digest() {
    let baseline = effectful_artifact(ECHO_CONTINUE_OBSTRUCTED_REQUIRE);
    assert_target_ir_digest_changes(&baseline, "require predicate", |artifact| {
        requirement_mut(artifact).predicate = CorePredicate::False;
    });
    assert_target_ir_digest_changes(&baseline, "require reason kind", |artifact| {
        let TargetIrRequireFailure::ContinueObstructed { reason } =
            &mut requirement_mut(artifact).on_failure
        else {
            panic!("baseline requirement stays preserved");
        };
        reason.kind = "jim.EditObstruction.Other".to_owned();
    });
    assert_target_ir_digest_changes(&baseline, "require reason payload value", |artifact| {
        let TargetIrRequireFailure::ContinueObstructed { reason } =
            &mut requirement_mut(artifact).on_failure
        else {
            panic!("baseline requirement stays preserved");
        };
        reason.payload.insert(
            "provided".to_owned(),
            CoreExpr::Const(CoreValue::String("changed".to_owned())),
        );
    });
    assert_target_ir_digest_changes(&baseline, "require failure disposition", |artifact| {
        let TargetIrRequireFailure::ContinueObstructed { reason } =
            requirement_mut(artifact).on_failure.clone()
        else {
            panic!("baseline requirement stays preserved");
        };
        requirement_mut(artifact).on_failure = TargetIrRequireFailure::Terminal { reason };
    });
}

#[test]
fn targets_without_obstruction_requirement_support_reject_with_stable_feature_kind() {
    let report = lower_to_target_ir(&gitwarp_obstruction_core(), &gitwarp_facts());

    assert_eq!(report.status, TargetLoweringStatus::Unsupported);
    assert!(report.artifact.is_none());
    assert_eq!(
        failure_kinds(&report),
        vec![TargetLoweringFailureKind::UnsupportedTargetFeature]
    );
    assert_eq!(report.failures[0].detail, "obstruction_requirement");
}

#[test]
fn requirement_that_reads_step_output_rejects_with_stable_feature_kind() {
    let module = edict_syntax::parse_module(ECHO_EFFECT_OUTPUT_DEPENDENT_REQUIRE)
        .expect("effect output dependent require source parses");
    let core = compile_to_core(&module, &effectful_context())
        .expect("effect output dependent require source compiles to Core");

    let report = lower_to_target_ir(&core, &echo_facts());

    assert_eq!(report.status, TargetLoweringStatus::Unsupported);
    assert!(report.artifact.is_none());
    assert_eq!(
        failure_kinds(&report),
        vec![TargetLoweringFailureKind::UnsupportedTargetFeature]
    );
    assert_eq!(
        report.failures[0].detail,
        "obstruction_requirement_step_output_dependency"
    );
}

#[test]
fn requirement_after_target_step_rejects_with_stable_feature_kind() {
    let module = edict_syntax::parse_module(ECHO_POST_STEP_INPUT_REQUIRE)
        .expect("post-step input require source parses");
    let core = compile_to_core(&module, &effectful_context())
        .expect("post-step input require source compiles to Core");

    let report = lower_to_target_ir(&core, &echo_facts());

    assert_eq!(report.status, TargetLoweringStatus::Unsupported);
    assert!(report.artifact.is_none());
    assert_eq!(
        failure_kinds(&report),
        vec![TargetLoweringFailureKind::UnsupportedTargetFeature]
    );
    assert_eq!(
        report.failures[0].detail,
        "obstruction_requirement_after_target_step"
    );
}

#[test]
fn lowerability_bridge_carries_only_selected_native_effect() {
    let mut profile_facts = echo_profile_facts();
    profile_facts.native_effects.push(NativeEffectSupport {
        coordinate: "target.replace".to_owned(),
        target_intrinsic: "echo.dpo@1.replace.unselected".to_owned(),
        write_class: WriteClass::Replace,
        guard_kinds: Vec::new(),
    });
    let lowerability = check_lowerability(&echo_requirements(), &profile_facts);
    assert_eq!(lowerability.status, LowerabilityStatus::Native);
    assert_eq!(
        lowerability.failures,
        Vec::<edict_syntax::LowerabilityFailure>::new()
    );

    let target_facts = TargetIrLoweringFacts::from_lowerability_report(
        echo_profile_ref(),
        ECHO_SPAN_IR_DOMAIN,
        &lowerability,
    )
    .expect("native lowerability builds target facts");
    let report = lower_to_target_ir(&effectful_core(), &target_facts);

    assert_eq!(report.status, TargetLoweringStatus::Lowered);
    let artifact = report
        .artifact
        .expect("unselected native support does not make target lowering ambiguous");
    let step = &artifact.intents.get("t").expect("intent t").steps[0];
    assert_eq!(step.target_intrinsic, "echo.dpo@1.replace");
}

#[test]
fn lowerability_bridge_deduplicates_identical_native_effect_selection() {
    let mut requirements = echo_requirements();
    requirements
        .semantic_effects
        .push(requirements.semantic_effects[0].clone());
    let lowerability = check_lowerability(&requirements, &echo_profile_facts());
    assert_eq!(lowerability.status, LowerabilityStatus::Native);
    assert_eq!(lowerability.effect_results.len(), 2);

    let target_facts = TargetIrLoweringFacts::from_lowerability_report(
        echo_profile_ref(),
        ECHO_SPAN_IR_DOMAIN,
        &lowerability,
    )
    .expect("native lowerability builds target facts");
    let report = lower_to_target_ir(&effectful_core(), &target_facts);

    assert_eq!(report.status, TargetLoweringStatus::Lowered);
    assert_eq!(
        report.failures,
        Vec::<edict_syntax::TargetLoweringFailure>::new()
    );
    let artifact = report
        .artifact
        .expect("duplicate selected effect still lowers once");
    let step = &artifact.intents.get("t").expect("intent t").steps[0];
    assert_eq!(step.target_intrinsic, "echo.dpo@1.replace");
}

#[test]
fn unused_duplicate_effect_lowerings_do_not_reject_supported_effect() {
    let mut facts = echo_facts();
    facts.effect_lowerings.push(TargetEffectLowering {
        effect: "target.archive".to_owned(),
        target_intrinsic: "echo.dpo@1.archive".to_owned(),
        failure_mappings: BTreeMap::new(),
    });
    facts.effect_lowerings.push(TargetEffectLowering {
        effect: "target.archive".to_owned(),
        target_intrinsic: "echo.dpo@1.archive.v2".to_owned(),
        failure_mappings: BTreeMap::new(),
    });

    let report = lower_to_target_ir(&effectful_core(), &facts);

    assert_eq!(report.status, TargetLoweringStatus::Lowered);
    assert_eq!(
        report.failures,
        Vec::<edict_syntax::TargetLoweringFailure>::new()
    );
    let artifact = report
        .artifact
        .expect("unused duplicate lowerings do not block supported effect");
    let step = &artifact.intents.get("t").expect("intent t").steps[0];
    assert_eq!(step.effect, "target.replace");
    assert_eq!(step.target_intrinsic, "echo.dpo@1.replace");
}

#[test]
fn unsupported_lowerability_report_does_not_build_target_ir_facts() {
    let mut profile_facts = echo_profile_facts();
    profile_facts.operation_profiles.clear();
    let lowerability = check_lowerability(&echo_requirements(), &profile_facts);
    assert_eq!(lowerability.status, LowerabilityStatus::Unsupported);

    let error = TargetIrLoweringFacts::from_lowerability_report(
        echo_profile_ref(),
        ECHO_SPAN_IR_DOMAIN,
        &lowerability,
    )
    .expect_err("unsupported lowerability cannot build target facts");

    assert_eq!(
        error.kind,
        TargetLoweringFailureKind::UnsupportedLowerabilityReport
    );
}

#[test]
fn lowerability_bridge_uses_report_target_profile_identity() {
    let mut profile_facts = echo_profile_facts();
    profile_facts.coordinate = "gitwarp.ref_crdt@1".to_owned();
    let lowerability = check_lowerability(&echo_requirements(), &profile_facts);
    assert_eq!(lowerability.status, LowerabilityStatus::Native);

    let error = TargetIrLoweringFacts::from_lowerability_report(
        echo_profile_ref(),
        ECHO_SPAN_IR_DOMAIN,
        &lowerability,
    )
    .expect_err("target profile reference must match lowerability report");

    assert_eq!(
        error.kind,
        TargetLoweringFailureKind::UnsupportedTargetProfile
    );
}

#[test]
fn lowerability_bridge_uses_report_operation_profile_identity() {
    let lowerability = check_lowerability(&echo_requirements(), &echo_profile_facts());
    assert_eq!(lowerability.status, LowerabilityStatus::Native);

    let target_facts = TargetIrLoweringFacts::from_lowerability_report(
        echo_profile_ref(),
        ECHO_SPAN_IR_DOMAIN,
        &lowerability,
    )
    .expect("native lowerability builds target facts");
    let module = edict_syntax::parse_module(EFFECTFUL_REPLACE).expect("effectful source parses");
    let core = compile_to_core(
        &module,
        &effectful_context_with_profile("continuum.profile.unreviewed/v1"),
    )
    .expect("effectful source compiles to Core with caller-supplied profile");
    let report = lower_to_target_ir(&core, &target_facts);

    assert_eq!(report.status, TargetLoweringStatus::Unsupported);
    assert!(report.artifact.is_none());
    assert_eq!(
        failure_kinds(&report),
        vec![TargetLoweringFailureKind::MissingOperationProfile]
    );
}

#[test]
fn lowerability_bridge_requires_matching_target_profile_reference() {
    let lowerability = check_lowerability(&echo_requirements(), &echo_profile_facts());
    assert_eq!(lowerability.status, LowerabilityStatus::Native);

    let error = TargetIrLoweringFacts::from_lowerability_report(
        ResourceRef {
            coordinate: "gitwarp.ref_crdt@1".to_owned(),
            digest: Some(echo_profile_digest()),
        },
        ECHO_SPAN_IR_DOMAIN,
        &lowerability,
    )
    .expect_err("target profile reference must match lowerability report");

    assert_eq!(
        error.kind,
        TargetLoweringFailureKind::UnsupportedTargetProfile
    );
}

#[test]
fn obstruction_arm_values_are_preserved_in_echo_span_ir() {
    let artifact = effectful_artifact(
        &EFFECTFUL_REPLACE.replace("domain.WriteRejected", "domain.WriteDifferentlyRejected"),
    );
    let arm = &artifact.intents.get("t").expect("intent t").steps[0].obstruction_arms["rejected"];

    let CoreExpr::Call {
        callee,
        type_args,
        args,
    } = &arm.value
    else {
        panic!("obstruction arm value is preserved as a call expression");
    };
    assert_eq!(callee, "domain.WriteDifferentlyRejected");
    assert_eq!(type_args.as_slice(), Vec::<String>::new());
    assert_eq!(args.as_slice(), []);
}

#[test]
fn intent_result_is_preserved_in_echo_span_ir() {
    let artifact = effectful_artifact(
        &EFFECTFUL_REPLACE.replace("return { id: input.id };", "return { id: receipt.id };"),
    );
    let result = &artifact.intents.get("t").expect("intent t").result;

    let CoreExpr::Record { fields } = result else {
        panic!("intent result is preserved as a record expression");
    };
    let CoreExpr::Field { base, field } = &fields["id"] else {
        panic!("result id field is preserved as a field expression");
    };
    assert_eq!(field, "id");
    assert!(matches!(base.as_ref(), CoreExpr::Local { reference } if reference.id == "local.0"));
}

#[test]
fn intent_constraints_and_budget_are_preserved_in_echo_span_ir() {
    let constrained_source = EFFECTFUL_REPLACE.replace(
        "budget <= p.tiny {",
        "budget <= p.tiny\n      where input.id != \"\" {",
    );
    let artifact = effectful_artifact(&constrained_source);
    let intent = artifact.intents.get("t").expect("intent t");

    assert_eq!(
        intent.core_evaluation_budget,
        CoreBudget {
            max_steps: 8,
            max_allocated_bytes: 1024,
            max_output_bytes: 256,
        }
    );
    assert_eq!(intent.input_constraints.len(), 1);
    assert_eq!(intent.input_constraints[0].coordinate, "where.0");
    assert!(matches!(
        intent.input_constraints[0].predicate,
        CorePredicate::Compare { .. }
    ));
}

#[test]
fn effect_result_bindings_are_preserved_in_echo_span_ir() {
    let artifact = effectful_artifact(CHAINED_EFFECT_RESULTS);
    let intent = artifact.intents.get("t").expect("intent t");

    assert_eq!(intent.steps.len(), 2);
    assert_eq!(intent.steps[0].binding.id, "local.0");
    assert_eq!(intent.steps[1].binding.id, "local.1");

    let CoreExpr::Field { base, field } = &intent.steps[1].input else {
        panic!("second effect input reads from first effect result");
    };
    assert_eq!(field, "id");
    assert!(matches!(base.as_ref(), CoreExpr::Local { reference } if reference.id == "local.0"));
}

#[test]
fn unsupported_target_profile_rejects_without_artifact() {
    let core = effectful_core();
    let mut facts = echo_facts();
    facts.target_profile.coordinate = "kv.transactional@1".to_owned();
    facts.target_ir_domain = "kv.transaction-ir/v1".to_owned();

    let report = lower_to_target_ir(&core, &facts);

    assert_eq!(report.status, TargetLoweringStatus::Unsupported);
    assert!(report.artifact.is_none());
    assert_eq!(
        failure_kinds(&report),
        vec![TargetLoweringFailureKind::UnsupportedTargetProfile]
    );
}

#[test]
fn unsupported_target_ir_domain_rejects_without_artifact() {
    let mut facts = echo_facts();
    facts.target_ir_domain = "echo.span-ir/v99".to_owned();

    let report = lower_to_target_ir(&effectful_core(), &facts);

    assert_eq!(report.status, TargetLoweringStatus::Unsupported);
    assert!(report.artifact.is_none());
    assert_eq!(
        failure_kinds(&report),
        vec![TargetLoweringFailureKind::UnsupportedTargetIrDomain]
    );
}

#[test]
fn undigested_target_profile_rejects_without_artifact() {
    for digest in [None, Some("sha256:not-a-review-digest".to_owned())] {
        let mut facts = echo_facts();
        facts.target_profile.digest = digest;

        let report = lower_to_target_ir(&effectful_core(), &facts);

        assert_eq!(report.status, TargetLoweringStatus::Unsupported);
        assert!(report.artifact.is_none());
        assert_eq!(
            failure_kinds(&report),
            vec![TargetLoweringFailureKind::UndigestedTargetProfile]
        );
    }
}

#[test]
fn missing_effect_lowering_rejects_without_artifact() {
    let mut facts = echo_facts();
    facts.effect_lowerings.clear();

    let report = lower_to_target_ir(&effectful_core(), &facts);

    assert_eq!(report.status, TargetLoweringStatus::Unsupported);
    assert!(report.artifact.is_none());
    assert_eq!(
        failure_kinds(&report),
        vec![TargetLoweringFailureKind::MissingEffectLowering]
    );
}

#[test]
fn ambiguous_effect_lowering_rejects_without_artifact() {
    let mut facts = echo_facts();
    facts.effect_lowerings.push(TargetEffectLowering {
        effect: "target.replace".to_owned(),
        target_intrinsic: "echo.dpo@1.replace.alternate".to_owned(),
        failure_mappings: BTreeMap::new(),
    });

    let report = lower_to_target_ir(&effectful_core(), &facts);

    assert_eq!(report.status, TargetLoweringStatus::Unsupported);
    assert!(report.artifact.is_none());
    assert_eq!(
        failure_kinds(&report),
        vec![TargetLoweringFailureKind::AmbiguousEffectLowering]
    );
}

#[test]
fn unsupported_operation_profile_rejects_without_artifact() {
    let module = edict_syntax::parse_module(EFFECTFUL_REPLACE).expect("effectful source parses");
    let core = compile_to_core(
        &module,
        &effectful_context_with_profile("continuum.profile.unreviewed/v1"),
    )
    .expect("effectful source compiles to Core with caller-supplied profile");

    let report = lower_to_target_ir(&core, &echo_facts());

    assert_eq!(report.status, TargetLoweringStatus::Unsupported);
    assert!(report.artifact.is_none());
    assert_eq!(
        failure_kinds(&report),
        vec![TargetLoweringFailureKind::MissingOperationProfile]
    );
}

#[test]
fn foreign_target_intrinsic_rejects_without_artifact() {
    let mut facts = echo_facts();
    facts.effect_lowerings[0].target_intrinsic = "kv.transactional@1.get".to_owned();

    let report = lower_to_target_ir(&effectful_core(), &facts);

    assert_eq!(report.status, TargetLoweringStatus::Unsupported);
    assert!(report.artifact.is_none());
    assert_eq!(
        failure_kinds(&report),
        vec![TargetLoweringFailureKind::UnsupportedTargetIntrinsic]
    );
}

#[test]
fn unsupported_obstruction_key_rejects_without_artifact() {
    let module = edict_syntax::parse_module(
        &EFFECTFUL_REPLACE.replace("rejected(reason) =>", "unexpected(reason) =>"),
    )
    .expect("effectful source parses");
    let core =
        compile_to_core(&module, &effectful_context()).expect("effectful source compiles to Core");

    let report = lower_to_target_ir(&core, &echo_facts());

    assert_eq!(report.status, TargetLoweringStatus::Unsupported);
    assert!(report.artifact.is_none());
    assert_eq!(
        failure_kinds(&report),
        vec![TargetLoweringFailureKind::MissingObstruction]
    );
}

#[test]
fn every_copied_target_expression_requires_closed_helper_authority() {
    let unbound_call = || CoreExpr::Call {
        callee: "hello.echo@1.notExported".to_owned(),
        type_args: Vec::new(),
        args: Vec::new(),
    };
    let require_core = || {
        let module = edict_syntax::parse_module(ECHO_CONTINUE_OBSTRUCTED_REQUIRE)
            .expect("require source parses");
        compile_to_core(&module, &effectful_context()).expect("require source compiles")
    };

    let mut basis = require_core();
    basis.intents.get_mut("t").expect("intent t").basis = Some(unbound_call());

    let mut input_constraint = require_core();
    input_constraint
        .intents
        .get_mut("t")
        .expect("intent t")
        .input_constraints
        .push(InputConstraint {
            coordinate: "review.constraint".to_owned(),
            source: InputConstraintSource::Compiler,
            predicate: CorePredicate::Compare {
                op: CompareOp::Eq,
                left: unbound_call(),
                right: CoreExpr::Const(CoreValue::Bool(true)),
            },
        });

    let mut require_predicate = require_core();
    let CoreNode::Require { predicate, .. } = &mut require_predicate
        .intents
        .get_mut("t")
        .expect("intent t")
        .body
        .nodes[0]
    else {
        panic!("fixture begins with a requirement");
    };
    *predicate = CorePredicate::Compare {
        op: CompareOp::Eq,
        left: unbound_call(),
        right: CoreExpr::Const(CoreValue::Bool(true)),
    };

    let mut require_payload = require_core();
    let CoreNode::Require { arm, .. } = &mut require_payload
        .intents
        .get_mut("t")
        .expect("intent t")
        .body
        .nodes[0]
    else {
        panic!("fixture begins with a requirement");
    };
    let reason = match arm {
        edict_syntax::CoreRequireFailureArm::Terminal { reason }
        | edict_syntax::CoreRequireFailureArm::ContinueObstructed { reason } => reason,
    };
    reason.payload.insert("forged".to_owned(), unbound_call());

    let mut effect_input = effectful_core();
    let CoreNode::Effect { input, .. } = &mut effect_input
        .intents
        .get_mut("t")
        .expect("intent t")
        .body
        .nodes[0]
    else {
        panic!("fixture begins with an effect");
    };
    *input = unbound_call();

    let mut obstruction_value = effectful_core();
    let CoreNode::Effect {
        obstruction_map, ..
    } = &mut obstruction_value
        .intents
        .get_mut("t")
        .expect("intent t")
        .body
        .nodes[0]
    else {
        panic!("fixture begins with an effect");
    };
    let Some(arm) = obstruction_map.values_mut().next() else {
        panic!("fixture carries an obstruction arm");
    };
    arm.value = CoreExpr::Call {
        callee: "hello.echo@1.notExported".to_owned(),
        type_args: Vec::new(),
        args: vec![CoreExpr::Const(CoreValue::Bool(true))],
    };

    assert_invalid_copied_expression_cases([
        ("intent basis", basis),
        ("input constraint", input_constraint),
        ("require predicate", require_predicate),
        ("require payload", require_payload),
        ("effect input", effect_input),
        ("obstruction value", obstruction_value),
    ]);
}

fn assert_invalid_copied_expression_cases(
    cases: impl IntoIterator<Item = (&'static str, edict_syntax::CoreModule)>,
) {
    for (case, core) in cases {
        let report = lower_to_target_ir(&core, &echo_facts());

        assert_eq!(report.status, TargetLoweringStatus::Unsupported, "{case}");
        assert!(report.artifact.is_none(), "{case}");
        let [failure] = report.failures.as_slice() else {
            panic!("{case} must reject with one structured failure");
        };
        assert_eq!(
            failure.kind,
            TargetLoweringFailureKind::InvalidCoreIdentity,
            "{case}"
        );
    }
}

#[test]
fn colliding_target_obstruction_mappings_reject_without_artifact() {
    let mut core = effectful_core();
    let intent = core.intents.get_mut("t").expect("intent t");
    let mut alternate_arm = match &intent.body.nodes[0] {
        CoreNode::Effect {
            obstruction_map, ..
        } => obstruction_map
            .get("rejected")
            .expect("fixture declares rejected obstruction")
            .clone(),
        _ => panic!("fixture begins with an effect node"),
    };
    alternate_arm.binder = LocalRef {
        id: "obstruction.1".to_owned(),
        alpha_name: "$obstruction1".to_owned(),
        ty: "Unit".to_owned(),
    };
    intent.body.locals.push(alternate_arm.binder.clone());
    let CoreNode::Effect {
        obstruction_map, ..
    } = &mut intent.body.nodes[0]
    else {
        panic!("fixture begins with an effect node");
    };
    obstruction_map.insert("alternate".to_owned(), alternate_arm);
    let mut facts = echo_facts();
    facts.effect_lowerings[0]
        .failure_mappings
        .insert("alternate".to_owned(), "rejected".to_owned());

    let report = lower_to_target_ir(&core, &facts);

    assert_eq!(report.status, TargetLoweringStatus::Unsupported);
    assert!(report.artifact.is_none());
    assert_eq!(
        failure_kinds(&report),
        vec![TargetLoweringFailureKind::AmbiguousObstructionMapping]
    );
    assert_eq!(report.failures[0].detail, "rejected");
}

#[test]
fn empty_target_step_intents_reject_without_artifact() {
    let mut core = effectful_core();
    core.intents
        .get_mut("t")
        .expect("intent t")
        .body
        .nodes
        .clear();

    let report = lower_to_target_ir(&core, &echo_facts());

    assert_eq!(report.status, TargetLoweringStatus::Unsupported);
    assert!(report.artifact.is_none());
    assert_eq!(
        failure_kinds(&report),
        vec![TargetLoweringFailureKind::NoTargetSteps]
    );
}

#[test]
fn empty_core_modules_reject_without_artifact() {
    let mut core = effectful_core();
    core.intents.clear();

    let report = lower_to_target_ir(&core, &echo_facts());

    assert_eq!(report.status, TargetLoweringStatus::Unsupported);
    assert!(report.artifact.is_none());
    assert_eq!(
        failure_kinds(&report),
        vec![TargetLoweringFailureKind::NoTargetSteps]
    );
}

#[test]
fn pure_core_bindings_lower_as_generic_target_program() {
    let core = pure_core();
    let mut facts = echo_facts();
    facts
        .operation_profiles
        .push("continuum.profile.read-only/v1".to_owned());
    let report = lower_to_target_ir(&core, &facts);

    assert_eq!(report.status, TargetLoweringStatus::Lowered);
    assert_eq!(
        report.failures,
        Vec::<edict_syntax::TargetLoweringFailure>::new()
    );
    let artifact = report.artifact.as_ref().expect("pure Target IR");
    let intent = artifact.intents.get("sayHello").expect("pure intent");
    assert_eq!(intent.pure_bindings.len(), 1);
    assert_eq!(intent.pure_bindings[0].id, "sayHello.binding.0");
    assert!(matches!(
        intent.pure_bindings[0].value,
        CoreExpr::Call { ref callee, .. } if callee == "core.string.concat"
    ));
    assert!(
        report.result_projections.contains_key("sayHello"),
        "projection failures: {:?}",
        report.result_projection_failures
    );
}

#[test]
fn unsupported_core_nodes_reject_without_artifact() {
    let mut core = pure_core();
    core.intents
        .get_mut("sayHello")
        .expect("pure intent")
        .body
        .nodes = vec![CoreNode::For {
        binder: LocalRef {
            id: "local.1".to_owned(),
            alpha_name: "$local1".to_owned(),
            ty: "U64".to_owned(),
        },
        iter: CoreExpr::Const(CoreValue::Null),
        bound: CoreBound::Literal(1),
        body: CoreBlock {
            locals: Vec::new(),
            nodes: Vec::new(),
            result: CoreExpr::Const(CoreValue::Null),
        },
    }];

    let report = lower_to_target_ir(&core, &pure_target_facts());

    assert_eq!(report.status, TargetLoweringStatus::Unsupported);
    assert!(report.artifact.is_none());
    assert_eq!(
        failure_kinds(&report),
        vec![TargetLoweringFailureKind::UnsupportedCoreNode]
    );
}

#[test]
fn malformed_pure_binding_graphs_reject_before_target_artifact() {
    let cases = [
        (
            "dangling local",
            LocalRef {
                id: "local.999".to_owned(),
                alpha_name: "$local999".to_owned(),
                ty: "String<max=512,canonical=raw-utf8>".to_owned(),
            },
        ),
        (
            "self reference",
            LocalRef {
                id: "local.0".to_owned(),
                alpha_name: "$local0".to_owned(),
                ty: "String<max=512,canonical=raw-utf8>".to_owned(),
            },
        ),
        (
            "conflicting local type",
            LocalRef {
                id: "arg.0".to_owned(),
                alpha_name: "$arg0".to_owned(),
                ty: "Bytes<max=32>".to_owned(),
            },
        ),
    ];

    for (case, reference) in cases {
        let mut core = pure_core();
        let intent = core.intents.get_mut("sayHello").expect("pure intent");
        let CoreNode::Let { value, .. } = &mut intent.body.nodes[0] else {
            panic!("pure fixture starts with a let");
        };
        *value = CoreExpr::Local { reference };

        let report = lower_to_target_ir(&core, &pure_target_facts());

        assert_eq!(report.status, TargetLoweringStatus::Unsupported, "{case}");
        assert!(report.artifact.is_none(), "{case}");
        assert_eq!(
            failure_kinds(&report),
            vec![TargetLoweringFailureKind::InvalidCoreIdentity],
            "{case}"
        );
    }

    let mut duplicate = pure_core();
    let intent = duplicate.intents.get_mut("sayHello").expect("pure intent");
    intent.body.nodes.push(intent.body.nodes[0].clone());
    let report = lower_to_target_ir(&duplicate, &pure_target_facts());
    assert_eq!(report.status, TargetLoweringStatus::Unsupported);
    assert!(report.artifact.is_none());
    assert_eq!(
        failure_kinds(&report),
        vec![TargetLoweringFailureKind::InvalidCoreIdentity]
    );

    let mut duplicate_local = pure_core();
    let intent = duplicate_local
        .intents
        .get_mut("sayHello")
        .expect("pure intent");
    intent.body.locals.push(intent.body.locals[1].clone());
    let report = lower_to_target_ir(&duplicate_local, &pure_target_facts());
    assert_eq!(report.status, TargetLoweringStatus::Unsupported);
    assert!(report.artifact.is_none());
    assert_eq!(
        failure_kinds(&report),
        vec![TargetLoweringFailureKind::InvalidCoreIdentity]
    );
}

#[test]
fn dangling_pure_result_rejects_before_target_artifact() {
    let mut core = pure_core();
    core.intents
        .get_mut("sayHello")
        .expect("pure intent")
        .body
        .result = CoreExpr::Local {
        reference: LocalRef {
            id: "local.999".to_owned(),
            alpha_name: "$local999".to_owned(),
            ty: "examples.hello@1.HelloReading".to_owned(),
        },
    };

    let report = lower_to_target_ir(&core, &pure_target_facts());

    assert_eq!(report.status, TargetLoweringStatus::Unsupported);
    assert!(report.artifact.is_none());
    let [failure] = report.failures.as_slice() else {
        panic!("a dangling result rejects with exactly one failure");
    };
    assert_eq!(failure.kind, TargetLoweringFailureKind::InvalidCoreIdentity);
    assert_eq!(failure.intent.as_deref(), Some("sayHello"));
    assert_eq!(failure.node_index, None);
}

#[test]
fn type_incompatible_pure_binding_rejects_before_target_artifact() {
    let mut core = pure_core();
    let intent = core.intents.get_mut("sayHello").expect("pure intent");
    let CoreNode::Let { value, .. } = &mut intent.body.nodes[0] else {
        panic!("pure fixture starts with a let");
    };
    *value = CoreExpr::Const(CoreValue::Bool(true));

    let report = lower_to_target_ir(&core, &pure_target_facts());

    assert_eq!(report.status, TargetLoweringStatus::Unsupported);
    assert!(report.artifact.is_none());
    let [failure] = report.failures.as_slice() else {
        panic!("an incompatible binding rejects with exactly one failure");
    };
    assert_eq!(failure.kind, TargetLoweringFailureKind::InvalidCoreIdentity);
    assert_eq!(failure.intent.as_deref(), Some("sayHello"));
    assert_eq!(failure.node_index, Some(0));
}

#[test]
fn narrow_core_integer_widths_lower_as_builtin_types() {
    for (index, (width, value)) in [
        ("I8", "-128"),
        ("I16", "-32768"),
        ("U8", "255"),
        ("U16", "65535"),
    ]
    .into_iter()
    .enumerate()
    {
        let mut core = pure_core();
        assert!(
            !core.types.contains_key(width),
            "built-in widths must not require redundant Core type entries"
        );
        let intent = core.intents.get_mut("sayHello").expect("pure intent");
        let binding = LocalRef {
            id: format!("local.narrow.{index}"),
            alpha_name: format!("$narrow{index}"),
            ty: width.to_owned(),
        };
        intent.body.locals.push(binding.clone());
        intent.body.nodes.insert(
            0,
            CoreNode::Let {
                binding: binding.clone(),
                value: CoreExpr::Const(CoreValue::Int {
                    width: width.to_owned(),
                    value: value.to_owned(),
                }),
            },
        );

        let report = lower_to_target_ir(&core, &pure_target_facts());

        assert_eq!(
            report.status,
            TargetLoweringStatus::Lowered,
            "{width} should lower without a redundant type entry: {:?}",
            report.failures
        );
        let artifact = report.artifact.expect("narrow integer Target IR");
        let lowered = artifact
            .intents
            .get("sayHello")
            .expect("pure intent")
            .pure_bindings
            .iter()
            .find(|candidate| candidate.binding.id == binding.id)
            .expect("narrow integer binding");
        assert_eq!(lowered.binding.ty, width);
        assert_eq!(
            lowered.value,
            CoreExpr::Const(CoreValue::Int {
                width: width.to_owned(),
                value: value.to_owned(),
            })
        );
    }
}

#[test]
fn ranged_core_byte_coordinates_lower_as_builtin_types() {
    let mut core = pure_core();
    core.types.remove("HelloReading.message");
    let Some(CoreType::Record { fields }) = core.types.get_mut("HelloReading") else {
        panic!("pure fixture has a named result record");
    };
    fields.insert("message".to_owned(), "Bytes<min=2,max=4>".to_owned());
    let intent = core.intents.get_mut("sayHello").expect("pure intent");
    let CoreNode::Let { binding, value } = &mut intent.body.nodes[0] else {
        panic!("pure fixture starts with a let");
    };
    let binding_id = binding.id.clone();
    binding.ty = "Bytes<min=2,max=4>".to_owned();
    *value = CoreExpr::Const(CoreValue::Bytes(vec![0x01, 0x02, 0x03]));
    let local = intent
        .body
        .locals
        .iter_mut()
        .find(|local| local.id == binding_id)
        .expect("pure binding local");
    local.ty.clone_from(&binding.ty);
    let CoreExpr::Record { fields } = &mut intent.body.result else {
        panic!("pure fixture returns a record");
    };
    let Some(CoreExpr::Local { reference }) = fields.get_mut("message") else {
        panic!("pure fixture returns the binding as message");
    };
    reference.ty.clone_from(&binding.ty);

    let report = lower_to_target_ir(&core, &pure_target_facts());

    assert_eq!(report.status, TargetLoweringStatus::Lowered);
    assert_eq!(
        report.failures,
        Vec::<edict_syntax::TargetLoweringFailure>::new()
    );
    assert!(report.artifact.is_some());

    let CoreNode::Let { value, .. } = &mut core
        .intents
        .get_mut("sayHello")
        .expect("pure intent")
        .body
        .nodes[0]
    else {
        panic!("pure fixture starts with a let");
    };
    *value = CoreExpr::Const(CoreValue::Bytes(vec![0x01]));

    let report = lower_to_target_ir(&core, &pure_target_facts());

    assert_eq!(report.status, TargetLoweringStatus::Unsupported);
    assert!(report.artifact.is_none());
    assert_eq!(
        failure_kinds(&report),
        vec![TargetLoweringFailureKind::InvalidCoreIdentity]
    );
}

#[test]
fn pure_conditional_predicates_require_compatible_bounded_operands() {
    let cases = [
        (
            "incompatible operand types",
            CoreExpr::Const(CoreValue::Bool(true)),
            CoreExpr::Const(CoreValue::Int {
                width: "U64".to_owned(),
                value: "1".to_owned(),
            }),
        ),
        (
            "constant outside its declared width",
            CoreExpr::Const(CoreValue::Int {
                width: "U8".to_owned(),
                value: "0".to_owned(),
            }),
            CoreExpr::Const(CoreValue::Int {
                width: "U8".to_owned(),
                value: "256".to_owned(),
            }),
        ),
    ];

    for (case, left, right) in cases {
        let mut core = pure_core();
        let intent = core.intents.get_mut("sayHello").expect("pure intent");
        let CoreNode::Let { value, .. } = &mut intent.body.nodes[0] else {
            panic!("pure fixture starts with a let");
        };
        let valid_branch = value.clone();
        *value = CoreExpr::If {
            predicate: Box::new(CorePredicate::Compare {
                op: CompareOp::Eq,
                left,
                right,
            }),
            then_value: Box::new(valid_branch.clone()),
            else_value: Box::new(valid_branch),
        };

        let report = lower_to_target_ir(&core, &pure_target_facts());

        assert_eq!(report.status, TargetLoweringStatus::Unsupported, "{case}");
        assert!(report.artifact.is_none(), "{case}");
        let [failure] = report.failures.as_slice() else {
            panic!("{case} must reject with exactly one structured failure");
        };
        assert_eq!(failure.kind, TargetLoweringFailureKind::InvalidCoreIdentity);
        assert_eq!(failure.intent.as_deref(), Some("sayHello"));
        assert_eq!(failure.node_index, Some(0));
    }
}

#[test]
fn pure_conditional_comparison_accepts_supported_call_operands() {
    let mut core = pure_core();
    let intent = core.intents.get_mut("sayHello").expect("pure intent");
    let CoreNode::Let { value, .. } = &mut intent.body.nodes[0] else {
        panic!("pure fixture starts with a let");
    };
    let valid_call = value.clone();
    *value = CoreExpr::If {
        predicate: Box::new(CorePredicate::Compare {
            op: CompareOp::Eq,
            left: valid_call.clone(),
            right: valid_call.clone(),
        }),
        then_value: Box::new(valid_call.clone()),
        else_value: Box::new(valid_call),
    };

    let report = lower_to_target_ir(&core, &pure_target_facts());

    assert_eq!(report.status, TargetLoweringStatus::Lowered);
    assert_eq!(
        report.failures,
        Vec::<edict_syntax::TargetLoweringFailure>::new()
    );
    assert!(report.artifact.is_some());
}

#[test]
fn compiler_produced_conditional_comparisons_infer_compatible_branch_types() {
    let source = "package a.b@1;\n\
        type Input = { value: String<max=32>, };\n\
        type Output = { value: String<max=32>, };\n\
        intent t(input: Input) returns Output\n\
          profile hello.readOnly\n\
          basis none\n\
          budget <= hello.tinyBudget {\n\
          let value = if (if true then \"a\" else \"bb\")\n\
            == (if false then \"ccc\" else \"dddd\")\n\
            then input.value else input.value;\n\
          return { value };\n\
        }";
    let module = edict_syntax::parse_module(source).expect("conditional comparison parses");
    let core = compile_to_core(&module, &pure_context())
        .expect("compatible conditional comparison compiles to Core");
    let CoreNode::Let {
        value:
            CoreExpr::If {
                predicate,
                then_value: _,
                else_value: _,
            },
        ..
    } = &core.intents.get("t").expect("intent t").body.nodes[0]
    else {
        panic!("compiler preserves the outer conditional expression");
    };
    let CorePredicate::Compare { left, right, .. } = predicate.as_ref() else {
        panic!("compiler preserves the comparison predicate");
    };
    assert!(matches!(left, CoreExpr::If { .. }));
    assert!(matches!(right, CoreExpr::If { .. }));

    let report = lower_to_target_ir(&core, &pure_target_facts());

    assert_eq!(report.status, TargetLoweringStatus::Lowered);
    assert_eq!(
        report.failures,
        Vec::<edict_syntax::TargetLoweringFailure>::new()
    );
    assert!(report.artifact.is_some());
}

#[test]
fn compiler_produced_conditional_record_comparisons_preserve_structural_compatibility() {
    let source = "package a.b@1;\n\
        type Input = { value: String<max=32>, };\n\
        type Output = { value: String<max=32>, };\n\
        intent t(input: Input) returns Output\n\
          profile hello.readOnly\n\
          basis none\n\
          budget <= hello.tinyBudget {\n\
          let value = if (if true then { value: \"a\" } else { value: \"bb\" })\n\
            == { value: \"ccc\" }\n\
            then input.value else input.value;\n\
          return { value };\n\
        }";
    let module = edict_syntax::parse_module(source).expect("record comparison parses");
    let core = compile_to_core(&module, &pure_context())
        .expect("compatible anonymous record comparison compiles to Core");

    let report = lower_to_target_ir(&core, &pure_target_facts());

    assert_eq!(report.status, TargetLoweringStatus::Lowered);
    assert_eq!(
        report.failures,
        Vec::<edict_syntax::TargetLoweringFailure>::new()
    );
    assert!(report.artifact.is_some());

    let mut incompatible = core;
    let CoreNode::Let {
        value: CoreExpr::If { predicate, .. },
        ..
    } = &mut incompatible
        .intents
        .get_mut("t")
        .expect("intent t")
        .body
        .nodes[0]
    else {
        panic!("compiler preserves the outer conditional expression");
    };
    let CorePredicate::Compare { right, .. } = predicate.as_mut() else {
        panic!("compiler preserves the record comparison predicate");
    };
    let CoreExpr::Record { fields } = right else {
        panic!("comparison keeps the right anonymous record");
    };
    let value = fields.remove("value").expect("record value field");
    fields.insert("other".to_owned(), value);

    let report = lower_to_target_ir(&incompatible, &pure_target_facts());

    assert_eq!(report.status, TargetLoweringStatus::Unsupported);
    assert!(report.artifact.is_none());
    assert_eq!(
        failure_kinds(&report),
        vec![TargetLoweringFailureKind::InvalidCoreIdentity]
    );
}

#[test]
fn compiler_produced_anonymous_record_fields_preserve_structural_types() {
    let cases = [
        ("record", "({ value: input.value }).value"),
        (
            "conditional record",
            "(if true then { value: input.value } else { value: input.value }).value",
        ),
    ];

    for (case, value) in cases {
        let source = format!(
            "package a.b@1;\n\
             type Input = {{ value: U64, }};\n\
             type Output = {{ value: U64, }};\n\
             intent t(input: Input) returns Output\n\
               profile hello.readOnly\n\
               basis none\n\
               budget <= hello.tinyBudget {{\n\
               let value: U64 = {value};\n\
               return {{ value }};\n\
             }}"
        );
        let module = edict_syntax::parse_module(&source)
            .unwrap_or_else(|errors| panic!("{case} field source parses: {errors:?}"));
        let core = compile_to_core(&module, &pure_context())
            .unwrap_or_else(|errors| panic!("{case} field compiles to Core: {errors:?}"));
        let CoreNode::Let {
            value: CoreExpr::Field { base, .. },
            ..
        } = &core.intents.get("t").expect("intent t").body.nodes[0]
        else {
            panic!("{case} remains a field projection in Core");
        };
        assert!(
            matches!(
                (case, base.as_ref()),
                ("record", CoreExpr::Record { .. }) | ("conditional record", CoreExpr::If { .. })
            ),
            "{case} keeps its structural record base"
        );

        let report = lower_to_target_ir(&core, &pure_target_facts());

        assert_eq!(report.status, TargetLoweringStatus::Lowered, "{case}");
        assert!(report.failures.is_empty(), "{case}: {:?}", report.failures);
        assert!(report.artifact.is_some(), "{case}");
    }
}

#[test]
fn compiler_produced_string_concat_uses_raw_result_canonicalization() {
    let lawpack = ResourceRef {
        coordinate: "text.rules@1".to_owned(),
        digest: Some(format!("sha256:{}", "3".repeat(64))),
    };
    let context = pure_context().with_type_shape(TypeShapeFact {
        lawpack,
        coordinate: "text.rules@1.Nfc".to_owned(),
        definition: "String<max=8,canonical=unicode-scalar-nfc>".to_owned(),
    });

    for (case, tail_type) in [
        ("all NFC operands", "text.Nfc"),
        ("mixed NFC and raw operands", "String<max=8>"),
    ] {
        let source = format!(
            "package a.b@1;\n\
             use lawpack text.rules@1 digest \"sha256:{}\" as text;\n\
             type Input = {{ left: text.Nfc, right: text.Nfc, tail: {tail_type}, }};\n\
             type Output = {{ value: String<max=24>, }};\n\
             intent t(input: Input) returns Output\n\
               profile hello.readOnly\n\
               basis none\n\
               budget <= hello.tinyBudget {{\n\
               let value = input.left + input.right + input.tail;\n\
               return {{ value }};\n\
             }}",
            "3".repeat(64)
        );
        let module = edict_syntax::parse_module(&source)
            .unwrap_or_else(|errors| panic!("{case} source parses: {errors:?}"));
        let core = compile_to_core(&module, &context)
            .unwrap_or_else(|errors| panic!("{case} source compiles: {errors:?}"));
        let CoreNode::Let { binding, value } =
            &core.intents.get("t").expect("intent t").body.nodes[0]
        else {
            panic!("{case} starts with a pure let");
        };
        assert_eq!(
            binding.ty, "String<max=24,canonical=raw-utf8>",
            "the compiler defines concat result canonicalization"
        );
        assert!(matches!(
            value,
            CoreExpr::Call { callee, .. } if callee == "core.string.concat"
        ));

        let report = lower_to_target_ir(&core, &pure_target_facts());

        assert_eq!(report.status, TargetLoweringStatus::Lowered, "{case}");
        assert!(report.failures.is_empty(), "{case}: {:?}", report.failures);
        assert!(report.artifact.is_some(), "{case}");
    }
}

#[test]
fn target_comparisons_require_one_record_compatibility_direction() {
    let mut core = pure_core();
    let intent = core.intents.get_mut("sayHello").expect("pure intent");
    let CoreNode::Let { value, .. } = &mut intent.body.nodes[0] else {
        panic!("pure fixture starts with a let");
    };
    let valid_branch = value.clone();
    let record = |a: usize, b: usize| CoreExpr::Record {
        fields: BTreeMap::from([
            (
                "a".to_owned(),
                CoreExpr::Const(CoreValue::String("a".repeat(a))),
            ),
            (
                "b".to_owned(),
                CoreExpr::Const(CoreValue::String("b".repeat(b))),
            ),
        ]),
    };
    *value = CoreExpr::If {
        predicate: Box::new(CorePredicate::Compare {
            op: CompareOp::Eq,
            left: record(1, 3),
            right: record(3, 1),
        }),
        then_value: Box::new(valid_branch.clone()),
        else_value: Box::new(valid_branch),
    };

    let report = lower_to_target_ir(&core, &pure_target_facts());

    assert_eq!(report.status, TargetLoweringStatus::Unsupported);
    assert!(report.artifact.is_none());
    assert_eq!(
        failure_kinds(&report),
        vec![TargetLoweringFailureKind::InvalidCoreIdentity]
    );
}

#[test]
fn lawpack_effect_signatures_reject_mismatched_core_values() {
    let manifest = include_bytes!("../../../fixtures/lawpack/hello-echo/manifest.cbor").as_slice();
    let exports = include_bytes!("../../../fixtures/lawpack/hello-echo/exports.cbor").as_slice();
    let adapter = include_bytes!("../../../fixtures/lawpack/hello-echo/adapter.cbor").as_slice();
    let source = include_str!("../../../fixtures/lawpack/hello-echo/create-greeting.edict");
    let bundle =
        edict_syntax::decode_lawpack_bundle(manifest, exports).expect("load Hello Echo lawpack");
    let adapter = edict_syntax::decode_lawpack_adapter(&bundle, "echo.dpo@1", adapter)
        .expect("load Hello Echo adapter");
    let module = edict_syntax::parse_module(source).expect("parse Hello Echo source");
    let preparation = edict_syntax::prepare_lawpack_compilation(&module, &bundle, &adapter)
        .expect("prepare Hello Echo compilation");
    let core =
        compile_to_core(&module, preparation.compiler_context()).expect("compile Hello Echo Core");
    let facts = preparation.target_ir_facts();

    let control = lower_to_target_ir(&core, facts);
    assert_eq!(control.status, TargetLoweringStatus::Lowered);
    assert_eq!(
        control.failures,
        Vec::<edict_syntax::TargetLoweringFailure>::new()
    );
    assert!(control.artifact.is_some());

    let mut input_mismatch = core.clone();
    let CoreNode::Effect { input, .. } = &mut input_mismatch
        .intents
        .get_mut("createGreeting")
        .expect("createGreeting intent")
        .body
        .nodes[0]
    else {
        panic!("Hello Echo starts with its effect");
    };
    *input = CoreExpr::Const(CoreValue::Bool(true));

    let mut output_mismatch = core.clone();
    let intent = output_mismatch
        .intents
        .get_mut("createGreeting")
        .expect("createGreeting intent");
    let wrong_output = intent.input.clone();
    let binding_id = {
        let CoreNode::Effect { binding, .. } = &mut intent.body.nodes[0] else {
            panic!("Hello Echo starts with its effect");
        };
        binding.ty.clone_from(&wrong_output);
        binding.id.clone()
    };
    let Some(local) = intent
        .body
        .locals
        .iter_mut()
        .find(|local| local.id == binding_id)
    else {
        panic!("effect result remains in the Core local table");
    };
    local.ty = wrong_output;
    let CoreExpr::Record { fields } = &mut intent.body.result else {
        panic!("Hello Echo returns a record");
    };
    let CoreExpr::Field { base, .. } = fields.get_mut("key").expect("result key field") else {
        panic!("result key reads the effect receipt");
    };
    let CoreExpr::Local { reference } = base.as_mut() else {
        panic!("receipt field base is the effect result local");
    };
    assert_eq!(reference.id, binding_id);
    reference.ty.clone_from(&local.ty);

    let mut type_definition_mismatch = core.clone();
    let prior = type_definition_mismatch.types.insert(
        "hello.echo@1.CreateGreetingInput".to_owned(),
        CoreType::Bool,
    );
    assert!(matches!(prior, Some(CoreType::Record { .. })));

    let mut missing_facts = facts.clone();
    missing_facts.effect_signatures.clear();
    let missing_report = lower_to_target_ir(&core, &missing_facts);
    assert_invalid_core_identity(&missing_report, "missing signature fact");

    let mut duplicate_facts = facts.clone();
    duplicate_facts
        .effect_signatures
        .push(duplicate_facts.effect_signatures[0].clone());
    let duplicate_report = lower_to_target_ir(&core, &duplicate_facts);
    assert_invalid_core_identity(&duplicate_report, "duplicate signature fact");

    let reports = [
        ("input", lower_to_target_ir(&input_mismatch, facts)),
        ("output", lower_to_target_ir(&output_mismatch, facts)),
        (
            "type definition",
            lower_to_target_ir(&type_definition_mismatch, facts),
        ),
    ];
    for (case, report) in reports {
        assert_invalid_core_identity(&report, case);
    }
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
        let mut core = pure_core();
        core.types.insert(coordinate.to_owned(), CoreType::Bool);

        let report = lower_to_target_ir(&core, &pure_target_facts());

        assert_invalid_core_identity(&report, case);
    }
}

#[test]
fn compiler_produced_bounded_lists_lower_through_target_ir() {
    let source = "package a.b@1;\n\
        type Input = { items: List<String<max=8>, max=4>, };\n\
        type Output = { items: List<String<max=8>, max=4>, };\n\
        intent t(input: Input) returns Output\n\
          profile hello.readOnly\n\
          basis none\n\
          budget <= hello.tinyBudget {\n\
          let copy: List<String<max=8>, max=4> = input.items;\n\
          return { items: copy };\n\
        }";
    let module = edict_syntax::parse_module(source).expect("bounded-list source parses");
    let core = compile_to_core(&module, &pure_context()).expect("bounded-list source compiles");

    let report = lower_to_target_ir(&core, &pure_target_facts());

    assert_eq!(report.status, TargetLoweringStatus::Lowered);
    assert_eq!(
        report.failures,
        Vec::<edict_syntax::TargetLoweringFailure>::new()
    );
    assert!(report.artifact.is_some());

    let mut max_mismatch = core.clone();
    replace_first_let_binding_type(&mut max_mismatch, ",max=4>", ",max=3>");
    let mut item_mismatch = core;
    replace_first_let_binding_type(
        &mut item_mismatch,
        "String<max=8,canonical=raw-utf8>",
        "Bool",
    );

    for (case, incompatible) in [("maximum", max_mismatch), ("item", item_mismatch)] {
        let report = lower_to_target_ir(&incompatible, &pure_target_facts());
        assert_invalid_core_identity(&report, case);
    }
}

#[test]
fn caller_authored_byte_comparisons_require_directional_compatibility() {
    let byte = |length: usize| CoreExpr::Const(CoreValue::Bytes(vec![0x11; length]));
    let mut incomparable = pure_core();
    let intent = incomparable
        .intents
        .get_mut("sayHello")
        .expect("pure intent");
    let CoreNode::Let { value, .. } = &mut intent.body.nodes[0] else {
        panic!("pure fixture starts with a let");
    };
    let valid_branch = value.clone();
    *value = CoreExpr::If {
        predicate: Box::new(CorePredicate::Compare {
            op: CompareOp::Eq,
            left: byte(1),
            right: byte(2),
        }),
        then_value: Box::new(valid_branch.clone()),
        else_value: Box::new(valid_branch),
    };

    let report = lower_to_target_ir(&incomparable, &pure_target_facts());
    assert_invalid_core_identity(&report, "disjoint exact-byte comparison");

    let mut conditional_join = pure_core();
    let intent = conditional_join
        .intents
        .get_mut("sayHello")
        .expect("pure intent");
    let CoreNode::Let { value, .. } = &mut intent.body.nodes[0] else {
        panic!("pure fixture starts with a let");
    };
    let valid_branch = value.clone();
    let interval = |predicate| CoreExpr::If {
        predicate: Box::new(predicate),
        then_value: Box::new(byte(1)),
        else_value: Box::new(byte(2)),
    };
    *value = CoreExpr::If {
        predicate: Box::new(CorePredicate::Compare {
            op: CompareOp::Eq,
            left: interval(CorePredicate::True),
            right: interval(CorePredicate::False),
        }),
        then_value: Box::new(valid_branch.clone()),
        else_value: Box::new(valid_branch),
    };

    let report = lower_to_target_ir(&conditional_join, &pure_target_facts());
    assert_eq!(report.status, TargetLoweringStatus::Lowered);
    assert_eq!(
        report.failures,
        Vec::<edict_syntax::TargetLoweringFailure>::new()
    );
    assert!(report.artifact.is_some());
}

#[derive(Clone, Copy)]
struct SemanticParityCase {
    name: &'static str,
    left_definition: &'static str,
    right_definition: &'static str,
    directly_comparable: bool,
}

fn semantic_parity_cases() -> [SemanticParityCase; 10] {
    [
        SemanticParityCase {
            name: "Bool",
            left_definition: "Bool",
            right_definition: "Bool",
            directly_comparable: true,
        },
        SemanticParityCase {
            name: "U64",
            left_definition: "U64",
            right_definition: "U64",
            directly_comparable: true,
        },
        SemanticParityCase {
            name: "bounded strings",
            left_definition: "String<max=1,canonical=raw-utf8>",
            right_definition: "String<max=3,canonical=raw-utf8>",
            directly_comparable: true,
        },
        SemanticParityCase {
            name: "disjoint exact bytes",
            left_definition: "Bytes<exact=1>",
            right_definition: "Bytes<exact=2>",
            directly_comparable: false,
        },
        SemanticParityCase {
            name: "contained byte interval",
            left_definition: "Bytes<exact=1>",
            right_definition: "Bytes<min=1,max=3>",
            directly_comparable: true,
        },
        SemanticParityCase {
            name: "crossed list bounds",
            left_definition: "List<String<max=1,canonical=raw-utf8>,max=3>",
            right_definition: "List<String<max=3,canonical=raw-utf8>,max=1>",
            directly_comparable: false,
        },
        SemanticParityCase {
            name: "one-field records",
            left_definition: "Record<value:String<max=1,canonical=raw-utf8>>",
            right_definition: "Record<value:String<max=3,canonical=raw-utf8>>",
            directly_comparable: true,
        },
        SemanticParityCase {
            name: "two-field crossed records",
            left_definition: "Record<left:String<max=1,canonical=raw-utf8>,right:String<max=3,canonical=raw-utf8>>",
            right_definition: "Record<left:String<max=3,canonical=raw-utf8>,right:String<max=1,canonical=raw-utf8>>",
            directly_comparable: false,
        },
        SemanticParityCase {
            name: "nested crossed records",
            left_definition: "Record<inner:Record<value:String<max=1,canonical=raw-utf8>>,tail:String<max=3,canonical=raw-utf8>>",
            right_definition: "Record<inner:Record<value:String<max=3,canonical=raw-utf8>>,tail:String<max=1,canonical=raw-utf8>>",
            directly_comparable: false,
        },
        SemanticParityCase {
            name: "records inside crossed lists",
            left_definition: "List<Record<left:String<max=1,canonical=raw-utf8>,right:String<max=3,canonical=raw-utf8>>,max=3>",
            right_definition: "List<Record<left:String<max=3,canonical=raw-utf8>,right:String<max=1,canonical=raw-utf8>>,max=1>",
            directly_comparable: false,
        },
    ]
}

fn semantic_parity_lawpack() -> ResourceRef {
    ResourceRef {
        coordinate: "parity.types@1".to_owned(),
        digest: Some(format!("sha256:{}", "7".repeat(64))),
    }
}

fn semantic_parity_context(case: SemanticParityCase) -> CompilerContext {
    let lawpack = semantic_parity_lawpack();
    pure_context()
        .with_type_shape(TypeShapeFact {
            lawpack: lawpack.clone(),
            coordinate: "parity.types@1.Left".to_owned(),
            definition: case.left_definition.to_owned(),
        })
        .with_type_shape(TypeShapeFact {
            lawpack,
            coordinate: "parity.types@1.Right".to_owned(),
            definition: case.right_definition.to_owned(),
        })
}

fn semantic_parity_source(predicate: &str) -> String {
    format!(
        "package parity.application@1;\n\
         use lawpack parity.types@1 digest \"sha256:{}\" as shapes;\n\
         type Input = {{ left: shapes.Left, right: shapes.Right, value: String<max=8>, }};\n\
         type Output = {{ value: String<max=8>, }};\n\
         intent compare(input: Input) returns Output\n\
           profile hello.readOnly\n\
           basis none\n\
           budget <= hello.tinyBudget {{\n\
           let value = if {predicate} then input.value else input.value;\n\
           return {{ value }};\n\
         }}",
        "7".repeat(64)
    )
}

fn compile_semantic_parity_case(case: SemanticParityCase, predicate: &str) -> CoreModule {
    let source = semantic_parity_source(predicate);
    let module = edict_syntax::parse_module(&source)
        .unwrap_or_else(|errors| panic!("{} source parses: {errors:?}", case.name));
    compile_to_core(&module, &semantic_parity_context(case))
        .unwrap_or_else(|errors| panic!("{} source compiles: {errors:?}", case.name))
}

fn semantic_parity_bound_source(then_field: &str, else_field: &str) -> String {
    format!(
        "package parity.application@1;\n\
         use lawpack parity.types@1 digest \"sha256:{}\" as shapes;\n\
         type Input = {{ left: shapes.Left, right: shapes.Right, value: String<max=8>, }};\n\
         type Output = {{ value: String<max=8>, }};\n\
         intent compare(input: Input) returns Output\n\
           profile hello.readOnly\n\
           basis none\n\
           budget <= hello.tinyBudget {{\n\
           let joined = if true then input.{then_field} else input.{else_field};\n\
           return {{ value: input.value }};\n\
         }}",
        "7".repeat(64)
    )
}

fn compile_semantic_parity_bound_case(
    case: SemanticParityCase,
    then_field: &str,
    else_field: &str,
) -> CoreModule {
    let source = semantic_parity_bound_source(then_field, else_field);
    let module = edict_syntax::parse_module(&source)
        .unwrap_or_else(|errors| panic!("{} bound source parses: {errors:?}", case.name));
    compile_to_core(&module, &semantic_parity_context(case))
        .unwrap_or_else(|errors| panic!("{} bound source compiles: {errors:?}", case.name))
}

fn first_bound_type(core: &CoreModule) -> &str {
    let intent = core.intents.get("compare").expect("parity intent");
    let CoreNode::Let { binding, .. } = &intent.body.nodes[0] else {
        panic!("parity source starts with a bound conditional");
    };
    &binding.ty
}

fn canonical_value_contains_text(value: &CanonicalValue, needle: &str) -> bool {
    match value {
        CanonicalValue::Text(text) => text.contains(needle),
        CanonicalValue::Array(values) => values
            .iter()
            .any(|value| canonical_value_contains_text(value, needle)),
        CanonicalValue::Map(entries) => entries.iter().any(|(key, value)| {
            canonical_value_contains_text(key, needle)
                || canonical_value_contains_text(value, needle)
        }),
        CanonicalValue::Null
        | CanonicalValue::Bool(_)
        | CanonicalValue::Integer(_)
        | CanonicalValue::Bytes(_) => false,
    }
}

fn assert_compiler_core_has_named_type_entries_only(case: &str, core: &CoreModule) {
    let structural_prefixes = [
        "String<",
        "Bytes<",
        "Record<",
        "List<",
        "Option<",
        "Map<",
        "CapabilityRef<",
        "ExternalActionRequest<",
        "edict.external-action.request/v1<",
    ];
    assert!(
        core.types.keys().all(|coordinate| {
            !matches!(
                coordinate.as_str(),
                "Unit" | "Bool" | "I8" | "I16" | "I32" | "I64" | "U8" | "U16" | "U32" | "U64"
            ) && !structural_prefixes
                .iter()
                .any(|prefix| coordinate.starts_with(prefix))
        }),
        "{case} emitted a self-describing core.types key: {:?}",
        core.types.keys().collect::<Vec<_>>()
    );
    let bytes = encode_core_module(core).unwrap_or_else(|error| panic!("{case} encodes: {error}"));
    let canonical = decode_canonical_cbor(&bytes)
        .unwrap_or_else(|error| panic!("{case} canonical Core decodes: {error}"));
    assert!(
        !canonical_value_contains_text(&canonical, "anonymous.record"),
        "{case} leaked a scratch type coordinate"
    );
}

fn input_field(reference: &LocalRef, field: &str) -> CoreExpr {
    CoreExpr::Field {
        base: Box::new(CoreExpr::Local {
            reference: reference.clone(),
        }),
        field: field.to_owned(),
    }
}

#[test]
fn compiler_produced_crossed_list_conditionals_lower_through_target_ir() {
    let case = semantic_parity_cases()
        .into_iter()
        .find(|case| case.name == "crossed list bounds")
        .expect("list parity case");
    let core = compile_semantic_parity_case(
        case,
        "(if true then input.left else input.right) == (if false then input.right else input.left)",
    );

    let report = lower_to_target_ir(&core, &pure_target_facts());

    assert_eq!(report.status, TargetLoweringStatus::Lowered);
    assert_eq!(
        report.failures,
        Vec::<edict_syntax::TargetLoweringFailure>::new()
    );
    assert!(report.artifact.is_some());
}

#[test]
fn compiler_bound_record_joins_use_canonical_structural_identity() {
    let case = semantic_parity_cases()
        .into_iter()
        .find(|case| case.name == "two-field crossed records")
        .expect("record parity case");
    let forward = compile_semantic_parity_bound_case(case, "left", "right");
    let reverse = compile_semantic_parity_bound_case(case, "right", "left");
    let expected =
        "Record<left:String<max=3,canonical=raw-utf8>,right:String<max=3,canonical=raw-utf8>>";

    assert_eq!(first_bound_type(&forward), expected);
    assert_eq!(first_bound_type(&reverse), expected);
    assert_compiler_core_has_named_type_entries_only(case.name, &forward);
    assert_compiler_core_has_named_type_entries_only(case.name, &reverse);

    for core in [&forward, &reverse] {
        let report = lower_to_target_ir(core, &pure_target_facts());
        assert_eq!(report.status, TargetLoweringStatus::Lowered);
        assert_eq!(
            report.failures,
            Vec::<edict_syntax::TargetLoweringFailure>::new()
        );
        assert!(report.artifact.is_some());
    }
}

#[test]
fn compiler_bound_list_record_joins_use_canonical_structural_identity() {
    let case = semantic_parity_cases()
        .into_iter()
        .find(|case| case.name == "records inside crossed lists")
        .expect("record-list parity case");
    let forward = compile_semantic_parity_bound_case(case, "left", "right");
    let reverse = compile_semantic_parity_bound_case(case, "right", "left");
    let expected = "List<Record<left:String<max=3,canonical=raw-utf8>,right:String<max=3,canonical=raw-utf8>>,max=3>";

    assert_eq!(first_bound_type(&forward), expected);
    assert_eq!(first_bound_type(&reverse), expected);
    assert_compiler_core_has_named_type_entries_only(case.name, &forward);
    assert_compiler_core_has_named_type_entries_only(case.name, &reverse);

    for core in [&forward, &reverse] {
        let report = lower_to_target_ir(core, &pure_target_facts());
        assert_eq!(report.status, TargetLoweringStatus::Lowered);
        assert_eq!(
            report.failures,
            Vec::<edict_syntax::TargetLoweringFailure>::new()
        );
        assert!(report.artifact.is_some());
    }
}

#[test]
fn compiler_emitted_core_type_closure_uses_named_entries_only() {
    for (case, definition) in [
        ("nested record", "Record<inner:Record<value:U64>>"),
        ("record inside list", "List<Record<value:U64>,max=2>"),
    ] {
        let lawpack = semantic_parity_lawpack();
        let context = pure_context().with_type_shape(TypeShapeFact {
            lawpack,
            coordinate: "parity.types@1.Value".to_owned(),
            definition: definition.to_owned(),
        });
        let source = format!(
            "package parity.application@1;\n\
             use lawpack parity.types@1 digest \"sha256:{}\" as shapes;\n\
             type Input = {{ value: shapes.Value, }};\n\
             type Output = {{ value: shapes.Value, }};\n\
             intent copy(input: Input) returns Output\n\
               profile hello.readOnly\n\
               basis none\n\
               budget <= hello.tinyBudget {{\n\
               let value: shapes.Value = input.value;\n\
               return {{ value }};\n\
             }}",
            "7".repeat(64)
        );
        let module = edict_syntax::parse_module(&source)
            .unwrap_or_else(|errors| panic!("{case} source parses: {errors:?}"));
        let core = compile_to_core(&module, &context)
            .unwrap_or_else(|errors| panic!("{case} source compiles: {errors:?}"));

        assert!(
            core.types.contains_key("parity.types@1.Value"),
            "{case} retains its named lawpack root"
        );
        assert!(
            !core.types.contains_key("Record<value:U64>"),
            "{case} must not intern structural syntax as named authority"
        );
        assert_compiler_core_has_named_type_entries_only(case, &core);

        let report = lower_to_target_ir(&core, &pure_target_facts());
        assert_eq!(report.status, TargetLoweringStatus::Lowered, "{case}");
        assert!(report.failures.is_empty(), "{case}: {:?}", report.failures);
        assert!(report.artifact.is_some(), "{case}");
    }
}

#[test]
fn compiler_target_semantic_parity_matrix() {
    for case in semantic_parity_cases() {
        let direct_source = semantic_parity_source("input.left == input.right");
        let direct_module = edict_syntax::parse_module(&direct_source)
            .unwrap_or_else(|errors| panic!("{} direct source parses: {errors:?}", case.name));
        let compiler_accepts =
            compile_to_core(&direct_module, &semantic_parity_context(case)).is_ok();
        assert_eq!(
            compiler_accepts, case.directly_comparable,
            "{} compiler comparison oracle",
            case.name
        );

        let mut caller_core = compile_semantic_parity_case(case, "true");
        let intent = caller_core
            .intents
            .get_mut("compare")
            .expect("parity intent");
        let input = intent
            .body
            .locals
            .iter()
            .find(|local| local.id == "arg.0")
            .expect("compiler-owned input")
            .clone();
        let CoreNode::Let {
            value: CoreExpr::If { predicate, .. },
            ..
        } = &mut intent.body.nodes[0]
        else {
            panic!("parity source starts with a conditional binding");
        };
        **predicate = CorePredicate::Compare {
            op: CompareOp::Eq,
            left: input_field(&input, "left"),
            right: input_field(&input, "right"),
        };
        let report = lower_to_target_ir(&caller_core, &pure_target_facts());
        assert_eq!(
            report.status == TargetLoweringStatus::Lowered,
            compiler_accepts,
            "{} Target comparison parity: {:?}",
            case.name,
            report.failures
        );
        assert_eq!(report.artifact.is_some(), compiler_accepts, "{}", case.name);

        let compiler_core = compile_semantic_parity_case(
            case,
            "(if true then input.left else input.right) == (if false then input.right else input.left)",
        );
        let report = lower_to_target_ir(&compiler_core, &pure_target_facts());
        assert_eq!(
            report.status,
            TargetLoweringStatus::Lowered,
            "{} compiler-produced conditional: {:?}",
            case.name,
            report.failures
        );
        assert!(report.artifact.is_some(), "{}", case.name);

        let forward = compile_semantic_parity_bound_case(case, "left", "right");
        let reverse = compile_semantic_parity_bound_case(case, "right", "left");
        assert_eq!(
            first_bound_type(&forward),
            first_bound_type(&reverse),
            "{} conditional join is branch-order independent",
            case.name
        );
        assert_compiler_core_has_named_type_entries_only(case.name, &forward);
        assert_compiler_core_has_named_type_entries_only(case.name, &reverse);
        for (order, core) in [("forward", forward), ("reverse", reverse)] {
            let report = lower_to_target_ir(&core, &pure_target_facts());
            assert_eq!(
                report.status,
                TargetLoweringStatus::Lowered,
                "{} {order} bound conditional: {:?}",
                case.name,
                report.failures
            );
            assert!(report.artifact.is_some(), "{} {order}", case.name);
        }
    }
}

#[test]
fn empty_aggregate_predicates_reject_on_every_target_surface() {
    for (aggregate, predicate) in [
        ("all", CorePredicate::All(Vec::new())),
        ("any", CorePredicate::Any(Vec::new())),
    ] {
        let mut constraint = pure_core();
        constraint
            .intents
            .get_mut("sayHello")
            .expect("pure intent")
            .input_constraints[0]
            .predicate = predicate.clone();

        let mut conditional = pure_core();
        let intent = conditional
            .intents
            .get_mut("sayHello")
            .expect("pure intent");
        let CoreNode::Let { value, .. } = &mut intent.body.nodes[0] else {
            panic!("pure fixture starts with a let");
        };
        let valid_branch = value.clone();
        *value = CoreExpr::If {
            predicate: Box::new(predicate.clone()),
            then_value: Box::new(valid_branch.clone()),
            else_value: Box::new(valid_branch),
        };

        let module =
            edict_syntax::parse_module(ECHO_TERMINAL_REQUIRE).expect("require source parses");
        let mut requirement = compile_to_core(&module, &effectful_context())
            .expect("require source compiles to Core");
        let CoreNode::Require {
            predicate: requirement_predicate,
            ..
        } = &mut requirement
            .intents
            .get_mut("t")
            .expect("require intent")
            .body
            .nodes[0]
        else {
            panic!("require source starts with a requirement");
        };
        *requirement_predicate = predicate.clone();

        let mut copied_expression = effectful_core();
        let CoreNode::Effect {
            obstruction_map, ..
        } = &mut copied_expression
            .intents
            .get_mut("t")
            .expect("effect intent")
            .body
            .nodes[0]
        else {
            panic!("effect source starts with an effect");
        };
        obstruction_map
            .values_mut()
            .next()
            .expect("effect obstruction arm")
            .value = CoreExpr::If {
            predicate: Box::new(predicate),
            then_value: Box::new(CoreExpr::Const(CoreValue::Null)),
            else_value: Box::new(CoreExpr::Const(CoreValue::Null)),
        };

        for (surface, report) in [
            (
                "input constraint",
                lower_to_target_ir(&constraint, &pure_target_facts()),
            ),
            (
                "pure conditional",
                lower_to_target_ir(&conditional, &pure_target_facts()),
            ),
            (
                "requirement",
                lower_to_target_ir(&requirement, &echo_facts()),
            ),
            (
                "copied obstruction expression",
                lower_to_target_ir(&copied_expression, &echo_facts()),
            ),
        ] {
            assert_invalid_core_identity(&report, &format!("empty {aggregate} in {surface}"));
        }
    }
}

fn workspace_request_core() -> CoreModule {
    let source = include_str!("../../../fixtures/lang/external-actions/workspace-snapshot.edict");
    let module = edict_syntax::parse_module(source).expect("workspace request source parses");
    let context = pure_context()
        .with_operation_profile("workspace.read", "continuum.profile.read-only/v1")
        .with_budget(
            "workspace.tiny",
            CoreBudget {
                max_steps: 512,
                max_allocated_bytes: 256 * 1024,
                max_output_bytes: 128 * 1024,
            },
        );
    compile_to_core(&module, &context).expect("workspace request source compiles")
}

fn empty_matching_local(locals: &mut [LocalRef], id: &str) {
    locals
        .iter_mut()
        .find(|local| local.id == id)
        .unwrap_or_else(|| panic!("producer {id:?} exists in the local table"))
        .id
        .clear();
}

fn empty_result_reference(expression: &mut CoreExpr, id: &str) {
    let reference = match expression {
        CoreExpr::Local { reference } => reference,
        CoreExpr::Record { fields } => fields
            .values_mut()
            .find_map(|value| match value {
                CoreExpr::Local { reference } if reference.id == id => Some(reference),
                _ => None,
            })
            .unwrap_or_else(|| panic!("result references producer {id:?}")),
        _ => panic!("fixture result is a local or one-field local record"),
    };
    assert_eq!(reference.id, id);
    reference.id.clear();
}

fn core_with_empty_unused_local() -> CoreModule {
    let mut unused_local = pure_core();
    unused_local
        .intents
        .get_mut("sayHello")
        .expect("pure intent")
        .body
        .locals
        .push(LocalRef {
            id: String::new(),
            alpha_name: "$unused".to_owned(),
            ty: "Bool".to_owned(),
        });
    unused_local
}

fn core_with_empty_implicit_input() -> CoreModule {
    let mut implicit_input = pure_core();
    implicit_input
        .intents
        .get_mut("sayHello")
        .expect("pure intent")
        .body
        .locals
        .iter_mut()
        .find(|local| local.id == "arg.0")
        .expect("implicit input local")
        .id
        .clear();
    implicit_input
}

fn core_with_empty_pure_binding() -> CoreModule {
    let mut pure_binding = pure_core();
    let intent = pure_binding
        .intents
        .get_mut("sayHello")
        .expect("pure intent");
    let CoreNode::Let { binding, .. } = &mut intent.body.nodes[0] else {
        panic!("pure fixture starts with a let");
    };
    let id = binding.id.clone();
    binding.id.clear();
    empty_matching_local(&mut intent.body.locals, &id);
    empty_result_reference(&mut intent.body.result, &id);
    pure_binding
}

fn core_with_empty_effect_result() -> CoreModule {
    let mut effect_result = effectful_core();
    let intent = effect_result.intents.get_mut("t").expect("effect intent");
    let CoreNode::Effect { binding, .. } = &mut intent.body.nodes[0] else {
        panic!("effect fixture starts with an effect");
    };
    let id = binding.id.clone();
    binding.id.clear();
    empty_matching_local(&mut intent.body.locals, &id);
    effect_result
}

fn core_with_empty_obstruction_binder() -> CoreModule {
    let mut obstruction_binder = effectful_core();
    let intent = obstruction_binder
        .intents
        .get_mut("t")
        .expect("effect intent");
    let CoreNode::Effect {
        obstruction_map, ..
    } = &mut intent.body.nodes[0]
    else {
        panic!("effect fixture starts with an effect");
    };
    let binder = &mut obstruction_map
        .values_mut()
        .next()
        .expect("effect obstruction arm")
        .binder;
    let id = binder.id.clone();
    binder.id.clear();
    empty_matching_local(&mut intent.body.locals, &id);
    obstruction_binder
}

fn core_with_empty_external_request() -> CoreModule {
    let mut external_request = workspace_request_core();
    let intent = external_request
        .intents
        .get_mut("observe")
        .expect("request intent");
    let CoreNode::ExternalActionRequest { binding, .. } = &mut intent.body.nodes[0] else {
        panic!("request fixture starts with a request");
    };
    let id = binding.id.clone();
    binding.id.clear();
    empty_matching_local(&mut intent.body.locals, &id);
    empty_result_reference(&mut intent.body.result, &id);
    external_request
}

fn core_with_empty_branch_result() -> CoreModule {
    let branch_source = "package identity.branch@1;\n\
        type Input = { value: U64, };\n\
        type Output = { value: U64, };\n\
        intent choose(input: Input) returns Output\n\
          profile hello.readOnly\n\
          basis none\n\
          budget <= hello.tinyBudget {\n\
          let value = if true { yield input.value; } else { yield input.value; };\n\
          return { value };\n\
        }";
    let module = edict_syntax::parse_module(branch_source).expect("branch source parses");
    let mut branch_result =
        compile_to_core(&module, &pure_context()).expect("branch source compiles");
    let intent = branch_result
        .intents
        .get_mut("choose")
        .expect("branch intent");
    let CoreNode::Branch {
        binding: Some(binding),
        ..
    } = &mut intent.body.nodes[0]
    else {
        panic!("branch-yield source starts with a bound branch");
    };
    let id = binding.id.clone();
    binding.id.clear();
    empty_matching_local(&mut intent.body.locals, &id);
    empty_result_reference(&mut intent.body.result, &id);
    branch_result
}

fn core_with_empty_loop_binder() -> CoreModule {
    let mut loop_binder = pure_core();
    loop_binder
        .intents
        .get_mut("sayHello")
        .expect("pure intent")
        .body
        .nodes = vec![CoreNode::For {
        binder: LocalRef {
            id: String::new(),
            alpha_name: "$item".to_owned(),
            ty: "U64".to_owned(),
        },
        iter: CoreExpr::Const(CoreValue::Null),
        bound: CoreBound::Literal(1),
        body: CoreBlock {
            locals: Vec::new(),
            nodes: Vec::new(),
            result: CoreExpr::Const(CoreValue::Null),
        },
    }];
    loop_binder
}

#[test]
fn empty_local_identities_reject_across_all_producer_classes() {
    let unused_local = core_with_empty_unused_local();
    let implicit_input = core_with_empty_implicit_input();
    let pure_binding = core_with_empty_pure_binding();
    let effect_result = core_with_empty_effect_result();
    let obstruction_binder = core_with_empty_obstruction_binder();
    let external_request = core_with_empty_external_request();
    let branch_result = core_with_empty_branch_result();
    let loop_binder = core_with_empty_loop_binder();

    for (case, report) in [
        (
            "complete local table",
            lower_to_target_ir(&unused_local, &pure_target_facts()),
        ),
        (
            "implicit input",
            lower_to_target_ir(&implicit_input, &pure_target_facts()),
        ),
        (
            "pure binding",
            lower_to_target_ir(&pure_binding, &pure_target_facts()),
        ),
        (
            "effect result",
            lower_to_target_ir(&effect_result, &echo_facts()),
        ),
        (
            "obstruction binder",
            lower_to_target_ir(&obstruction_binder, &echo_facts()),
        ),
        (
            "external request",
            lower_to_target_ir(&external_request, &pure_target_facts()),
        ),
        (
            "branch result",
            lower_to_target_ir(&branch_result, &pure_target_facts()),
        ),
        (
            "loop binder",
            lower_to_target_ir(&loop_binder, &pure_target_facts()),
        ),
    ] {
        assert_invalid_core_identity(&report, case);
    }
}

type CoreTypeIntegrityCase = (&'static str, CoreModule, TargetIrLoweringFacts);

const INVALID: &str = "List<U64,max=01>";

fn set_invalid_core_type(target: &mut String) {
    INVALID.clone_into(target);
}

fn add_shallow_cached_over_depth_named_type(core: &mut CoreModule) {
    core.types.insert(
        "A.DepthBase".to_owned(),
        CoreType::Option {
            item: "U64".to_owned(),
        },
    );
    core.types.insert(
        "Z.DepthOverflow".to_owned(),
        CoreType::Option {
            item: (0..127).fold("A.DepthBase".to_owned(), |inner, _| {
                format!("Option<{inner}>")
            }),
        },
    );
}

fn pure_core_type_integrity_cases() -> Vec<CoreTypeIntegrityCase> {
    let mut cases = Vec::new();

    let mut unused_definition = pure_core();
    unused_definition.types.insert(
        "UnusedInvalid".to_owned(),
        CoreType::List {
            item: INVALID.to_owned(),
            max: 1,
        },
    );
    cases.push(("unused definition", unused_definition, pure_target_facts()));

    let mut intent_input = pure_core();
    set_invalid_core_type(
        &mut intent_input
            .intents
            .get_mut("sayHello")
            .expect("pure intent")
            .input,
    );
    cases.push(("intent input", intent_input, pure_target_facts()));

    let mut intent_output = pure_core();
    set_invalid_core_type(
        &mut intent_output
            .intents
            .get_mut("sayHello")
            .expect("pure intent")
            .output,
    );
    cases.push(("intent output", intent_output, pure_target_facts()));

    let mut local_table = pure_core();
    set_invalid_core_type(
        &mut local_table
            .intents
            .get_mut("sayHello")
            .expect("pure intent")
            .body
            .locals[0]
            .ty,
    );
    cases.push(("complete local table", local_table, pure_target_facts()));

    let mut let_binding = pure_core();
    let CoreNode::Let { binding, .. } = &mut let_binding
        .intents
        .get_mut("sayHello")
        .expect("pure intent")
        .body
        .nodes[0]
    else {
        panic!("pure fixture begins with a let");
    };
    set_invalid_core_type(&mut binding.ty);
    cases.push(("let binding", let_binding, pure_target_facts()));

    let mut predicate_local = pure_core();
    assert!(mutate_first_predicate_local_type(
        &mut predicate_local
            .intents
            .get_mut("sayHello")
            .expect("pure intent")
            .input_constraints[0]
            .predicate,
        INVALID,
    ));
    cases.push((
        "predicate local reference",
        predicate_local,
        pure_target_facts(),
    ));

    let mut call_type_argument = pure_core();
    let CoreNode::Let {
        value: CoreExpr::Call { type_args, .. },
        ..
    } = &mut call_type_argument
        .intents
        .get_mut("sayHello")
        .expect("pure intent")
        .body
        .nodes[0]
    else {
        panic!("pure fixture begins with a call-valued let");
    };
    type_args.push(INVALID.to_owned());
    cases.push((
        "call type argument",
        call_type_argument,
        pure_target_facts(),
    ));

    let mut result_local = pure_core();
    assert!(mutate_first_local_reference_type(
        &mut result_local
            .intents
            .get_mut("sayHello")
            .expect("pure intent")
            .body
            .result,
        INVALID,
    ));
    cases.push(("result local reference", result_local, pure_target_facts()));

    cases
}

fn effect_and_request_type_integrity_cases() -> Vec<CoreTypeIntegrityCase> {
    let mut cases = Vec::new();

    let mut effect_binding = effectful_core();
    let CoreNode::Effect { binding, .. } = &mut effect_binding
        .intents
        .get_mut("t")
        .expect("effect intent")
        .body
        .nodes[0]
    else {
        panic!("effect fixture begins with an effect");
    };
    set_invalid_core_type(&mut binding.ty);
    cases.push(("effect binding", effect_binding, echo_facts()));

    let mut effect_input = effectful_core();
    let CoreNode::Effect { input, .. } = &mut effect_input
        .intents
        .get_mut("t")
        .expect("effect intent")
        .body
        .nodes[0]
    else {
        panic!("effect fixture begins with an effect");
    };
    assert!(mutate_first_local_reference_type(input, INVALID));
    cases.push(("effect input local", effect_input, echo_facts()));

    let mut obstruction_binder = effectful_core();
    let CoreNode::Effect {
        obstruction_map, ..
    } = &mut obstruction_binder
        .intents
        .get_mut("t")
        .expect("effect intent")
        .body
        .nodes[0]
    else {
        panic!("effect fixture begins with an effect");
    };
    set_invalid_core_type(
        &mut obstruction_map
            .values_mut()
            .next()
            .expect("effect obstruction arm")
            .binder
            .ty,
    );
    cases.push(("obstruction binder", obstruction_binder, echo_facts()));

    for surface in ["binding", "input type", "settlement type"] {
        let mut request = workspace_request_core();
        let CoreNode::ExternalActionRequest {
            binding,
            input_type,
            settlement_type,
            ..
        } = &mut request
            .intents
            .get_mut("observe")
            .expect("request intent")
            .body
            .nodes[0]
        else {
            panic!("request fixture begins with a request");
        };
        match surface {
            "binding" => set_invalid_core_type(&mut binding.ty),
            "input type" => set_invalid_core_type(input_type),
            "settlement type" => set_invalid_core_type(settlement_type),
            _ => unreachable!(),
        }
        cases.push((surface, request, pure_target_facts()));
    }

    cases
}

fn control_flow_type_integrity_cases() -> Vec<CoreTypeIntegrityCase> {
    let mut cases = Vec::new();

    let branch_source = "package integrity.branch@1;\n\
        type Input = { value: U64, };\n\
        type Output = { value: U64, };\n\
        intent choose(input: Input) returns Output\n\
          profile hello.readOnly\n\
          basis none\n\
          budget <= hello.tinyBudget {\n\
          let value = if true { yield input.value; } else { yield input.value; };\n\
          return { value };\n\
        }";
    let module = edict_syntax::parse_module(branch_source).expect("branch source parses");
    let mut branch = compile_to_core(&module, &pure_context()).expect("branch source compiles");
    let CoreNode::Branch {
        binding: Some(binding),
        ..
    } = &mut branch
        .intents
        .get_mut("choose")
        .expect("branch intent")
        .body
        .nodes[0]
    else {
        panic!("branch fixture begins with a bound branch");
    };
    set_invalid_core_type(&mut binding.ty);
    cases.push(("branch binding", branch, pure_target_facts()));

    let mut loop_binder = pure_core();
    loop_binder
        .intents
        .get_mut("sayHello")
        .expect("pure intent")
        .body
        .nodes = vec![CoreNode::For {
        binder: LocalRef {
            id: "loop.item".to_owned(),
            alpha_name: "$loopItem".to_owned(),
            ty: INVALID.to_owned(),
        },
        iter: CoreExpr::Const(CoreValue::Null),
        bound: CoreBound::Literal(1),
        body: CoreBlock {
            locals: Vec::new(),
            nodes: Vec::new(),
            result: CoreExpr::Const(CoreValue::Null),
        },
    }];
    cases.push(("loop binder", loop_binder, pure_target_facts()));

    let mut nested_loop = pure_core();
    nested_loop
        .intents
        .get_mut("sayHello")
        .expect("pure intent")
        .body
        .nodes = vec![CoreNode::For {
        binder: LocalRef {
            id: "loop.item".to_owned(),
            alpha_name: "$loopItem".to_owned(),
            ty: "U64".to_owned(),
        },
        iter: CoreExpr::Const(CoreValue::Null),
        bound: CoreBound::Literal(1),
        body: CoreBlock {
            locals: vec![LocalRef {
                id: "loop.local".to_owned(),
                alpha_name: "$loopLocal".to_owned(),
                ty: INVALID.to_owned(),
            }],
            nodes: Vec::new(),
            result: CoreExpr::Local {
                reference: LocalRef {
                    id: "loop.local".to_owned(),
                    alpha_name: "$loopLocal".to_owned(),
                    ty: INVALID.to_owned(),
                },
            },
        },
    }];
    cases.push(("nested loop block", nested_loop, pure_target_facts()));

    cases
}

#[test]
fn every_core_type_bearing_surface_crosses_one_integrity_boundary() {
    let mut cases = pure_core_type_integrity_cases();
    cases.extend(effect_and_request_type_integrity_cases());
    cases.extend(control_flow_type_integrity_cases());

    let mut wrong = Vec::new();
    for (case, core, facts) in cases {
        let report = lower_to_target_ir(&core, &facts);
        if report.status != TargetLoweringStatus::Unsupported
            || report.artifact.is_some()
            || failure_kinds(&report) != vec![TargetLoweringFailureKind::InvalidCoreIdentity]
        {
            wrong.push((
                case,
                report.status,
                report.artifact.is_some(),
                failure_kinds(&report),
            ));
        }
    }
    assert!(
        wrong.is_empty(),
        "Core integrity was bypassed or classified inconsistently: {wrong:#?}"
    );
}

#[test]
fn over_depth_named_expansion_cannot_lower_to_target_ir() {
    let mut core = pure_core();
    add_shallow_cached_over_depth_named_type(&mut core);

    let report = lower_to_target_ir(&core, &pure_target_facts());
    assert_invalid_core_identity(&report, "shallow-cached over-depth named definition");
}

#[test]
fn target_lowering_accepts_compiler_type_depth_boundary() {
    const TYPE_DEPTH: usize = 128;

    let mut core = pure_core();
    let root_type = format!("{}.Deep0", core.coordinate);
    for index in 0..TYPE_DEPTH {
        let coordinate = format!("{}.Deep{index}", core.coordinate);
        let next = if index + 1 == TYPE_DEPTH {
            "U64".to_owned()
        } else {
            format!("{}.Deep{}", core.coordinate, index + 1)
        };
        core.types.insert(
            coordinate,
            CoreType::Record {
                fields: BTreeMap::from([("next".to_owned(), next)]),
            },
        );
    }

    let intent = core.intents.get_mut("sayHello").expect("pure intent");
    intent.input.clone_from(&root_type);
    intent.output.clone_from(&root_type);
    intent.input_constraints.clear();
    let input = intent
        .body
        .locals
        .iter()
        .find(|local| local.id == "arg.0")
        .expect("compiler-owned input local")
        .clone();
    let input = LocalRef {
        ty: root_type.clone(),
        ..input
    };
    let binding = LocalRef {
        id: "local.deep".to_owned(),
        alpha_name: "$deep".to_owned(),
        ty: root_type,
    };
    intent.body.locals = vec![input.clone(), binding.clone()];
    intent.body.nodes = vec![CoreNode::Let {
        binding: binding.clone(),
        value: CoreExpr::Local { reference: input },
    }];
    intent.body.result = CoreExpr::Local { reference: binding };

    let report = lower_to_target_ir(&core, &pure_target_facts());

    assert_eq!(report.status, TargetLoweringStatus::Lowered);
    assert_eq!(
        report.failures,
        Vec::<edict_syntax::TargetLoweringFailure>::new()
    );
    assert!(report.artifact.is_some());
}

#[test]
fn nested_string_conditional_predicates_require_compatible_bounded_operands() {
    let cases = [
        (
            "incompatible operand types",
            CoreExpr::Const(CoreValue::Bool(true)),
            CoreExpr::Const(CoreValue::Int {
                width: "U64".to_owned(),
                value: "1".to_owned(),
            }),
        ),
        (
            "constant outside its declared width",
            CoreExpr::Const(CoreValue::Int {
                width: "U8".to_owned(),
                value: "0".to_owned(),
            }),
            CoreExpr::Const(CoreValue::Int {
                width: "U8".to_owned(),
                value: "256".to_owned(),
            }),
        ),
    ];

    for (case, left, right) in cases {
        let mut core = pure_core();
        let intent = core.intents.get_mut("sayHello").expect("pure intent");
        let CoreNode::Let {
            value: CoreExpr::Call { callee, args, .. },
            ..
        } = &mut intent.body.nodes[0]
        else {
            panic!("pure fixture starts with a string concatenation binding");
        };
        assert_eq!(callee, "core.string.concat");
        args[0] = CoreExpr::If {
            predicate: Box::new(CorePredicate::Compare {
                op: CompareOp::Eq,
                left,
                right,
            }),
            then_value: Box::new(CoreExpr::Const(CoreValue::String("hello".to_owned()))),
            else_value: Box::new(CoreExpr::Const(CoreValue::String("hi".to_owned()))),
        };

        let report = lower_to_target_ir(&core, &pure_target_facts());

        assert_eq!(report.status, TargetLoweringStatus::Unsupported, "{case}");
        assert!(report.artifact.is_none(), "{case}");
        let [failure] = report.failures.as_slice() else {
            panic!("{case} must reject with exactly one structured failure");
        };
        assert_eq!(failure.kind, TargetLoweringFailureKind::InvalidCoreIdentity);
        assert_eq!(failure.intent.as_deref(), Some("sayHello"));
        assert_eq!(failure.node_index, Some(0));
    }
}

#[test]
fn non_raw_string_constant_rejects_before_target_artifact() {
    let mut core = pure_core();
    let intent = core.intents.get_mut("sayHello").expect("pure intent");
    let CoreNode::Let { binding, value } = &mut intent.body.nodes[0] else {
        panic!("pure fixture starts with a let");
    };
    binding.ty = "String<max=263,canonical=unicode-scalar-nfc>".to_owned();
    intent.body.locals[1].ty.clone_from(&binding.ty);
    *value = CoreExpr::Const(CoreValue::String("already normalized".to_owned()));

    let report = lower_to_target_ir(&core, &pure_target_facts());

    assert_eq!(report.status, TargetLoweringStatus::Unsupported);
    assert!(report.artifact.is_none());
    let [failure] = report.failures.as_slice() else {
        panic!("a raw string constant under a non-raw contract rejects once");
    };
    assert_eq!(failure.kind, TargetLoweringFailureKind::InvalidCoreIdentity);
    assert_eq!(failure.intent.as_deref(), Some("sayHello"));
    assert_eq!(failure.node_index, Some(0));
}

#[test]
fn pure_program_without_imports_or_basis_still_binds_source_core_identity() {
    let mut core = pure_core();
    core.imports.clear();
    core.intents.get_mut("sayHello").expect("pure intent").basis = None;

    let report = lower_to_target_ir(&core, &pure_target_facts());

    assert_eq!(report.status, TargetLoweringStatus::Lowered);
    assert!(
        report
            .artifact
            .as_ref()
            .expect("pure Target IR")
            .semantic_closure
            .is_some(),
        "pure executable expressions require an exact source-Core closure"
    );
    assert!(report.result_projections.contains_key("sayHello"));
}

#[test]
fn unsupported_core_abi_rejects_without_artifact() {
    let mut core = effectful_core();
    core.api_version = "edict.core/v2".to_owned();

    let report = lower_to_target_ir(&core, &echo_facts());

    assert_eq!(report.status, TargetLoweringStatus::Unsupported);
    assert!(report.artifact.is_none());
    assert_eq!(
        failure_kinds(&report),
        vec![TargetLoweringFailureKind::UnsupportedCoreAbi]
    );
}

#[test]
fn unsupported_core_capability_rejects_without_artifact() {
    let mut core = effectful_core();
    core.required_core_capabilities
        .push("edict.core.capability.variant-map/v1".to_owned());

    let report = lower_to_target_ir(&core, &echo_facts());

    assert_eq!(report.status, TargetLoweringStatus::Unsupported);
    assert!(report.artifact.is_none());
    assert_eq!(
        failure_kinds(&report),
        vec![TargetLoweringFailureKind::UnsupportedCoreCapability]
    );
}

#[test]
fn undigested_core_import_rejects_without_artifact() {
    let mut core = effectful_core();
    core.imports.push(CoreImport {
        kind: CoreImportKind::Lawpack,
        resource: ResourceRef {
            coordinate: "hello.optics@1".to_owned(),
            digest: None,
        },
        alias: Some("hello".to_owned()),
    });

    let report = lower_to_target_ir(&core, &echo_facts());

    assert_eq!(report.status, TargetLoweringStatus::Unsupported);
    assert!(report.artifact.is_none());
    assert_eq!(
        failure_kinds(&report),
        vec![TargetLoweringFailureKind::UndigestedCoreImport]
    );
}

#[test]
fn target_ir_artifact_bytes_and_digests_are_deterministic() {
    assert_eq!(
        TARGET_IR_ARTIFACT_DIGEST_DOMAIN,
        "edict.target-ir.artifact/v1"
    );

    let echo = effectful_artifact(EFFECTFUL_REPLACE);
    let gitwarp = gitwarp_artifact();

    let echo_bytes = encode_target_ir_artifact(&echo).expect("Echo Target IR encodes");
    let echo_bytes_again = encode_target_ir_artifact(&echo).expect("Echo Target IR re-encodes");
    assert_eq!(echo_bytes, echo_bytes_again);
    decode_canonical_cbor(&echo_bytes).expect("Echo Target IR bytes are canonical CBOR");

    let gitwarp_bytes = encode_target_ir_artifact(&gitwarp).expect("git-warp Target IR encodes");
    let gitwarp_bytes_again =
        encode_target_ir_artifact(&gitwarp).expect("git-warp Target IR re-encodes");
    assert_eq!(gitwarp_bytes, gitwarp_bytes_again);
    decode_canonical_cbor(&gitwarp_bytes).expect("git-warp Target IR bytes are canonical CBOR");

    assert_ne!(echo_bytes, gitwarp_bytes);

    let echo_digest = digest_target_ir_artifact(&echo).expect("Echo Target IR digests");
    let echo_digest_again = digest_target_ir_artifact(&echo).expect("Echo Target IR re-digests");
    assert_eq!(echo_digest, echo_digest_again);
    assert!(echo_digest.to_review_string().starts_with("sha256:"));
    assert_eq!(echo_digest.to_review_string().len(), "sha256:".len() + 64);

    let gitwarp_digest = digest_target_ir_artifact(&gitwarp).expect("git-warp Target IR digests");
    assert_ne!(echo_digest, gitwarp_digest);
}

#[test]
fn target_ir_artifact_canonicalization_ignores_equivalent_construction_order() {
    let mut left = gitwarp_artifact();
    let mut right = left.clone();

    let extra_constraint = InputConstraint {
        coordinate: "compiler.0".to_owned(),
        source: InputConstraintSource::Compiler,
        predicate: CorePredicate::True,
    };

    let left_intent = left.intents.get_mut("t").expect("intent t");
    left_intent.input_constraints.push(extra_constraint.clone());
    let left_step = left_intent.steps.get_mut(0).expect("step 0");
    let conflict_arm = left_step
        .obstruction_arms
        .get("conflict")
        .expect("conflict arm")
        .clone();
    left_step
        .obstruction_arms
        .insert("retry".to_owned(), conflict_arm.clone());
    left_step.obstruction_failures = vec!["retry".to_owned(), "conflict".to_owned()];

    let right_intent = right.intents.get_mut("t").expect("intent t");
    right_intent.input_constraints.insert(0, extra_constraint);
    let right_step = right_intent.steps.get_mut(0).expect("step 0");
    let mut rebuilt_arms = BTreeMap::new();
    rebuilt_arms.insert("retry".to_owned(), conflict_arm);
    rebuilt_arms.insert(
        "conflict".to_owned(),
        right_step
            .obstruction_arms
            .get("conflict")
            .expect("conflict arm")
            .clone(),
    );
    right_step.obstruction_arms = rebuilt_arms;
    right_step.obstruction_failures = vec!["conflict".to_owned(), "retry".to_owned()];

    assert_eq!(
        encode_target_ir_artifact(&left).expect("left Target IR encodes"),
        encode_target_ir_artifact(&right).expect("right Target IR encodes")
    );
    assert_eq!(
        digest_target_ir_artifact(&left).expect("left Target IR digests"),
        digest_target_ir_artifact(&right).expect("right Target IR digests")
    );
}

#[test]
fn target_ir_step_order_changes_digest() {
    let baseline = effectful_artifact(CHAINED_EFFECT_RESULTS);
    let mut reordered = baseline.clone();
    reordered
        .intents
        .get_mut("t")
        .expect("intent t")
        .steps
        .reverse();

    assert_ne!(
        digest_target_ir_artifact(&baseline).expect("baseline Target IR digests"),
        digest_target_ir_artifact(&reordered).expect("reordered Target IR digests")
    );
}

#[test]
fn target_ir_digest_moves_for_artifact_semantic_mutations() {
    let baseline = effectful_artifact(EFFECTFUL_REPLACE);
    assert_target_ir_digest_changes(&baseline, "target profile digest", |artifact| {
        artifact.target_profile.digest = Some(digest_text('3'));
    });
    assert_target_ir_digest_changes(&baseline, "source Core coordinate", |artifact| {
        artifact.source_core_coordinate = "a.changed@1".to_owned();
    });
    assert_target_ir_digest_changes(&baseline, "intent name", |artifact| {
        let intent = artifact.intents.remove("t").expect("intent t");
        artifact.intents.insert("renamed".to_owned(), intent);
    });
    assert_target_ir_digest_changes(&baseline, "effect coordinate", |artifact| {
        target_step_mut(artifact).effect = "target.replace.changed".to_owned();
    });
    assert_target_ir_digest_changes(&baseline, "selected target intrinsic", |artifact| {
        target_step_mut(artifact).target_intrinsic = "echo.dpo@1.replace.changed".to_owned();
    });
    assert_target_ir_digest_changes(&baseline, "input expression", |artifact| {
        target_step_mut(artifact).input = CoreExpr::Const(CoreValue::String("changed".to_owned()));
    });
    assert_target_ir_digest_changes(&baseline, "obstruction failure", |artifact| {
        target_step_mut(artifact)
            .obstruction_failures
            .push("timeout".to_owned());
    });
    assert_target_ir_digest_changes(&baseline, "input constraint", |artifact| {
        artifact
            .intents
            .get_mut("t")
            .expect("intent t")
            .input_constraints
            .push(InputConstraint {
                coordinate: "compiler.0".to_owned(),
                source: InputConstraintSource::Compiler,
                predicate: CorePredicate::True,
            });
    });
    assert_target_ir_digest_changes(&baseline, "Core evaluation budget", |artifact| {
        artifact
            .intents
            .get_mut("t")
            .expect("intent t")
            .core_evaluation_budget
            .max_steps += 1;
    });
    assert_target_ir_digest_changes(&baseline, "result expression", |artifact| {
        artifact.intents.get_mut("t").expect("intent t").result =
            CoreExpr::Const(CoreValue::String("changed".to_owned()));
    });
}

#[test]
fn target_ir_obstruction_arm_value_mutation_moves_digest() {
    let baseline = effectful_artifact(EFFECTFUL_REPLACE);
    assert_target_ir_digest_changes(&baseline, "obstruction arm value", |artifact| {
        target_step_mut(artifact)
            .obstruction_arms
            .get_mut("rejected")
            .expect("rejected arm")
            .value = CoreExpr::Const(CoreValue::String("changed".to_owned()));
    });
}

#[test]
fn pure_binding_semantic_mutations_move_target_ir_identity() {
    let baseline = pure_artifact();
    assert_target_ir_digest_changes(&baseline, "pure binding id", |artifact| {
        pure_binding_mut(artifact).id = "sayHello.binding.changed".to_owned();
    });
    assert_target_ir_digest_changes(&baseline, "pure binding local", |artifact| {
        pure_binding_mut(artifact).binding.alpha_name = "$changed".to_owned();
    });
    assert_target_ir_digest_changes(&baseline, "pure helper identity", |artifact| {
        let CoreExpr::Call { callee, .. } = &mut pure_binding_mut(artifact).value else {
            panic!("pure fixture binding is a helper call");
        };
        *callee = "core.string.other".to_owned();
    });
    assert_target_ir_digest_changes(
        &baseline,
        "pure helper implementation closure",
        |artifact| {
            artifact
                .semantic_closure
                .as_mut()
                .expect("pure artifact is closed")
                .lawpacks[0]
                .digest = Some(format!("sha256:{}", "f".repeat(64)));
        },
    );
    assert_target_ir_digest_changes(&baseline, "pure conditional arm", |artifact| {
        pure_binding_mut(artifact).value = CoreExpr::If {
            predicate: Box::new(CorePredicate::True),
            then_value: Box::new(CoreExpr::Const(CoreValue::String("then".to_owned()))),
            else_value: Box::new(CoreExpr::Const(CoreValue::String("else".to_owned()))),
        };
    });
    assert_target_ir_digest_changes(&baseline, "pure result dependency", |artifact| {
        artifact
            .intents
            .get_mut("sayHello")
            .expect("pure intent")
            .result = CoreExpr::Const(CoreValue::Null);
    });

    let mut two_bindings = baseline;
    let first = pure_binding_mut(&mut two_bindings).clone();
    let mut second = first;
    second.id = "sayHello.binding.1".to_owned();
    second.binding.id = "local.1".to_owned();
    second.binding.alpha_name = "$local1".to_owned();
    two_bindings
        .intents
        .get_mut("sayHello")
        .expect("pure intent")
        .pure_bindings
        .push(second);
    assert_target_ir_digest_changes(&two_bindings, "pure binding order", |artifact| {
        artifact
            .intents
            .get_mut("sayHello")
            .expect("pure intent")
            .pure_bindings
            .swap(0, 1);
    });
}

#[test]
fn pure_binding_encoder_rejects_duplicate_identity_or_missing_closure() {
    let mut duplicate = pure_artifact();
    let binding = pure_binding_mut(&mut duplicate).clone();
    duplicate
        .intents
        .get_mut("sayHello")
        .expect("pure intent")
        .pure_bindings
        .push(binding);
    assert_eq!(
        encode_target_ir_artifact(&duplicate)
            .expect_err("duplicate pure binding id must reject")
            .kind(),
        CanonicalErrorKind::UnsupportedValue
    );

    let mut duplicate_local = pure_artifact();
    let mut binding = pure_binding_mut(&mut duplicate_local).clone();
    binding.id = "sayHello.binding.1".to_owned();
    duplicate_local
        .intents
        .get_mut("sayHello")
        .expect("pure intent")
        .pure_bindings
        .push(binding);
    assert_eq!(
        encode_target_ir_artifact(&duplicate_local)
            .expect_err("distinct target ids cannot share one compiler-local identity")
            .kind(),
        CanonicalErrorKind::UnsupportedValue
    );

    let mut unclosed = pure_artifact();
    unclosed.semantic_closure = None;
    assert_eq!(
        encode_target_ir_artifact(&unclosed)
            .expect_err("pure binding without semantic closure must reject")
            .kind(),
        CanonicalErrorKind::UnsupportedValue
    );
}

#[test]
fn target_ir_encoder_rejects_local_identity_shared_across_producer_classes() {
    let baseline = pure_artifact();
    let shared_binding = baseline
        .intents
        .get("sayHello")
        .expect("pure intent")
        .pure_bindings[0]
        .binding
        .clone();

    let mut step_collision = baseline.clone();
    step_collision
        .intents
        .get_mut("sayHello")
        .expect("pure intent")
        .steps
        .push(TargetIrStep {
            id: "sayHello.step.0".to_owned(),
            binding: shared_binding.clone(),
            effect: "example.effect".to_owned(),
            target_intrinsic: "echo.dpo@1.example".to_owned(),
            input: CoreExpr::Const(CoreValue::Null),
            obstruction_failures: Vec::new(),
            obstruction_arms: BTreeMap::new(),
        });
    assert_eq!(
        encode_target_ir_artifact(&step_collision)
            .expect_err("a target step cannot reproduce a pure-binding local")
            .kind(),
        CanonicalErrorKind::UnsupportedValue
    );

    let operation = ResourceRef {
        coordinate: "example.observe@1".to_owned(),
        digest: Some(digest_text('4')),
    };
    let mut request_collision = baseline;
    request_collision
        .semantic_closure
        .as_mut()
        .expect("pure artifact closure")
        .capabilities
        .push(operation.clone());
    request_collision
        .intents
        .get_mut("sayHello")
        .expect("pure intent")
        .external_action_requests
        .push(TargetIrExternalActionRequest {
            id: "sayHello.request.0".to_owned(),
            binding: shared_binding,
            operation,
            input_type: "U64".to_owned(),
            settlement_type: "U64".to_owned(),
            input_schema: ResourceRef {
                coordinate: "example.input-schema@1".to_owned(),
                digest: Some(digest_text('5')),
            },
            settlement_schema: ResourceRef {
                coordinate: "example.settlement-schema@1".to_owned(),
                digest: Some(digest_text('6')),
            },
            input: CoreExpr::Const(CoreValue::Int {
                width: "U64".to_owned(),
                value: "0".to_owned(),
            }),
            authority_scope: CoreExpr::Const(CoreValue::Null),
            basis: CoreExpr::Const(CoreValue::Null),
            budget: CoreExternalActionBudget {
                max_settlement_bytes: CoreExpr::Const(CoreValue::Int {
                    width: "U64".to_owned(),
                    value: "64".to_owned(),
                }),
                max_attempts: CoreExpr::Const(CoreValue::Int {
                    width: "U64".to_owned(),
                    value: "1".to_owned(),
                }),
            },
            reconciliation_law: ResourceRef {
                coordinate: "example.reconciliation@1".to_owned(),
                digest: Some(digest_text('7')),
            },
        });
    assert_eq!(
        encode_target_ir_artifact(&request_collision)
            .expect_err("an external request cannot reproduce a pure-binding local")
            .kind(),
        CanonicalErrorKind::UnsupportedValue
    );
}

#[test]
fn target_ir_encoder_reserves_application_input_local_identity() {
    let mut artifact = pure_artifact();
    pure_binding_mut(&mut artifact).binding.id = "arg.0".to_owned();

    assert_eq!(
        encode_target_ir_artifact(&artifact)
            .expect_err("a pure binding cannot reproduce the application input local")
            .kind(),
        CanonicalErrorKind::UnsupportedValue
    );
}

#[test]
fn target_ir_encoder_rejects_unlocked_or_uppercase_target_profile_digest() {
    let mut missing = effectful_artifact(EFFECTFUL_REPLACE);
    missing.target_profile.digest = None;
    assert_eq!(
        encode_target_ir_artifact(&missing)
            .expect_err("missing target profile digest rejects before hashing")
            .kind(),
        CanonicalErrorKind::UnresolvedDigest
    );
    assert_eq!(
        digest_target_ir_artifact(&missing)
            .expect_err("missing target profile digest rejects during digest")
            .kind(),
        CanonicalErrorKind::UnresolvedDigest
    );

    let mut uppercase = effectful_artifact(EFFECTFUL_REPLACE);
    uppercase.target_profile.digest =
        Some("sha256:AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA".to_owned());
    assert_eq!(
        encode_target_ir_artifact(&uppercase)
            .expect_err("uppercase target profile digest rejects before hashing")
            .kind(),
        CanonicalErrorKind::InvalidDigest
    );
}

fn assert_target_ir_digest_changes(
    baseline: &TargetIrArtifact,
    case: &str,
    mutate: impl FnOnce(&mut TargetIrArtifact),
) {
    let baseline_digest =
        digest_target_ir_artifact(baseline).expect("baseline Target IR artifact digests");
    let mut mutated = baseline.clone();
    mutate(&mut mutated);
    let mutated_digest =
        digest_target_ir_artifact(&mutated).expect("mutated Target IR artifact digests");
    assert_ne!(baseline_digest, mutated_digest, "{case} must move digest");
}

fn target_step_mut(artifact: &mut TargetIrArtifact) -> &mut edict_syntax::TargetIrStep {
    artifact
        .intents
        .get_mut("t")
        .expect("intent t")
        .steps
        .get_mut(0)
        .expect("step 0")
}

fn pure_binding_mut(artifact: &mut TargetIrArtifact) -> &mut edict_syntax::TargetIrPureBinding {
    artifact
        .intents
        .get_mut("sayHello")
        .expect("pure intent")
        .pure_bindings
        .get_mut(0)
        .expect("pure binding 0")
}

fn requirement_mut(artifact: &mut TargetIrArtifact) -> &mut edict_syntax::TargetIrRequirement {
    artifact
        .intents
        .get_mut("t")
        .expect("intent t")
        .requirements
        .get_mut(0)
        .expect("requirement 0")
}

fn replace_required(source: &str, from: &str, to: &str) -> String {
    assert!(
        source.contains(from),
        "test fixture must contain replacement fragment {from:?}"
    );
    source.replace(from, to)
}

fn digest_text(hex: char) -> String {
    format!("sha256:{}", hex.to_string().repeat(64))
}

#[path = "target_ir/ordered.rs"]
mod ordered;

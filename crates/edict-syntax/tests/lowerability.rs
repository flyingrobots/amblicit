//! Lowerability checks for typed v1 requirement artifacts.
//!
//! These tests assert public behavior: classification and stable failure kinds.
//! They do not inspect diagnostic prose, serialized bytes, or documentation
//! layout.

use edict_syntax::{
    check_lowerability, AtomicityRequirement, DirectAdapterSupport, GuardKind,
    LowerabilityFailureKind, LowerabilityReport, LowerabilityStatus, LoweringRequirements,
    NativeEffectSupport, ResourceRef, SemanticEffectRequirement, TargetProfileFacts, WriteClass,
};

fn read_requirements() -> LoweringRequirements {
    LoweringRequirements {
        operation_profile: "kv.transactional@1.readOnly".to_owned(),
        semantic_effects: vec![SemanticEffectRequirement {
            coordinate: "hello.optics@1.readGreeting".to_owned(),
            write_class: WriteClass::Read,
            guard_kinds: vec![GuardKind::PrecommitAtomic],
            obstruction_coordinates: vec!["hello.optics@1.GreetingMissing".to_owned()],
            footprint_obligations: vec!["hello.optics@1.readGreeting.footprint".to_owned()],
            cost_obligations: vec!["hello.optics@1.readGreeting.cost".to_owned()],
        }],
        required_write_classes: vec![WriteClass::Read],
        guard_kinds: vec![GuardKind::PrecommitAtomic],
        atomicity: AtomicityRequirement::Atomic,
        postcondition_support: true,
        obstruction_coordinates: vec!["hello.optics@1.GreetingMissing".to_owned()],
        footprint_obligations: vec!["hello.optics@1.readGreeting.footprint".to_owned()],
        cost_obligations: vec!["hello.optics@1.readGreeting.cost".to_owned()],
        optic_contract: "read-only-point".to_owned(),
    }
}

fn profile_facts() -> TargetProfileFacts {
    TargetProfileFacts {
        coordinate: "kv.transactional@1".to_owned(),
        operation_profiles: vec!["kv.transactional@1.readOnly".to_owned()],
        native_effects: vec![NativeEffectSupport {
            coordinate: "hello.optics@1.readGreeting".to_owned(),
            target_intrinsic: "kv.transactional@1.get".to_owned(),
            write_class: WriteClass::Read,
            guard_kinds: vec![GuardKind::PrecommitAtomic],
        }],
        direct_adapters: Vec::new(),
        write_classes: vec![WriteClass::Read],
        guard_kinds: vec![GuardKind::PrecommitAtomic],
        atomicity: vec![AtomicityRequirement::Atomic],
        postcondition_support: true,
        obstruction_coordinates: vec!["hello.optics@1.GreetingMissing".to_owned()],
        footprint_obligations: vec!["hello.optics@1.readGreeting.footprint".to_owned()],
        cost_obligations: vec!["hello.optics@1.readGreeting.cost".to_owned()],
        optic_contracts: vec!["read-only-point".to_owned()],
    }
}

fn direct_adapter() -> DirectAdapterSupport {
    DirectAdapterSupport {
        semantic_effect: "hello.optics@1.readGreeting".to_owned(),
        target_intrinsic: "kv.transactional@1.get".to_owned(),
        adapter: ResourceRef {
            coordinate: "hello.optics@1.kv.transactional.adapter/v1".to_owned(),
            digest: Some(
                "sha256:1111111111111111111111111111111111111111111111111111111111111111"
                    .to_owned(),
            ),
        },
        write_class: WriteClass::Read,
        guard_kinds: vec![GuardKind::PrecommitAtomic],
        emits_semantic_effects: Vec::new(),
    }
}

fn failure_kinds(report: &LowerabilityReport) -> Vec<LowerabilityFailureKind> {
    report.failures.iter().map(|failure| failure.kind).collect()
}

fn assert_single_failure(
    requirements: &LoweringRequirements,
    facts: &TargetProfileFacts,
    kind: LowerabilityFailureKind,
) {
    let report = check_lowerability(requirements, facts);

    assert_eq!(report.status, LowerabilityStatus::Unsupported);
    assert_eq!(failure_kinds(&report), vec![kind]);
}

#[test]
fn native_target_facts_satisfy_lowering_requirements() {
    let report = check_lowerability(&read_requirements(), &profile_facts());

    assert_eq!(report.status, LowerabilityStatus::Native);
    assert_eq!(report.operation_profile, "kv.transactional@1.readOnly");
    assert_eq!(
        report.obstruction_coordinates,
        vec!["hello.optics@1.GreetingMissing".to_owned()]
    );
    assert_eq!(
        report.failures,
        [] as [edict_syntax::LowerabilityFailure; 0]
    );
    assert_eq!(report.effect_results.len(), 1);
    assert!(report.effect_results[0].is_native());
}

#[test]
fn one_direct_adapter_satisfies_v1_lowering_requirements() {
    let mut facts = profile_facts();
    facts.native_effects.clear();
    facts.direct_adapters.push(direct_adapter());

    let report = check_lowerability(&read_requirements(), &facts);

    assert_eq!(report.status, LowerabilityStatus::Adapted);
    assert_eq!(
        report.failures,
        [] as [edict_syntax::LowerabilityFailure; 0]
    );
    assert_eq!(
        report.effect_results[0].adapter_coordinate(),
        Some("hello.optics@1.kv.transactional.adapter/v1")
    );
}

#[test]
fn missing_operation_profile_reports_stable_failure_kind() {
    let mut facts = profile_facts();
    facts.operation_profiles.clear();

    assert_single_failure(
        &read_requirements(),
        &facts,
        LowerabilityFailureKind::MissingOperationProfile,
    );
}

#[test]
fn unsupported_required_write_class_reports_stable_failure_kind() {
    let mut facts = profile_facts();
    facts.write_classes.clear();

    assert_single_failure(
        &read_requirements(),
        &facts,
        LowerabilityFailureKind::UnsupportedWriteClass,
    );
}

#[test]
fn unsupported_global_guard_reports_stable_failure_kind() {
    let mut facts = profile_facts();
    facts.guard_kinds.clear();

    assert_single_failure(
        &read_requirements(),
        &facts,
        LowerabilityFailureKind::UnsupportedGuard,
    );
}

#[test]
fn unsupported_atomicity_reports_stable_failure_kind() {
    let mut facts = profile_facts();
    facts.atomicity.clear();

    assert_single_failure(
        &read_requirements(),
        &facts,
        LowerabilityFailureKind::UnsupportedAtomicity,
    );
}

#[test]
fn unsupported_postcondition_reports_stable_failure_kind() {
    let mut facts = profile_facts();
    facts.postcondition_support = false;

    assert_single_failure(
        &read_requirements(),
        &facts,
        LowerabilityFailureKind::UnsupportedPostcondition,
    );
}

#[test]
fn missing_obstruction_reports_stable_failure_kind() {
    let mut facts = profile_facts();
    facts.obstruction_coordinates.clear();

    assert_single_failure(
        &read_requirements(),
        &facts,
        LowerabilityFailureKind::MissingObstruction,
    );
}

#[test]
fn missing_footprint_obligation_reports_stable_failure_kind() {
    let mut facts = profile_facts();
    facts.footprint_obligations.clear();

    assert_single_failure(
        &read_requirements(),
        &facts,
        LowerabilityFailureKind::MissingFootprintObligation,
    );
}

#[test]
fn missing_cost_obligation_reports_stable_failure_kind() {
    let mut facts = profile_facts();
    facts.cost_obligations.clear();

    assert_single_failure(
        &read_requirements(),
        &facts,
        LowerabilityFailureKind::MissingCostObligation,
    );
}

#[test]
fn unsupported_optic_contract_reports_stable_failure_kind() {
    let mut facts = profile_facts();
    facts.optic_contracts.clear();

    assert_single_failure(
        &read_requirements(),
        &facts,
        LowerabilityFailureKind::UnsupportedOpticContract,
    );
}

#[test]
fn all_non_effect_obligation_failures_report_stable_kinds_together() {
    let mut facts = profile_facts();
    facts.operation_profiles.clear();
    facts.write_classes.clear();
    facts.guard_kinds.clear();
    facts.atomicity.clear();
    facts.postcondition_support = false;
    facts.obstruction_coordinates.clear();
    facts.footprint_obligations.clear();
    facts.cost_obligations.clear();
    facts.optic_contracts.clear();

    let report = check_lowerability(&read_requirements(), &facts);

    assert_eq!(report.status, LowerabilityStatus::Unsupported);
    assert_eq!(
        failure_kinds(&report),
        vec![
            LowerabilityFailureKind::MissingOperationProfile,
            LowerabilityFailureKind::UnsupportedAtomicity,
            LowerabilityFailureKind::UnsupportedPostcondition,
            LowerabilityFailureKind::UnsupportedWriteClass,
            LowerabilityFailureKind::UnsupportedGuard,
            LowerabilityFailureKind::MissingObstruction,
            LowerabilityFailureKind::MissingFootprintObligation,
            LowerabilityFailureKind::MissingCostObligation,
            LowerabilityFailureKind::UnsupportedOpticContract,
        ]
    );
}

#[test]
fn missing_native_or_adapter_support_is_unsupported() {
    let mut facts = profile_facts();
    facts.native_effects.clear();

    let report = check_lowerability(&read_requirements(), &facts);

    assert_eq!(report.status, LowerabilityStatus::Unsupported);
    assert_eq!(
        report
            .failures
            .iter()
            .map(|failure| failure.kind)
            .collect::<Vec<_>>(),
        vec![LowerabilityFailureKind::MissingEffectSupport]
    );
}

#[test]
fn v1_rejects_floating_direct_adapter_claims() {
    let mut facts = profile_facts();
    facts.native_effects.clear();
    let mut adapter = direct_adapter();
    adapter.adapter.digest = None;
    facts.direct_adapters.push(adapter);

    let report = check_lowerability(&read_requirements(), &facts);

    assert_eq!(report.status, LowerabilityStatus::Unsupported);
    assert_eq!(
        report
            .failures
            .iter()
            .map(|failure| failure.kind)
            .collect::<Vec<_>>(),
        vec![LowerabilityFailureKind::UndigestedAdapter]
    );
}

#[test]
fn native_effects_must_support_required_per_effect_guards() {
    let mut facts = profile_facts();
    facts.native_effects[0].guard_kinds.clear();

    let report = check_lowerability(&read_requirements(), &facts);

    assert_eq!(report.status, LowerabilityStatus::Unsupported);
    assert_eq!(
        report
            .failures
            .iter()
            .map(|failure| failure.kind)
            .collect::<Vec<_>>(),
        vec![LowerabilityFailureKind::UnsupportedEffectGuard]
    );
}

#[test]
fn direct_adapter_can_satisfy_guard_when_native_intrinsic_cannot() {
    let mut facts = profile_facts();
    facts.native_effects[0].guard_kinds.clear();
    facts.direct_adapters.push(direct_adapter());

    let report = check_lowerability(&read_requirements(), &facts);

    assert_eq!(report.status, LowerabilityStatus::Adapted);
    assert_eq!(
        report.failures,
        [] as [edict_syntax::LowerabilityFailure; 0]
    );
    assert_eq!(
        report.effect_results[0].adapter_coordinate(),
        Some("hello.optics@1.kv.transactional.adapter/v1")
    );
}

#[test]
fn v1_rejects_ambiguous_native_support() {
    let mut facts = profile_facts();
    let mut second = facts.native_effects[0].clone();
    second.target_intrinsic = "kv.transactional@1.get.alt".to_owned();
    facts.native_effects.push(second);

    let report = check_lowerability(&read_requirements(), &facts);

    assert_eq!(report.status, LowerabilityStatus::Unsupported);
    assert_eq!(
        report
            .failures
            .iter()
            .map(|failure| failure.kind)
            .collect::<Vec<_>>(),
        vec![LowerabilityFailureKind::AmbiguousNativeSupport]
    );
}

#[test]
fn v1_rejects_chained_adapter_claims() {
    let mut facts = profile_facts();
    facts.native_effects.clear();
    let mut adapter = direct_adapter();
    adapter
        .emits_semantic_effects
        .push("other.lawpack@1.remainingEffect".to_owned());
    facts.direct_adapters.push(adapter);

    let report = check_lowerability(&read_requirements(), &facts);

    assert_eq!(report.status, LowerabilityStatus::Unsupported);
    assert_eq!(
        report
            .failures
            .iter()
            .map(|failure| failure.kind)
            .collect::<Vec<_>>(),
        vec![LowerabilityFailureKind::ChainedAdapterUnsupported]
    );
}

#[test]
fn v1_rejects_ambiguous_direct_adapters() {
    let mut facts = profile_facts();
    facts.native_effects.clear();
    facts.direct_adapters.push(direct_adapter());
    let mut second = direct_adapter();
    second.adapter.coordinate = "hello.optics@1.kv.transactional.adapter.alt/v1".to_owned();
    facts.direct_adapters.push(second);

    let report = check_lowerability(&read_requirements(), &facts);

    assert_eq!(report.status, LowerabilityStatus::Unsupported);
    assert_eq!(
        report
            .failures
            .iter()
            .map(|failure| failure.kind)
            .collect::<Vec<_>>(),
        vec![LowerabilityFailureKind::AmbiguousAdapter]
    );
}

#[test]
fn guard_incompatible_adapter_does_not_make_compatible_adapter_ambiguous() {
    let mut facts = profile_facts();
    facts.native_effects.clear();
    let mut incompatible = direct_adapter();
    incompatible.adapter.coordinate = "hello.optics@1.unguarded.adapter/v1".to_owned();
    incompatible.guard_kinds.clear();
    facts.direct_adapters.push(incompatible);
    facts.direct_adapters.push(direct_adapter());

    let report = check_lowerability(&read_requirements(), &facts);

    assert_eq!(report.status, LowerabilityStatus::Adapted);
    assert_eq!(
        report.failures,
        [] as [edict_syntax::LowerabilityFailure; 0]
    );
    assert_eq!(
        report.effect_results[0].adapter_coordinate(),
        Some("hello.optics@1.kv.transactional.adapter/v1")
    );
}

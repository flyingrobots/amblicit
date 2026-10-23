//! Typed v1 lowerability checks.
//!
//! This module models the pre-target-lowering question: whether a typed
//! `LoweringRequirements` artifact is supported by explicit target-profile
//! facts. It does not lower Core to Target IR and it does not perform
//! admission.

use std::collections::BTreeSet;

use crate::core_ir::ResourceRef;

/// Runtime write authority class required by a semantic effect or profile.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum WriteClass {
    None,
    Read,
    Create,
    Ensure,
    Append,
    Replace,
    Delete,
    Custom(String),
}

/// Target guard capability required for lowering.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum GuardKind {
    PrecommitAtomic,
    Custom(String),
}

/// Atomic application requirement for a lowering request.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum AtomicityRequirement {
    Atomic,
    Custom(String),
}

/// Per-effect lowering requirements in a typed requirements artifact.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SemanticEffectRequirement {
    pub coordinate: String,
    pub write_class: WriteClass,
    pub guard_kinds: Vec<GuardKind>,
    pub obstruction_coordinates: Vec<String>,
    pub footprint_obligations: Vec<String>,
    pub cost_obligations: Vec<String>,
}

/// Typed contract checked before target lowering.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoweringRequirements {
    pub operation_profile: String,
    pub semantic_effects: Vec<SemanticEffectRequirement>,
    pub required_write_classes: Vec<WriteClass>,
    pub guard_kinds: Vec<GuardKind>,
    pub atomicity: AtomicityRequirement,
    pub postcondition_support: bool,
    pub obstruction_coordinates: Vec<String>,
    pub footprint_obligations: Vec<String>,
    pub cost_obligations: Vec<String>,
    pub optic_contract: String,
}

/// Native target support for one semantic effect.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeEffectSupport {
    pub coordinate: String,
    pub target_intrinsic: String,
    pub write_class: WriteClass,
    pub guard_kinds: Vec<GuardKind>,
}

/// Direct lawpack adapter support for one semantic effect.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirectAdapterSupport {
    pub semantic_effect: String,
    pub target_intrinsic: String,
    pub adapter: ResourceRef,
    pub write_class: WriteClass,
    pub guard_kinds: Vec<GuardKind>,
    pub emits_semantic_effects: Vec<String>,
}

/// Explicit target-profile facts available to the v1 lowerability checker.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TargetProfileFacts {
    pub coordinate: String,
    pub operation_profiles: Vec<String>,
    pub native_effects: Vec<NativeEffectSupport>,
    pub direct_adapters: Vec<DirectAdapterSupport>,
    pub write_classes: Vec<WriteClass>,
    pub guard_kinds: Vec<GuardKind>,
    pub atomicity: Vec<AtomicityRequirement>,
    pub postcondition_support: bool,
    pub obstruction_coordinates: Vec<String>,
    pub footprint_obligations: Vec<String>,
    pub cost_obligations: Vec<String>,
    pub optic_contracts: Vec<String>,
}

/// Overall lowerability classification for v1.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LowerabilityStatus {
    Native,
    Adapted,
    Unsupported,
}

/// Stable failure categories returned by lowerability checks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LowerabilityFailureKind {
    MissingOperationProfile,
    UnsupportedWriteClass,
    UnsupportedGuard,
    UnsupportedAtomicity,
    UnsupportedPostcondition,
    MissingObstruction,
    MissingFootprintObligation,
    MissingCostObligation,
    UnsupportedOpticContract,
    MissingEffectSupport,
    AmbiguousNativeSupport,
    AmbiguousAdapter,
    ChainedAdapterUnsupported,
    UndigestedAdapter,
    UnsupportedEffectGuard,
}

/// One failed lowerability obligation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LowerabilityFailure {
    pub kind: LowerabilityFailureKind,
    pub obligation: String,
}

/// Per-effect support selected by lowerability.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LowerabilityEffectStatus {
    Native {
        target_intrinsic: String,
    },
    Adapted {
        adapter: ResourceRef,
        target_intrinsic: String,
    },
    Unsupported,
}

/// Per-effect lowerability result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LowerabilityEffectResult {
    pub semantic_effect: String,
    pub status: LowerabilityEffectStatus,
}

impl LowerabilityEffectResult {
    #[must_use]
    pub const fn is_native(&self) -> bool {
        matches!(self.status, LowerabilityEffectStatus::Native { .. })
    }

    #[must_use]
    pub fn adapter_coordinate(&self) -> Option<&str> {
        match &self.status {
            LowerabilityEffectStatus::Adapted { adapter, .. } => Some(adapter.coordinate.as_str()),
            LowerabilityEffectStatus::Native { .. } | LowerabilityEffectStatus::Unsupported => None,
        }
    }
}

/// Complete v1 lowerability report.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LowerabilityReport {
    pub status: LowerabilityStatus,
    pub target_profile: String,
    pub operation_profile: String,
    pub obstruction_coordinates: Vec<String>,
    pub effect_results: Vec<LowerabilityEffectResult>,
    pub failures: Vec<LowerabilityFailure>,
}

enum NativeEffectSelection {
    Selected(LowerabilityEffectResult),
    NoNativeMatch,
    NativeGuardMismatch,
}

/// Check typed lowering requirements against explicit target-profile facts.
///
/// The v1 checker supports native target facts and exactly one direct adapter
/// per semantic effect. Adapter chains/composite discharge are deliberately
/// rejected and belong to future v2 adapter-composition work.
#[must_use]
pub fn check_lowerability(
    requirements: &LoweringRequirements,
    facts: &TargetProfileFacts,
) -> LowerabilityReport {
    let operation_profiles = string_set(&facts.operation_profiles);
    let write_classes = value_set(&facts.write_classes);
    let guard_kinds = value_set(&facts.guard_kinds);
    let atomicity = value_set(&facts.atomicity);
    let obstructions = string_set(&facts.obstruction_coordinates);
    let footprints = string_set(&facts.footprint_obligations);
    let costs = string_set(&facts.cost_obligations);
    let optic_contracts = string_set(&facts.optic_contracts);

    let mut failures = Vec::new();
    if !operation_profiles.contains(&requirements.operation_profile) {
        push_failure(
            &mut failures,
            LowerabilityFailureKind::MissingOperationProfile,
            &requirements.operation_profile,
        );
    }
    if !atomicity.contains(&requirements.atomicity) {
        push_failure(
            &mut failures,
            LowerabilityFailureKind::UnsupportedAtomicity,
            format!("{:?}", requirements.atomicity),
        );
    }
    if requirements.postcondition_support && !facts.postcondition_support {
        push_failure(
            &mut failures,
            LowerabilityFailureKind::UnsupportedPostcondition,
            "postconditionSupport",
        );
    }

    check_values(
        &requirements.required_write_classes,
        &write_classes,
        LowerabilityFailureKind::UnsupportedWriteClass,
        &mut failures,
    );
    check_values(
        &requirements.guard_kinds,
        &guard_kinds,
        LowerabilityFailureKind::UnsupportedGuard,
        &mut failures,
    );
    check_strings(
        &requirements.obstruction_coordinates,
        &obstructions,
        LowerabilityFailureKind::MissingObstruction,
        &mut failures,
    );
    check_strings(
        &requirements.footprint_obligations,
        &footprints,
        LowerabilityFailureKind::MissingFootprintObligation,
        &mut failures,
    );
    check_strings(
        &requirements.cost_obligations,
        &costs,
        LowerabilityFailureKind::MissingCostObligation,
        &mut failures,
    );
    if !requirements.optic_contract.is_empty()
        && !optic_contracts.contains(&requirements.optic_contract)
    {
        push_failure(
            &mut failures,
            LowerabilityFailureKind::UnsupportedOpticContract,
            &requirements.optic_contract,
        );
    }

    let mut effect_results = Vec::new();
    let mut used_adapter = false;
    for effect in &requirements.semantic_effects {
        check_effect_obligations(effect, facts, &mut failures);
        let result = classify_effect(effect, facts, &mut failures);
        used_adapter |= matches!(result.status, LowerabilityEffectStatus::Adapted { .. });
        effect_results.push(result);
    }

    let status = if failures.is_empty() {
        if used_adapter {
            LowerabilityStatus::Adapted
        } else {
            LowerabilityStatus::Native
        }
    } else {
        LowerabilityStatus::Unsupported
    };

    LowerabilityReport {
        status,
        target_profile: facts.coordinate.clone(),
        operation_profile: requirements.operation_profile.clone(),
        obstruction_coordinates: requirements.obstruction_coordinates.clone(),
        effect_results,
        failures,
    }
}

fn classify_effect(
    effect: &SemanticEffectRequirement,
    facts: &TargetProfileFacts,
    failures: &mut Vec<LowerabilityFailure>,
) -> LowerabilityEffectResult {
    match classify_native_effect(effect, facts, failures) {
        NativeEffectSelection::Selected(native_result) => native_result,
        NativeEffectSelection::NoNativeMatch => classify_adapter_effect(
            effect,
            facts,
            LowerabilityFailureKind::MissingEffectSupport,
            failures,
        ),
        NativeEffectSelection::NativeGuardMismatch => classify_adapter_effect(
            effect,
            facts,
            LowerabilityFailureKind::UnsupportedEffectGuard,
            failures,
        ),
    }
}

fn classify_native_effect(
    effect: &SemanticEffectRequirement,
    facts: &TargetProfileFacts,
    failures: &mut Vec<LowerabilityFailure>,
) -> NativeEffectSelection {
    let native_matches = facts
        .native_effects
        .iter()
        .filter(|support| {
            support.coordinate == effect.coordinate && support.write_class == effect.write_class
        })
        .collect::<Vec<_>>();
    let guarded_native_matches = native_matches
        .iter()
        .copied()
        .filter(|support| supports_required_guards(&effect.guard_kinds, &support.guard_kinds))
        .collect::<Vec<_>>();
    match guarded_native_matches.as_slice() {
        [] if native_matches.is_empty() => NativeEffectSelection::NoNativeMatch,
        [] => NativeEffectSelection::NativeGuardMismatch,
        [native] => NativeEffectSelection::Selected(LowerabilityEffectResult {
            semantic_effect: effect.coordinate.clone(),
            status: LowerabilityEffectStatus::Native {
                target_intrinsic: native.target_intrinsic.clone(),
            },
        }),
        _ => {
            push_failure(
                failures,
                LowerabilityFailureKind::AmbiguousNativeSupport,
                &effect.coordinate,
            );
            NativeEffectSelection::Selected(unsupported_effect(effect))
        }
    }
}

fn classify_adapter_effect(
    effect: &SemanticEffectRequirement,
    facts: &TargetProfileFacts,
    missing_support_kind: LowerabilityFailureKind,
    failures: &mut Vec<LowerabilityFailure>,
) -> LowerabilityEffectResult {
    let adapters = facts
        .direct_adapters
        .iter()
        .filter(|adapter| {
            adapter.semantic_effect == effect.coordinate
                && adapter.write_class == effect.write_class
        })
        .collect::<Vec<_>>();
    let guarded_adapters = adapters
        .iter()
        .copied()
        .filter(|adapter| supports_required_guards(&effect.guard_kinds, &adapter.guard_kinds))
        .collect::<Vec<_>>();

    match guarded_adapters.as_slice() {
        [] => {
            let kind = if adapters.is_empty() {
                missing_support_kind
            } else {
                LowerabilityFailureKind::UnsupportedEffectGuard
            };
            push_failure(failures, kind, &effect.coordinate);
            unsupported_effect(effect)
        }
        [adapter] if !adapter.adapter.is_digest_locked() => {
            push_failure(
                failures,
                LowerabilityFailureKind::UndigestedAdapter,
                &effect.coordinate,
            );
            unsupported_effect(effect)
        }
        [adapter] if adapter.emits_semantic_effects.is_empty() => LowerabilityEffectResult {
            semantic_effect: effect.coordinate.clone(),
            status: LowerabilityEffectStatus::Adapted {
                adapter: adapter.adapter.clone(),
                target_intrinsic: adapter.target_intrinsic.clone(),
            },
        },
        [_adapter] => {
            push_failure(
                failures,
                LowerabilityFailureKind::ChainedAdapterUnsupported,
                &effect.coordinate,
            );
            unsupported_effect(effect)
        }
        _ => {
            push_failure(
                failures,
                LowerabilityFailureKind::AmbiguousAdapter,
                &effect.coordinate,
            );
            unsupported_effect(effect)
        }
    }
}

fn unsupported_effect(effect: &SemanticEffectRequirement) -> LowerabilityEffectResult {
    LowerabilityEffectResult {
        semantic_effect: effect.coordinate.clone(),
        status: LowerabilityEffectStatus::Unsupported,
    }
}

fn supports_required_guards(required: &[GuardKind], supported: &[GuardKind]) -> bool {
    required.iter().all(|guard| supported.contains(guard))
}

fn check_effect_obligations(
    effect: &SemanticEffectRequirement,
    facts: &TargetProfileFacts,
    failures: &mut Vec<LowerabilityFailure>,
) {
    let write_classes = value_set(&facts.write_classes);
    if !write_classes.contains(&effect.write_class) {
        push_failure(
            failures,
            LowerabilityFailureKind::UnsupportedWriteClass,
            format!("{:?}", effect.write_class),
        );
    }
    check_values(
        &effect.guard_kinds,
        &value_set(&facts.guard_kinds),
        LowerabilityFailureKind::UnsupportedGuard,
        failures,
    );
    check_strings(
        &effect.obstruction_coordinates,
        &string_set(&facts.obstruction_coordinates),
        LowerabilityFailureKind::MissingObstruction,
        failures,
    );
    check_strings(
        &effect.footprint_obligations,
        &string_set(&facts.footprint_obligations),
        LowerabilityFailureKind::MissingFootprintObligation,
        failures,
    );
    check_strings(
        &effect.cost_obligations,
        &string_set(&facts.cost_obligations),
        LowerabilityFailureKind::MissingCostObligation,
        failures,
    );
}

fn check_values<T>(
    required: &[T],
    supported: &BTreeSet<T>,
    kind: LowerabilityFailureKind,
    failures: &mut Vec<LowerabilityFailure>,
) where
    T: Clone + Ord + std::fmt::Debug,
{
    for item in required {
        if !supported.contains(item) {
            push_failure(failures, kind, format!("{item:?}"));
        }
    }
}

fn check_strings(
    required: &[String],
    supported: &BTreeSet<String>,
    kind: LowerabilityFailureKind,
    failures: &mut Vec<LowerabilityFailure>,
) {
    for item in required {
        if !supported.contains(item) {
            push_failure(failures, kind, item);
        }
    }
}

fn value_set<T>(items: &[T]) -> BTreeSet<T>
where
    T: Clone + Ord,
{
    items.iter().cloned().collect()
}

fn string_set(items: &[String]) -> BTreeSet<String> {
    items.iter().cloned().collect()
}

fn push_failure(
    failures: &mut Vec<LowerabilityFailure>,
    kind: LowerabilityFailureKind,
    obligation: impl Into<String>,
) {
    let failure = LowerabilityFailure {
        kind,
        obligation: obligation.into(),
    };
    if !failures.contains(&failure) {
        failures.push(failure);
    }
}

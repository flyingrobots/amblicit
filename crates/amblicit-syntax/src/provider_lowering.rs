//! In-process compatibility adapters for the current built-in target lowerers.
//!
//! This module is a migration seam. It gives provider-shaped callers an
//! explicit lowerer selection and borrowed request without defining provider
//! discovery, manifest resolution, component identity, or the external WIT
//! transport ABI.

use crate::core_ir::CoreModule;
use crate::target_ir::{
    lower_to_target_ir, TargetIrLoweringFacts, TargetLoweringReport, ECHO_DPO_TARGET_PROFILE,
    GITWARP_REF_CRDT_TARGET_PROFILE,
};

/// Existing in-tree target lowerer selected through the provider migration seam.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuiltinTargetLowerer {
    EchoDpo,
    GitwarpRefCrdt,
}

impl BuiltinTargetLowerer {
    /// Target-profile coordinate served by this built-in lowerer.
    #[must_use]
    pub const fn target_profile_coordinate(self) -> &'static str {
        match self {
            Self::EchoDpo => ECHO_DPO_TARGET_PROFILE,
            Self::GitwarpRefCrdt => GITWARP_REF_CRDT_TARGET_PROFILE,
        }
    }
}

/// Explicit inputs passed to a built-in target lowerer.
#[derive(Debug, Clone, Copy)]
pub struct BuiltinLowererRequest<'a> {
    pub core: &'a CoreModule,
    pub facts: &'a TargetIrLoweringFacts,
}

/// Stable incompatibility categories detected before invoking a built-in lowerer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuiltinLowererCompatibilityFailureKind {
    TargetProfileMismatch,
}

/// A built-in lowerer cannot serve the explicitly requested target profile.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuiltinLowererCompatibilityFailure {
    pub kind: BuiltinLowererCompatibilityFailureKind,
    pub lowerer: BuiltinTargetLowerer,
    pub expected_target_profile: String,
    pub actual_target_profile: String,
}

impl std::fmt::Display for BuiltinLowererCompatibilityFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "built-in lowerer compatibility failure {:?} for {:?}: expected target profile `{}`, got `{}`",
            self.kind,
            self.lowerer,
            self.expected_target_profile,
            self.actual_target_profile
        )
    }
}

impl std::error::Error for BuiltinLowererCompatibilityFailure {}

/// Result of selecting and invoking a built-in target lowerer.
///
/// A successful lowerer selection still returns the complete target-lowering
/// report, including a structured `Unsupported` result when the target lowerer
/// faithfully refuses the supplied Core or facts.
pub type BuiltinLoweringResult = Result<TargetLoweringReport, BuiltinLowererCompatibilityFailure>;

/// Invoke one existing in-tree lowerer through the provider migration seam.
///
/// Once the selected lowerer and target-profile coordinate agree, this function
/// returns [`lower_to_target_ir`] unchanged. Target semantic failures therefore
/// retain their existing structured kinds and fields.
///
/// # Errors
///
/// Returns `TargetProfileMismatch` when `lowerer` does not serve the target
/// profile selected by `request.facts`.
pub fn lower_with_builtin_lowerer(
    lowerer: BuiltinTargetLowerer,
    request: BuiltinLowererRequest<'_>,
) -> BuiltinLoweringResult {
    let expected_target_profile = lowerer.target_profile_coordinate();
    let actual_target_profile = request.facts.target_profile.coordinate.as_str();
    if actual_target_profile != expected_target_profile {
        return Err(BuiltinLowererCompatibilityFailure {
            kind: BuiltinLowererCompatibilityFailureKind::TargetProfileMismatch,
            lowerer,
            expected_target_profile: expected_target_profile.to_owned(),
            actual_target_profile: actual_target_profile.to_owned(),
        });
    }

    Ok(lower_to_target_ir(request.core, request.facts))
}

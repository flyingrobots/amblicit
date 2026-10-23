use std::fmt;

use amblicit_provider_schema::ProviderArtifactSchemaRegistry;
use amblicit_syntax::{
    ProviderInvocationValidationReport, ValidatedProviderLoweringRequest,
    ValidatedProviderVerificationRequest,
};

use super::{
    PreparedProviderComponent, ProviderComponentHost, ProviderHostFailure, ProviderHostFailureKind,
    ProviderHostLimits, ProviderHostPhase, ValidatedProviderLoweringOutcome,
    ValidatedProviderVerificationOutcome,
};

/// Stable host-failure identity used by deterministic replay.
///
/// Opaque engine diagnostics are deliberately excluded because they are not
/// part of Edict's public provider-failure identity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderHostFailureIdentity {
    kind: ProviderHostFailureKind,
    phase: ProviderHostPhase,
    validation: Option<ProviderInvocationValidationReport>,
}

impl ProviderHostFailureIdentity {
    /// Return the stable host failure category.
    #[must_use]
    pub const fn kind(&self) -> ProviderHostFailureKind {
        self.kind
    }

    /// Return the stable host phase.
    #[must_use]
    pub const fn phase(&self) -> ProviderHostPhase {
        self.phase
    }

    /// Return the structured response-validation report when one exists.
    #[must_use]
    pub const fn validation_report(&self) -> Option<&ProviderInvocationValidationReport> {
        self.validation.as_ref()
    }
}

impl From<&ProviderHostFailure> for ProviderHostFailureIdentity {
    fn from(failure: &ProviderHostFailure) -> Self {
        Self {
            kind: failure.kind(),
            phase: failure.phase(),
            validation: failure.validation_report().cloned(),
        }
    }
}

/// One stable observation produced by a replayed provider invocation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProviderReplayObservation<T> {
    /// The component returned a completely validated success or typed refusal.
    Completed(T),
    /// The host rejected the invocation with this stable failure identity.
    Rejected(ProviderHostFailureIdentity),
}

/// Exact stable reason two replay observations disagreed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderReplayFailureKind {
    /// One invocation completed while the other was rejected.
    DispositionMismatch,
    /// Both invocations completed but their sealed outcomes differed.
    CompletedOutcomeMismatch,
    /// Both invocations failed but their stable host identities differed.
    HostFailureIdentityMismatch,
}

/// Structured failure returned when two fresh-store observations disagree.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProviderReplayFailure {
    kind: ProviderReplayFailureKind,
}

impl ProviderReplayFailure {
    /// Return the stable replay mismatch category.
    #[must_use]
    pub const fn kind(&self) -> ProviderReplayFailureKind {
        self.kind
    }
}

impl fmt::Display for ProviderReplayFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "provider replay mismatch: {:?}", self.kind)
    }
}

impl std::error::Error for ProviderReplayFailure {}

/// Opaque proof that two fresh-store invocations produced one equal observation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidatedProviderReplay<T> {
    observation: ProviderReplayObservation<T>,
}

impl<T> ValidatedProviderReplay<T> {
    /// Return the exact completed outcome or stable rejected identity.
    #[must_use]
    pub const fn observation(&self) -> &ProviderReplayObservation<T> {
        &self.observation
    }
}

impl ProviderComponentHost {
    /// Invoke one lowerer twice through independent fresh stores and compare
    /// the complete stable observations.
    ///
    /// # Errors
    ///
    /// Returns a structured mismatch without exposing either observation as
    /// authoritative when the two executions disagree.
    pub fn replay_lowerer(
        &self,
        prepared: &PreparedProviderComponent<'_>,
        validated: &ValidatedProviderLoweringRequest<'_>,
        registry: &ProviderArtifactSchemaRegistry,
        limits: ProviderHostLimits,
    ) -> Result<ValidatedProviderReplay<ValidatedProviderLoweringOutcome>, ProviderReplayFailure>
    {
        let first = observe(self.invoke_lowerer(prepared, validated, registry, limits));
        let second = observe(self.invoke_lowerer(prepared, validated, registry, limits));
        compare_observations(first, &second)
    }

    /// Invoke one verifier twice through independent fresh stores and compare
    /// the complete stable observations.
    ///
    /// # Errors
    ///
    /// Returns a structured mismatch without exposing either observation as
    /// authoritative when the two executions disagree.
    pub fn replay_verifier(
        &self,
        prepared: &PreparedProviderComponent<'_>,
        validated: &ValidatedProviderVerificationRequest<'_>,
        registry: &ProviderArtifactSchemaRegistry,
        limits: ProviderHostLimits,
    ) -> Result<ValidatedProviderReplay<ValidatedProviderVerificationOutcome>, ProviderReplayFailure>
    {
        let first = observe(self.invoke_verifier(prepared, validated, registry, limits));
        let second = observe(self.invoke_verifier(prepared, validated, registry, limits));
        compare_observations(first, &second)
    }
}

fn observe<T>(result: Result<T, ProviderHostFailure>) -> ProviderReplayObservation<T> {
    match result {
        Ok(outcome) => ProviderReplayObservation::Completed(outcome),
        Err(failure) => ProviderReplayObservation::Rejected((&failure).into()),
    }
}

fn compare_observations<T: PartialEq>(
    first: ProviderReplayObservation<T>,
    second: &ProviderReplayObservation<T>,
) -> Result<ValidatedProviderReplay<T>, ProviderReplayFailure> {
    if &first == second {
        return Ok(ValidatedProviderReplay { observation: first });
    }
    let kind = match (&first, second) {
        (ProviderReplayObservation::Completed(_), ProviderReplayObservation::Completed(_)) => {
            ProviderReplayFailureKind::CompletedOutcomeMismatch
        }
        (ProviderReplayObservation::Rejected(_), ProviderReplayObservation::Rejected(_)) => {
            ProviderReplayFailureKind::HostFailureIdentityMismatch
        }
        _ => ProviderReplayFailureKind::DispositionMismatch,
    };
    Err(ProviderReplayFailure { kind })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rejected(kind: ProviderHostFailureKind) -> ProviderReplayObservation<&'static str> {
        ProviderReplayObservation::Rejected(ProviderHostFailureIdentity {
            kind,
            phase: ProviderHostPhase::Lower,
            validation: None,
        })
    }

    #[test]
    fn every_replay_mismatch_has_a_distinct_stable_kind() {
        let completed = compare_observations(
            ProviderReplayObservation::Completed("first"),
            &ProviderReplayObservation::Completed("second"),
        )
        .expect_err("different completed outcomes reject");
        assert_eq!(
            completed.kind(),
            ProviderReplayFailureKind::CompletedOutcomeMismatch
        );

        let rejected_mismatch = compare_observations(
            rejected(ProviderHostFailureKind::GuestTrap),
            &rejected(ProviderHostFailureKind::FuelExhausted),
        )
        .expect_err("different stable host failures reject");
        assert_eq!(
            rejected_mismatch.kind(),
            ProviderReplayFailureKind::HostFailureIdentityMismatch
        );

        let disposition = compare_observations(
            ProviderReplayObservation::Completed("completed"),
            &rejected(ProviderHostFailureKind::GuestTrap),
        )
        .expect_err("completed versus rejected observations reject");
        assert_eq!(
            disposition.kind(),
            ProviderReplayFailureKind::DispositionMismatch
        );
    }

    #[test]
    fn opaque_engine_diagnostics_do_not_define_replay_identity() {
        let first = ProviderHostFailure::message(
            ProviderHostFailureKind::GuestTrap,
            ProviderHostPhase::Lower,
            "first engine rendering",
        );
        let second = ProviderHostFailure::message(
            ProviderHostFailureKind::GuestTrap,
            ProviderHostPhase::Lower,
            "different engine rendering",
        );

        assert_eq!(
            ProviderHostFailureIdentity::from(&first),
            ProviderHostFailureIdentity::from(&second)
        );
    }
}

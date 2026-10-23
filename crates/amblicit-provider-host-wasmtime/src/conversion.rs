use amblicit_syntax::{
    ProviderArtifact, ProviderBoundArtifact, ProviderDiagnostic, ProviderDiagnosticSeverity,
    ProviderDigest, ProviderDigestAlgorithm, ProviderLoweringOutputArtifact,
    ProviderLoweringOutputKind, ProviderLoweringRequest, ProviderLoweringResult,
    ProviderLoweringSuccess, ProviderRefusal, ProviderRefusalKind, ProviderResourceRef,
    ProviderSemanticInput, ProviderSemanticInputKind, ProviderVerificationOutputArtifact,
    ProviderVerificationOutputKind, ProviderVerificationRequest, ProviderVerificationResult,
    ProviderVerificationSuccess,
};

use super::{lowerer_bindings, verifier_bindings};

pub(crate) fn lowering_request(
    request: &ProviderLoweringRequest,
) -> lowerer_bindings::LoweringRequestV1 {
    use lowerer_bindings::edict::target_provider::protocol as wit;
    lowerer_bindings::LoweringRequestV1 {
        protocol_version: wit::ProtocolVersionV1 {
            major: request.protocol_version.major,
            minor: request.protocol_version.minor,
            patch: request.protocol_version.patch,
        },
        core: lower_bound_artifact(&request.core),
        target_profile: lower_bound_artifact(&request.target_profile),
        semantic_inputs: request
            .semantic_inputs
            .iter()
            .map(lower_semantic_input)
            .collect(),
        requested_outputs: request
            .requested_outputs
            .iter()
            .map(|output| wit::LoweringOutputRequest {
                role: output.role.clone(),
                kind: match output.kind {
                    ProviderLoweringOutputKind::TargetIr => wit::LoweringOutputKind::TargetIr,
                    ProviderLoweringOutputKind::GeneratedArtifact => {
                        wit::LoweringOutputKind::GeneratedArtifact
                    }
                    ProviderLoweringOutputKind::ReviewPayload => {
                        wit::LoweringOutputKind::ReviewPayload
                    }
                },
                domain: output.domain.clone(),
            })
            .collect(),
        limits: wit::ResponseLimitsV1 {
            max_output_count: request.limits.max_output_count,
            max_diagnostic_count: request.limits.max_diagnostic_count,
            max_total_response_bytes: request.limits.max_total_response_bytes,
        },
    }
}

pub(crate) fn verification_request(
    request: &ProviderVerificationRequest,
) -> verifier_bindings::VerificationRequestV1 {
    use verifier_bindings::edict::target_provider::protocol as wit;
    verifier_bindings::VerificationRequestV1 {
        protocol_version: wit::ProtocolVersionV1 {
            major: request.protocol_version.major,
            minor: request.protocol_version.minor,
            patch: request.protocol_version.patch,
        },
        core: verify_bound_artifact(&request.core),
        target_profile: verify_bound_artifact(&request.target_profile),
        target_ir: verify_bound_artifact(&request.target_ir),
        semantic_inputs: request
            .semantic_inputs
            .iter()
            .map(verify_semantic_input)
            .collect(),
        requested_outputs: request
            .requested_outputs
            .iter()
            .map(|output| wit::VerificationOutputRequest {
                role: output.role.clone(),
                kind: match output.kind {
                    ProviderVerificationOutputKind::VerifierReport => {
                        wit::VerificationOutputKind::VerifierReport
                    }
                },
                domain: output.domain.clone(),
            })
            .collect(),
        limits: wit::ResponseLimitsV1 {
            max_output_count: request.limits.max_output_count,
            max_diagnostic_count: request.limits.max_diagnostic_count,
            max_total_response_bytes: request.limits.max_total_response_bytes,
        },
    }
}

pub(crate) fn lowering_result(
    result: lowerer_bindings::LoweringResultV1,
) -> ProviderLoweringResult {
    use lowerer_bindings::edict::target_provider::protocol as wit;
    result
        .map(|success| ProviderLoweringSuccess {
            outputs: success
                .outputs
                .into_iter()
                .map(|output| ProviderLoweringOutputArtifact {
                    role: output.role,
                    kind: match output.kind {
                        wit::LoweringOutputKind::TargetIr => ProviderLoweringOutputKind::TargetIr,
                        wit::LoweringOutputKind::GeneratedArtifact => {
                            ProviderLoweringOutputKind::GeneratedArtifact
                        }
                        wit::LoweringOutputKind::ReviewPayload => {
                            ProviderLoweringOutputKind::ReviewPayload
                        }
                    },
                    artifact: ProviderArtifact {
                        domain: output.artifact.domain,
                        bytes: output.artifact.bytes,
                    },
                    logical_path: output.logical_path,
                })
                .collect(),
            diagnostics: success
                .diagnostics
                .into_iter()
                .map(lower_diagnostic)
                .collect(),
        })
        .map_err(|refusal| ProviderRefusal {
            kind: lower_refusal_kind(refusal.kind),
            subject: refusal.subject,
            diagnostics: refusal
                .diagnostics
                .into_iter()
                .map(lower_diagnostic)
                .collect(),
        })
}

pub(crate) fn verification_result(
    result: verifier_bindings::VerificationResultV1,
) -> ProviderVerificationResult {
    result
        .map(|success| ProviderVerificationSuccess {
            outputs: success
                .outputs
                .into_iter()
                .map(|output| ProviderVerificationOutputArtifact {
                    role: output.role,
                    kind: ProviderVerificationOutputKind::VerifierReport,
                    artifact: ProviderArtifact {
                        domain: output.artifact.domain,
                        bytes: output.artifact.bytes,
                    },
                    logical_path: output.logical_path,
                })
                .collect(),
            diagnostics: success
                .diagnostics
                .into_iter()
                .map(verify_diagnostic)
                .collect(),
        })
        .map_err(|refusal| ProviderRefusal {
            kind: verify_refusal_kind(refusal.kind),
            subject: refusal.subject,
            diagnostics: refusal
                .diagnostics
                .into_iter()
                .map(verify_diagnostic)
                .collect(),
        })
}

pub(crate) fn lowering_input_bytes(request: &ProviderLoweringRequest) -> Option<u64> {
    let mut count = ByteCount::default();
    count.bound_artifact(&request.core)?;
    count.bound_artifact(&request.target_profile)?;
    for input in &request.semantic_inputs {
        count.semantic_input(input)?;
    }
    for output in &request.requested_outputs {
        count.string(&output.role)?;
        count.string(&output.domain)?;
    }
    Some(count.value)
}

pub(crate) fn verification_input_bytes(request: &ProviderVerificationRequest) -> Option<u64> {
    let mut count = ByteCount::default();
    count.bound_artifact(&request.core)?;
    count.bound_artifact(&request.target_profile)?;
    count.bound_artifact(&request.target_ir)?;
    for input in &request.semantic_inputs {
        count.semantic_input(input)?;
    }
    for output in &request.requested_outputs {
        count.string(&output.role)?;
        count.string(&output.domain)?;
    }
    Some(count.value)
}

pub(crate) fn lowering_diagnostic_bytes(result: &ProviderLoweringResult) -> Option<u64> {
    diagnostic_bytes(match result {
        Ok(success) => &success.diagnostics,
        Err(refusal) => &refusal.diagnostics,
    })
}

pub(crate) fn verification_diagnostic_bytes(result: &ProviderVerificationResult) -> Option<u64> {
    diagnostic_bytes(match result {
        Ok(success) => &success.diagnostics,
        Err(refusal) => &refusal.diagnostics,
    })
}

fn diagnostic_bytes(diagnostics: &[ProviderDiagnostic]) -> Option<u64> {
    let mut count = ByteCount::default();
    for diagnostic in diagnostics {
        count.string(&diagnostic.code)?;
        count.string(&diagnostic.message)?;
        if let Some(repair) = &diagnostic.repair {
            count.string(repair)?;
        }
    }
    Some(count.value)
}

fn lower_bound_artifact(
    artifact: &ProviderBoundArtifact,
) -> lowerer_bindings::edict::target_provider::protocol::BoundArtifact {
    use lowerer_bindings::edict::target_provider::protocol as wit;
    wit::BoundArtifact {
        reference: lower_resource(&artifact.reference),
        artifact: wit::Artifact {
            domain: artifact.artifact.domain.clone(),
            bytes: artifact.artifact.bytes.clone(),
        },
    }
}

fn verify_bound_artifact(
    artifact: &ProviderBoundArtifact,
) -> verifier_bindings::edict::target_provider::protocol::BoundArtifact {
    use verifier_bindings::edict::target_provider::protocol as wit;
    wit::BoundArtifact {
        reference: verify_resource(&artifact.reference),
        artifact: wit::Artifact {
            domain: artifact.artifact.domain.clone(),
            bytes: artifact.artifact.bytes.clone(),
        },
    }
}

fn lower_resource(
    resource: &ProviderResourceRef,
) -> lowerer_bindings::edict::target_provider::protocol::ResourceRef {
    use lowerer_bindings::edict::target_provider::protocol as wit;
    wit::ResourceRef {
        coordinate: resource.coordinate.clone(),
        digest: lower_digest(&resource.digest),
    }
}

fn verify_resource(
    resource: &ProviderResourceRef,
) -> verifier_bindings::edict::target_provider::protocol::ResourceRef {
    use verifier_bindings::edict::target_provider::protocol as wit;
    wit::ResourceRef {
        coordinate: resource.coordinate.clone(),
        digest: verify_digest(&resource.digest),
    }
}

fn lower_digest(
    digest: &ProviderDigest,
) -> lowerer_bindings::edict::target_provider::protocol::Digest {
    use lowerer_bindings::edict::target_provider::protocol as wit;
    wit::Digest {
        algorithm: match digest.algorithm {
            ProviderDigestAlgorithm::Sha256 => wit::DigestAlgorithm::Sha256,
        },
        bytes: digest.bytes.clone(),
    }
}

fn verify_digest(
    digest: &ProviderDigest,
) -> verifier_bindings::edict::target_provider::protocol::Digest {
    use verifier_bindings::edict::target_provider::protocol as wit;
    wit::Digest {
        algorithm: match digest.algorithm {
            ProviderDigestAlgorithm::Sha256 => wit::DigestAlgorithm::Sha256,
        },
        bytes: digest.bytes.clone(),
    }
}

fn lower_semantic_input(
    input: &ProviderSemanticInput,
) -> lowerer_bindings::edict::target_provider::protocol::SemanticInput {
    use lowerer_bindings::edict::target_provider::protocol as wit;
    wit::SemanticInput {
        role: input.role.clone(),
        kind: match &input.kind {
            ProviderSemanticInputKind::Lawpack => wit::SemanticInputKind::Lawpack,
            ProviderSemanticInputKind::AuthorityFacts => wit::SemanticInputKind::AuthorityFacts,
            ProviderSemanticInputKind::LowerabilityFacts => {
                wit::SemanticInputKind::LowerabilityFacts
            }
            ProviderSemanticInputKind::Auxiliary(value) => {
                wit::SemanticInputKind::Auxiliary(value.clone())
            }
        },
        artifact: lower_bound_artifact(&input.artifact),
    }
}

fn verify_semantic_input(
    input: &ProviderSemanticInput,
) -> verifier_bindings::edict::target_provider::protocol::SemanticInput {
    use verifier_bindings::edict::target_provider::protocol as wit;
    wit::SemanticInput {
        role: input.role.clone(),
        kind: match &input.kind {
            ProviderSemanticInputKind::Lawpack => wit::SemanticInputKind::Lawpack,
            ProviderSemanticInputKind::AuthorityFacts => wit::SemanticInputKind::AuthorityFacts,
            ProviderSemanticInputKind::LowerabilityFacts => {
                wit::SemanticInputKind::LowerabilityFacts
            }
            ProviderSemanticInputKind::Auxiliary(value) => {
                wit::SemanticInputKind::Auxiliary(value.clone())
            }
        },
        artifact: verify_bound_artifact(&input.artifact),
    }
}

fn lower_diagnostic(
    diagnostic: lowerer_bindings::edict::target_provider::protocol::Diagnostic,
) -> ProviderDiagnostic {
    use lowerer_bindings::edict::target_provider::protocol::DiagnosticSeverity;
    ProviderDiagnostic {
        code: diagnostic.code,
        severity: match diagnostic.severity {
            DiagnosticSeverity::Error => ProviderDiagnosticSeverity::Error,
            DiagnosticSeverity::Warning => ProviderDiagnosticSeverity::Warning,
            DiagnosticSeverity::Info => ProviderDiagnosticSeverity::Info,
        },
        message: diagnostic.message,
        repair: diagnostic.repair,
    }
}

fn verify_diagnostic(
    diagnostic: verifier_bindings::edict::target_provider::protocol::Diagnostic,
) -> ProviderDiagnostic {
    use verifier_bindings::edict::target_provider::protocol::DiagnosticSeverity;
    ProviderDiagnostic {
        code: diagnostic.code,
        severity: match diagnostic.severity {
            DiagnosticSeverity::Error => ProviderDiagnosticSeverity::Error,
            DiagnosticSeverity::Warning => ProviderDiagnosticSeverity::Warning,
            DiagnosticSeverity::Info => ProviderDiagnosticSeverity::Info,
        },
        message: diagnostic.message,
        repair: diagnostic.repair,
    }
}

fn lower_refusal_kind(
    kind: lowerer_bindings::edict::target_provider::protocol::ProviderRefusalKind,
) -> ProviderRefusalKind {
    use lowerer_bindings::edict::target_provider::protocol::ProviderRefusalKind as Wit;
    match kind {
        Wit::UnsupportedCoreAbi => ProviderRefusalKind::UnsupportedCoreAbi,
        Wit::UnsupportedTargetProfile => ProviderRefusalKind::UnsupportedTargetProfile,
        Wit::UnsupportedSemantics => ProviderRefusalKind::UnsupportedSemantics,
        Wit::UnsupportedOutputRole => ProviderRefusalKind::UnsupportedOutputRole,
        Wit::InvalidSemanticArtifact => ProviderRefusalKind::InvalidSemanticArtifact,
    }
}

fn verify_refusal_kind(
    kind: verifier_bindings::edict::target_provider::protocol::ProviderRefusalKind,
) -> ProviderRefusalKind {
    use verifier_bindings::edict::target_provider::protocol::ProviderRefusalKind as Wit;
    match kind {
        Wit::UnsupportedCoreAbi => ProviderRefusalKind::UnsupportedCoreAbi,
        Wit::UnsupportedTargetProfile => ProviderRefusalKind::UnsupportedTargetProfile,
        Wit::UnsupportedSemantics => ProviderRefusalKind::UnsupportedSemantics,
        Wit::UnsupportedOutputRole => ProviderRefusalKind::UnsupportedOutputRole,
        Wit::InvalidSemanticArtifact => ProviderRefusalKind::InvalidSemanticArtifact,
    }
}

#[derive(Default)]
struct ByteCount {
    value: u64,
}

impl ByteCount {
    fn add(&mut self, value: usize) -> Option<()> {
        let value = u64::try_from(value).ok()?;
        self.value = self.value.checked_add(value)?;
        Some(())
    }

    fn string(&mut self, value: &str) -> Option<()> {
        self.add(value.len())
    }

    fn bound_artifact(&mut self, artifact: &ProviderBoundArtifact) -> Option<()> {
        self.string(&artifact.reference.coordinate)?;
        self.add(artifact.reference.digest.bytes.len())?;
        self.string(&artifact.artifact.domain)?;
        self.add(artifact.artifact.bytes.len())
    }

    fn semantic_input(&mut self, input: &ProviderSemanticInput) -> Option<()> {
        self.string(&input.role)?;
        if let ProviderSemanticInputKind::Auxiliary(value) = &input.kind {
            self.string(value)?;
        }
        self.bound_artifact(&input.artifact)
    }
}

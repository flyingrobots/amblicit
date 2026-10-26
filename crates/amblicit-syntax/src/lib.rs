//! `amblicit-syntax`: the Edict language front end.
//!
//! Scope is `edict.implementation/minimal-v1` (see SPEC - Edict Language v1).
//! Phase 1 parses: package and imports; `type` records and refined scalars;
//! `enum` declarations and `variant` types; `intent`s with their clauses;
//! `let`/`return`/`require`/`guarantee`/`assert`; the `if` family; bounded
//! `for`; calls and type-calls; variant-literal constructors; and `match`.
//! Phase 2 currently exposes the `validate_surface` compiler stage for
//! source-AST constraints that do not require import resolution, resolved typing,
//! target/lawpack facts, or Core IR: bounded runtime `String`/`Bytes`, required
//! intent operation-mode/budget/basis clauses, duplicate singleton intent
//! clauses, module namespace collisions, and source binder shadowing.
//! Phase 3 begins the executable compiler spine with `resolve_module`,
//! `type_check`, `lower_core`, and `compile_to_core`, currently covering the
//! initial pure local-record subset and producing in-memory Core IR only.
//! Authority-facts loading can build compiler context from explicit,
//! digest-bound JSON files for the first supported profile, budget, and
//! write-class facts.
//! The crate also exposes typed v1 target-profile conformance, lowerability, and
//! Echo/git-warp Target IR lowering, contract-bundle checks, and typed Gate C
//! admission-boundary checks.
//! Developer-tooling support begins with lexical highlighting roles for editor
//! adapters.
//! Conformance validates runtime-neutral target-profile manifests. Lowerability
//! checks `LoweringRequirements` against explicit target-profile facts and
//! classifies support as native, directly adapted, or unsupported.
//! Contract-bundle validation checks participant-neutral, SHA-locked bundle and
//! assurance evidence manifests. Admission-boundary checks validate Edict-owned
//! artifact and invocation evidence bindings without evaluating participant
//! policy.
//! Provider support validates generated/component provenance plus pure,
//! runtime-neutral invocation request and result envelopes through an explicit
//! host-owned artifact-schema validator. Opaque validated request wrappers and
//! sealed host-computed output manifests remain separate from external
//! component dispatch. The crate also exposes an explicit in-process
//! compatibility seam over the current Echo and git-warp lowerers.
//! First-order nongeneric source `fn` declarations lower through typed pure Core
//! bodies. `const` declarations, `record` semantic-effect statements,
//! list/map/unit expression literals, full source-language lowering, general
//! target lowering, and full admission execution tooling are deferred. The
//! crate exposes the reference canonical Core and Target IR encoders for
//! `edict.canonical-cbor/v1`, plus the domain-separated `edict.core.module/v1`
//! Core digest and `edict.target-ir.artifact/v1` Target IR digest used by
//! reviewed golden fixtures.
//!
//! Assurance tooling (HOLMES / Watson / Moriarty) remains shared platform
//! machinery. This crate validates typed references to its evidence; it does not
//! execute those tools or make admission decisions.
//!
//! # Example
//!
//! The one-call front-end entry point is [`check`], which parses and
//! surface-validates a source string:
//!
//! ```
//! use amblicit_syntax::{check, CheckOutcome};
//!
//! assert_eq!(check("package examples.hello@1;\n"), CheckOutcome::Valid);
//! ```
//!
//! The underlying stages remain available for callers that need the parsed
//! module ([`parse_module`] then [`validate_surface`]).
//!
//! # API stability
//!
//! During the alpha train, [`check`] / [`CheckOutcome`] and the
//! `parse_module` → `validate_surface` pair are the supported front-end entry
//! points. The deeper module surfaces (compiler spine, canonical encoder,
//! target IR, admission, bundles) are exposed for evidence and integration but
//! may move before 1.0.

pub mod admission;
pub mod ast;
pub mod authority_facts;
pub mod canonical;
pub mod compiler;
pub mod contract_bundle;
pub mod core_ir;
pub mod highlight;
pub mod lawpack;
pub mod lawpack_adapter;
pub mod lawpack_authoring;
pub mod lowerability;
pub mod numeric;
pub mod parser;
pub mod provider;
pub mod provider_invocation;
pub mod provider_lowering;
pub mod result_projection;
pub mod semantic;
pub mod source_closure;
pub mod source_linker;
pub mod target_intrinsics;
pub mod target_ir;
pub mod target_profile;
pub mod target_profile_contract_resources;
pub mod token;

pub use admission::{
    check_gate_c_invocation, digest_admission_request, validate_admission_receipt,
    validate_admission_request, AdmissionDecision, AdmissionEvidenceRef, AdmissionReceiptBody,
    AdmissionRequest, AdmissionValidationFailure, AdmissionValidationFailureKind,
    AdmissionValidationReport, AdmissionValidationStatus, AuthoringProvenance, CapabilityReceipt,
    CapabilityReceiptKind, ExecutionInputKind, ExecutionInputRef, GateCInvocation,
    OperationRequirementRef, ADMISSION_RECEIPT_API_VERSION, ADMISSION_REQUEST_API_VERSION,
    ADMISSION_REQUEST_DIGEST_DOMAIN,
};
pub use authority_facts::{
    compiler_context_from_authority_facts, decode_authority_facts_cbor,
    digest_authority_facts_document, encode_authority_facts_cbor, load_authority_facts_file,
    load_compiler_context_from_authority_fact_files, AuthorityFactSource, AuthorityFactSourceKind,
    AuthorityFactsDocument, AuthorityFactsLoadFailure, AuthorityFactsLoadFailureKind, BudgetFact,
    EffectWriteClassFact, OperationProfileFact, AUTHORITY_FACTS_API_VERSION,
    AUTHORITY_FACTS_CDDL_ROOT,
};
pub use canonical::{
    decode_canonical_cbor, digest_bundle_layer, digest_canonical_artifact, digest_core_module,
    digest_target_ir_artifact, encode_canonical_cbor, encode_core_module,
    encode_target_ir_artifact, BundleDigestDomain, BundlePreimageComponent, BundleSourceDescriptor,
    CanonicalError, CanonicalErrorKind, CanonicalValue, CoreDigest, BUNDLE_RELEASE_DIGEST_DOMAIN,
    BUNDLE_SEMANTIC_DIGEST_DOMAIN, CORE_CANONICAL_ENCODING, CORE_DIGEST_FRAME,
    CORE_MODULE_DIGEST_DOMAIN, EXTERNAL_ACTION_RESOURCE_API_VERSION,
    EXTERNAL_ACTION_RESOURCE_DIGEST_DOMAIN, MAX_CANONICAL_NESTING_DEPTH,
    TARGET_IR_ARTIFACT_DIGEST_DOMAIN,
};
pub use compiler::{
    compile_to_core, lower_core, resolve_module, type_check, BoundFact, CompilerContext,
    CompilerError, CompilerErrorKind, CompilerStage, EffectSignatureMismatch, PureFunctionFact,
    PureHelperCostFact, ResolvedIntent, ResolvedModule, ResolvedTypeDecl,
    SignatureMismatchPosition, SignaturePathSegment, TypeShapeFact, TypedIntent, TypedModule,
};
pub use contract_bundle::{
    assemble_contract_bundle, assemble_contract_bundle_from_target_ir,
    validate_contract_bundle_manifest, AssuranceEvidenceRef, AssuranceRole, BundleSubject,
    BundleSubjectKind, ContractBundleAssemblyError, ContractBundleAssemblyErrorKind,
    ContractBundleAssemblyFromTargetIrInput, ContractBundleAssemblyInput,
    ContractBundleAssuranceEvidenceInput, ContractBundleManifest, ContractBundleSourceArtifact,
    ContractBundleValidationFailure, ContractBundleValidationFailureKind,
    ContractBundleValidationReport, ContractBundleValidationStatus, DigestLockedResource,
    SourceArtifactRef, SuppliedDigest, SuppliedTargetIrResource, CONTRACT_BUNDLE_API_VERSION,
};
pub use core_ir::{
    validate_core_module_type_integrity, CompareOp, CoreBlock, CoreBound, CoreBudget, CoreExpr,
    CoreExternalActionBudget, CoreFunction, CoreImport, CoreImportKind, CoreIntent, CoreModule,
    CoreNode, CoreObstructionArm, CoreObstructionReason, CorePredicate, CorePureBinding,
    CorePureBlock, CoreRequireFailureArm, CoreType, CoreTypeIntegrityFailure,
    CoreTypeIntegrityFailureKind, CoreValue, InputConstraint, InputConstraintSource, LocalRef,
    ResourceRef, ValidatedCoreModule, CORE_API_VERSION,
};
pub use highlight::{highlight_source, HighlightRole, HighlightToken};
pub use lawpack::{
    decode_lawpack_bundle, validate_lawpack_dependency_graph, LawpackApertureRequirement,
    LawpackAuthorityClass, LawpackDependency, LawpackDeterminismClass, LawpackEffectFailure,
    LawpackEffectKind, LawpackExecutableComponent, LawpackExecutionClass, LawpackExportedConstant,
    LawpackExportedType, LawpackExports, LawpackManifest, LawpackObstruction,
    LawpackOperationProfile, LawpackOpticTemplate, LawpackPureFunction,
    LawpackPureFunctionImplementation, LawpackResourceRef, LawpackSemanticEffect,
    LawpackTargetAdapter, LawpackValidationFailure, LawpackValidationFailureKind, LawpackVerifier,
    LawpackVerifierClass, ValidatedLawpackBundle, LAWPACK_API_VERSION,
};
pub use lawpack_adapter::{
    decode_lawpack_adapter, prepare_lawpack_compilation, LawpackAdapterEffect,
    LawpackAdapterFailure, LawpackAdapterFailureKind, LawpackAdapterOperationProfile,
    PreparedLawpackCompilation, ValidatedLawpackAdapter, LAWPACK_ADAPTER_API_VERSION,
};
pub use lawpack_authoring::{
    author_lawpack, preflight_lawpack_authoring_paths, LawpackArtifactKind,
    LawpackAuthoredArtifact, LawpackAuthoredArtifactSet, LawpackAuthoringAdapter,
    LawpackAuthoringAdapterBudget, LawpackAuthoringAdapterEffect,
    LawpackAuthoringAdapterOperationProfile, LawpackAuthoringApertureRequirement,
    LawpackAuthoringAuthorityClass, LawpackAuthoringConstant, LawpackAuthoringDefinition,
    LawpackAuthoringDependency, LawpackAuthoringDeterminismClass, LawpackAuthoringEffect,
    LawpackAuthoringEffectFailure, LawpackAuthoringEffectKind, LawpackAuthoringExecutableComponent,
    LawpackAuthoringExecutionClass, LawpackAuthoringExports, LawpackAuthoringFailure,
    LawpackAuthoringFailureCause, LawpackAuthoringFailureKind, LawpackAuthoringLocalReference,
    LawpackAuthoringLocalResource, LawpackAuthoringObstruction, LawpackAuthoringOperationProfile,
    LawpackAuthoringOpticTemplate, LawpackAuthoringPinnedResource, LawpackAuthoringPureFunction,
    LawpackAuthoringResourceRef, LawpackAuthoringType, LawpackAuthoringVerifier,
    LAWPACK_AUTHORING_API_VERSION, MAX_LAWPACK_AUTHORING_VALUE_NESTING_DEPTH,
};
pub use lowerability::{
    check_lowerability, AtomicityRequirement, DirectAdapterSupport, GuardKind,
    LowerabilityEffectResult, LowerabilityEffectStatus, LowerabilityFailure,
    LowerabilityFailureKind, LowerabilityReport, LowerabilityStatus, LoweringRequirements,
    NativeEffectSupport, SemanticEffectRequirement, TargetProfileFacts, WriteClass,
};
pub use parser::{parse_module, ParseError, ParseErrorKind, MAX_SOURCE_NESTING_DEPTH};
pub use provider::{
    bind_target_provider_manifest, select_provider_component, validate_target_provider_manifest,
    ProviderArtifactKind, ProviderArtifactRef, ProviderArtifactSource,
    ProviderComponentSelectionFailure, ProviderComponentSelectionFailureKind,
    ProviderManifestValidationFailure, ProviderManifestValidationFailureKind,
    ProviderManifestValidationReport, ProviderManifestValidationStatus, ProviderSchemaBinding,
    ProviderSchemaFormat, SelectedProviderComponent, TargetProviderManifest,
    ValidatedTargetProviderManifest, TARGET_PROVIDER_ABI, TARGET_PROVIDER_LOWERER_CONTRACT,
    TARGET_PROVIDER_MANIFEST_API_VERSION, TARGET_PROVIDER_VERIFIER_CONTRACT,
};
pub use provider_invocation::{
    validate_provider_lowering_limit_independence, validate_provider_lowering_request,
    validate_provider_lowering_result, validate_provider_verification_limit_independence,
    validate_provider_verification_request, validate_provider_verification_result,
    ProviderArtifact, ProviderArtifactBinding, ProviderArtifactSchemaValidationErrorKind,
    ProviderArtifactSchemaValidator, ProviderBoundArtifact, ProviderDiagnostic,
    ProviderDiagnosticSeverity, ProviderDigest, ProviderDigestAlgorithm,
    ProviderInvocationInputManifest, ProviderInvocationKind, ProviderInvocationValidationFailure,
    ProviderInvocationValidationFailureKind, ProviderInvocationValidationReport,
    ProviderInvocationValidationStatus, ProviderLoweringInvocationContract,
    ProviderLoweringOutputArtifact, ProviderLoweringOutputKind, ProviderLoweringOutputRequest,
    ProviderLoweringRequest, ProviderLoweringResult, ProviderLoweringSuccess,
    ProviderOutputManifest, ProviderOutputManifestEntry, ProviderOutputRequestBinding,
    ProviderProtocolVersion, ProviderRefusal, ProviderRefusalKind, ProviderResourceRef,
    ProviderResponseLimits, ProviderSemanticInput, ProviderSemanticInputBinding,
    ProviderSemanticInputKind, ProviderVerificationInvocationContract,
    ProviderVerificationOutputArtifact, ProviderVerificationOutputKind,
    ProviderVerificationOutputRequest, ProviderVerificationRequest, ProviderVerificationResult,
    ProviderVerificationSuccess, ValidatedProviderLoweringRequest, ValidatedProviderOutcome,
    ValidatedProviderVerificationRequest, PROVIDER_LAWPACK_ARTIFACT_DOMAIN,
    TARGET_PROVIDER_PROTOCOL_VERSION,
};
pub use provider_lowering::{
    lower_with_builtin_lowerer, BuiltinLowererCompatibilityFailure,
    BuiltinLowererCompatibilityFailureKind, BuiltinLowererRequest, BuiltinLoweringResult,
    BuiltinTargetLowerer,
};
pub use result_projection::{
    decode_result_projection, digest_result_projection, emit_result_projection,
    encode_result_projection, verify_result_projection, ResultProjection, ResultProjectionArtifact,
    ResultProjectionExpr, ResultProjectionFailure, ResultProjectionFailureKind,
    ResultProjectionSource, VerifiedResultProjection, MAX_RESULT_PROJECTION_ARTIFACT_BYTES,
    MAX_RESULT_PROJECTION_NODES, MAX_RESULT_PROJECTION_PATH_SEGMENTS,
    MAX_RESULT_PROJECTION_TEXT_BYTES, RESULT_PROJECTION_API_VERSION, RESULT_PROJECTION_CDDL_ROOT,
    RESULT_PROJECTION_DIGEST_DOMAIN,
};
pub use semantic::{validate_module, validate_surface, SemanticError, SemanticErrorKind};
pub use target_intrinsics::{
    admit_target_pure_intrinsics, digest_target_intrinsics_catalog, RequiredPureIntrinsic,
    TargetPureIntrinsicFact, TargetPureIntrinsicFailure, TargetPureIntrinsicFailureKind,
    TARGET_INTRINSICS_API_VERSION,
};
pub use target_ir::{
    lower_to_target_ir, TargetEffectLowering, TargetEffectSignatureFact, TargetIrArtifact,
    TargetIrBranchEntry, TargetIrExternalActionRequest, TargetIrForEntry, TargetIrIntent,
    TargetIrLoweringFacts, TargetIrOrderEntry, TargetIrPureBinding, TargetIrRequireFailure,
    TargetIrRequirement, TargetIrSemanticClosure, TargetIrStep, TargetLoweringFailure,
    TargetLoweringFailureKind, TargetLoweringReport, TargetLoweringStatus, TargetPureFunctionFact,
    ECHO_DPO_TARGET_PROFILE, ECHO_ORDERED_SPAN_IR_DOMAIN, ECHO_SPAN_IR_DOMAIN,
    ECHO_STRUCTURED_SPAN_IR_DOMAIN, GITWARP_COMMIT_REDUCER_IR_DOMAIN,
    GITWARP_REF_CRDT_TARGET_PROFILE,
};
pub use target_profile::{
    validate_target_profile_manifest, TargetProfileConformanceFailure,
    TargetProfileConformanceFailureKind, TargetProfileConformanceReport,
    TargetProfileConformanceStatus, TargetProfileManifest, CANONICAL_CBOR_ABI,
    TARGET_PROFILE_API_VERSION,
};
pub use target_profile_contract_resources::{
    canonical_target_profile_contract_resources, digest_target_profile_contract_resource,
    validate_target_profile_contract_resources, TargetProfileContractResource,
    TargetProfileContractResourceFailure, TargetProfileContractResourceFailureKind,
    TargetProfileContractResourceProvenance, ValidatedTargetProfileContractResources,
    CANONICAL_CBOR_CONTRACT_COORDINATE, DETERMINISM_CONTRACT_COORDINATE,
    DIAGNOSTICS_CONTRACT_COORDINATE, FUEL_CONTRACT_COORDINATE,
    TARGET_PROFILE_CONTRACT_RESOURCE_API_VERSION, TARGET_PROFILE_CONTRACT_RESOURCE_REPOSITORY,
    WASM_COMPONENT_CONTRACT_COORDINATE,
};
pub use token::{lex, IntSuffix, LexError, Span, Token, TokenKind};

/// Outcome of [`check`]: the result of parsing and surface-validating a source
/// string in one call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CheckOutcome {
    /// The source parsed and passed source/surface semantic validation.
    Valid,
    /// The source failed to parse; lexing or grammar error.
    ParseFailed(ParseError),
    /// The source parsed but failed source/surface semantic validation.
    SemanticFailed(Vec<SemanticError>),
}

/// Parse and surface-validate `source` in one call.
///
/// This is the front-end entry point: it runs [`parse_module`] and, on success,
/// [`validate_surface`], returning a [`CheckOutcome`]. It does not resolve
/// imports, infer contextual types, lower to Core IR, or canonicalize.
///
/// # Examples
///
/// ```
/// use amblicit_syntax::{check, CheckOutcome};
///
/// assert_eq!(check("package examples.hello@1;\n"), CheckOutcome::Valid);
/// assert!(matches!(check("not edict"), CheckOutcome::ParseFailed(_)));
/// ```
#[must_use]
pub fn check(source: &str) -> CheckOutcome {
    match parse_module(source) {
        Err(error) => CheckOutcome::ParseFailed(error),
        Ok(module) => match validate_surface(&module) {
            Ok(()) => CheckOutcome::Valid,
            Err(errors) => CheckOutcome::SemanticFailed(errors),
        },
    }
}

#[cfg(test)]
mod check_facade_tests {
    use super::{check, CheckOutcome, ParseErrorKind, SemanticErrorKind};

    #[test]
    fn valid_source_is_valid() {
        assert_eq!(check("package examples.hello@1;\n"), CheckOutcome::Valid);
    }

    #[test]
    fn unparsable_source_is_parse_failed_with_stable_kind() {
        let outcome = check("@@@ not edict");
        let CheckOutcome::ParseFailed(error) = outcome else {
            panic!("expected parse failure, got {outcome:?}");
        };
        assert_eq!(error.kind, ParseErrorKind::ExpectedKeyword);
    }

    #[test]
    fn unbounded_scalar_is_semantic_failed_with_stable_kind() {
        let outcome = check("package examples.unbounded@1;\ntype Bad = { name: String };\n");
        let CheckOutcome::SemanticFailed(errors) = outcome else {
            panic!("expected semantic failure, got {outcome:?}");
        };
        assert!(
            errors
                .iter()
                .any(|error| error.kind == SemanticErrorKind::UnboundedScalar),
            "expected an UnboundedScalar semantic error, got {errors:?}"
        );
    }
}

#[cfg(doctest)]
mod topic_shelf_doctests {
    #[doc = include_str!("../../../docs/topics/authority-facts/README.md")]
    pub struct AuthorityFactsTopicDocs;

    #[doc = include_str!("../../../docs/topics/core-ir/README.md")]
    pub struct CoreIrTopicDocs;

    #[doc = include_str!("../../../docs/topics/compiler-spine/README.md")]
    pub struct CompilerSpineTopicDocs;

    #[doc = include_str!("../../../docs/topics/contract-bundles/README.md")]
    pub struct ContractBundlesTopicDocs;

    #[doc = include_str!("../../../docs/topics/admission/README.md")]
    pub struct AdmissionTopicDocs;

    #[doc = include_str!("../../../docs/topics/lowerability/README.md")]
    pub struct LowerabilityTopicDocs;

    #[doc = include_str!("../../../docs/topics/target-profiles/README.md")]
    pub struct TargetProfilesTopicDocs;

    #[doc = include_str!("../../../docs/topics/semantic-validation/README.md")]
    pub struct SemanticValidationTopicDocs;

    #[doc = include_str!("../../../docs/topics/syntax/README.md")]
    pub struct SyntaxTopicDocs;

    #[doc = include_str!("../../../docs/topics/developer-tooling/README.md")]
    pub struct DeveloperToolingTopicDocs;

    #[doc = include_str!("../../../docs/topics/external-action-requests/README.md")]
    pub struct ExternalActionRequestsTopicDocs;
}

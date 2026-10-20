//! Curated public Rust facade for Edict.
//!
//! The [`check`] entry point is the supported one-call source check. Stable
//! failure classifications are grouped under [`diagnostic`], while canonical
//! semantic-artifact identity operations are grouped under [`artifact`]. The
//! implementation crate's broad module tree is intentionally not re-exported.
//!
//! ```
//! use edict::{check, CheckOutcome};
//!
//! assert_eq!(
//!     check("package examples.public_facade@1;\n"),
//!     CheckOutcome::Valid
//! );
//! ```
//!
//! Implementation modules are not part of this facade:
//!
//! ```compile_fail
//! use edict::parser::parse_module;
//! ```

pub use edict_syntax::{check, CheckOutcome};

/// Stable machine-usable failure classifications exposed by the public
/// facade.
pub mod diagnostic {
    pub use edict_syntax::{
        CanonicalError, CanonicalErrorKind, CompilerError, CompilerErrorKind, CompilerStage,
        ParseError, ParseErrorKind, ResultProjectionFailure, ResultProjectionFailureKind,
        SemanticError, SemanticErrorKind, Span, TargetLoweringFailure, TargetLoweringFailureKind,
    };
}

/// Canonical semantic-artifact values, encoders, and domain-framed identity
/// operations. Nested value models are exported so callers can construct
/// candidates and inspect results without importing implementation modules.
pub mod artifact {
    pub use edict_syntax::{
        decode_canonical_cbor, decode_result_projection, digest_core_module,
        digest_result_projection, digest_target_ir_artifact, encode_core_module,
        encode_result_projection, encode_target_ir_artifact, verify_result_projection,
        CanonicalError, CanonicalErrorKind, CanonicalValue, CompareOp, CoreBlock, CoreBound,
        CoreBudget, CoreDigest, CoreExpr, CoreExternalActionBudget, CoreFunction, CoreImport,
        CoreImportKind, CoreIntent, CoreModule, CoreNode, CoreObstructionArm,
        CoreObstructionReason, CorePredicate, CorePureBinding, CorePureBlock,
        CoreRequireFailureArm, CoreType, CoreValue, InputConstraint, InputConstraintSource,
        LocalRef, ResourceRef, ResultProjection, ResultProjectionArtifact, ResultProjectionExpr,
        ResultProjectionFailure, ResultProjectionFailureKind, ResultProjectionSource,
        TargetIrArtifact, TargetIrExternalActionRequest, TargetIrIntent, TargetIrPureBinding,
        TargetIrRequireFailure, TargetIrRequirement, TargetIrSemanticClosure, TargetIrStep,
        VerifiedResultProjection,
    };
}

/// Checked Bunny Q32.32 arithmetic for compiler consumers.
///
/// This API evaluates raw fixed-point values. It does not imply source-language
/// fixed-point syntax or a corresponding Core/Target artifact representation.
///
/// ```
/// use edict::numeric::{NumericError, Q32_32};
///
/// let one = Q32_32::from_raw(4_294_967_296);
/// let half = Q32_32::from_raw(2_147_483_648);
/// assert_eq!(one.checked_mul(half).map(Q32_32::raw), Ok(2_147_483_648));
/// assert_eq!(one.checked_div(Q32_32::from_raw(0)), Err(NumericError::DivisionByZero));
/// ```
pub mod numeric {
    pub use edict_syntax::numeric::{NumericError, Q32_32, Q32_32_PROFILE};
}

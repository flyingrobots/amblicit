//! Structured context for the first incompatible effect signature boundary.
use serde::Serialize;

/// The expected/actual boundary involved in one effect mismatch.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum SignatureMismatchPosition {
    /// Exported input versus the authored argument.
    Input,
    /// Authored receipt annotation versus the exported output.
    Receipt,
}

/// An unambiguous segment in a bounded type path.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum SignaturePathSegment {
    /// A record field, whose name remains separate from path syntax.
    Field { name: String },
    /// The element type of a bounded list.
    ListItem,
    /// The settlement type of an external-action request.
    Settlement,
}

/// Typed first-difference detail without changing `TypeMismatch` identity.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EffectSignatureMismatch {
    /// Source coordinate of the called effect.
    pub effect: String,
    /// Direction of the signature comparison.
    pub position: SignatureMismatchPosition,
    /// Deterministically selected structural path.
    pub path: Vec<SignaturePathSegment>,
    /// Self-describing expected bounded type; absent for an extra field.
    pub expected_type: Option<String>,
    /// Self-describing actual bounded type; absent for a missing field.
    pub actual_type: Option<String>,
}

use super::{
    compatible, error, render_self_describing_core_type, CompilerError, CompilerErrorKind,
    CompilerStage, Span, TypeKind, TypeShape,
};
use std::collections::BTreeSet;
use std::fmt::Write;

struct Difference {
    path: Vec<SignaturePathSegment>,
    expected: Option<String>,
    actual: Option<String>,
}

fn describe(shape: &TypeShape) -> String {
    render_self_describing_core_type(&shape.core_type()).unwrap_or_else(|| shape.coord.clone())
}

fn at_path(
    path: &[SignaturePathSegment],
    expected: Option<&TypeShape>,
    actual: Option<&TypeShape>,
) -> Difference {
    Difference {
        path: path.to_vec(),
        expected: expected.map(describe),
        actual: actual.map(describe),
    }
}

fn first_difference(
    expected: &TypeShape,
    actual: &TypeShape,
    path: &mut Vec<SignaturePathSegment>,
) -> Option<Difference> {
    if compatible(expected, actual) {
        return None;
    }
    match (&expected.kind, &actual.kind) {
        (TypeKind::Record(left), TypeKind::Record(right)) => {
            let names: BTreeSet<_> = left.keys().chain(right.keys()).collect();
            for name in names {
                path.push(SignaturePathSegment::Field { name: name.clone() });
                let difference = match (left.get(name), right.get(name)) {
                    (Some(left), Some(right)) => first_difference(left, right, path),
                    (left, right) => Some(at_path(path, left, right)),
                };
                path.pop();
                if difference.is_some() {
                    return difference;
                }
            }
        }
        (
            TypeKind::List {
                item: left,
                max: left_max,
            },
            TypeKind::List {
                item: right,
                max: right_max,
            },
        ) if right_max <= left_max => {
            path.push(SignaturePathSegment::ListItem);
            let difference = first_difference(left, right, path);
            path.pop();
            if difference.is_some() {
                return difference;
            }
        }
        (
            TypeKind::ExternalActionRequest { settlement: left },
            TypeKind::ExternalActionRequest { settlement: right },
        ) => {
            path.push(SignaturePathSegment::Settlement);
            let difference = first_difference(left, right, path);
            path.pop();
            if difference.is_some() {
                return difference;
            }
        }
        _ => {}
    }
    Some(at_path(path, Some(expected), Some(actual)))
}

pub(super) fn mismatch(
    effect: &str,
    position: SignatureMismatchPosition,
    expected: &TypeShape,
    actual: &TypeShape,
    span: Span,
) -> Option<CompilerError> {
    let difference = first_difference(expected, actual, &mut Vec::new())?;
    let mut path = match position {
        SignatureMismatchPosition::Input => "input".to_owned(),
        SignatureMismatchPosition::Receipt => "receipt".to_owned(),
    };
    for part in &difference.path {
        match part {
            SignaturePathSegment::Field { name }
                if name.chars().all(|c| c.is_alphanumeric() || c == '_') =>
            {
                path.push('.');
                path.push_str(name);
            }
            SignaturePathSegment::Field { name } => {
                let _ = write!(path, "[{name:?}]");
            }
            SignaturePathSegment::ListItem => path.push_str("[]"),
            SignaturePathSegment::Settlement => path.push_str(".settlement"),
        }
    }
    let mut failure = error(
        CompilerStage::TypeCheck,
        CompilerErrorKind::TypeMismatch,
        format!(
            "effect `{effect}` {path} does not match its exported signature: expected {}, got {}",
            difference.expected.as_deref().unwrap_or("<absent>"),
            difference.actual.as_deref().unwrap_or("<absent>")
        ),
        span,
    );
    failure.signature_mismatch = Some(EffectSignatureMismatch {
        effect: effect.to_owned(),
        position,
        path: difference.path,
        expected_type: difference.expected,
        actual_type: difference.actual,
    });
    Some(failure)
}

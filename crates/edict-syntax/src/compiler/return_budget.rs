//! Canonical result-size bounds apply to ordinary intent results, independently of helpers.
use super::{
    error, CompilerErrorKind, CompilerStage, ResolvedIntent, TypeChecker, TypeKind, TypeShape,
};

#[derive(Debug, Clone, Copy)]
enum BoundFailure {
    Overflow,
    Unsupported,
}

impl TypeChecker<'_> {
    pub(super) fn check_return_budget(
        &mut self,
        intent: &ResolvedIntent,
        output: &TypeShape,
    ) -> Option<()> {
        // Request-only intents produce protocol data under the separate
        // external-action contract, not ordinary canonical result projection.
        if matches!(output.kind, TypeKind::ExternalActionRequest { .. }) {
            return Some(());
        }
        match encoded_maximum(output) {
            Ok(maximum) if maximum <= intent.budget.max_output_bytes => Some(()),
            bound => {
                let (kind, message) = match bound {
                    Ok(maximum) => (CompilerErrorKind::InvalidBound, format!(
                        "return type `{}` can encode to {maximum} bytes; operation budget allows {} (maxOutputBytes)",
                        output.coord, intent.budget.max_output_bytes)),
                    Err(BoundFailure::Overflow) => (CompilerErrorKind::InvalidBound,
                        format!("canonical return-size bound for `{}` exceeds U64", output.coord)),
                    Err(BoundFailure::Unsupported) => (CompilerErrorKind::UnsupportedSourceShape,
                        format!("return type `{}` has no supported canonical encoding bound", output.coord)),
                };
                self.errors.push(error(
                    CompilerStage::TypeCheck,
                    kind,
                    message,
                    intent.source.span,
                ));
                None
            }
        }
    }
}

fn encoded_maximum(shape: &TypeShape) -> Result<u64, BoundFailure> {
    match &shape.kind {
        TypeKind::Bool => Ok(1),
        TypeKind::Int { width } => match width.as_str() {
            "I8" | "U8" => Ok(2),
            "I16" | "U16" => Ok(3),
            "I32" | "U32" => Ok(5),
            "I64" | "U64" => Ok(9),
            _ => Err(BoundFailure::Unsupported),
        },
        TypeKind::Bytes { max, .. } => length_prefixed(*max),
        TypeKind::String { max, .. } => {
            length_prefixed(max.checked_mul(4).ok_or(BoundFailure::Overflow)?)
        }
        TypeKind::Nominal { representation, .. } => encoded_maximum(representation),
        TypeKind::List { item, max } => {
            let payload = encoded_maximum(item)?
                .checked_mul(*max)
                .ok_or(BoundFailure::Overflow)?;
            header_size(*max)
                .checked_add(payload)
                .ok_or(BoundFailure::Overflow)
        }
        TypeKind::Record(fields) => {
            let count = u64::try_from(fields.len()).map_err(|_| BoundFailure::Overflow)?;
            fields
                .iter()
                .try_fold(header_size(count), |total, (name, field)| {
                    let name_bytes =
                        u64::try_from(name.len()).map_err(|_| BoundFailure::Overflow)?;
                    let key = length_prefixed(name_bytes)?;
                    let value = encoded_maximum(field)?;
                    total
                        .checked_add(key)
                        .and_then(|sum| sum.checked_add(value))
                        .ok_or(BoundFailure::Overflow)
                })
        }
        TypeKind::ExternalActionRequest { .. } => Err(BoundFailure::Unsupported),
    }
}

fn length_prefixed(payload: u64) -> Result<u64, BoundFailure> {
    header_size(payload)
        .checked_add(payload)
        .ok_or(BoundFailure::Overflow)
}

const fn header_size(argument: u64) -> u64 {
    match argument {
        0..=23 => 1,
        24..=255 => 2,
        256..=65_535 => 3,
        65_536..=4_294_967_295 => 5,
        _ => 9,
    }
}

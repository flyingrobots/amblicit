//! Source alternatives retain named Core identities without inventing a value ABI.
use super::{error, CompilerErrorKind, CompilerStage, Span, TypeChecker, TypeKind, TypeShape};
use crate::ast::VariantCase;
use std::collections::BTreeMap;

impl TypeChecker<'_> {
    pub(super) fn variant_shape(
        &mut self,
        coordinate: &str,
        cases: &[VariantCase],
        span: Span,
    ) -> Option<TypeShape> {
        if cases.is_empty() {
            self.errors.push(error(
                CompilerStage::TypeCheck,
                CompilerErrorKind::UnsupportedSourceShape,
                "source variant must contain at least one case",
                span,
            ));
            return None;
        }
        let mut alternatives = BTreeMap::new();
        for case in cases {
            if alternatives.contains_key(&case.name) {
                self.errors.push(error(
                    CompilerStage::TypeCheck,
                    CompilerErrorKind::SurfaceValidation,
                    format!("duplicate variant case `{}`", case.name),
                    case.span,
                ));
                return None;
            }
            let payload = match &case.payload {
                Some(reference) => Some(self.type_ref_shape(reference, case.span)?),
                None => None,
            };
            alternatives.insert(case.name.clone(), payload);
        }
        Some(TypeShape {
            coord: coordinate.to_owned(),
            kind: TypeKind::Variant(alternatives),
            preserve_named_identity: true,
        })
    }
}

//! Source alternatives retain named Core identities without inventing a value ABI.
use super::{error, CompilerErrorKind, CompilerStage, Span, TypeChecker, TypeKind, TypeShape};
use crate::ast::VariantCase;
use std::collections::{BTreeMap, BTreeSet};

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
        let mut names = BTreeSet::new();
        let mut complete = true;
        for case in cases {
            if !names.insert(&case.name) {
                self.errors.push(error(
                    CompilerStage::TypeCheck,
                    CompilerErrorKind::SurfaceValidation,
                    format!("duplicate variant case `{}`", case.name),
                    case.span,
                ));
                complete = false;
                continue;
            }
            let payload = match &case.payload {
                Some(reference) => {
                    let Some(payload) = self.type_ref_shape(reference, case.span) else {
                        complete = false;
                        continue;
                    };
                    Some(payload)
                }
                None => None,
            };
            alternatives.insert(case.name.clone(), payload);
        }
        complete.then(|| TypeShape {
            coord: coordinate.to_owned(),
            kind: TypeKind::Variant(alternatives),
            preserve_named_identity: true,
        })
    }
}

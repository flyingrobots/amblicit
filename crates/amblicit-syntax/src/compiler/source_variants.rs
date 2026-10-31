//! Source alternatives retain named Core identities without inventing a value ABI.
use super::{
    compatible, error, CompilerErrorKind, CompilerStage, CoreExpr, Expr, LocalEnvironment, Span,
    TypeChecker, TypeKind, TypeRef, TypeShape, TypedValue,
};
use crate::ast::VariantCase;
use std::collections::{BTreeMap, BTreeSet};

impl TypeChecker<'_> {
    pub(super) fn check_variant_constructor(
        &mut self,
        path: &[String],
        case: &str,
        payload: Option<&Expr>,
        env: &LocalEnvironment,
        expected: Option<&TypeShape>,
        span: Span,
    ) -> Option<TypedValue> {
        let ty = self.type_ref_shape(
            &TypeRef::Named {
                path: path.to_vec(),
                args: Vec::new(),
            },
            span,
        )?;
        let TypeKind::Variant(cases) = &ty.kind else {
            self.errors.push(error(
                CompilerStage::TypeCheck,
                CompilerErrorKind::TypeMismatch,
                "constructor requires a variant type",
                span,
            ));
            return None;
        };
        let Some(declared) = cases.get(case) else {
            self.errors.push(error(
                CompilerStage::TypeCheck,
                CompilerErrorKind::UnknownField,
                format!("unknown variant case `{case}`"),
                span,
            ));
            return None;
        };
        if expected.is_some_and(|expected| !compatible(expected, &ty)) {
            self.errors.push(error(
                CompilerStage::TypeCheck,
                CompilerErrorKind::TypeMismatch,
                "constructor type does not match its use",
                span,
            ));
            return None;
        }
        let value = match (declared, payload) {
            (None, None) => None,
            (Some(declared), Some(payload)) => {
                let value = self.check_expr_with_expected(payload, env, Some(declared))?;
                if !compatible(declared, &value.ty) {
                    self.errors.push(error(
                        CompilerStage::TypeCheck,
                        CompilerErrorKind::TypeMismatch,
                        "variant payload does not match its declared type",
                        span,
                    ));
                    return None;
                }
                Some(Box::new(value.expr))
            }
            _ => {
                self.errors.push(error(
                    CompilerStage::TypeCheck,
                    CompilerErrorKind::TypeMismatch,
                    "variant payload presence does not match its case",
                    span,
                ));
                return None;
            }
        };
        Some(TypedValue {
            expr: CoreExpr::Variant {
                ty: ty.value_type_coord(),
                case: case.to_owned(),
                payload: value,
            },
            ty,
        })
    }

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

//! Typed lexical frame and synthesized-read costs for the complete list pass.
use super::{
    expression_identity, CoreExpr, CoreObstructionArm, Expr, HelperCost, LocalEnvironment,
    LocalRef, RecordEntry, Span, TypeChecker, TypeShape,
};
use crate::ast::MatchArm;
use std::collections::BTreeMap;

// The immutable resolved AST outlives these local caches. Keys are never emitted.
fn identity<T>(node: &T) -> usize {
    std::ptr::from_ref(node).addr()
}

impl TypeChecker<'_> {
    fn source_binding_read(&mut self, shape: &TypeShape, span: Span) -> Option<HelperCost> {
        let (cells, bytes) = self.source_value_bound(shape, span)?;
        Some(HelperCost {
            steps: cells
                .checked_add(1)
                .or_else(|| self.function_bound_failure(span))?,
            allocated_bytes: bytes,
            output_bytes: 0,
        })
    }

    pub(super) fn record_source_block_frame<'b, T>(
        &mut self,
        block: &T,
        locals: impl Iterator<Item = &'b LocalRef>,
        env: &LocalEnvironment,
        span: Span,
    ) -> Option<()> {
        if !self.meters_list_work() {
            return Some(());
        }
        let shapes = env
            .values
            .values()
            .map(|(local, shape)| (local.id.as_str(), shape))
            .collect::<BTreeMap<_, _>>();
        let mut cost = HelperCost::default();
        for local in locals {
            let value = if let Some(shape) = shapes.get(local.id.as_str()) {
                self.source_binding_read(shape, span)?
            } else {
                self.source_obstruction_frames
                    .get(&local.ty)
                    .copied()
                    .or_else(|| self.function_bound_failure(span))?
            };
            cost = self.checked_helper_cost_add(cost, value, span)?;
        }
        self.source_block_frames.insert(identity(block), cost);
        Some(())
    }

    pub(super) fn record_source_obstruction_frame(
        &mut self,
        shape: &TypeShape,
        span: Span,
    ) -> Option<()> {
        if self.meters_list_work() {
            let cost = self.source_binding_read(shape, span)?;
            self.source_obstruction_frames
                .insert(shape.value_type_coord(), cost);
        }
        Some(())
    }

    pub(super) fn with_source_block_frame<T>(
        &mut self,
        block: &T,
        span: Span,
        action: impl FnOnce(&mut Self) -> Option<HelperCost>,
    ) -> Option<HelperCost> {
        let frame = if self.meters_list_work() {
            self.source_block_frames
                .get(&identity(block))
                .copied()
                .or_else(|| self.function_bound_failure(span))?
        } else {
            HelperCost::default()
        };
        self.with_source_frame(frame, span, action)
    }

    fn with_source_frame(
        &mut self,
        frame: HelperCost,
        span: Span,
        action: impl FnOnce(&mut Self) -> Option<HelperCost>,
    ) -> Option<HelperCost> {
        let outer = self.source_cost_frame;
        self.source_cost_frame = self.checked_helper_cost_add(outer, frame, span)?;
        let result = action(self);
        self.source_cost_frame = outer;
        result
    }

    pub(super) fn record_source_effect_frame(
        &mut self,
        call: &Expr,
        receipt: &TypeShape,
        obstructions: &BTreeMap<String, CoreObstructionArm>,
        span: Span,
    ) -> Option<()> {
        if !self.meters_list_work() {
            return Some(());
        }
        let receipt = self.source_binding_read(receipt, span)?;
        let mut cost = self.checked_helper_cost_add(
            receipt,
            HelperCost {
                steps: 1,
                ..HelperCost::default()
            },
            span,
        )?;
        for arm in obstructions.values() {
            let binder = self
                .source_obstruction_frames
                .get(&arm.binder.ty)
                .copied()
                .or_else(|| self.function_bound_failure(span))?;
            let CoreExpr::Call {
                callee,
                type_args,
                args,
            } = &arm.value
            else {
                return self.function_bound_failure(span);
            };
            if !type_args.is_empty() || !args.is_empty() {
                return self.function_bound_failure(span);
            }
            let bytes = u64::try_from(callee.len())
                .ok()?
                .checked_add(64)
                .or_else(|| self.function_bound_failure(span))?;
            let annotation = HelperCost {
                steps: 2,
                allocated_bytes: bytes,
                output_bytes: 0,
            };
            let arm_cost = self.checked_helper_cost_add(binder, annotation, span)?;
            cost = self.checked_helper_cost_add(cost, arm_cost, span)?;
        }
        self.source_expression_costs
            .insert(expression_identity(call), cost);
        Some(())
    }

    pub(super) fn record_source_reason_shorthand(
        &mut self,
        entry: &RecordEntry,
        shape: &TypeShape,
        span: Span,
    ) -> Option<()> {
        if self.meters_list_work() {
            let cost = self.source_binding_read(shape, span)?;
            self.source_reason_shorthand_costs
                .insert(identity(entry), cost);
        }
        Some(())
    }

    pub(super) fn source_reason_shorthand_cost(&self, entry: &RecordEntry) -> HelperCost {
        self.source_reason_shorthand_costs
            .get(&identity(entry))
            .copied()
            .unwrap_or_default()
    }

    pub(super) fn record_source_match_binder(
        &mut self,
        arm: &MatchArm,
        shape: &TypeShape,
        span: Span,
    ) -> Option<()> {
        if self.meters_list_work() {
            let frame = self.source_binding_read(shape, span)?;
            self.source_match_binder_frames.insert(identity(arm), frame);
        }
        Some(())
    }

    pub(super) fn helper_cost_for_match(
        &mut self,
        scrutinee: &Expr,
        arms: &[MatchArm],
        span: Span,
    ) -> Option<HelperCost> {
        let before = self.helper_cost_for_expr(scrutinee)?;
        let mut selected = HelperCost::default();
        for arm in arms {
            let cost = if self.meters_list_work() && arm.binder.is_some() {
                let frame = self.source_cost_frame;
                let binder = self
                    .source_match_binder_frames
                    .get(&identity(arm))
                    .copied()
                    .or_else(|| self.function_bound_failure(span))?;
                let body = self
                    .with_source_frame(binder, span, |this| this.helper_cost_for_expr(&arm.body))?;
                // Existing match storage already reserves scrutinee bytes. Native
                // additionally charges its cells and copies the surrounding frame.
                let cells = self
                    .source_value_bounds
                    .get(&expression_identity(scrutinee))?
                    .0;
                let storage = self.checked_helper_cost_add(
                    frame,
                    HelperCost {
                        steps: cells,
                        ..HelperCost::default()
                    },
                    span,
                )?;
                self.checked_helper_cost_add(body, storage, span)?
            } else {
                self.helper_cost_for_expr(&arm.body)?
            };
            selected = selected.component_max(cost);
        }
        self.checked_helper_cost_add(before, selected, span)
    }
}

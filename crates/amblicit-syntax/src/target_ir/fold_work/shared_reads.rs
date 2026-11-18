//! This module owns the shared-read Core cost schedule, `edict.core-cost/v2`.
//!
//! Values are immutable and shared by reference. Reading a local, field, list
//! element or fold state, passing an argument and returning a result cost one
//! step and allocate nothing. Building a value charges its shell: one cell for
//! the value plus one per field, item or case header. Shared children are
//! references, already charged where they were built. Comparisons, input and
//! output keep their full-size charges, and every operation without a rule
//! here keeps its copying (`v1`) charge, which is never lower.
use super::reachable::Length;
use super::{CoreExpr, CoreValue, Cost, Meter, Scope, Value};

/// Core capability that selects the shared-read schedule. A consumer that does
/// not meter this schedule must refuse the module.
pub(crate) const SHARED_READS_CAPABILITY: &str = "core.cost.shared-reads/v1";

impl<'a> Meter<'a> {
    /// The work of evaluating `expr` itself, excluding its children.
    pub(super) fn shared_own(
        &self,
        expr: &CoreExpr,
        available: &Scope<'_>,
        depth: usize,
    ) -> Option<Cost> {
        match expr {
            CoreExpr::Const(CoreValue::Null | CoreValue::Bool(_) | CoreValue::Int { .. }) => {
                Value::CELL.evaluation()
            }
            // Text and byte constants are copied in full, as in v1.
            CoreExpr::Const(_) => self.value(expr, available, depth)?.evaluation(),
            CoreExpr::Record { fields } => fields
                .keys()
                .try_fold(Value::CELL, |total, key| {
                    total.add(Self::field(key, Value::CELL)?)
                })?
                .evaluation(),
            CoreExpr::OrderedRecord { fields } => fields
                .iter()
                .try_fold(Value::CELL, |total, (key, _)| {
                    total.add(Self::field(key, Value::CELL)?)
                })?
                .evaluation(),
            CoreExpr::List { values } => shell(u64::try_from(values.len()).ok()?)?.evaluation(),
            CoreExpr::Variant { ty, case, .. } => Value::CELL
                .add(Value {
                    cells: 6,
                    bytes: 399_u64
                        .checked_add(u64::try_from(ty.len()).ok()?)?
                        .checked_add(u64::try_from(case.len()).ok()?)?,
                })?
                .evaluation(),
            CoreExpr::Local { .. }
            | CoreExpr::Field { .. }
            | CoreExpr::If { .. }
            | CoreExpr::Match { .. }
            | CoreExpr::Fold { .. }
            | CoreExpr::Call { .. }
            | CoreExpr::Fail { .. } => Some(Cost::steps(1)),
        }
    }

    /// Whether `callee` has a shared-read call rule; others keep v1 charges.
    pub(super) fn has_shared_rule(callee: &str) -> bool {
        matches!(
            callee,
            "core.list.at" | "core.list.length" | "core.list.append" | "core.list.concat"
        )
    }

    /// The call work of a list operation with a shared-read rule.
    pub(super) fn shared_prelude(
        &mut self,
        callee: &str,
        expr: &'a CoreExpr,
        available: &Scope<'a>,
        depth: usize,
    ) -> Option<Cost> {
        match callee {
            // An index read returns a shared element.
            "core.list.at" => Some(Cost::default()),
            // The length is a fresh integer.
            "core.list.length" => Value::CELL.evaluation(),
            // A new list shell holds one reference per item.
            _ => self.list_shell(expr, available, depth),
        }
    }

    /// A foreign call (lawpack helper or target intrinsic) runs outside the
    /// shared heap: every argument is copied in and the result copied out in
    /// full, as under the copying schedule, on top of its declared template.
    pub(super) fn shared_foreign_copies(
        &self,
        args: &[CoreExpr],
        expr: &CoreExpr,
        available: &Scope<'_>,
        depth: usize,
    ) -> Option<Cost> {
        args.iter().try_fold(
            self.value(expr, available, depth + 1)?.evaluation()?,
            |total, arg| total.add(self.value(arg, available, depth + 1)?.evaluation()?),
        )
    }

    /// A new list shell, sized by the proven length of the result when one
    /// exists and by the declared maximum otherwise.
    fn list_shell(
        &mut self,
        expr: &'a CoreExpr,
        available: &Scope<'a>,
        depth: usize,
    ) -> Option<Cost> {
        if depth > super::MAX_CORE_GRAPH_DEPTH {
            return None;
        }
        let max = self.declared_list_max(expr, available)?;
        let items = match self.reach_length(expr, available, depth)? {
            Length::Proven(bound) => bound.min(max),
            Length::Declared => max,
        };
        shell(items)?.evaluation()
    }
}

/// A list value with `items` shared item references.
fn shell(items: u64) -> Option<Value> {
    Value::CELL.add(Value {
        cells: items,
        bytes: items.checked_mul(64)?,
    })
}

//! This module owns the shared-read Core cost schedule, `edict.core-cost/v2`.
//!
//! Values are immutable and shared by reference. Reading a local, field, list
//! element or fold state, passing an argument and returning a result cost one
//! step and allocate nothing. Building a value charges its shell: one cell for
//! the value plus one per field, item or case header. Shared children are
//! references, already charged where they were built. Comparisons, input and
//! output keep their full-size charges, and every operation without a rule
//! here keeps its copying (`v1`) charge, which is never lower.
use super::{
    expression_type_coordinate, resolved_core_type, CoreExpr, CoreType, CoreValue, Cost, Meter,
    Scope, Value,
};

/// Core capability that selects the shared-read schedule. A consumer that does
/// not meter this schedule must refuse the module.
pub(crate) const SHARED_READS_CAPABILITY: &str = "core.cost.shared-reads/v1";

impl Meter<'_> {
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
        &self,
        callee: &str,
        expr: &CoreExpr,
        available: &Scope<'_>,
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

    fn list_shell(&self, expr: &CoreExpr, available: &Scope<'_>, depth: usize) -> Option<Cost> {
        if depth > super::MAX_CORE_GRAPH_DEPTH {
            return None;
        }
        let coordinate = expression_type_coordinate(self.core, self.facts, expr, available)?;
        let CoreType::List { max, .. } = resolved_core_type(self.core, &coordinate)? else {
            return None;
        };
        shell(max)?.evaluation()
    }
}

/// A list value with `items` shared item references.
fn shell(items: u64) -> Option<Value> {
    Value::CELL.add(Value {
        cells: items,
        bytes: items.checked_mul(64)?,
    })
}

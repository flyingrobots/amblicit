//! Canonical ordering integrity for the explicitly selected v2 (ordered) and
//! v3 (structured) artifacts.

use std::collections::{BTreeMap, BTreeSet};

use super::{
    TargetIrArtifact, TargetIrIntent, TargetIrOrderEntry, TargetIrPureBinding,
    TargetIrRequireFailure, TargetIrRequirement, TargetIrStep, ECHO_DPO_TARGET_PROFILE,
    ECHO_ORDERED_SPAN_IR_DOMAIN, ECHO_STRUCTURED_SPAN_IR_DOMAIN,
};
use crate::core_ir::{
    CompareOp, CoreBound, CoreExpr, CorePredicate, CoreValue, LocalRef, ResourceRef,
    CORE_APPLICATION_INPUT_LOCAL_ID,
};
use crate::MAX_CANONICAL_NESTING_DEPTH;

/// `None` for a phased domain (no execution order), otherwise whether the
/// ordered domain is the structured v3 one.
fn order_mode(domain: &str) -> Option<bool> {
    match domain {
        ECHO_ORDERED_SPAN_IR_DOMAIN => Some(false),
        ECHO_STRUCTURED_SPAN_IR_DOMAIN => Some(true),
        _ => None,
    }
}

/// The artifact-level half of the execution-order check: ordered and
/// structured domains require the Echo DPO target profile.
pub(crate) fn header_has_valid_execution_order(domain: &str, target_profile: &ResourceRef) -> bool {
    order_mode(domain).is_none() || target_profile.coordinate == ECHO_DPO_TARGET_PROFILE
}

/// The per-intent half of the execution-order check, on a borrowed intent.
pub(crate) fn intent_has_valid_execution_order(domain: &str, intent: &TargetIrIntent) -> bool {
    match order_mode(domain) {
        None => intent.execution_order.is_none(),
        Some(structured) => intent_has_valid_order(intent, structured),
    }
}

pub(crate) fn artifact_has_valid_execution_order(artifact: &TargetIrArtifact) -> bool {
    header_has_valid_execution_order(&artifact.domain, &artifact.target_profile)
        && artifact
            .intents
            .values()
            .all(|intent| intent_has_valid_execution_order(&artifact.domain, intent))
}

#[derive(Clone, Copy)]
enum Instruction<'a> {
    Binding(&'a TargetIrPureBinding),
    Step(&'a TargetIrStep),
    Require(&'a TargetIrRequirement),
}

fn intent_has_valid_order(intent: &TargetIrIntent, structured: bool) -> bool {
    let Some(order) = &intent.execution_order else {
        return false;
    };
    if !intent.external_action_requests.is_empty() {
        return false;
    }
    let instructions = intent
        .pure_bindings
        .iter()
        .map(|node| (node.id.as_str(), Instruction::Binding(node)))
        .chain(
            intent
                .steps
                .iter()
                .map(|node| (node.id.as_str(), Instruction::Step(node))),
        )
        .chain(
            intent
                .requirements
                .iter()
                .map(|node| (node.id.as_str(), Instruction::Require(node))),
        )
        .collect::<BTreeMap<_, _>>();
    let count = intent.pure_bindings.len() + intent.steps.len() + intent.requirements.len();
    // Every instruction appears exactly once; a v2 order is flat, so its
    // length must also match. A v3 tree is checked by the walk below.
    if instructions.len() != count
        || (!structured && order.len() != count)
        || instructions.contains_key("")
    {
        return false;
    }
    // The Core schema requires one or more `all`/`any` values. Under v3 every
    // predicate the walk visits must satisfy it; v2 encoding is unchanged.
    let mut available = LocalAvailability {
        structured_checks: structured,
        ..LocalAvailability::default()
    };
    if !intent
        .basis
        .as_ref()
        .is_none_or(|basis| available.expression(basis, 0))
        || !intent
            .input_constraints
            .iter()
            .all(|constraint| available.predicate(&constraint.predicate, 0))
    {
        return false;
    }
    let instruction_ids = instructions.keys().copied().collect();
    let table_locals = intent
        .pure_bindings
        .iter()
        .map(|node| node.binding.id.as_str())
        .chain(intent.steps.iter().map(|node| node.binding.id.as_str()))
        .collect();
    let mut walk = OrderWalk {
        structured,
        instruction_ids,
        table_locals,
        instructions,
        control_ids: BTreeSet::new(),
        claimed: BTreeSet::new(),
        available,
    };
    walk.entries(order, 0)
        && walk.instructions.is_empty()
        && walk.available.expression(&intent.result, 0)
}

/// One pass over an order tree. Every table instruction is consumed exactly
/// once across all scopes; control ids are nonempty, unique and disjoint from
/// instruction ids; locals introduced inside a loop body or branch arm are
/// visible only to later entries of that block.
struct OrderWalk<'a> {
    structured: bool,
    instruction_ids: BTreeSet<&'a str>,
    /// Locals produced by table instructions, wherever they appear.
    table_locals: BTreeSet<&'a str>,
    instructions: BTreeMap<&'a str, Instruction<'a>>,
    control_ids: BTreeSet<&'a str>,
    /// Loop binders across the whole intent and, under v3, every obstruction
    /// binder and producer binding, so no identity is introduced twice.
    claimed: BTreeSet<&'a str>,
    available: LocalAvailability<'a>,
}

impl<'a> OrderWalk<'a> {
    fn entries(&mut self, entries: &'a [TargetIrOrderEntry], depth: usize) -> bool {
        if depth > MAX_CANONICAL_NESTING_DEPTH {
            return false;
        }
        entries.iter().all(|entry| self.entry(entry, depth))
    }

    fn control_id(&mut self, id: &'a str) -> bool {
        self.structured
            && !id.is_empty()
            && !self.instruction_ids.contains(id)
            && self.control_ids.insert(id)
    }

    /// Run `body` in a child scope and restore the enclosing scope afterwards.
    fn scoped(&mut self, body: impl FnOnce(&mut Self) -> bool) -> bool {
        let saved = self.available.producers.clone();
        let valid = body(self);
        self.available.producers = saved;
        valid
    }

    /// Under v3, every identity an instruction introduces (obstruction binders
    /// and its result binding) is unique across the whole intent, although
    /// references to an obstruction binder stay scoped to its arm.
    fn claim_identities(&mut self, instruction: Instruction<'a>) -> bool {
        match instruction {
            Instruction::Binding(node) => self.claimed.insert(node.binding.id.as_str()),
            Instruction::Step(node) => {
                node.obstruction_arms
                    .values()
                    .all(|arm| self.claimed.insert(arm.binder.id.as_str()))
                    && self.claimed.insert(node.binding.id.as_str())
            }
            Instruction::Require(_) => true,
        }
    }

    fn entry(&mut self, entry: &'a TargetIrOrderEntry, depth: usize) -> bool {
        match entry {
            TargetIrOrderEntry::Instruction(id) => {
                if self.control_ids.contains(id.as_str()) {
                    return false;
                }
                let Some(instruction) = self.instructions.remove(id.as_str()) else {
                    return false;
                };
                (!self.structured || self.claim_identities(instruction))
                    && self.available.instruction(instruction)
            }
            TargetIrOrderEntry::For(entry) => {
                // Target has no authenticated constant values, so a v3 loop
                // bound must be literal, as lowering requires.
                matches!(entry.bound, CoreBound::Literal(_))
                    && self.control_id(&entry.id)
                    && self.available.expression(&entry.iterable, 0)
                    && !self.table_locals.contains(entry.binder.id.as_str())
                    && self.claimed.insert(entry.binder.id.as_str())
                    && self.scoped(|walk| {
                        walk.available.introduce(&entry.binder)
                            && walk.entries(&entry.body, depth + 1)
                    })
            }
            TargetIrOrderEntry::Branch(entry) => {
                self.control_id(&entry.id)
                    && self.available.predicate(&entry.predicate, 0)
                    && self.scoped(|walk| walk.entries(&entry.then_entries, depth + 1))
                    && self.scoped(|walk| walk.entries(&entry.else_entries, depth + 1))
            }
        }
    }
}

#[derive(Default)]
struct LocalAvailability<'a> {
    input: Option<&'a LocalRef>,
    producers: BTreeMap<&'a str, &'a LocalRef>,
    /// v3 refuses `all`/`any` with no values and ordered comparisons of
    /// provably non-integer operands wherever a predicate appears.
    structured_checks: bool,
}

impl<'a> LocalAvailability<'a> {
    fn introduce(&mut self, binding: &'a LocalRef) -> bool {
        !binding.id.is_empty()
            && binding.id != CORE_APPLICATION_INPUT_LOCAL_ID
            && self.producers.insert(&binding.id, binding).is_none()
    }

    fn reference(&mut self, reference: &'a LocalRef) -> bool {
        if reference.id == CORE_APPLICATION_INPUT_LOCAL_ID {
            // The artifact has no separate input declaration. Core validation
            // proves its type; the encoder additionally requires consistency.
            *self.input.get_or_insert(reference) == reference
        } else {
            self.producers.get(reference.id.as_str()).copied() == Some(reference)
        }
    }

    fn instruction(&mut self, instruction: Instruction<'a>) -> bool {
        match instruction {
            Instruction::Binding(node) => {
                self.expression(&node.value, 0) && self.introduce(&node.binding)
            }
            Instruction::Step(node) => {
                if !self.expression(&node.input, 0) {
                    return false;
                }
                for arm in node.obstruction_arms.values() {
                    if !self.introduce(&arm.binder) {
                        return false;
                    }
                    let valid = self.expression(&arm.value, 0);
                    self.producers.remove(arm.binder.id.as_str());
                    if !valid {
                        return false;
                    }
                }
                self.introduce(&node.binding)
            }
            Instruction::Require(node) => {
                let (TargetIrRequireFailure::Terminal { reason }
                | TargetIrRequireFailure::ContinueObstructed { reason }) = &node.on_failure;
                self.predicate(&node.predicate, 0)
                    && reason
                        .payload
                        .values()
                        .all(|value| self.expression(value, 0))
            }
        }
    }

    fn expression(&mut self, expression: &'a CoreExpr, depth: usize) -> bool {
        if depth > MAX_CANONICAL_NESTING_DEPTH {
            return false;
        }
        match expression {
            CoreExpr::Local { reference } => self.reference(reference),
            CoreExpr::Const(_) => true,
            CoreExpr::Record { fields } => fields
                .values()
                .all(|value| self.expression(value, depth + 1)),
            CoreExpr::List { values } => {
                values.iter().all(|value| self.expression(value, depth + 1))
            }
            CoreExpr::Field { base, .. } => self.expression(base, depth + 1),
            CoreExpr::Call { args, .. } => args.iter().all(|arg| self.expression(arg, depth + 1)),
            CoreExpr::If {
                predicate,
                then_value,
                else_value,
            } => {
                self.predicate(predicate, depth + 1)
                    && self.expression(then_value, depth + 1)
                    && self.expression(else_value, depth + 1)
            }
        }
    }

    fn predicate(&mut self, predicate: &'a CorePredicate, depth: usize) -> bool {
        if depth > MAX_CANONICAL_NESTING_DEPTH {
            return false;
        }
        match predicate {
            CorePredicate::True | CorePredicate::False => true,
            CorePredicate::Not(inner) => self.predicate(inner, depth + 1),
            CorePredicate::All(predicates) | CorePredicate::Any(predicates) => {
                (!self.structured_checks || !predicates.is_empty())
                    && predicates
                        .iter()
                        .all(|predicate| self.predicate(predicate, depth + 1))
            }
            CorePredicate::Compare { op, left, right } => {
                (!self.structured_checks
                    || !matches!(
                        op,
                        CompareOp::Lt | CompareOp::Le | CompareOp::Gt | CompareOp::Ge
                    )
                    || !(provably_non_integer(left) || provably_non_integer(right)))
                    && self.expression(left, depth + 1)
                    && self.expression(right, depth + 1)
            }
        }
    }
}

/// Whether an operand is non-integer from the artifact alone. The encoder has
/// no type table, so field projections and calls are left to lowering, which
/// resolves operand types exactly.
fn provably_non_integer(expression: &CoreExpr) -> bool {
    match expression {
        CoreExpr::Const(value) => !matches!(value, CoreValue::Int { .. }),
        CoreExpr::Record { .. } | CoreExpr::List { .. } => true,
        CoreExpr::Local { reference } => {
            let ty = reference.ty.as_str();
            ty == "Bool"
                || ty == "Unit"
                || ["String<", "Bytes<", "Record<", "Option<", "List<", "Map<"]
                    .into_iter()
                    .any(|prefix| ty.starts_with(prefix))
        }
        CoreExpr::If {
            then_value,
            else_value,
            ..
        } => provably_non_integer(then_value) || provably_non_integer(else_value),
        CoreExpr::Field { .. } | CoreExpr::Call { .. } => false,
    }
}

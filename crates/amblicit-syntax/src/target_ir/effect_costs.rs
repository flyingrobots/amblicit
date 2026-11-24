//! This module owns provider-published effect cost templates (#341).
//!
//! An `edict.target-profile.intrinsics/v3` catalog gives each effect intrinsic
//! an `effectCostRule`: closed `u64` expressions for steps and bytes over the
//! effect's declared input and output types, named parameters read from the
//! operation configuration (`effect_parameters`), the module's costliest
//! source helper and a source helper that a parameter names. Under the
//! shared-read schedule the rule replaces the generic charge of every effect
//! node bound to that intrinsic; under the copying schedule it is added.
//! `copy` and `validations` follow the provider's value shape: a scalar or
//! byte string is one cell, records, lists, Options and variants add their
//! headers, as tabulated in `docs/topics/core-cost-schedules/rationale.md`.
use super::effect_parameters::{ParameterKind, ParameterValue};
use super::{resolved_core_type, CoreModule, CoreType};
use crate::canonical::CanonicalValue;
use std::collections::BTreeMap;

/// Deepest level the decoder reads in an `effectCostRule`. Expressions,
/// measures and types share one level counter: the rule's `steps` and `bytes`
/// expressions are level 0, and each nested expression, measure, type or
/// record field is one level deeper.
pub const TARGET_EFFECT_COST_MAX_DEPTH: usize = 16;
/// Deepest level the published catalog schema admits, on the same level
/// counter. `cargo xtask provider-contract-pack` writes the schema's cost
/// rules out level by level to this depth, because the provider
/// artifact-schema registry refuses recursive cost rules. It is tighter than
/// the decoder: the registry's construction work doubles with each level
/// (12 to 21 ms in a release build at 10, about 750 ms at 16), so a catalog nested
/// deeper than this is refused when its provider package loads.
pub const TARGET_EFFECT_COST_SCHEMA_MAX_DEPTH: usize = 10;
const _: () = assert!(TARGET_EFFECT_COST_SCHEMA_MAX_DEPTH <= TARGET_EFFECT_COST_MAX_DEPTH);
/// Most terms a `sum` may carry; the published schema states the same bound.
pub const TARGET_EFFECT_COST_MAX_SUM_TERMS: usize = 16;
const MAX_EXPRESSION_DEPTH: usize = TARGET_EFFECT_COST_MAX_DEPTH;
const MAX_SUM_TERMS: usize = TARGET_EFFECT_COST_MAX_SUM_TERMS;

/// Which half of a copy cost a measure reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Half {
    Steps,
    Bytes,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Side {
    Input,
    Output,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum TypeExpr {
    Path {
        side: Side,
        fields: Vec<String>,
        items: u64,
        /// Record fields inside the item the `items` list steps reach.
        then: Vec<String>,
    },
    Word,
    Record(BTreeMap<String, TypeExpr>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Measure {
    BytesMax(TypeExpr),
    ListMax(TypeExpr),
    Copy(TypeExpr, Half),
    Validations(TypeExpr),
    Parameter(String),
    CostliestHelper(Half),
    /// One call to the source helper the named `helper` parameter selects.
    Helper(String, Half),
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Expr {
    Const(u64),
    Sum(Vec<Expr>),
    Product(Box<Expr>, Box<Expr>),
    Measure(Measure),
}

/// A validated effect cost rule from a pinned catalog.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TargetEffectCostRule {
    steps: Expr,
    bytes: Expr,
}

/// An effect cost template bound to the semantic effect it lowers.
///
/// Built only by [`crate::admit_target_effect_costs`], so its rule always
/// came from a catalog checked against a trusted pin.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TargetEffectCostFact {
    pub(crate) effect: String,
    pub(crate) intrinsic: String,
    pub(crate) rule: TargetEffectCostRule,
    /// The parameters the rule uses, read from the operation configuration.
    pub(crate) parameters: BTreeMap<String, ParameterValue>,
}

impl TargetEffectCostFact {
    /// The semantic effect coordinate this template charges.
    #[must_use]
    pub fn effect(&self) -> &str {
        &self.effect
    }

    /// The target effect intrinsic the effect lowers to.
    #[must_use]
    pub fn intrinsic(&self) -> &str {
        &self.intrinsic
    }

    /// This fact with every `helper` parameter renamed by `rename`, or `None`
    /// when `rename` resolves some helper to nothing.
    pub(crate) fn rename_helpers<'a>(
        &self,
        rename: impl Fn(&str) -> Option<&'a str>,
    ) -> Option<Self> {
        let mut renamed = self.clone();
        for value in renamed.parameters.values_mut() {
            if let ParameterValue::Helper(name) = value {
                *name = rename(name)?.to_owned();
            }
        }
        Some(renamed)
    }

    /// The source helpers this template's `helper` parameters name.
    pub(crate) fn helpers(&self) -> impl Iterator<Item = &str> {
        self.parameters.values().filter_map(|value| match value {
            ParameterValue::Helper(name) => Some(name.as_str()),
            ParameterValue::Uint(_) => None,
        })
    }
}

impl TargetEffectCostRule {
    /// Every parameter the rule reads, with the kind each use needs.
    pub(crate) fn parameter_uses(&self) -> Vec<(&str, ParameterKind)> {
        let mut uses = Vec::new();
        let mut pending = vec![&self.steps, &self.bytes];
        while let Some(expr) = pending.pop() {
            match expr {
                Expr::Sum(terms) => pending.extend(terms),
                Expr::Product(left, right) => pending.extend([left.as_ref(), right.as_ref()]),
                Expr::Measure(Measure::Parameter(name)) => {
                    uses.push((name.as_str(), ParameterKind::Uint));
                }
                Expr::Measure(Measure::Helper(name, _)) => {
                    uses.push((name.as_str(), ParameterKind::Helper));
                }
                Expr::Const(_) | Expr::Measure(_) => {}
            }
        }
        uses
    }
}

fn text(value: &CanonicalValue) -> Option<&str> {
    match value {
        CanonicalValue::Text(text) => Some(text),
        _ => None,
    }
}

fn single(value: &CanonicalValue) -> Option<(&str, &CanonicalValue)> {
    let CanonicalValue::Map(entries) = value else {
        return None;
    };
    let [(key, inner)] = entries.as_slice() else {
        return None;
    };
    Some((text(key)?, inner))
}

fn field<'v>(value: &'v CanonicalValue, name: &str) -> Option<&'v CanonicalValue> {
    let CanonicalValue::Map(entries) = value else {
        return None;
    };
    entries
        .iter()
        .find_map(|(key, inner)| (text(key) == Some(name)).then_some(inner))
}

fn unsigned(value: &CanonicalValue) -> Option<u64> {
    match value {
        CanonicalValue::Integer(value) => u64::try_from(*value).ok(),
        _ => None,
    }
}

fn half(value: &CanonicalValue) -> Option<Half> {
    match text(value)? {
        "steps" => Some(Half::Steps),
        "bytes" => Some(Half::Bytes),
        _ => None,
    }
}

fn type_expr(value: &CanonicalValue, depth: usize) -> Option<TypeExpr> {
    if depth > MAX_EXPRESSION_DEPTH {
        return None;
    }
    match single(value)? {
        ("word", CanonicalValue::Null) => Some(TypeExpr::Word),
        ("record", CanonicalValue::Map(entries)) => entries
            .iter()
            .map(|(key, inner)| Some((text(key)?.to_owned(), type_expr(inner, depth + 1)?)))
            .collect::<Option<BTreeMap<_, _>>>()
            .map(TypeExpr::Record),
        ("path", path) => {
            let CanonicalValue::Map(entries) = path else {
                return None;
            };
            let then = match field(path, "then") {
                None => Vec::new(),
                Some(then) => field_names(then).filter(|then| !then.is_empty())?,
            };
            if entries.len() != 3 + usize::from(!then.is_empty()) {
                return None;
            }
            let side = match text(field(path, "side")?)? {
                "input" => Side::Input,
                "output" => Side::Output,
                _ => return None,
            };
            let fields = field_names(field(path, "fields")?)?;
            let items = unsigned(field(path, "item")?)?;
            // `then` after no item step would duplicate a longer `fields`.
            if items == 0 && !then.is_empty() {
                return None;
            }
            Some(TypeExpr::Path {
                side,
                fields,
                items,
                then,
            })
        }
        _ => None,
    }
}

fn field_names(value: &CanonicalValue) -> Option<Vec<String>> {
    let CanonicalValue::Array(names) = value else {
        return None;
    };
    names
        .iter()
        .map(|name| text(name).map(str::to_owned))
        .collect()
}

fn parameter_name(value: &CanonicalValue) -> Option<String> {
    text(value)
        .filter(|name| !name.is_empty())
        .map(str::to_owned)
}

fn measure(value: &CanonicalValue, depth: usize) -> Option<Measure> {
    // `{ copy: type-expr, of: half }` and `{ helper: { parameter: name },
    // of: half }` are the two-key measures.
    if let Some(of) = field(value, "of") {
        let CanonicalValue::Map(entries) = value else {
            return None;
        };
        if entries.len() != 2 {
            return None;
        }
        if let Some(copy) = field(value, "copy") {
            return Some(Measure::Copy(type_expr(copy, depth + 1)?, half(of)?));
        }
        return match single(field(value, "helper")?)? {
            ("parameter", name) => Some(Measure::Helper(parameter_name(name)?, half(of)?)),
            _ => None,
        };
    }
    let (kind, inner) = single(value)?;
    Some(match kind {
        "bytesMax" => Measure::BytesMax(type_expr(inner, depth + 1)?),
        "listMax" => Measure::ListMax(type_expr(inner, depth + 1)?),
        "validations" => Measure::Validations(type_expr(inner, depth + 1)?),
        "parameter" => Measure::Parameter(parameter_name(inner)?),
        "costliestHelper" => Measure::CostliestHelper(half(inner)?),
        _ => return None,
    })
}

fn expr(value: &CanonicalValue, depth: usize) -> Option<Expr> {
    if depth > MAX_EXPRESSION_DEPTH {
        return None;
    }
    let (kind, inner) = single(value)?;
    Some(match kind {
        "const" => Expr::Const(unsigned(inner)?),
        "sum" => {
            let CanonicalValue::Array(terms) = inner else {
                return None;
            };
            if !(2..=MAX_SUM_TERMS).contains(&terms.len()) {
                return None;
            }
            Expr::Sum(
                terms
                    .iter()
                    .map(|term| expr(term, depth + 1))
                    .collect::<Option<Vec<_>>>()?,
            )
        }
        "product" => {
            let CanonicalValue::Array(factors) = inner else {
                return None;
            };
            let [left, right] = factors.as_slice() else {
                return None;
            };
            Expr::Product(
                Box::new(expr(left, depth + 1)?),
                Box::new(expr(right, depth + 1)?),
            )
        }
        "measure" => Expr::Measure(measure(inner, depth + 1)?),
        _ => return None,
    })
}

/// Parse an `effectCostRule`, or `None` when it is malformed.
pub(crate) fn parse_rule(value: &CanonicalValue) -> Option<TargetEffectCostRule> {
    let CanonicalValue::Map(entries) = value else {
        return None;
    };
    if entries.len() != 2 {
        return None;
    }
    Some(TargetEffectCostRule {
        steps: expr(field(value, "steps")?, 0)?,
        bytes: expr(field(value, "bytes")?, 0)?,
    })
}

/// Everything a rule reads when it charges one effect node.
pub(crate) struct Charge<'c> {
    pub(crate) core: &'c CoreModule,
    pub(crate) input: &'c str,
    pub(crate) output: &'c str,
    pub(crate) parameters: &'c BTreeMap<String, ParameterValue>,
    /// The costliest source helper's (steps, bytes), or `None` when some
    /// helper's cost is unestablished.
    pub(crate) costliest_helper: Option<(u64, u64)>,
    /// The (steps, bytes) of one call to each source helper a `helper`
    /// parameter names; a helper with no established cost is absent.
    pub(crate) helpers: &'c BTreeMap<String, (u64, u64)>,
}

/// A value's provider shape: copy steps, copy bytes and validation steps.
#[derive(Clone, Copy)]
struct Shape {
    steps: u64,
    bytes: u64,
    validations: u64,
}

/// The provider shape's validation steps for a value of the Core type
/// `coordinate`, or `None` when the shape has no such value.
pub(crate) fn validations(core: &CoreModule, coordinate: &str) -> Option<u64> {
    let none = BTreeMap::new();
    let charge = Charge {
        core,
        input: coordinate,
        output: coordinate,
        parameters: &none,
        costliest_helper: None,
        helpers: &BTreeMap::new(),
    };
    let (name, ty) = charge.structural(coordinate)?;
    Some(charge.shape(&name, &ty, 0)?.validations)
}

impl Charge<'_> {
    /// The rule's (steps, bytes) for this effect node, or `None` when any
    /// measure cannot be established.
    pub(crate) fn evaluate(&self, rule: &TargetEffectCostRule) -> Option<(u64, u64)> {
        Some((self.expr(&rule.steps)?, self.expr(&rule.bytes)?))
    }

    fn expr(&self, expr: &Expr) -> Option<u64> {
        match expr {
            Expr::Const(value) => Some(*value),
            Expr::Sum(terms) => terms
                .iter()
                .try_fold(0_u64, |sum, term| sum.checked_add(self.expr(term)?)),
            Expr::Product(left, right) => self.expr(left)?.checked_mul(self.expr(right)?),
            Expr::Measure(measure) => self.measure(measure),
        }
    }

    fn measure(&self, measure: &Measure) -> Option<u64> {
        match measure {
            Measure::BytesMax(ty) => match self.resolve(ty)?.1 {
                CoreType::Bytes { max, .. } => Some(max),
                _ => None,
            },
            Measure::ListMax(ty) => match self.resolve(ty)?.1 {
                CoreType::List { max, .. } => Some(max),
                _ => None,
            },
            Measure::Copy(ty, Half::Steps) => Some(self.shape_of(ty, 0)?.steps),
            Measure::Copy(ty, Half::Bytes) => Some(self.shape_of(ty, 0)?.bytes),
            Measure::Validations(ty) => Some(self.shape_of(ty, 0)?.validations),
            Measure::Parameter(name) => match self.parameters.get(name)? {
                ParameterValue::Uint(value) => Some(*value),
                ParameterValue::Helper(_) => None,
            },
            Measure::Helper(name, half) => {
                let ParameterValue::Helper(helper) = self.parameters.get(name)? else {
                    return None;
                };
                let (steps, bytes) = *self.helpers.get(helper)?;
                Some(match half {
                    Half::Steps => steps,
                    Half::Bytes => bytes,
                })
            }
            Measure::CostliestHelper(Half::Steps) => Some(self.costliest_helper?.0),
            Measure::CostliestHelper(Half::Bytes) => Some(self.costliest_helper?.1),
        }
    }

    /// Follow a path to the Core type it names, through nominal wrappers.
    fn resolve(&self, ty: &TypeExpr) -> Option<(String, CoreType)> {
        let TypeExpr::Path {
            side,
            fields,
            items,
            then,
        } = ty
        else {
            return None;
        };
        let mut coordinate = match side {
            Side::Input => self.input.to_owned(),
            Side::Output => self.output.to_owned(),
        };
        let step_fields = |coordinate: &mut String, names: &[String]| -> Option<()> {
            for name in names {
                let CoreType::Record { fields } = self.structural(coordinate)?.1 else {
                    return None;
                };
                coordinate.clone_from(fields.get(name)?);
            }
            Some(())
        };
        step_fields(&mut coordinate, fields)?;
        for _ in 0..*items {
            let CoreType::List { item, .. } = self.structural(&coordinate)?.1 else {
                return None;
            };
            coordinate = item;
        }
        step_fields(&mut coordinate, then)?;
        self.structural(&coordinate)
    }

    /// The structural type behind a coordinate, and the coordinate that names
    /// it (a variant's identity).
    fn structural(&self, coordinate: &str) -> Option<(String, CoreType)> {
        let mut name = coordinate.to_owned();
        for _ in 0..=MAX_EXPRESSION_DEPTH {
            match resolved_core_type(self.core, &name)? {
                CoreType::Nominal { representation, .. } => name = representation,
                other => return Some((name, other)),
            }
        }
        None
    }

    fn shape_of(&self, ty: &TypeExpr, depth: usize) -> Option<Shape> {
        if depth > MAX_EXPRESSION_DEPTH {
            return None;
        }
        match ty {
            TypeExpr::Word => Some(Shape {
                steps: 1,
                bytes: 64,
                validations: 1,
            }),
            TypeExpr::Record(fields) => {
                let mut shape = Shape {
                    steps: 1,
                    bytes: 64,
                    validations: 1,
                };
                for (name, field) in fields {
                    let child = self.shape_of(field, depth + 1)?;
                    shape = record_field(shape, name, child)?;
                }
                Some(shape)
            }
            TypeExpr::Path { .. } => {
                let (name, core) = self.resolve(ty)?;
                self.shape(&name, &core, depth + 1)
            }
        }
    }

    fn shape(&self, name: &str, core: &CoreType, depth: usize) -> Option<Shape> {
        if depth > 4 * MAX_EXPRESSION_DEPTH {
            return None;
        }
        let child = |coordinate: &str| -> Option<Shape> {
            let (name, core) = self.structural(coordinate)?;
            self.shape(&name, &core, depth + 1)
        };
        match core {
            CoreType::Bool | CoreType::Int { .. } => Some(Shape {
                steps: 1,
                bytes: 64,
                validations: 1,
            }),
            CoreType::Bytes { max, .. } => Some(Shape {
                steps: 1,
                bytes: max.checked_add(64)?,
                validations: 1,
            }),
            CoreType::Option { item } => {
                let item = child(item)?;
                Some(Shape {
                    steps: item.steps.checked_add(1)?,
                    bytes: item.bytes.checked_add(64)?,
                    validations: item.validations.checked_add(1)?,
                })
            }
            CoreType::List { item, max } => {
                let item = child(item)?;
                Some(Shape {
                    steps: item.steps.checked_mul(*max)?.checked_add(1)?,
                    bytes: item.bytes.checked_mul(*max)?.checked_add(64)?,
                    validations: item.validations.checked_mul(*max)?.checked_add(1)?,
                })
            }
            CoreType::Record { fields } => {
                let mut shape = Shape {
                    steps: 1,
                    bytes: 64,
                    validations: 1,
                };
                for (field, coordinate) in fields {
                    shape = record_field(shape, field, child(coordinate)?)?;
                }
                Some(shape)
            }
            CoreType::Variant { cases } => {
                let mut payload = Shape {
                    steps: 1,
                    bytes: 64,
                    validations: 1,
                };
                let mut checks = 1_u64;
                let mut label = 0_u64;
                for (tag, case) in cases {
                    label = label.max(u64::try_from(tag.len()).ok()?);
                    if let Some(case) = case {
                        let shape = child(case)?;
                        payload.steps = payload.steps.max(shape.steps);
                        payload.bytes = payload.bytes.max(shape.bytes);
                        checks = checks.max(shape.validations);
                    }
                }
                let metadata = (6 * 64 + 15_u64)
                    .checked_add(u64::try_from(name.len()).ok()?)?
                    .checked_add(label)?;
                Some(Shape {
                    steps: payload.steps.checked_add(6)?,
                    bytes: payload.bytes.checked_add(metadata)?,
                    validations: checks.checked_add(3)?,
                })
            }
            // The provider shape has no text, unit, map or reference values.
            _ => None,
        }
    }
}

fn record_field(shape: Shape, name: &str, child: Shape) -> Option<Shape> {
    Some(Shape {
        steps: shape.steps.checked_add(child.steps)?.checked_add(1)?,
        bytes: shape
            .bytes
            .checked_add(child.bytes)?
            .checked_add(64)?
            .checked_add(u64::try_from(name.len()).ok()?)?,
        validations: shape.validations.checked_add(child.validations)?,
    })
}

#[cfg(test)]
mod tests {
    //! Oracle: the provider value shape tabulated in
    //! `docs/topics/core-cost-schedules/rationale.md`, computed by hand.
    use super::{parse_rule, Charge};
    use crate::canonical::CanonicalValue as C;
    use crate::{compile_to_core, parse_module, CompilerContext};
    use std::collections::BTreeMap;

    const SOURCE: &str = "package t.shape@1;
type Row = { a: Bytes<max=10>, b: List<U64, max=3>, c: Option<Bool> };
type Rows = { rows: List<Row, max=2> };
fn keep(value: Rows) -> Rows { return value; }";

    fn text(value: &str) -> C {
        C::Text(value.into())
    }

    fn map(entries: Vec<(&str, C)>) -> C {
        C::Map(entries.into_iter().map(|(k, v)| (text(k), v)).collect())
    }

    fn path(fields: &[&str], item: i128) -> C {
        map(vec![(
            "path",
            map(vec![
                ("side", text("input")),
                ("fields", C::Array(fields.iter().map(|f| text(f)).collect())),
                ("item", C::Integer(item)),
            ]),
        )])
    }

    /// Evaluate `{ steps: measure(steps), bytes: measure(bytes) }`.
    fn charge(steps: C, bytes: C) -> (u64, u64) {
        let core = compile_to_core(&parse_module(SOURCE).unwrap(), &CompilerContext::new())
            .expect("shape fixture compiles");
        let rule = parse_rule(&map(vec![
            ("steps", map(vec![("measure", steps)])),
            ("bytes", map(vec![("measure", bytes)])),
        ]))
        .expect("well-formed rule");
        let parameters = BTreeMap::new();
        let helpers = BTreeMap::new();
        Charge {
            core: &core,
            input: "t.shape@1.Rows",
            output: "t.shape@1.Rows",
            parameters: &parameters,
            costliest_helper: Some((0, 0)),
            helpers: &helpers,
        }
        .evaluate(&rule)
        .expect("established charge")
    }

    /// A path that names `then` fields inside the item `item` list steps reach.
    fn item_path(fields: &[&str], item: i128, then: &[&str]) -> C {
        map(vec![(
            "path",
            map(vec![
                ("side", text("input")),
                ("fields", C::Array(fields.iter().map(|f| text(f)).collect())),
                ("item", C::Integer(item)),
                ("then", C::Array(then.iter().map(|f| text(f)).collect())),
            ]),
        )])
    }

    #[test]
    fn a_path_names_a_field_inside_a_list_item() {
        // rows[*].a is Bytes<max=10>; rows[*].b is List<U64, max=3>.
        let bytes_max = map(vec![("bytesMax", item_path(&["rows"], 1, &["a"]))]);
        let list_max = map(vec![("listMax", item_path(&["rows"], 1, &["b"]))]);
        assert_eq!(charge(bytes_max, list_max), (10, 3));
    }

    #[test]
    fn malformed_paths_are_refused() {
        let rule = |ty: C| {
            parse_rule(&map(vec![
                ("steps", map(vec![("measure", map(vec![("bytesMax", ty)]))])),
                ("bytes", map(vec![("const", C::Integer(0))])),
            ]))
        };
        assert!(rule(item_path(&["rows"], 1, &["a"])).is_some());
        // An empty `then` has a shorter canonical spelling and is refused.
        assert!(rule(item_path(&["rows"], 1, &[])).is_none());
        // `then` without an item step is the same as a longer `fields`.
        assert!(rule(item_path(&["rows"], 0, &["a"])).is_none());
        let extra = map(vec![(
            "path",
            map(vec![
                ("side", text("input")),
                ("fields", C::Array(vec![text("rows")])),
                ("item", C::Integer(1)),
                ("unknown", C::Null),
            ]),
        )]);
        assert!(rule(extra).is_none());
    }

    #[test]
    fn copy_of_a_record_follows_the_provider_shape() {
        // a: (1, 74); b: (3+1, 3*64+64) = (4, 256); c: (1+1, 64+64) = (2, 128).
        // Record: steps 1 + (1+1) + (4+1) + (2+1) = 11;
        // bytes 64 + (74+64+1) + (256+64+1) + (128+64+1) = 717.
        let row = path(&["rows"], 1);
        let steps = map(vec![("copy", row.clone()), ("of", text("steps"))]);
        let bytes = map(vec![("copy", row), ("of", text("bytes"))]);
        assert_eq!(charge(steps, bytes), (11, 717));
    }

    #[test]
    fn validations_and_maxima_follow_the_provider_shape() {
        // Validations of a row: 1 + 1 + (1*3+1) + (1+1) = 8.
        let validations = map(vec![("validations", path(&["rows"], 1))]);
        let list_max = map(vec![("listMax", path(&["rows"], 0))]);
        assert_eq!(charge(validations, list_max), (8, 2));
        let bytes_max = map(vec![("bytesMax", path(&["rows", "a"], 0))]);
        let inner_list = map(vec![("listMax", path(&["rows", "b"], 0))]);
        // `rows` is a list, so a field step through it refuses: `item` first.
        let core =
            compile_to_core(&parse_module(SOURCE).unwrap(), &CompilerContext::new()).unwrap();
        let parameters = BTreeMap::new();
        let helpers = BTreeMap::new();
        let rule = parse_rule(&map(vec![
            ("steps", map(vec![("measure", bytes_max)])),
            ("bytes", map(vec![("measure", inner_list)])),
        ]))
        .unwrap();
        let unestablished = Charge {
            core: &core,
            input: "t.shape@1.Rows",
            output: "t.shape@1.Rows",
            parameters: &parameters,
            costliest_helper: None,
            helpers: &helpers,
        }
        .evaluate(&rule);
        assert_eq!(unestablished, None);
    }

    #[test]
    fn a_synthesized_record_is_shaped_from_its_parts() {
        // { w: word, r: Row }: steps 1 + (1+1) + (11+1) = 15;
        // bytes 64 + (64+64+1) + (717+64+1) = 975.
        let record = map(vec![(
            "record",
            map(vec![
                ("w", map(vec![("word", C::Null)])),
                ("r", path(&["rows"], 1)),
            ]),
        )]);
        let steps = map(vec![("copy", record.clone()), ("of", text("steps"))]);
        let bytes = map(vec![("copy", record), ("of", text("bytes"))]);
        assert_eq!(charge(steps, bytes), (15, 975));
    }

    /// `n` nested products whose first factor nests; the innermost
    /// expression, `leaf`, sits at level `n`.
    fn nested_expression(n: usize, leaf: C) -> C {
        (0..n).fold(leaf, |inner, _| {
            map(vec![(
                "product",
                C::Array(vec![inner, map(vec![("const", C::Integer(1))])]),
            )])
        })
    }

    /// A `bytesMax` measure whose type, at level 2, nests records until a
    /// `word` at level `n`.
    fn nested_type_measure(n: usize) -> C {
        let ty = (2..n).fold(map(vec![("word", C::Null)]), |inner, _| {
            map(vec![("record", map(vec![("f", inner)]))])
        });
        map(vec![("measure", map(vec![("bytesMax", ty)]))])
    }

    fn rule_with_steps(steps: C) -> C {
        map(vec![
            ("steps", steps),
            ("bytes", map(vec![("const", C::Integer(0))])),
        ])
    }

    /// Oracle: the level rule in `docs/abi/edict-target-profile.cddl`: steps
    /// is level 0, each nested expression, measure, type or record field one
    /// deeper, and the decoder reads at most `TARGET_EFFECT_COST_MAX_DEPTH`.
    #[test]
    fn decoder_reads_nesting_to_level_16_and_no_deeper() {
        let depth = super::TARGET_EFFECT_COST_MAX_DEPTH;
        let constant = map(vec![("const", C::Integer(1))]);
        let parameter = map(vec![("measure", map(vec![("parameter", text("p"))]))]);
        assert!(parse_rule(&rule_with_steps(nested_expression(depth, constant.clone()))).is_some());
        assert!(parse_rule(&rule_with_steps(nested_expression(depth + 1, constant))).is_none());
        // A measure one level below the deepest expression needs no type.
        assert!(parse_rule(&rule_with_steps(nested_expression(depth, parameter))).is_some());
        assert!(parse_rule(&rule_with_steps(nested_type_measure(depth))).is_some());
        assert!(parse_rule(&rule_with_steps(nested_type_measure(depth + 1))).is_none());
    }
}

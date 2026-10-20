use std::collections::{BTreeMap, BTreeSet};

use cddl_cat::cbor::validate_cbor;
use cddl_cat::context::BasicContext;
use cddl_cat::ivt::{
    Array, Choice, Control, KeyValue, Literal, Map, Node, PreludeType, RuleDef, RulesByName,
};
use edict_syntax::{encode_canonical_cbor, CanonicalValue};

use crate::PROVIDER_SCHEMA_VALIDATION_MAX_NESTING_DEPTH;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Position {
    Value,
    ArrayMember,
    MapMember,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RecursiveDispatchRequirement {
    Native,
    Specialized,
}

pub(crate) fn root_dispatch_requirement(
    rules: &RulesByName,
    root: &str,
) -> Option<RecursiveDispatchRequirement> {
    let definition = rules.get(root)?;
    if !definition.generic_parms.is_empty() {
        return None;
    }
    let mut active = BTreeMap::from([((root.to_owned(), Position::Value), 0)]);
    let mut requires_dispatch = false;
    audit_node(
        &definition.node,
        Position::Value,
        rules,
        &mut active,
        0,
        &mut requires_dispatch,
    )?;
    if requires_dispatch && !specialization_is_total(&definition.node, rules, &mut BTreeSet::new())
    {
        return None;
    }
    Some(if requires_dispatch {
        RecursiveDispatchRequirement::Specialized
    } else {
        RecursiveDispatchRequirement::Native
    })
}

pub(crate) fn specialize_root_for_value(
    context: &BasicContext,
    root: &str,
    value: &CanonicalValue,
) -> Option<RuleDef> {
    let rules = &context.rules;
    let definition = rules.get(root)?;
    if !definition.generic_parms.is_empty() {
        return None;
    }
    let mut specializer = Specializer {
        context,
        active: BTreeMap::from([(root.to_owned(), 0)]),
        #[cfg(test)]
        values_visited: 0,
    };
    let node = specializer.specialize_value(&definition.node, value, 0)?;
    if contains_rule_reference(&node) {
        return None;
    }
    Some(RuleDef {
        generic_parms: Vec::new(),
        node,
    })
}

fn audit_node(
    node: &Node,
    position: Position,
    rules: &RulesByName,
    active: &mut BTreeMap<(String, Position), usize>,
    input_depth: usize,
    requires_dispatch: &mut bool,
) -> Option<bool> {
    match (position, node) {
        (_, Node::Rule(rule)) => audit_rule(
            rule,
            position,
            rules,
            active,
            input_depth,
            requires_dispatch,
        ),
        (Position::Value | Position::ArrayMember | Position::MapMember, Node::Choice(choice)) => {
            audit_choice(
                choice,
                position,
                rules,
                active,
                input_depth,
                requires_dispatch,
            )
        }
        (Position::Value, Node::Map(map)) => audit_members(
            &map.members,
            Position::MapMember,
            rules,
            active,
            input_depth,
            requires_dispatch,
        ),
        (Position::Value, Node::Array(array)) => audit_members(
            &array.members,
            Position::ArrayMember,
            rules,
            active,
            input_depth,
            requires_dispatch,
        ),
        (Position::Value, Node::Group(group)) if group.members.len() == 1 => audit_node(
            &group.members[0],
            Position::Value,
            rules,
            active,
            input_depth,
            requires_dispatch,
        ),
        (Position::ArrayMember | Position::MapMember, Node::Occur(occur)) => {
            let reenters = audit_node(
                &occur.node,
                position,
                rules,
                active,
                input_depth,
                requires_dispatch,
            )?;
            let (minimum, maximum) = occur.limits();
            if reenters && minimum != maximum {
                *requires_dispatch = true;
            }
            Some(reenters)
        }
        (Position::ArrayMember | Position::MapMember, Node::KeyValue(pair)) => audit_pair(
            pair,
            position,
            rules,
            active,
            input_depth,
            requires_dispatch,
        ),
        (Position::ArrayMember | Position::MapMember, Node::Group(group)) => audit_members(
            &group.members,
            position,
            rules,
            active,
            input_depth,
            requires_dispatch,
        ),
        (Position::ArrayMember, _) => audit_node(
            node,
            Position::Value,
            rules,
            active,
            input_depth.saturating_add(1),
            requires_dispatch,
        ),
        (Position::Value | Position::MapMember, _) => Some(false),
    }
}

fn audit_pair(
    pair: &KeyValue,
    position: Position,
    rules: &RulesByName,
    active: &mut BTreeMap<(String, Position), usize>,
    input_depth: usize,
    requires_dispatch: &mut bool,
) -> Option<bool> {
    let key_depth = match position {
        Position::MapMember => input_depth.saturating_add(1),
        Position::ArrayMember => input_depth,
        Position::Value => return None,
    };
    let value_depth = input_depth.saturating_add(1);
    let key = audit_node(
        &pair.key,
        Position::Value,
        rules,
        active,
        key_depth,
        requires_dispatch,
    )?;
    let value = audit_node(
        &pair.value,
        Position::Value,
        rules,
        active,
        value_depth,
        requires_dispatch,
    )?;
    Some(key || value)
}

fn audit_rule(
    rule: &cddl_cat::ivt::Rule,
    position: Position,
    rules: &RulesByName,
    active: &mut BTreeMap<(String, Position), usize>,
    input_depth: usize,
    requires_dispatch: &mut bool,
) -> Option<bool> {
    if !rule.generic_args.is_empty() {
        return None;
    }
    let key = (rule.name.clone(), position);
    if let Some(active_depth) = active.get(&key) {
        return (input_depth > *active_depth).then_some(true);
    }
    let definition = rules.get(&rule.name)?;
    if !definition.generic_parms.is_empty() {
        return None;
    }
    active.insert(key.clone(), input_depth);
    let result = audit_node(
        &definition.node,
        position,
        rules,
        active,
        input_depth,
        requires_dispatch,
    );
    active.remove(&key);
    result
}

fn audit_choice(
    choice: &Choice,
    position: Position,
    rules: &RulesByName,
    active: &mut BTreeMap<(String, Position), usize>,
    input_depth: usize,
    requires_dispatch: &mut bool,
) -> Option<bool> {
    let mut recursive_alternatives = 0;
    let mut reenters = false;
    for option in &choice.options {
        let option_reenters = audit_node(
            option,
            position,
            rules,
            active,
            input_depth,
            requires_dispatch,
        )?;
        recursive_alternatives += usize::from(option_reenters);
        reenters |= option_reenters;
    }
    if recursive_alternatives > 1 {
        if tagged_choice(choice, rules).is_none() && required_key_choice(choice, rules).is_none() {
            return None;
        }
        *requires_dispatch = true;
    }
    Some(reenters)
}

fn audit_members(
    members: &[Node],
    position: Position,
    rules: &RulesByName,
    active: &mut BTreeMap<(String, Position), usize>,
    input_depth: usize,
    requires_dispatch: &mut bool,
) -> Option<bool> {
    let mut reenters = false;
    for member in members {
        reenters |= audit_node(
            member,
            position,
            rules,
            active,
            input_depth,
            requires_dispatch,
        )?;
    }
    Some(reenters)
}

fn specialization_is_total(
    node: &Node,
    rules: &RulesByName,
    visiting: &mut BTreeSet<String>,
) -> bool {
    match node {
        Node::Literal(_) | Node::PreludeType(_) => true,
        Node::Range(_) => inline_scalar_node(node, rules).is_some(),
        Node::Control(control) => inline_control(control, rules).is_some(),
        Node::Rule(rule) if rule.generic_args.is_empty() => {
            let Some(definition) = rules.get(&rule.name) else {
                return false;
            };
            if !definition.generic_parms.is_empty() {
                return false;
            }
            if !visiting.insert(rule.name.clone()) {
                return true;
            }
            let total = specialization_is_total(&definition.node, rules, visiting);
            visiting.remove(&rule.name);
            total
        }
        Node::Choice(choice) => choice
            .options
            .iter()
            .all(|option| specialization_is_total(option, rules, visiting)),
        Node::Map(map) => specialization_map_is_total(map, rules, visiting),
        Node::Array(array) => specialization_array_is_total(array, rules, visiting),
        Node::Group(group) if group.members.len() == 1 => {
            specialization_is_total(&group.members[0], rules, visiting)
        }
        Node::Rule(_)
        | Node::Group(_)
        | Node::KeyValue(_)
        | Node::Occur(_)
        | Node::Unwrap(_)
        | Node::Choiceify(_)
        | Node::ChoiceifyInline(_) => false,
    }
}

fn specialization_map_is_total(
    map: &Map,
    rules: &RulesByName,
    visiting: &mut BTreeSet<String>,
) -> bool {
    let Some(patterns) = map_patterns(&map.members, rules) else {
        return false;
    };
    if !patterns.iter().all(|pattern| {
        specialization_is_total(&pattern.pair.value, rules, visiting)
            && bounded_scalar_key(&pattern.pair.key, rules, &mut BTreeSet::new())
    }) {
        return false;
    }

    let mut exact = Vec::new();
    let mut flexible = 0_usize;
    for pattern in &patterns {
        match exact_scalar_literal(&pattern.pair.key, rules) {
            Some(literal) => {
                if exact.contains(&literal) {
                    return false;
                }
                exact.push(literal);
            }
            None => flexible = flexible.saturating_add(1),
        }
    }

    flexible == 0 || (flexible == 1 && patterns.len() == 1)
}

fn specialization_array_is_total(
    array: &Array,
    rules: &RulesByName,
    visiting: &mut BTreeSet<String>,
) -> bool {
    let Some(patterns) = array_patterns(&array.members, rules) else {
        return false;
    };
    if !patterns
        .iter()
        .all(|pattern| specialization_is_total(&pattern.node, rules, visiting))
    {
        return false;
    }
    let variable: Vec<_> = patterns
        .iter()
        .enumerate()
        .filter_map(|(index, pattern)| (pattern.minimum != pattern.maximum).then_some(index))
        .collect();
    let fixed_total = patterns
        .iter()
        .enumerate()
        .filter(|(index, _)| !variable.contains(index))
        .try_fold(0_usize, |total, (_, pattern)| {
            total.checked_add(pattern.minimum)
        });
    fixed_total.is_some()
        && (variable.is_empty()
            || (variable.len() == 1 && variable[0] == patterns.len().saturating_sub(1)))
}

fn exact_scalar_literal<'a>(node: &'a Node, rules: &'a RulesByName) -> Option<&'a Literal> {
    match terminal_node(node, rules)? {
        Node::Literal(
            literal @ (Literal::Bool(_) | Literal::Int(_) | Literal::Text(_) | Literal::Bytes(_)),
        ) => Some(literal),
        _ => None,
    }
}

fn bounded_scalar_key(node: &Node, rules: &RulesByName, visiting: &mut BTreeSet<String>) -> bool {
    match node {
        Node::Literal(
            Literal::Bool(_) | Literal::Int(_) | Literal::Text(_) | Literal::Bytes(_),
        )
        | Node::PreludeType(
            PreludeType::Nil
            | PreludeType::Bool
            | PreludeType::Int
            | PreludeType::Uint
            | PreludeType::Nint
            | PreludeType::Tstr
            | PreludeType::Bstr,
        ) => true,
        Node::Range(_) => inline_scalar_node(node, rules).is_some(),
        Node::Control(control) => inline_control(control, rules).is_some(),
        Node::Choice(choice) => choice
            .options
            .iter()
            .all(|option| bounded_scalar_key(option, rules, visiting)),
        Node::Group(group) if group.members.len() == 1 => {
            bounded_scalar_key(&group.members[0], rules, visiting)
        }
        Node::Rule(rule) if rule.generic_args.is_empty() && visiting.insert(rule.name.clone()) => {
            let total = rules.get(&rule.name).is_some_and(|definition| {
                definition.generic_parms.is_empty()
                    && bounded_scalar_key(&definition.node, rules, visiting)
            });
            visiting.remove(&rule.name);
            total
        }
        Node::Literal(Literal::Float(_))
        | Node::PreludeType(PreludeType::Any | PreludeType::Float)
        | Node::Rule(_)
        | Node::Map(_)
        | Node::Array(_)
        | Node::Group(_)
        | Node::KeyValue(_)
        | Node::Occur(_)
        | Node::Unwrap(_)
        | Node::Choiceify(_)
        | Node::ChoiceifyInline(_) => false,
    }
}

#[derive(Debug)]
struct TaggedChoice<'a> {
    key: &'a str,
    arms: BTreeMap<&'a str, &'a Node>,
}

#[derive(Debug)]
struct RequiredKeyChoice<'a> {
    key: String,
    present: &'a Node,
    absent: &'a Node,
}

fn tagged_choice<'a>(choice: &'a Choice, rules: &'a RulesByName) -> Option<TaggedChoice<'a>> {
    let first = choice.options.first()?;
    let candidates = required_text_literals(first, rules)?;
    for (key, _) in candidates {
        let mut arms = BTreeMap::new();
        let mut complete = true;
        for option in &choice.options {
            let Some(value) = required_text_literals(option, rules)?
                .into_iter()
                .find_map(|(candidate, value)| (candidate == key).then_some(value))
            else {
                complete = false;
                break;
            };
            if arms.insert(value, option).is_some() {
                complete = false;
                break;
            }
        }
        if complete {
            return Some(TaggedChoice { key, arms });
        }
    }
    None
}

fn required_key_choice<'a>(
    choice: &'a Choice,
    rules: &'a RulesByName,
) -> Option<RequiredKeyChoice<'a>> {
    let [first, second] = choice.options.as_slice() else {
        return None;
    };
    let first_keys = exact_text_map_key_requirements(first, rules)?;
    let second_keys = exact_text_map_key_requirements(second, rules)?;

    for (key, minimum) in &first_keys {
        if *minimum > 0 && !second_keys.contains_key(key) {
            return Some(RequiredKeyChoice {
                key: key.clone(),
                present: first,
                absent: second,
            });
        }
    }
    for (key, minimum) in &second_keys {
        if *minimum > 0 && !first_keys.contains_key(key) {
            return Some(RequiredKeyChoice {
                key: key.clone(),
                present: second,
                absent: first,
            });
        }
    }
    None
}

fn exact_text_map_key_requirements(
    node: &Node,
    rules: &RulesByName,
) -> Option<BTreeMap<String, usize>> {
    let Node::Map(map) = terminal_node(node, rules)? else {
        return None;
    };
    let mut requirements = BTreeMap::new();
    for pattern in map_patterns(&map.members, rules)? {
        let Literal::Text(key) = exact_scalar_literal(&pattern.pair.key, rules)? else {
            return None;
        };
        if requirements.insert(key.clone(), pattern.minimum).is_some() {
            return None;
        }
    }
    Some(requirements)
}

fn required_text_literals<'a>(
    node: &'a Node,
    rules: &'a RulesByName,
) -> Option<Vec<(&'a str, &'a str)>> {
    required_text_literals_cached(node, rules, 0, &mut BTreeMap::new())
}

type DiscriminatorLiterals<'a> = Vec<(&'a str, &'a str)>;
type DiscriminatorCache<'a> = BTreeMap<(&'a str, usize), Option<DiscriminatorLiterals<'a>>>;

fn required_text_literals_cached<'a>(
    node: &'a Node,
    rules: &'a RulesByName,
    depth: usize,
    cache: &mut DiscriminatorCache<'a>,
) -> Option<DiscriminatorLiterals<'a>> {
    if depth > PROVIDER_SCHEMA_VALIDATION_MAX_NESTING_DEPTH {
        return None;
    }
    if let Node::Rule(rule) = node {
        if !rule.generic_args.is_empty() {
            return None;
        }
        let key = (rule.name.as_str(), depth);
        if let Some(literals) = cache.get(&key) {
            return literals.clone();
        }
        let literals = required_text_literals_at_depth(node, rules, depth, cache);
        cache.insert(key, literals.clone());
        return literals;
    }
    required_text_literals_at_depth(node, rules, depth, cache)
}

fn required_text_literals_at_depth<'a>(
    node: &'a Node,
    rules: &'a RulesByName,
    depth: usize,
    cache: &mut DiscriminatorCache<'a>,
) -> Option<DiscriminatorLiterals<'a>> {
    let node = terminal_node(node, rules)?;
    if let Node::Choice(choice) = node {
        let (first, rest) = choice.options.split_first()?;
        let mut shared = required_text_literals_cached(first, rules, depth + 1, cache)?;
        for option in rest {
            let required = required_text_literals_cached(option, rules, depth + 1, cache)?;
            shared.retain(|literal| required.contains(literal));
        }
        return Some(shared);
    }
    let Node::Map(map) = node else {
        return Some(Vec::new());
    };
    let mut literals = Vec::new();
    for member in &map.members {
        let Node::KeyValue(pair) = member else {
            continue;
        };
        let (Some(Node::Literal(Literal::Text(key))), Some(Node::Literal(Literal::Text(value)))) = (
            terminal_node(&pair.key, rules),
            terminal_node(&pair.value, rules),
        ) else {
            continue;
        };
        if literals.iter().any(|(existing, _)| existing == key) {
            return None;
        }
        literals.push((key.as_str(), value.as_str()));
    }
    Some(literals)
}

fn terminal_node<'a>(node: &'a Node, rules: &'a RulesByName) -> Option<&'a Node> {
    let mut current = node;
    let mut visiting = BTreeSet::new();
    while let Node::Rule(rule) = current {
        if !rule.generic_args.is_empty() || !visiting.insert(&rule.name) {
            return None;
        }
        let definition = rules.get(&rule.name)?;
        if !definition.generic_parms.is_empty() {
            return None;
        }
        current = &definition.node;
    }
    Some(current)
}

struct Specializer<'a> {
    context: &'a BasicContext,
    active: BTreeMap<String, usize>,
    #[cfg(test)]
    values_visited: usize,
}

impl Specializer<'_> {
    fn specialize_value(
        &mut self,
        node: &Node,
        value: &CanonicalValue,
        input_depth: usize,
    ) -> Option<Node> {
        #[cfg(test)]
        {
            self.values_visited = self.values_visited.saturating_add(1);
        }
        match value {
            CanonicalValue::Array(_) | CanonicalValue::Map(_)
                if input_depth >= PROVIDER_SCHEMA_VALIDATION_MAX_NESTING_DEPTH =>
            {
                return None;
            }
            CanonicalValue::Null
            | CanonicalValue::Bool(_)
            | CanonicalValue::Integer(_)
            | CanonicalValue::Bytes(_)
            | CanonicalValue::Text(_)
                if input_depth > PROVIDER_SCHEMA_VALIDATION_MAX_NESTING_DEPTH =>
            {
                return None;
            }
            _ => {}
        }
        let context = self.context;
        let rules = &context.rules;
        match node {
            Node::Literal(_) | Node::PreludeType(_) => Some(node.clone()),
            Node::Range(range) => {
                let mut range = range.clone();
                range.start = Box::new(inline_scalar_node(&range.start, rules)?);
                range.end = Box::new(inline_scalar_node(&range.end, rules)?);
                Some(Node::Range(range))
            }
            Node::Control(control) => Some(Node::Control(inline_control(control, rules)?)),
            Node::Rule(rule) => {
                if !rule.generic_args.is_empty() {
                    return None;
                }
                let definition = rules.get(&rule.name)?;
                if !definition.generic_parms.is_empty() {
                    return None;
                }
                let previous = self.active.insert(rule.name.clone(), input_depth);
                if previous.is_some_and(|depth| input_depth <= depth) {
                    if let Some(depth) = previous {
                        self.active.insert(rule.name.clone(), depth);
                    }
                    return None;
                }
                let specialized = self.specialize_value(&definition.node, value, input_depth);
                if let Some(depth) = previous {
                    self.active.insert(rule.name.clone(), depth);
                } else {
                    self.active.remove(&rule.name);
                }
                specialized
            }
            Node::Choice(choice) => self.specialize_choice(choice, value, input_depth),
            Node::Map(map) => self.specialize_map(map, value, input_depth),
            Node::Array(array) => self.specialize_array(array, value, input_depth),
            Node::Group(group) if group.members.len() == 1 => {
                self.specialize_value(&group.members[0], value, input_depth)
            }
            Node::Group(_)
            | Node::KeyValue(_)
            | Node::Occur(_)
            | Node::Unwrap(_)
            | Node::Choiceify(_)
            | Node::ChoiceifyInline(_) => None,
        }
    }

    fn specialize_choice(
        &mut self,
        choice: &Choice,
        value: &CanonicalValue,
        input_depth: usize,
    ) -> Option<Node> {
        if let Some(dispatch) = tagged_choice(choice, &self.context.rules) {
            let CanonicalValue::Map(entries) = value else {
                return None;
            };
            let mut discriminators = entries.iter().filter_map(|(key, value)| {
                (key == &CanonicalValue::Text(dispatch.key.to_owned())).then_some(value)
            });
            let CanonicalValue::Text(discriminator) = discriminators.next()? else {
                return None;
            };
            if discriminators.next().is_some() {
                return None;
            }
            let option = dispatch.arms.get(discriminator.as_str())?;
            return self.specialize_value(option, value, input_depth);
        }
        if let Some(dispatch) = required_key_choice(choice, &self.context.rules) {
            let CanonicalValue::Map(entries) = value else {
                return None;
            };
            let mut occurrences = entries.iter().filter(
                |(key, _)| matches!(key, CanonicalValue::Text(text) if text == &dispatch.key),
            );
            let option = if occurrences.next().is_some() {
                if occurrences.next().is_some() {
                    return None;
                }
                dispatch.present
            } else {
                dispatch.absent
            };
            return self.specialize_value(option, value, input_depth);
        }

        let mut options = Vec::new();
        for option in &choice.options {
            if let Some(option) = self.specialize_value(option, value, input_depth) {
                options.push(option);
            }
        }
        match options.len() {
            0 => None,
            1 => options.pop(),
            _ => Some(Node::Choice(Choice { options })),
        }
    }

    fn specialize_map(
        &mut self,
        map: &Map,
        value: &CanonicalValue,
        input_depth: usize,
    ) -> Option<Node> {
        let CanonicalValue::Map(entries) = value else {
            return None;
        };
        let patterns = map_patterns(&map.members, &self.context.rules)?;
        let mut counts = vec![0_usize; patterns.len()];
        let mut members = Vec::with_capacity(entries.len());

        for (key, value) in entries {
            let mut exact = Vec::new();
            let mut fallback = Vec::new();
            for (index, pattern) in patterns.iter().enumerate() {
                if counts[index] >= pattern.maximum
                    || !key_matches(&pattern.pair.key, key, self.context)?
                {
                    continue;
                }
                if literal_matches(&pattern.pair.key, key, &self.context.rules) {
                    exact.push(index);
                } else {
                    fallback.push(index);
                }
            }
            let index = match (exact.as_slice(), fallback.as_slice()) {
                ([index], _) | ([], [index]) => *index,
                _ => return None,
            };
            counts[index] = counts[index].saturating_add(1);
            let specialized = self.specialize_value(
                &patterns[index].pair.value,
                value,
                input_depth.saturating_add(1),
            )?;
            let mut pair = patterns[index].pair.clone();
            pair.key = Box::new(canonical_key_node(key)?);
            pair.value = Box::new(specialized);
            members.push(Node::KeyValue(pair));
        }

        if patterns
            .iter()
            .zip(counts)
            .any(|(pattern, count)| count < pattern.minimum || count > pattern.maximum)
        {
            return None;
        }
        Some(Node::Map(Map { members }))
    }

    fn specialize_array(
        &mut self,
        array: &Array,
        value: &CanonicalValue,
        input_depth: usize,
    ) -> Option<Node> {
        let CanonicalValue::Array(values) = value else {
            return None;
        };
        let patterns = array_patterns(&array.members, &self.context.rules)?;
        let variable: Vec<_> = patterns
            .iter()
            .enumerate()
            .filter(|(_, pattern)| pattern.minimum != pattern.maximum)
            .map(|(index, _)| index)
            .collect();
        if variable.len() > 1 {
            return None;
        }
        let fixed = patterns
            .iter()
            .enumerate()
            .filter(|(index, _)| !variable.contains(index))
            .try_fold(0_usize, |total, (_, pattern)| {
                total.checked_add(pattern.minimum)
            })?;
        let mut counts: Vec<_> = patterns.iter().map(|pattern| pattern.minimum).collect();
        if let Some(index) = variable.first().copied() {
            let count = values.len().checked_sub(fixed)?;
            if count < patterns[index].minimum || count > patterns[index].maximum {
                return None;
            }
            counts[index] = count;
        } else if fixed != values.len() {
            return None;
        }

        let mut value_index = 0;
        let mut members = Vec::with_capacity(values.len());
        for (pattern, count) in patterns.iter().zip(counts) {
            for _ in 0..count {
                let value = values.get(value_index)?;
                value_index += 1;
                members.push(self.specialize_value(
                    &pattern.node,
                    value,
                    input_depth.saturating_add(1),
                )?);
            }
        }
        (value_index == values.len()).then_some(Node::Array(Array { members }))
    }
}

#[derive(Clone)]
struct MapPattern {
    pair: KeyValue,
    minimum: usize,
    maximum: usize,
}

fn map_patterns(members: &[Node], rules: &RulesByName) -> Option<Vec<MapPattern>> {
    let mut patterns = Vec::new();
    let mut visiting = BTreeSet::new();
    for member in members {
        collect_map_pattern(member, rules, &mut visiting, None, &mut patterns)?;
    }
    Some(patterns)
}

fn collect_map_pattern(
    node: &Node,
    rules: &RulesByName,
    visiting: &mut BTreeSet<String>,
    occurrence: Option<(usize, usize)>,
    patterns: &mut Vec<MapPattern>,
) -> Option<()> {
    match node {
        Node::Occur(occur) if occurrence.is_none() => {
            collect_map_pattern(&occur.node, rules, visiting, Some(occur.limits()), patterns)
        }
        Node::KeyValue(pair) => {
            let (minimum, maximum) = occurrence.unwrap_or((1, 1));
            patterns.push(MapPattern {
                pair: pair.clone(),
                minimum,
                maximum,
            });
            Some(())
        }
        Node::Group(group) if occurrence.is_none() => {
            for member in &group.members {
                collect_map_pattern(member, rules, visiting, None, patterns)?;
            }
            Some(())
        }
        Node::Rule(rule) if rule.generic_args.is_empty() && visiting.insert(rule.name.clone()) => {
            let definition = rules.get(&rule.name)?;
            let result = definition
                .generic_parms
                .is_empty()
                .then_some(())
                .and_then(|()| {
                    collect_map_pattern(&definition.node, rules, visiting, occurrence, patterns)
                });
            visiting.remove(&rule.name);
            result
        }
        _ => None,
    }
}

#[derive(Clone)]
struct ArrayPattern {
    node: Node,
    minimum: usize,
    maximum: usize,
}

fn array_patterns(members: &[Node], rules: &RulesByName) -> Option<Vec<ArrayPattern>> {
    let mut patterns = Vec::new();
    let mut visiting = BTreeSet::new();
    for member in members {
        collect_array_pattern(member, rules, &mut visiting, None, &mut patterns)?;
    }
    Some(patterns)
}

fn collect_array_pattern(
    node: &Node,
    rules: &RulesByName,
    visiting: &mut BTreeSet<String>,
    occurrence: Option<(usize, usize)>,
    patterns: &mut Vec<ArrayPattern>,
) -> Option<()> {
    match node {
        Node::Occur(occur) if occurrence.is_none() => {
            collect_array_pattern(&occur.node, rules, visiting, Some(occur.limits()), patterns)
        }
        Node::KeyValue(pair) => {
            let (minimum, maximum) = occurrence.unwrap_or((1, 1));
            patterns.push(ArrayPattern {
                node: (*pair.value).clone(),
                minimum,
                maximum,
            });
            Some(())
        }
        Node::Group(group) if occurrence.is_none() => {
            for member in &group.members {
                collect_array_pattern(member, rules, visiting, None, patterns)?;
            }
            Some(())
        }
        Node::Rule(rule) if rule.generic_args.is_empty() => {
            if let Some(definition) = rules.get(&rule.name) {
                if definition.generic_parms.is_empty()
                    && matches!(
                        definition.node,
                        Node::Group(_) | Node::Occur(_) | Node::KeyValue(_)
                    )
                    && visiting.insert(rule.name.clone())
                {
                    let result = collect_array_pattern(
                        &definition.node,
                        rules,
                        visiting,
                        occurrence,
                        patterns,
                    );
                    visiting.remove(&rule.name);
                    return result;
                }
            }
            let (minimum, maximum) = occurrence.unwrap_or((1, 1));
            patterns.push(ArrayPattern {
                node: node.clone(),
                minimum,
                maximum,
            });
            Some(())
        }
        _ => {
            let (minimum, maximum) = occurrence.unwrap_or((1, 1));
            patterns.push(ArrayPattern {
                node: node.clone(),
                minimum,
                maximum,
            });
            Some(())
        }
    }
}

fn key_matches(node: &Node, value: &CanonicalValue, context: &BasicContext) -> Option<bool> {
    canonical_key_node(value)?;
    let bytes = encode_canonical_cbor(value).ok()?;
    let cbor_value: ciborium::Value = ciborium::from_reader(bytes.as_slice()).ok()?;
    let rule = RuleDef {
        generic_parms: Vec::new(),
        node: node.clone(),
    };
    Some(validate_cbor(&rule, &cbor_value, context).is_ok())
}

fn literal_matches(node: &Node, value: &CanonicalValue, rules: &RulesByName) -> bool {
    matches!(terminal_node(node, rules), Some(Node::Literal(literal)) if literal_matches_value(literal, value))
}

fn literal_matches_value(literal: &Literal, value: &CanonicalValue) -> bool {
    match (literal, value) {
        (Literal::Bool(left), CanonicalValue::Bool(right)) => left == right,
        (Literal::Int(left), CanonicalValue::Integer(right)) => left == right,
        (Literal::Text(left), CanonicalValue::Text(right)) => left == right,
        (Literal::Bytes(left), CanonicalValue::Bytes(right)) => left == right,
        _ => false,
    }
}

fn canonical_key_node(value: &CanonicalValue) -> Option<Node> {
    match value {
        CanonicalValue::Null => Some(Node::PreludeType(PreludeType::Nil)),
        CanonicalValue::Bool(value) => Some(Node::Literal(Literal::Bool(*value))),
        CanonicalValue::Integer(value) => Some(Node::Literal(Literal::Int(*value))),
        CanonicalValue::Bytes(value) => Some(Node::Literal(Literal::Bytes(value.clone()))),
        CanonicalValue::Text(value) => Some(Node::Literal(Literal::Text(value.clone()))),
        CanonicalValue::Array(_) | CanonicalValue::Map(_) => None,
    }
}

fn inline_scalar_node(node: &Node, rules: &RulesByName) -> Option<Node> {
    match node {
        Node::Rule(rule) if rule.generic_args.is_empty() => {
            let definition = rules.get(&rule.name)?;
            definition
                .generic_parms
                .is_empty()
                .then(|| inline_scalar_node(&definition.node, rules))?
        }
        Node::Range(range) => {
            let mut range = range.clone();
            range.start = Box::new(inline_scalar_node(&range.start, rules)?);
            range.end = Box::new(inline_scalar_node(&range.end, rules)?);
            Some(Node::Range(range))
        }
        Node::Literal(_) | Node::PreludeType(_) => Some(node.clone()),
        _ => None,
    }
}

fn inline_control(control: &Control, rules: &RulesByName) -> Option<Control> {
    match control {
        Control::Size(value) => {
            let mut value = value.clone();
            value.target = Box::new(inline_scalar_node(&value.target, rules)?);
            value.size = Box::new(inline_scalar_node(&value.size, rules)?);
            Some(Control::Size(value))
        }
        Control::Lt(value) => {
            let mut value = value.clone();
            value.target = Box::new(inline_scalar_node(&value.target, rules)?);
            value.lt = Box::new(inline_scalar_node(&value.lt, rules)?);
            Some(Control::Lt(value))
        }
        Control::Le(value) => {
            let mut value = value.clone();
            value.target = Box::new(inline_scalar_node(&value.target, rules)?);
            value.le = Box::new(inline_scalar_node(&value.le, rules)?);
            Some(Control::Le(value))
        }
        Control::Gt(value) => {
            let mut value = value.clone();
            value.target = Box::new(inline_scalar_node(&value.target, rules)?);
            value.gt = Box::new(inline_scalar_node(&value.gt, rules)?);
            Some(Control::Gt(value))
        }
        Control::Ge(value) => {
            let mut value = value.clone();
            value.target = Box::new(inline_scalar_node(&value.target, rules)?);
            value.ge = Box::new(inline_scalar_node(&value.ge, rules)?);
            Some(Control::Ge(value))
        }
        Control::Regexp(value) => Some(Control::Regexp(value.clone())),
        _ => None,
    }
}

fn contains_rule_reference(node: &Node) -> bool {
    match node {
        Node::Rule(_) | Node::Unwrap(_) | Node::Choiceify(_) => true,
        Node::Choice(choice) => choice.options.iter().any(contains_rule_reference),
        Node::Map(map) => map.members.iter().any(contains_rule_reference),
        Node::Array(array) | Node::ChoiceifyInline(array) => {
            array.members.iter().any(contains_rule_reference)
        }
        Node::Group(group) => group.members.iter().any(contains_rule_reference),
        Node::KeyValue(pair) => {
            contains_rule_reference(&pair.key) || contains_rule_reference(&pair.value)
        }
        Node::Occur(occur) => contains_rule_reference(&occur.node),
        Node::Range(range) => {
            contains_rule_reference(&range.start) || contains_rule_reference(&range.end)
        }
        Node::Control(control) => match control {
            Control::Size(value) => {
                contains_rule_reference(&value.target) || contains_rule_reference(&value.size)
            }
            Control::Lt(value) => {
                contains_rule_reference(&value.target) || contains_rule_reference(&value.lt)
            }
            Control::Le(value) => {
                contains_rule_reference(&value.target) || contains_rule_reference(&value.le)
            }
            Control::Gt(value) => {
                contains_rule_reference(&value.target) || contains_rule_reference(&value.gt)
            }
            Control::Ge(value) => {
                contains_rule_reference(&value.target) || contains_rule_reference(&value.ge)
            }
            Control::Regexp(_) => false,
            _ => true,
        },
        Node::Literal(_) | Node::PreludeType(_) => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cddl_cat::flatten::flatten_from_str;

    #[test]
    fn shared_discriminator_rules_do_not_expand_exponentially() {
        use std::fmt::Write;

        const LAYERS: usize = 40;
        let mut schema = format!("layer{LAYERS} = {{kind: \"leaf\"}}\n");
        for index in 0..LAYERS {
            let next = index + 1;
            writeln!(schema, "layer{index} = layer{next} / layer{next}").expect("write schema");
        }
        let rules = flatten_from_str(&schema).expect("shared schema compiles");
        let mut cache = BTreeMap::new();
        let required = required_text_literals_cached(&rules["layer0"].node, &rules, 0, &mut cache);
        assert_eq!(required, Some(vec![("kind", "leaf")]));
        assert_eq!(
            cache.len(),
            LAYERS,
            "each shared rule/depth pair is computed once"
        );
    }

    const TAGGED_RECURSION: &str = r#"
        root = leaf / left / right
        leaf = { kind: "leaf", value: uint }
        left = { kind: "left", a: root }
        right = { kind: "right", payload: root }
    "#;

    fn map(entries: &[(&str, CanonicalValue)]) -> CanonicalValue {
        CanonicalValue::Map(
            entries
                .iter()
                .map(|(key, value)| (CanonicalValue::Text((*key).to_owned()), value.clone()))
                .collect(),
        )
    }

    #[test]
    fn specialization_selects_one_arm_and_eliminates_rule_reentry() {
        let rules = flatten_from_str(TAGGED_RECURSION).expect("test CDDL compiles");
        let context = BasicContext::new(rules);
        assert_eq!(
            root_dispatch_requirement(&context.rules, "root"),
            Some(RecursiveDispatchRequirement::Specialized)
        );
        let value = map(&[
            (
                "a",
                map(&[
                    ("kind", CanonicalValue::Text("leaf".to_owned())),
                    ("value", CanonicalValue::Integer(1)),
                ]),
            ),
            ("kind", CanonicalValue::Text("left".to_owned())),
        ]);

        let specialized = specialize_root_for_value(&context, "root", &value)
            .expect("known discriminator specializes exactly one arm");
        assert!(!contains_rule_reference(&specialized.node));
        let rendering = format!("{:?}", specialized.node);
        assert!(rendering.contains("left"));
        assert!(rendering.contains("leaf"));
        assert!(!rendering.contains("right"));
        assert!(!rendering.contains("payload"));
    }

    #[test]
    fn unknown_tag_does_not_specialize_a_recursive_child() {
        let rules = flatten_from_str(TAGGED_RECURSION).expect("test CDDL compiles");
        let context = BasicContext::new(rules);
        let deep = (0..PROVIDER_SCHEMA_VALIDATION_MAX_NESTING_DEPTH + 10)
            .fold(CanonicalValue::Text("unexamined".to_owned()), |value, _| {
                CanonicalValue::Array(vec![value])
            });
        let unknown = map(&[
            ("a", deep.clone()),
            ("kind", CanonicalValue::Text("unknown".to_owned())),
        ]);
        let mut unknown_specializer = Specializer {
            context: &context,
            active: BTreeMap::from([("root".to_owned(), 0)]),
            values_visited: 0,
        };
        assert!(unknown_specializer
            .specialize_value(&context.rules["root"].node, &unknown, 0)
            .is_none());
        assert_eq!(
            unknown_specializer.values_visited, 1,
            "unknown dispatch must inspect only the choice value"
        );

        let mismatching_visits = |child| {
            let mismatching = map(&[
                ("a", child),
                ("kind", CanonicalValue::Text("right".to_owned())),
            ]);
            let mut specializer = Specializer {
                context: &context,
                active: BTreeMap::from([("root".to_owned(), 0)]),
                values_visited: 0,
            };
            assert!(specializer
                .specialize_value(&context.rules["root"].node, &mismatching, 0)
                .is_none());
            specializer.values_visited
        };
        let shallow_visits =
            mismatching_visits(CanonicalValue::Text("shallow-unexamined".to_owned()));
        let deep_visits = mismatching_visits(deep);
        assert_eq!(
            deep_visits, shallow_visits,
            "unmatched child depth must not change specialization work"
        );
        assert_eq!(
            deep_visits, 3,
            "known dispatch may inspect the selected map but not its unmatched child"
        );
    }
}

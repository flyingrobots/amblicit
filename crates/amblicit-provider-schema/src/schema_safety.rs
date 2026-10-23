use std::collections::{BTreeMap, BTreeSet};

use cddl_cat::ivt::{Control, Literal, Node, PreludeType, Range, Rule, RuleDef};

// cddl-cat validation runs in native host code, outside Wasmtime fuel. Admit a
// conservative IVT subset whose rule graph terminates and whose repeating
// container members are guaranteed to consume an input element.

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Position {
    Value,
    ArrayMember,
    MapMember,
}

pub(crate) fn root_supports_total_validation(
    rules: &BTreeMap<String, RuleDef>,
    root: &str,
) -> bool {
    let Some(rule) = rules.get(root) else {
        return false;
    };
    if !rule.generic_parms.is_empty() {
        return false;
    }
    // A recursive back-edge is safe only after validation has moved from the
    // current value into a proper CBOR child value.
    let mut visiting = BTreeMap::from([((root.to_owned(), Position::Value), 0)]);
    node_is_supported(&rule.node, Position::Value, rules, &mut visiting, 0)
}

fn node_is_supported(
    node: &Node,
    position: Position,
    rules: &BTreeMap<String, RuleDef>,
    visiting: &mut BTreeMap<(String, Position), usize>,
    input_depth: usize,
) -> bool {
    match position {
        Position::Value => value_is_supported(node, rules, visiting, input_depth),
        Position::ArrayMember => array_member_is_supported(node, rules, visiting, input_depth),
        Position::MapMember => map_member_is_supported(node, rules, visiting, input_depth),
    }
}

fn value_is_supported(
    node: &Node,
    rules: &BTreeMap<String, RuleDef>,
    visiting: &mut BTreeMap<(String, Position), usize>,
    input_depth: usize,
) -> bool {
    match node {
        Node::Literal(_) | Node::PreludeType(_) => true,
        Node::Rule(rule) => rule_is_supported(rule, Position::Value, rules, visiting, input_depth),
        Node::Choice(choice) => {
            !choice.options.is_empty()
                && choice
                    .options
                    .iter()
                    .all(|node| value_is_supported(node, rules, visiting, input_depth))
        }
        Node::Map(map) => map
            .members
            .iter()
            .all(|node| map_member_is_supported(node, rules, visiting, input_depth)),
        Node::Array(array) => array
            .members
            .iter()
            .all(|node| array_member_is_supported(node, rules, visiting, input_depth)),
        Node::Group(group) => {
            group.members.len() == 1
                && value_is_supported(&group.members[0], rules, visiting, input_depth)
        }
        Node::Range(range) => range_is_supported(range, rules),
        Node::Control(control) => control_is_supported(control, rules),
        Node::KeyValue(_)
        | Node::Occur(_)
        | Node::Unwrap(_)
        | Node::Choiceify(_)
        | Node::ChoiceifyInline(_) => false,
    }
}

fn array_member_is_supported(
    node: &Node,
    rules: &BTreeMap<String, RuleDef>,
    visiting: &mut BTreeMap<(String, Position), usize>,
    input_depth: usize,
) -> bool {
    match node {
        Node::Occur(occur) => {
            array_member_is_supported(&occur.node, rules, visiting, input_depth)
                && (occur.limits().1 <= 1
                    || guarantees_progress(&occur.node, Position::ArrayMember, rules))
        }
        Node::KeyValue(pair) => {
            value_is_supported(&pair.key, rules, visiting, input_depth)
                && value_is_supported(&pair.value, rules, visiting, input_depth.saturating_add(1))
        }
        Node::Rule(rule) => {
            rule_is_supported(rule, Position::ArrayMember, rules, visiting, input_depth)
        }
        Node::Choice(choice) => {
            !choice.options.is_empty()
                && choice
                    .options
                    .iter()
                    .all(|node| array_member_is_supported(node, rules, visiting, input_depth))
        }
        Node::Group(group) => group
            .members
            .iter()
            .all(|node| array_member_is_supported(node, rules, visiting, input_depth)),
        Node::Unwrap(_) | Node::Choiceify(_) | Node::ChoiceifyInline(_) => false,
        _ => value_is_supported(node, rules, visiting, input_depth.saturating_add(1)),
    }
}

fn map_member_is_supported(
    node: &Node,
    rules: &BTreeMap<String, RuleDef>,
    visiting: &mut BTreeMap<(String, Position), usize>,
    input_depth: usize,
) -> bool {
    match node {
        Node::Occur(occur) => {
            map_member_is_supported(&occur.node, rules, visiting, input_depth)
                && (occur.limits().1 <= 1
                    || guarantees_progress(&occur.node, Position::MapMember, rules))
        }
        Node::KeyValue(pair) => {
            let child_depth = input_depth.saturating_add(1);
            value_is_supported(&pair.key, rules, visiting, child_depth)
                && value_is_supported(&pair.value, rules, visiting, child_depth)
        }
        Node::Rule(rule) => {
            rule_is_supported(rule, Position::MapMember, rules, visiting, input_depth)
        }
        Node::Choice(choice) => {
            !choice.options.is_empty()
                && choice
                    .options
                    .iter()
                    .all(|node| map_member_is_supported(node, rules, visiting, input_depth))
        }
        Node::Group(group) => group
            .members
            .iter()
            .all(|node| map_member_is_supported(node, rules, visiting, input_depth)),
        Node::Unwrap(_)
        | Node::Choiceify(_)
        | Node::ChoiceifyInline(_)
        | Node::Literal(_)
        | Node::PreludeType(_)
        | Node::Map(_)
        | Node::Array(_)
        | Node::Range(_)
        | Node::Control(_) => false,
    }
}

fn rule_is_supported(
    rule: &Rule,
    position: Position,
    rules: &BTreeMap<String, RuleDef>,
    visiting: &mut BTreeMap<(String, Position), usize>,
    input_depth: usize,
) -> bool {
    if !rule.generic_args.is_empty() {
        return false;
    }
    let Some(definition) = rules.get(&rule.name) else {
        return false;
    };
    if !definition.generic_parms.is_empty() {
        return false;
    }
    let key = (rule.name.clone(), position);
    if let Some(active_depth) = visiting.get(&key) {
        // Strictly greater depth witnesses structural descent; equal depth is
        // an alias/combinator cycle that could revisit the same input forever.
        return input_depth > *active_depth;
    }
    visiting.insert(key.clone(), input_depth);
    let supported = node_is_supported(&definition.node, position, rules, visiting, input_depth);
    visiting.remove(&key);
    supported
}

fn guarantees_progress(node: &Node, position: Position, rules: &BTreeMap<String, RuleDef>) -> bool {
    let mut visiting = BTreeSet::new();
    progress_inner(node, position, rules, &mut visiting)
}

fn progress_inner(
    node: &Node,
    position: Position,
    rules: &BTreeMap<String, RuleDef>,
    visiting: &mut BTreeSet<(String, Position)>,
) -> bool {
    match (position, node) {
        (_, Node::Rule(rule)) => {
            let key = (rule.name.clone(), position);
            if !rule.generic_args.is_empty() || !visiting.insert(key.clone()) {
                return false;
            }
            let progress = rules.get(&rule.name).is_some_and(|definition| {
                definition.generic_parms.is_empty()
                    && progress_inner(&definition.node, position, rules, visiting)
            });
            visiting.remove(&key);
            progress
        }
        (position @ (Position::ArrayMember | Position::MapMember), Node::Choice(choice)) => {
            !choice.options.is_empty()
                && choice
                    .options
                    .iter()
                    .all(|node| progress_inner(node, position, rules, visiting))
        }
        (position @ (Position::ArrayMember | Position::MapMember), Node::Group(group)) => group
            .members
            .iter()
            .any(|node| progress_inner(node, position, rules, visiting)),
        (position @ (Position::ArrayMember | Position::MapMember), Node::Occur(occur)) => {
            occur.limits().0 > 0 && progress_inner(&occur.node, position, rules, visiting)
        }
        (Position::MapMember, Node::KeyValue(_)) | (Position::ArrayMember, _) => true,
        (Position::MapMember | Position::Value, _) => false,
    }
}

fn range_is_supported(range: &Range, rules: &BTreeMap<String, RuleDef>) -> bool {
    matches!(
        (
            terminal_node(&range.start, rules),
            terminal_node(&range.end, rules)
        ),
        (
            Some(Node::Literal(Literal::Int(_))),
            Some(Node::Literal(Literal::Int(_)))
        ) | (
            Some(Node::Literal(Literal::Float(_))),
            Some(Node::Literal(Literal::Float(_)))
        )
    )
}

fn control_is_supported(control: &Control, rules: &BTreeMap<String, RuleDef>) -> bool {
    match control {
        Control::Size(value) => {
            matches!(
                terminal_node(&value.target, rules),
                Some(Node::PreludeType(
                    PreludeType::Uint | PreludeType::Tstr | PreludeType::Bstr
                ))
            ) && size_limit_is_supported(&value.size, rules)
        }
        Control::Lt(value) => numeric_control_is_supported(&value.target, &value.lt, rules),
        Control::Le(value) => numeric_control_is_supported(&value.target, &value.le, rules),
        Control::Gt(value) => numeric_control_is_supported(&value.target, &value.gt, rules),
        Control::Ge(value) => numeric_control_is_supported(&value.target, &value.ge, rules),
        Control::Regexp(_) => true,
        _ => false,
    }
}

fn numeric_control_is_supported(
    target: &Node,
    limit: &Node,
    rules: &BTreeMap<String, RuleDef>,
) -> bool {
    matches!(
        terminal_node(target, rules),
        Some(Node::PreludeType(
            PreludeType::Int | PreludeType::Uint | PreludeType::Nint
        ))
    ) && matches!(
        terminal_node(limit, rules),
        Some(Node::Literal(Literal::Int(_)))
    )
}

fn size_limit_is_supported(limit: &Node, rules: &BTreeMap<String, RuleDef>) -> bool {
    match terminal_node(limit, rules) {
        Some(Node::Literal(Literal::Int(value))) => *value >= 0,
        Some(Node::Range(range)) => matches!(
            (terminal_node(&range.start, rules), terminal_node(&range.end, rules)),
            (
                Some(Node::Literal(Literal::Int(start))),
                Some(Node::Literal(Literal::Int(end)))
            ) if *start >= 0 && *end >= 0
        ),
        _ => false,
    }
}

fn terminal_node<'a>(node: &'a Node, rules: &'a BTreeMap<String, RuleDef>) -> Option<&'a Node> {
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

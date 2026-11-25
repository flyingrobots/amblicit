//! Generates the catalog v3 cost-expression rules of
//! `docs/abi/edict-target-profile.cddl`.
//!
//! The provider artifact-schema registry refuses schemas whose recursive
//! choices it cannot dispatch to one arm per value, because `cddl-cat`
//! validates natively, outside Wasmtime fuel. Cost expressions and types
//! nest, so the schema writes the nesting out level by level, one rule family
//! per level, to `TARGET_EFFECT_COST_MAX_DEPTH`, the limit the decoder
//! enforces. No rule refers to itself
//! or to a shallower level, so the rule graph is acyclic.

use std::fmt::Write as _;

use amblicit_syntax::{TARGET_EFFECT_COST_MAX_DEPTH, TARGET_EFFECT_COST_MAX_SUM_TERMS};

pub(crate) const BEGIN_MARKER: &str =
    "; BEGIN GENERATED effect-cost levels (cargo xtask provider-contract-pack --write)\n";
pub(crate) const END_MARKER: &str = "; END GENERATED effect-cost levels\n";

/// The generated block, markers included.
pub(crate) fn effect_cost_levels_cddl() -> String {
    let depth = TARGET_EFFECT_COST_MAX_DEPTH;
    let mut out = String::from(BEGIN_MARKER);
    let _ = write!(
        out,
        "; Do not edit. Level n is the decoder's nesting depth: steps and bytes are\n\
         ; level 0 and each nested expression, measure, type or record field is one\n\
         ; level deeper, up to {depth}. A deeper value has no rule here.\n"
    );
    for level in 0..=depth {
        let mut arms = vec!["{ const: uint }".to_owned()];
        if level < depth {
            let next = level + 1;
            arms.push(format!(
                "{{ sum: [2*{TARGET_EFFECT_COST_MAX_SUM_TERMS} effect-cost-expr-{next}] }}"
            ));
            arms.push(format!("{{ product: [2*2 effect-cost-expr-{next}] }}"));
            arms.push(format!("{{ measure: effect-cost-measure-{next} }}"));
        }
        push_rule(&mut out, &format!("effect-cost-expr-{level}"), &arms);
    }
    for level in 1..=depth {
        let mut arms = Vec::new();
        if level < depth {
            let ty = format!("effect-cost-type-{}", level + 1);
            arms.push(format!("{{ bytesMax: {ty} }}"));
            arms.push(format!("{{ listMax: {ty} }}"));
            arms.push(format!("{{ validations: {ty} }}"));
            arms.push(format!("{{ copy: {ty}, of: effect-cost-half }}"));
        }
        arms.push("{ parameter: nonempty-text }".to_owned());
        arms.push("{ helper: { parameter: nonempty-text }, of: effect-cost-half }".to_owned());
        arms.push("{ costliestHelper: effect-cost-half }".to_owned());
        push_rule(&mut out, &format!("effect-cost-measure-{level}"), &arms);
    }
    for level in 2..=depth {
        let record = if level < depth {
            format!(
                "{{ record: {{ * tstr => effect-cost-type-{} }} }}",
                level + 1
            )
        } else {
            "{ record: {} }".to_owned()
        };
        let arms = [
            "{ path: effect-cost-path }".to_owned(),
            "{ word: null }".to_owned(),
            record,
        ];
        push_rule(&mut out, &format!("effect-cost-type-{level}"), &arms);
    }
    out.push_str(END_MARKER);
    out
}

fn push_rule(out: &mut String, name: &str, arms: &[String]) {
    let indent = " ".repeat(name.len() + 1);
    for (index, arm) in arms.iter().enumerate() {
        if index == 0 {
            let _ = writeln!(out, "{name} = {arm}");
        } else {
            let _ = writeln!(out, "{indent}/ {arm}");
        }
    }
}

/// `source` with its generated block replaced by the current generation.
///
/// # Errors
///
/// Returns an error when `source` does not hold exactly one marked block.
pub(crate) fn splice_effect_cost_levels(source: &str) -> Result<String, String> {
    let (Some(begin), 1) = (
        source.find(BEGIN_MARKER),
        source.matches(BEGIN_MARKER).count(),
    ) else {
        return Err(
            "edict-target-profile.cddl must hold exactly one generated effect-cost block"
                .to_owned(),
        );
    };
    let Some(end_offset) = source[begin..].find(END_MARKER) else {
        return Err("generated effect-cost block has no end marker".to_owned());
    };
    let end = begin + end_offset + END_MARKER.len();
    Ok(format!(
        "{}{}{}",
        &source[..begin],
        effect_cost_levels_cddl(),
        &source[end..]
    ))
}

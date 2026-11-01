//! Source work admission must cover native Target's independently derived bound.
use amblicit_syntax::{
    compile_to_core, lower_to_target_ir, parse_module, CompilerContext, CompilerErrorKind,
    CoreBudget, ResourceRef, TargetEffectLowering, TargetIrLoweringFacts,
    TargetLoweringFailureKind, TargetLoweringStatus, WriteClass,
};

const CEILING: u64 = 1_000_000;

fn minimum(mut accepts: impl FnMut(u64) -> bool) -> u64 {
    assert!(accepts(CEILING), "generous-budget control");
    let (mut low, mut high) = (0, CEILING);
    while low < high {
        let middle = low + (high - low) / 2;
        if accepts(middle) {
            high = middle;
        } else {
            low = middle + 1;
        }
    }
    assert!(accepts(high));
    assert!(high == 0 || !accepts(high - 1));
    high
}

fn assert_source_covers_native(source: &str) {
    let parsed = parse_module(&format!("package work.budgets@1; {source}")).unwrap();
    let facts = TargetIrLoweringFacts {
        target_profile: ResourceRef {
            coordinate: "echo.dpo@1".into(),
            digest: Some(format!("sha256:{}", "1".repeat(64))),
        },
        target_ir_domain: "echo.span-ir/v3".into(),
        operation_profiles: vec![
            "continuum.profile.read-only/v1".into(),
            "continuum.profile.write/v1".into(),
        ],
        obstruction_coordinates: vec!["example.No".into(), "rejected".into()],
        effect_lowerings: vec![TargetEffectLowering {
            effect: "target.replace".into(),
            target_intrinsic: "echo.dpo@1.replace".into(),
            failure_mappings: std::collections::BTreeMap::new(),
        }],
        effect_signatures: Vec::new(),
        pure_functions: Vec::new(),
    };
    for steps_axis in [true, false] {
        let budget = |value| CoreBudget {
            max_steps: if steps_axis { value } else { CEILING },
            max_allocated_bytes: if steps_axis { CEILING } else { value },
            max_output_bytes: CEILING,
        };
        let compile = |value| {
            compile_to_core(
                &parsed,
                &CompilerContext::new()
                    .with_operation_profile("p.read", "continuum.profile.read-only/v1")
                    .with_operation_profile("p.write", "continuum.profile.write/v1")
                    .with_operation_profile_write_classes("p.write", [WriteClass::Replace])
                    .with_effect_write_class("target.replace", WriteClass::Replace)
                    .with_budget("p.large", budget(value)),
            )
        };
        let control = compile(CEILING).expect("source setup");
        let report = lower_to_target_ir(&control, &facts);
        assert_eq!(
            report.status,
            TargetLoweringStatus::Lowered,
            "native admission of supported Core: {source}: {report:?}"
        );
        let source_min = minimum(|value| match compile(value) {
            Ok(_) => true,
            Err(errors) => {
                assert!(
                    errors
                        .iter()
                        .all(|e| e.kind == CompilerErrorKind::InvalidBound),
                    "{errors:?}"
                );
                false
            }
        });
        let native_min = minimum(|value| {
            let mut core = control.clone();
            core.intents.get_mut("run").unwrap().core_evaluation_budget = budget(value);
            lower_to_target_ir(&core, &facts).status == TargetLoweringStatus::Lowered
        });
        assert!(
            source_min >= native_min,
            "source {source_min} < native {native_min}, steps={steps_axis}: {source}"
        );
        let mut too_small = control.clone();
        too_small
            .intents
            .get_mut("run")
            .unwrap()
            .core_evaluation_budget = budget(native_min - 1);
        let refused = lower_to_target_ir(&too_small, &facts);
        assert_eq!(refused.status, TargetLoweringStatus::Unsupported);
        assert!(refused.artifact.is_none());
        assert!(refused
            .failures
            .iter()
            .any(|failure| failure.kind == TargetLoweringFailureKind::InvalidCoreIdentity));
        let core = compile(source_min).unwrap();
        let report = lower_to_target_ir(&core, &facts);
        assert_eq!(report.status, TargetLoweringStatus::Lowered, "{report:?}");
    }
}

#[test]
fn obstruction_payload_shorthand_covers_the_emitted_local_read() {
    for payload in ["copied: copied", "copied"] {
        for requirement in [
            format!("require true else example.No({{ {payload} }});"),
            format!("require true else continue obstructed {{ reason: example.No, {payload} }};"),
        ] {
            assert_source_covers_native(&format!(
                "type Input = {{ xs: List<U8,max=2> }}; type Output = {{ n: U64 }};
              intent run(input: Input) returns Output profile p.read basis none budget <= p.large {{
                let copied = input.xs; {requirement} let n = listLen(input.xs); return {{ n }};
              }}"
            ));
        }
    }
}

const MATCH_TYPES: &str = "type Item = variant { Value(U8), Empty }; type Input = { xs: List<U8,max=2>, item: Item }; type Output = { n: U64 };";
const MATCH: &str = "match input.item { Value(value) => value, Empty => 0u8 }";

#[test]
fn reified_boolean_values_include_selected_constants() {
    for predicate in [
        "input.flag == true",
        "input.flag != false",
        "!input.flag",
        "input.flag && true",
        "input.flag || false",
        "input.count < 3u64",
        "!(input.flag && input.count <= 3u64)",
    ] {
        assert_source_covers_native(&format!(
            "type Input = {{ xs: List<U8,max=2>, flag: Bool, count: U64 }};
             type Output = {{ n: U64 }};
             intent run(input: Input) returns Output profile p.read basis none budget <= p.large {{
                let selected = {predicate}; let n = listLen(input.xs); return {{ n }};
             }}"
        ));
    }
}

#[test]
fn projected_lists_retain_coordinates_and_storage_bounds() {
    for item in ["U8", "Item", "List<Item,max=1>"] {
        for projection in [
            "input.xs",
            "(if input.flag then { items: input.xs } else { items: input.ys }).items",
            "(match input.choice { A => { items: input.xs }, B => { items: input.ys } }).items",
            "(if input.flag then { outer: { items: input.xs } } else { outer: { items: input.ys } }).outer.items",
        ] {
            assert_source_covers_native(&format!(
                "type Item = {{ x: U8 }}; type Choice = variant {{ A, B }};
                 type Items = List<{item},max=2>;
                 type Input = {{ xs: Items, ys: Items, flag: Bool, choice: Choice }};
                 type Output = {{ n: U64 }};
                 intent run(input: Input) returns Output profile p.read basis none budget <= p.large {{
                    let n = listLen({projection}); return {{ n }};
                 }}"
            ));
        }
    }
}

#[test]
fn payload_matches_charge_complete_frames_and_nested_scopes() {
    for statements in [
        "let selected = match input.item { Value => 1u8, Empty => 0u8 };".to_owned(),
        format!("let selected = {MATCH};"),
        format!("let selected = {MATCH}; let later = input.xs;"),
        format!("for item in input.xs bounded 2 {{ let selected = {MATCH}; }}"),
        format!("if true {{ let selected = {MATCH}; }}"),
        "let selected = match input.item { Value(a) => match input.item { Value(b) => b, Empty => a }, Empty => 0u8 };".to_owned(),
    ] {
        assert_source_covers_native(&format!("{MATCH_TYPES}
          intent run(input: Input) returns Output profile p.read basis none budget <= p.large {{
            {statements} let n = listLen(input.xs); return {{ n }};
          }}"));
    }
}

#[test]
fn helper_and_fold_payload_matches_retain_frame_work() {
    for expression in ["step(input.item)", "fold(input.item, step, 2u64)"] {
        assert_source_covers_native(&format!(
            "{MATCH_TYPES}
          fn step(item: Item) -> Item {{
            let value = match item {{ Value(payload) => payload, Empty => 0u8 }};
            return Item::Value(value);
          }}
          intent run(input: Input) returns Output profile p.read basis none budget <= p.large {{
            let selected = {expression}; let n = listLen(input.xs); return {{ n }};
          }}"
        ));
    }
}

#[test]
fn effect_frames_and_obstructions_are_charged_in_list_capable_intents() {
    let effect =
        "let receipt: Receipt = target.replace(input.xs) else { rejected(reason) => example.No };";
    for statements in [
        effect.to_owned(),
        format!("{effect} let other: Receipt = target.replace(input.xs) else {{ rejected(again) => example.No }};"),
        format!("for item in input.xs bounded 2 {{ {effect} }}"),
        format!("{effect} let selected = {MATCH};"),
    ] {
        assert_source_covers_native(&format!("{MATCH_TYPES} type Receipt = {{ value: U8 }};
          intent run(input: Input) returns Output profile p.write basis none budget <= p.large {{
            {statements} let n = listLen(input.xs); return {{ n }};
          }}"));
    }
}

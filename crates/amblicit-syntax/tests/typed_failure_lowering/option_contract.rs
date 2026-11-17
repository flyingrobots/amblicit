//! Terminal typing does not extend the refined-local source contract for unwrap.
use super::BASE;
use amblicit_syntax::{
    compile_to_core, parse_module, CompilerContext, CompilerErrorKind, CoreBudget,
};

#[test]
fn terminal_failure_does_not_bypass_the_source_unwrap_operand_contract() {
    // EDICT-LANG-OPTION-REFINE-001: source unwrap takes an exactly refined
    // local, and does not accept a source type argument.
    for (expression, diagnostic) in [
        (
            "unwrap(fail ClosureTooSmall)",
            "unwrap requires a local variable refined by isSome",
        ),
        (
            "unwrap<U64>(fail ClosureTooSmall)",
            "unwrap takes 1 value argument(s) and at most 0 type argument(s)",
        ),
    ] {
        let helper = format!(
            "fn invalid() -> U64 fails ClosureTooSmall {{ return {expression}; }}\nintent evaluate"
        );
        let source = BASE.replace("intent evaluate", &helper);
        let context = CompilerContext::new()
            .with_operation_profile("p.read", "continuum.profile.read-only/v1")
            .with_budget(
                "p.small",
                CoreBudget {
                    max_steps: 100_000,
                    max_allocated_bytes: u64::MAX,
                    max_output_bytes: u64::MAX,
                },
            );
        let module = parse_module(&source).expect("unwrap/fail source parses");
        let errors = compile_to_core(&module, &context)
            .expect_err("terminal typing does not widen source unwrap admission");
        assert!(
            errors.iter().any(|error| {
                error.kind == CompilerErrorKind::UnsupportedSourceShape
                    && error.message == diagnostic
            }),
            "{expression}: {errors:#?}"
        );
    }
}

//! Shared helpers for the parser integration tests.
//!
//! Lives in a `common/` subdirectory so Cargo treats it as a module included by
//! each test binary (`mod common;`) rather than its own test target. Not every
//! test uses every helper, hence the crate-level `dead_code` allowance.
#![allow(dead_code)]

use edict_syntax::ast::{Decl, IntentDecl, Module};
use edict_syntax::{compile_to_core, parse_module, CompilerContext, CoreBudget, CoreModule};

/// Shared source fixture for the initial pure local-record compiler slice.
pub const BOUNDED_HELLO: &str =
    include_str!("../../../../fixtures/lang/bounds/bounded-hello.edict");

/// Wrap a statement sequence in a minimal, well-formed intent body.
pub fn body(stmts: &str) -> String {
    format!(
        "package a.b@1;\n\
         intent t(input: shape.In) returns shape.Out basis none budget <= p.b {{\n\
         {stmts}\n\
         }}"
    )
}

/// Parse a source string, panicking with the parse error on failure.
pub fn parse_ok(src: &str) -> Module {
    parse_module(src).unwrap_or_else(|e| panic!("expected a parse, got error: {e}"))
}

/// Compiler facts required by the bounded-hello fixture.
pub fn hello_context() -> CompilerContext {
    CompilerContext::new()
        .with_operation_profile("hello.readOnly", "continuum.profile.read-only/v1")
        .with_budget(
            "hello.tinyBudget",
            CoreBudget {
                max_steps: 64,
                max_allocated_bytes: 4096,
                max_output_bytes: 4096,
            },
        )
}

/// Compile the bounded-hello source fixture to the current in-memory Core model.
pub fn bounded_hello_core() -> CoreModule {
    let module = parse_module(BOUNDED_HELLO).expect("bounded-hello fixture parses");
    compile_to_core(&module, &hello_context()).expect("bounded-hello fixture compiles to Core")
}

/// The first declaration as an intent.
pub fn intent_of(m: &Module) -> &IntentDecl {
    let Decl::Intent(intent) = &m.decls[0] else {
        panic!("decl 0 is an intent");
    };
    intent
}

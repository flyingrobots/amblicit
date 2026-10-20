# Semantic Validation Topic

Status: current HEAD contract.

This chapter describes the source/surface validation stage that exists today. It
is the first named compiler-spine stage after parsing:

```text
parse -> validate_surface -> resolve -> type_check -> lower_core -> canonicalize
```

This pass validates only source-AST constraints that do not require import
resolution, resolved typing, target profile data, lawpack ABI data, Core IR, or
canonicalization.

## Public Surface

The public validation entry point is `validate_surface`:

```rust
use edict_syntax::{parse_module, validate_surface, SemanticErrorKind};

let module = parse_module(
    "package examples.hello@1;\n\
     type HelloInput = { name: String, };\n",
)
.expect("source parses");

let errors = validate_surface(&module).expect_err("unbounded String rejects");
assert_eq!(errors[0].kind, SemanticErrorKind::UnboundedScalar);
```

`validate_module` remains a compatibility alias for the same stage, but new code
should call `validate_surface` so stage ownership stays explicit.
[SEMVAL-REQ-008]

Validation accepts a parsed source AST and returns either `Ok(())` or all
source-level semantic errors found by a deterministic source-AST traversal. Exact
error ordering is not part of the Phase 2 contract. Tests assert structured
`SemanticErrorKind` values, not message prose. [SEMVAL-REQ-001]

## Current Contract

- Runtime `String` and `Bytes` type references must carry explicit bounds. The
  pass checks nested type references recursively, including `Option`, `List`,
  `Map`, `CapabilityRef`, variant payloads, function/intent parameters and returns,
  typed `let` declarations, and expression type arguments. [SEMVAL-REQ-002]
- Every intent must declare at least one operation mode: `profile` or
  `implements`. An intent may declare both. [SEMVAL-REQ-003]
- Every intent must declare a `budget` clause. [SEMVAL-REQ-004]
- Every intent must declare a `basis` clause. This pass cannot yet resolve
  profile- or lawpack-supplied basis templates, so the current source-level
  contract requires an explicit source clause. [SEMVAL-REQ-005]
- Singleton intent clauses reject duplicates for `profile`, `implements`,
  `basis`, `footprint`, and `budget`. [SEMVAL-REQ-006]
- Module-scope import aliases, `type`, `enum`, `fn`, and `intent` declarations
  share a source-AST namespace and reject duplicate names.
  [SEMVAL-REQ-007]
- Source binders reject shadowing of visible module/prelude names, parameters,
  and earlier local binders. The check covers function and intent parameters, `let` binders,
  bounded-`for` binders, match-arm binders, obstruction-map binders, ordinary
  blocks, and branch-yield blocks. [SEMVAL-REQ-007]
- Branch, loop, match-arm, obstruction-map, and branch-yield scopes are
  deterministic and do not leak into sibling or outer scopes. [SEMVAL-REQ-007]
- The pass deliberately accepts unresolved import aliases, unresolved named
  types, unresolved callees, unresolved field paths, contextual integer
  mismatches, unproved loop cardinality, and incomplete obstruction maps when no
  source/surface rule is otherwise violated. Those checks belong to later
  compiler-spine stages. [SEMVAL-REQ-009] [SEMVAL-REQ-010]

Semantic errors carry source spans. Clause-level duplicate diagnostics currently
report the enclosing intent span because the parser's `IntentClause` AST does
not yet retain per-clause spans. [SEMVAL-REQ-001]

## Deferred

The following issue #10-adjacent checks are intentionally not part of
`validate_surface`:

- type checking and integer contextual-width validation;
- loop bound provability;
- target/lawpack failure taxonomy and obstruction exhaustiveness;
- read-only inference;
- full Core IR lowering, canonical artifacts, and downstream assurance relapse
  fixtures.

Those checks require contextual typing, cardinality proof machinery,
target/lawpack facts, or Core IR and are tracked by the roadmap issues for the
compiler spine, lowerability, and admission stages. They are not part of the
source/surface `validate_surface(&Module)` contract.

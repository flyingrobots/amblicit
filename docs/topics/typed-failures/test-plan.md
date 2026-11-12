# Typed Function Failures Test Plan

Status: verification ledger for declared failures on source functions,
propagation with `expect`, intent-level mapping with `else`, and their Core
and Target IR representation.

## Scope

In scope:

- parsing of `failure` declarations, `fails` clauses, and `fail` and `expect`
  expressions as contextual syntax;
- the source compiler's failure-set, propagation and mapping judgments;
- the Core representation and its independent integrity judgment;
- Target IR lowering of mapped calls and its execution-model boundary;
- the provider contract pack that publishes the new schema.

Out of scope:

- Echo's provider lowerer, verifier and runtime, which must adopt
  `core.failure/v1` before a failing program can run;
- failure payloads, mapping inside function bodies, and other features the
  Jedit rope drafts mark as proposed (function `where`, `decreases`, tuple
  patterns, wildcard arms).

## Requirements

| ID | Status | Requirement | Source |
| --- | --- | --- | --- |
| TFAIL-REQ-001 | planned | `failure Name;` declares a payload-free, module-owned failure whose coordinate is the declaring module coordinate plus its name, also under source linking; `fails` names resolve to declared failures with no repeats and no ambiguous bare names. | EDICT-LANG-FAIL-DECL-001 |
| TFAIL-REQ-002 | planned | A function's declared set equals exactly the failures its body produces with `fail` and `expect`; `fail` takes its context type and is legal only inside a function that declares it. | EDICT-LANG-FAIL-SET-001 |
| TFAIL-REQ-003 | planned | `expect` propagates a direct callee's whole failure set only inside a function that declares that set; every other failing call refuses with `UnhandledFailure`. | EDICT-LANG-FAIL-EXPECT-001 |
| TFAIL-REQ-004 | planned | An intent `let` whose whole value is a failing source call maps each callee failure exactly once to a terminal obstruction; one-failure shorthand only; missing, unknown, repeated and binding arms refuse. | EDICT-LANG-FAIL-MAP-001 |
| TFAIL-REQ-005 | planned | Core carries sorted function `failures`, `fail` expressions and `attempt` nodes and requires `core.failure/v1`. It omits them all when unused, so existing artifacts stay byte-identical, and its integrity judgment independently refuses forged sets, placements and maps. | EDICT-CORE-FAIL-001 |
| TFAIL-REQ-006 | planned | Target lowers an `attempt` to a pure binding with `failureArms` under the ordered and structured models; the pre-step-guard model refuses `failure_mapped_binding`. | EDICT-TARGET-FAIL-001 |

## Test Cases

| ID | Status | Category | Requirement | Oracle | Evidence | Fixtures | Notes |
| --- | --- | --- | --- | --- | --- | --- | --- |
| TFAIL-TP-001 | planned | Parser | TFAIL-REQ-001 | `failure`, `fails`, `fail` and `expect` parse into their AST forms in the positions the grammar gives them. | - | - | Grammar in the language spec. |
| TFAIL-TP-002 | planned | Contextual syntax | TFAIL-REQ-001 | The four words stay ordinary identifiers (locals, fields, effect failure names) everywhere else. | - | - | Existing programs keep compiling. |
| TFAIL-TP-003 | planned | Declarations | TFAIL-REQ-001, TFAIL-REQ-005 | Declared sets compile to sorted module-owned coordinates and add `core.failure/v1`. | - | - | Spec example coordinates. |
| TFAIL-TP-004 | planned | Declarations | TFAIL-REQ-001 | Unknown, repeated and ambiguous names report `UnknownFailure` or `UnsupportedSourceShape` at the offending span. | - | - | Map keys must identify one failure. |
| TFAIL-TP-005 | planned | Exact sets | TFAIL-REQ-002 | A body that produces exactly its declared failures through `fail` in returns, conditionals and matches compiles. | - | - | `fail` takes its context type. |
| TFAIL-TP-006 | planned | Exact sets | TFAIL-REQ-002 | Undeclared, unraised and context-free `fail` report `UndeclaredFailure`, `UnraisedFailure` and `TypeMismatch`. | - | - | The judgment is syntactic. |
| TFAIL-TP-007 | planned | Propagation | TFAIL-REQ-003 | `expect` in arguments, record fields and `let` values propagates the callee set. | - | - | Calls keep their ordinary Core form. |
| TFAIL-TP-008 | planned | Propagation | TFAIL-REQ-003 | A failing call without `expect`, `expect` in an intent, `expect` on a failure-free call, a widening `expect`, and a failing `fold` step refuse with their spec kinds. | - | - | Precise diagnostic kinds. |
| TFAIL-TP-009 | planned | Mapping | TFAIL-REQ-004 | A mapped intent `let` becomes an `attempt` node whose map has one terminal reason per callee failure. | - | - | Payloads may read earlier bindings. |
| TFAIL-TP-010 | planned | Mapping | TFAIL-REQ-004 | Incomplete, unknown, repeated, binding and shorthand-misuse maps refuse with their spec kinds. | - | - | Same arm syntax as effects. |
| TFAIL-TP-011 | planned | Linking | TFAIL-REQ-001, TFAIL-REQ-004 | A failure declared in an imported source module keeps its original coordinate and is mapped by its bare name. | - | - | Hygienic link names never reach Core. |
| TFAIL-TP-012 | planned | Core encoding | TFAIL-REQ-005 | Canonical Core carries `failures`, `fail` and `attempt` as specified, and a module without failures encodes as before. | - | - | Reviewed Core goldens stay byte-identical. |
| TFAIL-TP-013 | planned | Core integrity | TFAIL-REQ-005 | Forged Core (unsorted or inexact sets, misplaced `fail`, unmapped failing call, wrong map keys, failing fold step, missing capability) refuses `InvalidDefinition` at the exact path. | - | - | Independent of the source compiler. |
| TFAIL-TP-014 | planned | Target lowering | TFAIL-REQ-006 | Ordered and structured Target IR carry the mapped call as a pure binding with `failureArms`; existing Target goldens stay byte-identical. | - | - | Provider execution is out of scope. |
| TFAIL-TP-015 | planned | Target lowering | TFAIL-REQ-006 | The pre-step-guard model refuses `UnsupportedCoreNode` with detail `failure_mapped_binding`. | - | - | No silent ordering. |
| TFAIL-TP-016 | planned | Consumer reproducer | TFAIL-REQ-003, TFAIL-REQ-004 | The reconciled Jedit `pure-typed-failure` reproducer compiles to Core with one `attempt`. | - | - | Jedit's draft needs the edits listed in the README. |
| TFAIL-TP-017 | planned | Provider schema | TFAIL-REQ-005, TFAIL-REQ-006 | The new contract pack accepts failure-bearing Core and Target IR; the earlier pack refuses them and still reproduces from frozen schemas. | - | - | Schema acceptance is not runtime support. |
| TFAIL-TP-018 | gap | Runtime | TFAIL-REQ-005, TFAIL-REQ-006 | Echo executes a failing program and obstructs with the mapped reason. | - | - | Needs Echo support for `core.failure/v1`. |

# Typed Function Failures

A source function can declare the failures it may end with. A caller
propagates them with `expect` or maps them to obstructions with `else`. The
language specification defines the feature in
[Typed Function Failures](../../SPEC_edict-language-v1.md#typed-function-failures).
This page records what HEAD implements. The [test plan](./test-plan.md) lists
the evidence.

## Source

- `failure Name;` declares a payload-free failure. Its coordinate is the
  declaring module's coordinate plus the name, for example
  `jedit.rope@1.ClosureTooSmall`. Source linking keeps that coordinate.
- `fn f(...) -> T fails A, alias.B { ... }` declares the set. An unknown name
  reports `UnknownFailure`. A repeated or ambiguous entry (two failures with the
  same bare name) reports `UnsupportedSourceShape`. A repeated declaration
  reports the usual duplicate-name `SurfaceValidation` refusal.
- The set must equal what the body produces with `fail` and `expect`. An extra
  failure reports `UndeclaredFailure` and a missing one `UnraisedFailure`.
- `fail Name` takes its type from its context: a `return`, a conditional or
  `match` peer, or an annotated `let`. Without one it reports `TypeMismatch`.
- `expect f(...)` is legal only inside a function whose set covers the
  callee's set. In an intent it reports `UnhandledFailure`. On a call that
  declares no failures it reports `UnsupportedSourceShape`.
- Any other failing call, including one in a `require` predicate, a mapped
  call's argument, or a `fold` step, reports `UnhandledFailure`.
- An intent maps a failing call with `let x = f(...) else { Name => Obstruction }`,
  or with `else Obstruction` when the callee has exactly one failure. Missing
  arms report `IncompleteFailureMapping`. An unknown arm reports
  `UnknownFailure`, a repeated one `DuplicateObstructionFailure`, and a binder
  `UnsupportedSourceShape`.
- `failure`, `fails`, `fail` and `expect` are contextual keywords and remain
  ordinary identifiers elsewhere.

## Core and Target IR

- A Core source function carries sorted `failures`, omitted when empty. `fail`
  is the Core expression `{ kind: "fail", failure }`. A mapped `let` is an
  `attempt` node: binding, call, and a `failureMap` from each failure
  coordinate to a terminal obstruction reason. A module with failures requires
  `core.failure/v1`. A module without them encodes byte-identically to before.
- Core integrity independently refuses an unsorted or inexact set, a `fail`
  outside a function, an unmapped failing call outside a function, an
  `attempt` on a failure-free call or with wrong map keys, a failing `fold`
  step, and a missing or spurious `core.failure/v1`. Each refusal is
  `InvalidDefinition` at an exact path.
- Ordered (`echo.span-ir/v2`) and structured (`echo.span-ir/v3`) Target IR
  lower an `attempt` to a pure binding with `failureArms`. A binding without
  arms encodes as before. The pre-step-guard model (`echo.span-ir/v1`) refuses
  with `UnsupportedCoreNode` and the detail `failure_mapped_binding`.
- The [typed-failures contract pack](../../../fixtures/provider-contracts/typed-failures-v1/README.md)
  publishes the schema; earlier packs stay byte-identical and refuse these
  forms.

## Not implemented here

Echo's provider lowerer, verifier and runtime do not implement
`core.failure/v1`, so no failing program executes yet. Failure payloads,
mapping inside function bodies, and the other forms the Jedit rope drafts
mark as proposed are outside this feature.

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
- The set must equal what the body produces with `fail` and `expect`. A declared
  failure the body never produces reports `UnraisedFailure`. A failure the body
  produces but does not declare reports `UndeclaredFailure`.
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
  The CLI's `targetIr` review includes each nonempty failure map, including
  its failure coordinate, obstruction reason and payload; ordinary bindings
  continue to omit `failureArms`.
- The [typed-failures contract pack](../../../fixtures/provider-contracts/typed-failures-v1/README.md)
  publishes the schema; earlier packs stay byte-identical and refuse these
  forms.

Fallible record construction uses `orderedRecord` with one array of
`{name, value}` entries in source evaluation order. Nested failures and calls
to functions with declared failure sets require this form, including calls
written with `expect`. Core checks unique names, record field types and the
choice of representation: fallible fields cannot use the unordered `record`
map, and total fields cannot use `orderedRecord`. Total records keep the old
canonical map and bytes, even within a partial function. Canonical result-key
ordering is unchanged. The CLI review retains the ordered entry array.

### Terminal values and work

When encoding ordered or structured Target IR, each failure-arm payload is
checked in the instruction's incoming scope. It may reference the application
input or earlier bindings, but not the failed call's result, a later binding or
an out-of-scope local. Invalid payload references refuse with
CanonicalErrorKind::UnsupportedValue.

A terminal failure produces no returned value. Native Target's static work
accounting assigns that explicit terminal expression zero result cells and zero
result bytes, while its evaluation still contributes one step. Enclosing
predicates, calls, parameter/result validation and mapped obstruction payloads
keep their existing charges. Unknown or invalid expressions still refuse; they
are not terminal failures.

A failure map executes only its selected arm. Work accounting sums every field
in that arm, then takes separate maxima for steps and allocated bytes across
arms. Different arms may establish the two maxima; the call's work is added
once. Explicit failures in Boolean conditions retain the generated Boolean
comparison charge. That Boolean context also reaches failures inside nested
conditional and match arms; genuinely context-free failures still refuse.
Integer operands retain the existing source overflow and
underflow proof rules; accepting a terminal operand's type does not relax them.

For a size-charged intrinsic argument, a terminal arm contributes no returned
bytes or elements to its bound maximum. Returning arms keep their validated
bounds; predicate, operand and evaluation work retain their existing charges.
This does not turn failure into a successful empty value.

The source compiler gives a failure its contextual type and conservatively
reserves that type's result storage. Source and Target minimum budgets therefore
need not coincide. The typed-failure tests exercise Target's own minimum step
budget and a one-step-short refusal; they do not claim universal compiler/Target
budget parity or runtime metering.

## Not implemented here

Echo's provider lowerer, verifier and runtime do not implement
`core.failure/v1`, so no failing program executes yet. Failure payloads,
mapping inside function bodies, and the other forms the Jedit rope drafts
mark as proposed are outside this feature.
Those downstream consumers also need explicit `orderedRecord` support before
they can execute failure-bearing records in source order; schema acceptance
and native Target lowering do not establish that execution.

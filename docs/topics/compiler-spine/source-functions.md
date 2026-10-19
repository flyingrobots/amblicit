# Source-owned pure functions

This reference describes the implemented compiler profile for source functions.
It does not declare provider or runtime support.

## Declarations and calls

A source function has ordered typed parameters, a declared return type,
immutable `let` bindings and one terminal `return`:

```edict
fn assembleFragments(left: Bytes<max=8>, right: Bytes<max=8>) -> Bytes<max=16> {
  let bytes = left + right;
  return bytes;
}
```

This is the function in the compiler-built
[Jim range-assembly fixture](../../../fixtures/lang/functions/range-assembly.edict).
The compiler resolves all function signatures before checking bodies, so a
function can call one declared later. Zero parameters are allowed. Calls are
first-order and nongeneric: functions cannot be passed as values or instantiate
type arguments. Supported parameter/result types are the compiler's bounded
scalar, record, list and authenticated nominal families; this adds `Bool` as an
explicit type reference as well as Boolean value and predicate composition.
It does not add variant/match or Option lowering, list literals, recursive
functions, general checked arithmetic or byte-literal escapes.

Each body has a fresh lexical environment. Parameters and earlier local bindings
are visible; caller locals, later bindings and another function's locals are
not. Duplicate names and shadowing of module or prelude names reject. All
bodies are checked even if no intent calls them. Effects, requests, reads,
loops, statement conditionals, `require`, `guarantee`, assertions, statements
after return and missing returns reject. Assertions are not silently erased:
this profile has no function-body proof-node lowering.
Unsupported and post-return statement diagnostics identify the offending
statement; a missing return identifies the function declaration. Well-typed
non-Boolean predicate operands report `ExpectedPredicate` after expression
typing, while malformed expressions retain their own typing diagnostics.

A call retains its arguments once each in source order. It does not inline the
body, discard an unused argument, duplicate a reused argument, or lift a call
out of a conditional, predicate, basis or obstruction payload. Runtime support
must evaluate arguments left to right exactly once before entering the fresh
callee frame. `if` and ordered `All`/`Any` predicates retain lazy evaluation.
Boolean-valued expressions use existing Core `If` and predicate forms; Boolean
values in predicate position compare with `true`.

## Authority and identity

`CoreModule.functions` is a map from local function names to `CoreFunction`:
ordered `params`, `returnType`, and a pure `body` containing ordered `locals`,
`bindings` and `result`. Calls name `module-coordinate.function-name`. The
function table is source-owned executable authority. It is separate from the
lawpack-owned signature/cost facts supplied through compiler context.
An imported call still needs its exact digest-locked owning lawpack, signature,
type closure and cost evidence; a similarly named source definition cannot
manufacture that authority. A shared package coordinate is not a namespace
ban: a source function can call a disjoint imported export in that package.
Source-graph membership comes from the declared function table; imported calls
still require their independently authenticated facts.

An empty table is omitted from canonical Core. Existing function-free bytes
remain unchanged. Nonempty tables authenticate signatures, bodies and callees;
changing an unused definition changes module identity. Parameter/local names
are replaced by deterministic positional identities, so alpha-renaming those
names preserves bytes. Function names remain semantic coordinates.

Public Core validation checks type references, binder identity, ordered lexical
availability, source-call arity, recursion and source-call height. Independent Target
validation also checks each binding/result type, complete call authority and
partial-operation totality, including unused function bodies. Caller input
proofs do not establish a function body's totality. Source-owned executable
code always makes Target bind the complete source Core digest, including a
require-only intent with no imports, basis or pure bindings.

## Conservative compiler budgeting

The compiler uses checked `u64` arithmetic and completed callee summaries.
A leaf source function has call height one; the limit is 128 source function
frames. Every caller adds the completed suffix height, including suffixes
visited earlier. Cycles and over-limit paths reject. Cost summaries charge
repeated call occurrences; the graph is not expanded into an exponential tree.
The graph scan also limits source-body expression occurrences to 65,536.

For modules containing functions, the current portable compiler convention
charges expression/predicate visits, function entry, argument/result and local
binding validation, bounded value storage/copies, intrinsic operand validation,
byte/text comparison work and bounded concatenation/slice copies. Value storage
uses 64 abstract bytes per value cell, plus bounded payload and child storage;
records include field-key cells and UTF-8 key bytes. Strings reserve up to four
UTF-8 bytes per scalar. Input validation/copy, output validation/encoding
scratch and a conservative definite-length CBOR output maximum are included.
CBOR integer/header bounds reserve at most nine bytes; Boolean output is one.
The independent ordinary-result check uses exact definite-length header widths
and declared integer widths. Both checks must pass. The source-function estimate
can be more conservative; it does not replace the universal ordinary-result
check. Portable value-storage cells are 64 bytes. These cells are accounting
units, not host pointer widths or a proof of physical backend allocation.
These are conservative compiler accounting units, not a claim about any
backend's physical allocator or an exact instruction count.

Sequential costs add; mutually exclusive branches use component-wise maxima
of their complete costs; bounded loops multiply body work. Both operands of a
short-circuit Boolean connective contribute to its worst-case estimate without
making runtime evaluation eager. Imported helper steps, allocation and output
costs compose into the same three budget dimensions. Overflow, missing owned
cost evidence or exceeding a declared operation budget returns `InvalidBound`
or the existing missing-fact diagnostic before Core is emitted.

The source-function accounting profile cannot bound `ExternalActionRequest`
values, including requests nested in nominal types, records or lists. Such
shapes return `UnsupportedSourceShape` in parameters, results, expressions and
intent inputs/outputs when a module contains source functions. Even adding an
unused function can expose this limitation. Function-free request-bearing
modules retain their existing behavior; genuine numeric bound overflow still
returns `InvalidBound`.

Graph diagnostics carry their function owner separately from the descriptive
Core failure path. Module-wide work exhaustion has no function owner; a
function named `work` or `expression` does not acquire an unrelated failure.

The compiler's source-call height and expression depth are separate measures.
Imported facts do not expose imported body height. Passing this compiler check
therefore does not prove a combined source-to-imported call depth, provider
schema depth, or target runtime meter bound. A provider must independently
validate the complete authenticated source/lawpack graph and its own admitted
type, depth and cost model. It can refuse unsupported types such as `Bool`,
`String` or `List` even when source compilation succeeds. No Echo runtime
acceptance follows from the compiler's 64-byte convention or 128-frame limit.

## Target and application boundaries

Target lowering still requires an executable intent body node. A return-only
intent retains `NoTargetSteps`; functions do not inject dummy nodes. Existing
result projection restrictions also remain: placing a computed value in an
authored `let`, then returning that local or a record of locals, permits the
existing projection contract. A direct call or conditional result can compile
to Core and Target yet lack a supported application result projection.

The function-bearing Core schema is a separately selected
[source-functions publication](../../../fixtures/provider-contracts/source-functions-v1/README.md).
The prior [v1 publication](../../../fixtures/provider-contracts/v1/README.md)
remains intact. Old provider manifests continue selecting their exact schema;
they must explicitly refuse a function-bearing Core request instead of ignoring
executable definitions. Echo provider verification and pure/read runtime support
are separate consumer work; Jim domain operations remain authored Edict.

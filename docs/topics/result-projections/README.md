# Result Projections

Status: current HEAD contract.

This shelf describes Edict's compiler-owned representation of an application
operation result. A result projection preserves how the authored result is
assembled from declared application input and capability-step results without
executing the operation or introducing host-authored application semantics.
[RESULT-PROJ-REQ-001] [RESULT-PROJ-REQ-007]

## Current Contract

`amblicit_syntax::emit_result_projection` accepts one exact Core module, its
matching Target IR artifact, and an intent name. The emitter requires:

- the raw Core candidate to pass the authoritative whole-module type-integrity
  judgment, including unused definitions, every graph-carried reference, and
  the fully expanded named-type depth of each occurrence;
- the intent to exist in both artifacts;
- for an explicitly selected `echo.span-ir/v3` artifact, the Core/order
  correspondence described in
  [Structured Target v3 bodies](#structured-target-v3-bodies), with result
  sources limited to the root block;
- the Target IR semantic closure to equal the complete Core-derived closure,
  including the exact Core coordinate, canonical Core digest, and every
  digest-locked lawpack;
- the Core and Target IR result expressions, operation profile, and evaluation
  budget to agree;
- the application input to be the declared `arg.0` local with the intent input
  type; and
- every pure-binding source to identify the same source-ordered binding,
  compiler local, and exact Core expression in Core and Target IR; and
- every capability-result source to identify exactly one Target IR step whose
  binding, effect coordinate, and input expression match the corresponding
  Core effect node.

No projection is emitted after any of these checks fails.
[RESULT-PROJ-REQ-001] [RESULT-PROJ-REQ-005] [RESULT-PROJ-REQ-011]

`lower_to_target_ir` invokes that emitter for every intent in an artifact with
an explicit semantic closure and returns the artifacts in
`TargetLoweringReport::result_projections`, keyed by intent name. An intent
outside the first closed projection subset appears instead in
`result_projection_failures`; no projection is claimed for that intent.
General Target IR lowering remains available because not every target consumer
requires an executable application result. The public application-build path
does require exactly one admitted projection and rejects any projection
failure. Legacy Target IR without an explicit Core/lawpack semantic closure
remains byte-compatible and claims neither projection nor projection failure.
[RESULT-PROJ-REQ-001] [RESULT-PROJ-REQ-006]

The closed `edict.result-projection/v1` expression language contains only:

- records with canonically ordered field names;
- the declared application input;
- compiler-owned pure bindings addressed by deterministic Target IR binding
  identity;
- capability results addressed by Target IR step identity; and
- bounded field paths rooted at one of those declared sources.

Constants, calls, undeclared locals, unbound capability steps, and other Core
expression forms are outside this version and fail with stable
`ResultProjectionFailureKind` values. Output records must reconstruct the
declared Core output type exactly; missing, extra, or type-incompatible fields
reject before artifact identity exists. [RESULT-PROJ-REQ-002]
[RESULT-PROJ-REQ-006]

Each projection carries:

- schema identity `edict.result-projection/v1`;
- the Core module and intent operation coordinate;
- the declared Core output type;
- the authored `maxOutputBytes` evaluation bound; and
- the closed projection expression.

The representation limits one artifact to 256 expression nodes, 32 path
segments per source, 1,024 UTF-8 bytes per coordinate or field string, and
65,536 canonical bytes. A zero output bound and every one-over-limit value
reject before admission. [RESULT-PROJ-REQ-004]

`ResultProjection` is an untrusted candidate value so callers can submit
hostile values to the encoder and verifier. A `ResultProjectionArtifact` can
only be constructed by the compiler-owned emitter and exposes its accepted
projection, canonical bytes, and identity through read-only accessors. Safe
callers therefore cannot detach the artifact bytes from their digest after
emission. [RESULT-PROJ-REQ-003] [RESULT-PROJ-REQ-006]

`encode_result_projection` emits `edict.canonical-cbor/v1` bytes.
`digest_result_projection` frames those exact bytes under
`edict.result-projection.artifact/v1`. The generated provider contract pack
publishes the `result-projection` CDDL root and the matching artifact-domain
binding. The root mirrors positive output bounds, bounded text, the 32-segment
path ceiling, and the maximum 255 children of a flat root record. Aggregate
recursive node count and encoded artifact bytes remain authoritative decoder
checks because CDDL cannot express either whole-value total. The reviewed Hello
Echo bytes and digest under
`fixtures/lawpack/hello-echo/` are reproduced only by
`cargo xtask lawpack-goldens`. [RESULT-PROJ-REQ-003]

## Structured Target v3 bodies

An adapter selects `echo.span-ir/v3` explicitly, through the same selection
paths as v2 (see [Target IR](../target-ir/README.md)). Under v3 an intent body
may contain literal-bounded loops and statement branches. Every intent in a v3
artifact takes the structured path described here, including an intent with no
loop or branch. [RESULT-PROJ-REQ-014]

Before selecting projection sources, `emit_result_projection` and
`verify_result_projection` check Core/order correspondence: the Core body's
nested instructions must match the Target `executionOrder` tree node for node.
The check compares control identity and placement, loop binder, iterable and
bound, branch predicate, instruction order and payloads, requirement failure
arms and reasons, effect obstruction arms, child-block local declarations and
null results, and exact table coverage. The flat v1/v2 path does not compare
obstruction arms. The Target order judgment enforces fresh instruction, binding
and loop-binder identities and lexical availability through both arms;
match-arm binders are not checked for freshness there (#304). Missing or changed
nested instructions refuse with `CoreTargetMismatch` even when they do not
contribute to the final result. A v3 artifact whose header lacks a valid
structured execution order refuses with subject `<intent>.structuredHeader`.
[RESULT-PROJ-REQ-014]

Emission and verification share this one correspondence matcher. Verification
is independent of the emitted bytes and the claimed digest, not of the matcher.
[RESULT-PROJ-REQ-005] [RESULT-PROJ-REQ-014]

Only the application input and pure/effect bindings introduced directly in the
root block are result sources. A nested instruction stays in the Target tables
for execution, but its local or step result is unavailable after that loop or
branch. The order judgment already refuses a Core result that reads a nested
local; root-only source selection additionally refuses a hostile projection
that names a child binding or step. Root bindings keep their original table
identities, including indices that follow nested instructions. No
iteration-result list or branch-result join is added to the projection
language. [RESULT-PROJ-REQ-014]

This correspondence check has no adapter or runtime authority, and it makes two
deliberate exceptions to exact payload matching. An adapter may rename
obstruction keys while retaining their Core arm bindings and values, and a
step's `target_intrinsic` is not compared with Core. This check does not
validate that obstruction-key mapping. Before the correspondence check, a v3
step whose `target_intrinsic` is outside the namespace of the artifact's
selected profile refuses with `CoreTargetMismatch` and subject
`<intent>.targetIntrinsic` (Target IR shelf, TIR-REQ-080); whether the named intrinsic exists
and fits the effect stays the provider's to check. No Echo provider lowerer or
verifier admits v3 at Echo `089c849f`, so a future v3 provider must validate
the key mapping and the intrinsic before execution; structured provider
support is tracked by Echo #947. Projection admission does not establish effect-budget feasibility
or successful execution. The existing application builder
independently verifies the projection and binds its exact bytes into provider
inputs. A public build of a structured operation therefore still fails today, at
the provider, which admits only v1 and v2 at that revision.
[RESULT-PROJ-REQ-007] [RESULT-PROJ-REQ-014]

A Core/order mismatch under v3 refuses with `CoreTargetMismatch` and subject
`<intent>.structuredControl`. Under v1 and v2 the same subject means that the
operation contains a loop or branch the selected domain cannot represent.
[RESULT-PROJ-REQ-008] [RESULT-PROJ-REQ-014]

## Independent Verification

`verify_result_projection` does not call the emitter or trust the claimed
digest. It:

1. obtains the same whole-module Core type-integrity witness required by the
   emitter, canonical encoder, and Target lowerer;
2. decodes the claimed bytes through the canonical CBOR decoder;
3. validates the closed projection shape and representation bounds;
4. reproduces the exact canonical bytes;
5. recomputes the domain-framed identity;
6. independently reconstructs and compares the complete Core-derived semantic
   closure;
7. independently rebuilds the source-ordered Core-to-Target pure-binding and
   capability-step correspondence;
8. reconstructs a Core result expression from the projection; and
9. requires that reconstruction to equal both the authored Core result and the
   matching Target IR result.

Only then does the API return `VerifiedResultProjection`, whose fields are
available through read-only accessors. [RESULT-PROJ-REQ-005]
[RESULT-PROJ-REQ-006] [RESULT-PROJ-REQ-011] [RESULT-PROJ-REQ-012]

The public application-build path requires exactly one compiler-emitted
projection for its current singleton executable-operation slice, runs the
independent verifier over that artifact, and adds the same canonical bytes and
domain-framed identity to both the provider lowerer and provider verifier
semantic-input closures under the `result-projection` auxiliary kind. A
provider package that does not declare and accept that input refuses rather
than receiving a projection reconstructed by the host. [RESULT-PROJ-REQ-001]
[RESULT-PROJ-REQ-003] [RESULT-PROJ-REQ-005]

```text
Edict source + lawpack closure
    -> Core result
    -> matching Target IR result
    -> bounded canonical projection
    -> independent reverse reconstruction
    -> verified projection
```

## Authority Boundary

The projection is data, not an execution callback. Emission and verification
perform no filesystem, network, process, model, runtime, or application-host
operation. The representation names application and capability data already
declared by Core and Target IR; it does not grant authority to obtain those
values. [RESULT-PROJ-REQ-007]

Echo #698 owns generic runtime evaluation, durable result binding, recovery,
package inclusion, and exposure of the projected value. Until that producer
change lands, the previous six-input Echo provider package is incompatible with
the new seven-input application-build closure. Hello Echo #18 owns the external
application proof. Neither runtime responsibility is implemented in this Edict
contract.

## Deferred

- Runtime evaluation of a verified projection.
- Provider admission and runtime execution of structured v3 intents
  (Echo #947).
- Durable binding of evaluated result bytes to an Echo Action, Tick, outcome,
  Receipt, or recovery record.
- Additional expression forms beyond records and bounded source field paths.
- External effects, model invocation, native callbacks, or application-specific
  reconstruction.

The executable verification matrix is tracked in
[test-plan.md](./test-plan.md).

## Variant-valued outputs

A source leaf can preserve a value's type without establishing its result encoding.
For otherwise projectable source leaves, emission and independent verification
reject output graphs containing variants with `OutputShapeMismatch`, including a
whole record or list projected from one source. A constructor expression itself
refuses earlier with `UnsupportedExpression`. Internal variant bindings can still lower to native Target IR;
the lowerer reports the missing result projection separately. The executable
witnesses cover direct, record and list outputs with scalar-output controls.
[RESULT-PROJ-REQ-013]

## Matches and projection leaves

A `CoreExpr::Match` directly in a result-projection leaf refuses with
`UnsupportedExpression`, as do other expressions outside the closed projection
language. Matching inside a helper or pure binding does not itself extend that
language: an ordinary supported scalar local can still be projected. Native
Target match validation and application-result projection are separate gates.
[RESULT-PROJ-REQ-007]

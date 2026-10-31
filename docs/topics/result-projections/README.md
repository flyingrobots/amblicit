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
- Durable binding of evaluated result bytes to an Echo Action, Tick, outcome,
  Receipt, or recovery record.
- Additional expression forms beyond records and bounded source field paths.
- External effects, model invocation, native callbacks, or application-specific
  reconstruction.

The executable verification matrix is tracked in
[test-plan.md](./test-plan.md).

## Variant-valued outputs

A source leaf can preserve a value's type without establishing its result encoding.
Emission and independent verification therefore reject output graphs containing
variants with `OutputShapeMismatch`, including a whole record or list projected
from one source. Internal variant bindings can still lower to native Target IR;
the lowerer reports the missing result projection separately. The executable
witnesses cover direct, record and list outputs with scalar-output controls.
[RESULT-PROJ-REQ-013]

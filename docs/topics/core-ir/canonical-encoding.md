# Canonical Core Encoding And Digest

Status: current Core IR byte and digest explanation for
`edict.canonical-cbor/v1`.

This page explains how Edict turns an in-memory `CoreModule` into reviewed
canonical bytes and a `sha256:<hex>` digest. The executable source of truth is
[`canonical.rs`](../../../crates/edict-syntax/src/canonical.rs); this page is the
human map for reviewers changing that file.

## Scope

In scope:

- the canonical value model used for Core modules;
- the canonical CBOR subset emitted by `encode_canonical_cbor`;
- the Core digest frame used by `digest_core_module`;
- the reviewed Core golden byte and digest fixtures;
- the change discipline for byte/hash-affecting edits.

Out of scope:

- canonical Target IR artifact details, except where they reuse the same
  canonical machinery;
- contract-bundle semantic/release preimage ordering;
- canonical `ContractBundleManifest` bytes;
- runtime execution, admission execution, or participant policy decisions.

## Value Model

The encoder does not serialize Rust structs directly. It first projects the
artifact into a small canonical value tree:

| Value | Meaning |
| --- | --- |
| `Null` | CBOR null. |
| `Bool` | CBOR false or true. |
| `Integer` | Signed integer represented through CBOR major type 0 or 1. |
| `Bytes` | Definite-length byte string. |
| `Text` | Definite-length UTF-8 text string. |
| `Array` | Definite-length array. |
| `Map` | Definite-length map sorted by canonical encoded key bytes. |

For Core, `core_module_value` projects a `CoreModule` into a map containing:

- `apiVersion`;
- `coordinate`;
- `imports`;
- `types`;
- `functions`, only when nonempty;
- `intents`;
- `requiredCoreCapabilities`.

Nested Core values follow the same rule: each semantic variant becomes an
explicit map with stable text keys and a `kind` field where the variant needs one.
For example, Core effect nodes encode their binding, effect coordinate, input
expression, and obstruction map; Core require nodes encode their predicate plus
an explicit terminal or `continueObstructed` failure disposition; Core local
references encode compiler-owned `id`, `alphaName`, and `type`.

## CBOR Subset

`encode_canonical_cbor` emits deterministic, definite-length CBOR:

- unsigned integers use CBOR major type 0;
- negative integers use CBOR major type 1 with the standard `-1 - n` magnitude;
- byte and text strings use definite lengths;
- arrays and maps use definite lengths;
- maps are sorted by the canonical encoded bytes of their keys;
- duplicate map keys reject before bytes are emitted;
- integer and length payloads use the shortest valid CBOR width;
- multi-byte integer and length payloads are big-endian;
- unsupported CBOR major types, indefinite lengths, reserved additional-info
  forms, trailing bytes, and invalid UTF-8 reject during decode.

`decode_canonical_cbor` validates bytes by decoding one value, requiring no
trailing data, re-encoding the decoded value, and requiring a byte-identical
match. That re-encode check is what rejects non-minimal integers, unsorted maps,
and other byte-equivalent but non-canonical forms.

## Nesting Limit

`MAX_CANONICAL_NESTING_DEPTH` publicly fixes the maximum canonical container
nesting depth at 128. The root value starts at depth zero. A scalar wrapped in
exactly 128 arrays or maps is therefore valid; the next nested value rejects on
both encode and decode with stable
`CanonicalErrorKind::NestingLimitExceeded`. Map keys and values consume the same
depth budget as array elements, and decode's byte-identical re-encoding uses the
same limit. Artifact digest framing encodes outside the artifact schema root, so
the `edict.digest/v1` tuple does not consume one level of the artifact's budget.

This bound prevents adversarial canonical values from driving unbounded
recursive traversal. It does not alter the encoding of any value within the
limit. `canonical_nesting_limit_is_enforced_on_encode_and_decode`,
`canonical_nesting_limit_counts_empty_terminal_containers`, and
`canonical_artifact_digest_accepts_the_public_nesting_boundary` exercise the
scalar, empty-container, and digest boundaries, while the existing Core golden
checks prove that the reviewed `bounded-hello` bytes and digest did not move.

## Ordering Rules

The canonicalizer preserves order only where order is semantic:

- Core block locals and nodes preserve their ordered execution shape.
- Function call type arguments and arguments preserve call order.
- Lists that model ordered source or execution structure remain ordered.

It sorts or canonicalizes collections where order is not semantic:

- Core maps such as type maps, record fields, variant cases, record expression
  fields, require reason payloads, and effect obstruction maps are encoded as
  canonical maps.
- Required Core capabilities are deduplicated and sorted as a text set.
- Input constraints are sorted by their canonical encoded value.
- Imports are sorted by their canonical encoded value.

This is why equivalent construction order does not move Core canonical bytes,
while semantic mutations such as a changed local identity, require-failure
disposition, effect coordinate, or Core expression do.

## Digest Values

Core import/resource digests are not hashed as review strings. `digest_value`
parses a review string of the form `sha256:<64 hex>` into this canonical value:

```text
["sha256", <32 raw digest bytes>]
```

Core import digest parsing accepts uppercase or lowercase hex and normalizes to
the same raw bytes. The displayed `CoreDigest` review rendering is always
lowercase:

```text
sha256:<64 lowercase hex>
```

Bundle and Target IR artifact references are stricter at their own boundary and
reject uppercase review strings before entering their canonical preimages. That
strictness is separate from the Core import normalization described here.

## Core Digest Frame

`digest_core_module` computes SHA-256 over the canonical CBOR encoding of this
framed value:

```text
["edict.digest/v1", "edict.core.module/v1", <canonical Core module value>]
```

The frame has three jobs:

- `edict.digest/v1` separates Edict artifact digests from arbitrary SHA-256
  hashes.
- `edict.core.module/v1` separates Core modules from Target IR artifacts,
  bundle layers, and future artifact families.
- The third element is the canonical value, not already-rendered bytes or a
  review string.

This means the same semantic value under a different artifact domain gets a
different digest. It also means digest review strings are outputs of the process,
not inputs to the Core digest preimage.

The implementation now shares this frame construction with a crate-private
helper that hashes an already decoded canonical value under its
caller-established domain. The helper cannot validate bytes or schemas by
itself. Provider invocation code calls it only after the explicit schema
capability accepts the value; crate privacy limits the framing helper's public
surface but is not itself a schema proof. This is not a new public generic
artifact API or a claim that Edict interprets provider-owned semantics. Core's
public digest function and reviewed digest contract remain unchanged.

## Golden Fixtures

The reviewed Core fixture pair is:

- [`fixtures/core/canonical/bounded-hello.core.cbor`](../../../fixtures/core/canonical/bounded-hello.core.cbor);
- [`fixtures/core/canonical/bounded-hello.core.sha256`](../../../fixtures/core/canonical/bounded-hello.core.sha256);
- [`fixtures/core/canonical/workspace-snapshot.core.cbor`](../../../fixtures/core/canonical/workspace-snapshot.core.cbor);
- [`fixtures/core/canonical/workspace-snapshot.core.sha256`](../../../fixtures/core/canonical/workspace-snapshot.core.sha256).

Both are generated from
[`fixtures/lang/bounds/bounded-hello.edict`](../../../fixtures/lang/bounds/bounded-hello.edict)
through the executable parser, compiler, canonical encoder, and digest function.

Use:

```bash
cargo xtask core-goldens --check
cargo xtask core-goldens --write
```

Check mode fails when checked-in bytes or digest text drift. Write mode
regenerates the reviewed artifacts and must be followed by a normal diff review.
`cargo xtask verify` runs the check mode.

## Change Discipline

The release rule is:

```text
meaning freezes before bytes; bytes freeze before hashes
```

Apply that rule in this order:

1. Change the semantic Core model or canonical value shape deliberately.
2. Add or update mutation and determinism tests that prove the intended meaning.
3. Update canonical byte tests for ordering, decode/re-encode, and rejection
   behavior.
4. Regenerate reviewed Core byte and digest fixtures with
   `cargo xtask core-goldens --write`.
5. Review the byte and digest diff as a cryptographic contract change.
6. Update the Core IR topic shelf, changelog, and release notes when the public
   contract changes.

Do not hash debug output, serde output, prose, file paths, or review strings as
a shortcut. The digest must follow the canonical value model and domain-separated
frame above.

## Adjacent Artifact Families

`canonical.rs` also hosts the current Target IR artifact encoder and
contract-bundle layer digest helper. They reuse the same `CanonicalValue`,
canonical CBOR encoder, and `edict.digest/v1` frame prefix, but they have their
own artifact domains and boundary rules:

- Target IR artifact digest domain: `edict.target-ir.artifact/v1`.
- Bundle semantic layer domain: `edict.bundle.semantic/v1`.
- Bundle release layer domain: `edict.bundle.release/v1`.

Those domains prevent a valid byte sequence for one artifact family from being
reinterpreted as another family with the same digest.

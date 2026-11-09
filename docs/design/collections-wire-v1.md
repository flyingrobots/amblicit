# Collections artifact contract v1

This reference fixes the artifact shapes and compatibility rules required by
[collections issue #290](https://github.com/flyingrobots/amblicit/issues/290).
It accompanies the [semantic contract](collections-reference-v1.md). The
reference header checker implements metadata agreement in this checkpoint. Production source
lowering, canonical schema integration, byte decoding, and provider admission
belong to [#293](https://github.com/flyingrobots/amblicit/issues/293) and its Echo
consumer issues. No current target advertises this contract.

## Envelope and identity

Every collection artifact is one canonical value map with these fields:

| Field | Required value |
| --- | --- |
| `apiVersion` | Exact text `amblicit.collections.artifact/v1` |
| `semantics` | `{id: "amblicit.collections/v1", digest: ["sha256", bytes32]}` |
| `features` | A sorted, unique text array drawn from `content`, `slot`, `traversal` |
| `kind` | Exactly `description`, `reading`, `candidate`, or `outcome` |
| `body` | The corresponding closed record defined below |

The `semantics` digest identifies the exact published semantic contract resource.
There is no default or implicitly trusted digest: consumers verify the referenced
resource bytes. Reference tests supply explicit symbolic digest bytes; they are
not a published resource or registry entry.
The contract resource does not embed its own digest in its preimage.

Encoding is the existing
[`edict.canonical-cbor/v1`](../topics/core-ir/canonical-encoding.md), including
shortest integer/length widths, big-endian multi-byte arguments, definite
containers, canonical encoded-key map order, UTF-8 validation, and rejection of
duplicate fields, noncanonical encodings, trailing bytes, and unsupported CBOR
types. Rust/Serde layout and review JSON never define authoritative bytes.
There is no second transport prefix: the decoded `apiVersion` is the format
discriminator. The enclosing package authenticates its byte length and digest.

The artifact identity is SHA-256 over canonical CBOR for
`["edict.digest/v1", "amblicit.collections.artifact/v1", artifact]`, using the
existing artifact digest function. `kind`, semantic resource digest, all
features, and the complete body contribute. Descriptions, readings, candidates,
and outcomes cannot alias by dropping their discriminator. A digest establishes
byte identity; it supplies neither observation authority nor admission evidence.

Unknown fields, kinds, versions, features, or digest algorithms refuse. Consumers
must not downgrade, reinterpret a newer version as v1, or ignore an unsupported
field. Changes to meaning or canonical representation require a new version and
new vectors. Existing artifacts keep their own version and bytes.

## Capability agreement

A producer requirement and a target offer each supply the envelope's first
three fields. These are claims to check against the digest-locked target profile
and semantic closure, not authority supplied by the artifact itself. `slot`
covers witnessed get/add/remove/replace, `traversal` covers bounded entry scans,
and `content` covers typed immutable resolution/ensure. Traversal requires slot;
content is independent. An inert mutable name inside content does not require a
slot read. An empty feature set is valid for pure descriptions and empty
operations. Actual feature use must be derived from the entire lowered program,
including nested and unexecuted branches, and equal its declared requirement;
a missing declaration must never hide an unsupported operation. Merely storing
a description or its permits as inert data does not use the described operation.

Validate the producer requirement first, then the offer. For each header:

1. Require the exact artifact version, then exact semantic coordinate.
2. Require a 32-byte SHA-256 semantic digest.
3. Require at most three features, recognized and strictly increasing in UTF-8
   text order, with no duplicates. Then check traversal's slot prerequisite.

Require identical semantic digests, then require every requested feature in the
offer, selecting the first missing feature in canonical text order. A target
may offer additional known features. Equality of unsupported versions is not
agreement. Any refusal terminates before artifact execution or state writes.
`negotiate_contract` in the reference model returns only agreement metadata. It
cannot construct a model snapshot, reading, candidate, or write receipt.

## Shared body values

The following records have exactly the listed fields. `uint` means an integer
from zero through 2^64 - 1, additionally bounded by the verified profile and
operation limits. Every resource reference
is `{id: nonempty-text, digest: ["sha256", bytes32]}`. Text identifiers are
case-sensitive UTF-8; normalization is not implicit. Field/type interpretation
comes from the digest-locked namespace resource.

| Value | Closed record or variant |
| --- | --- |
| Operation | `{sourceCore: resource-ref, intent: nonempty-text}` |
| Root occurrence | `{targetProfile: resource-ref, world: bytes, root: bytes}` |
| Typed value | `{type: resource-ref, value: canonical-data}` |
| Owner component | `{field: nonempty-text, key: typed-value}` |
| Collection | `{namespace: resource-ref, root: root-occurrence, owner: [owner-component], field: nonempty-text}` |
| Place | `{collection: collection, key: typed-value}` |
| Basis | `{root: root-occurrence, snapshot: bytes, evidence: typed-value}` |
| Support fact | `{kind: "slot"/"range"/"owner"/"children", subject: place-or-collection, evidence: typed-value}` |
| Support key | `{kind: "slot"/"range"/"owner"/"children", subject: place-or-collection}` |
| Content reference | `{type: resource-ref, identity: typed-value}` |
| Limits | `{work: uint, scan: uint, results: uint, requests: uint, ownerDepth: uint, groupDepth: uint, contentNodes: uint, contentLinks: uint}` |

`canonical-data` contains only the scalar/array/map canonical value model;
it is data, never a Core expression, callable, authority handle, or instruction.
Bytes used for world/root/snapshot are opaque target-defined identifiers with
the exact encoding and maximum length fixed by `targetProfile`. They must be
nonempty. Snapshot is separate from place identity: deleting and reusing a place
does not create an incarnation reference. The private in-process origin pointer
of the reference model has no wire representation and cannot be serialized here.

The owner path is structural: each component names the owning field and storage
key in the preceding prefix. Payloads cannot smuggle owning collections into
this path. Root occurrence and namespace remain part of identity even if two
payloads or content digests are equal. Names stored as ordinary data stay inert.
Canonical key values are type-checked and encoded under their namespace schema;
string concatenation, display labels, and payload key fields cannot define a
place. Within a namespace, logical order is root, owner path, field, then key,
using canonical encoded bytes for each typed component and prefix-first path
ordering. Cross-namespace order compares the complete namespace resource first.

Range support names a collection; the other three support kinds name a place.
Support is sorted by kind (`slot`, `range`, `owner`, `children`) then logical
subject, with exactly one fact per key. Evidence is target-typed and must bind
the complete subject and basis. Presence of a well-shaped evidence value, a
matching digest, or a copied reading is never sufficient verification.

## Body variants

**Description** is `{focus, permits, limits}`. Focus is
`{kind: "slot", place}` or `{kind: "entries", collection}`. Permits is a
sorted unique subset of `add`, `get`, `iter`, `remove`, `replace`. It describes
permitted interpretation, not authorization. Constructing or copying a
description performs no stored read, basis refresh, or mutation.

**Reading** is `{operation, basis, description, status, support, workUsed}`.
Status is one of these closed records:

- `{kind: "present", value: typed-value}` or `{kind: "absent"}` for a complete
  slot observation. Absence requires verified owner/slot support.
- `{kind: "entries", entries: [{key: typed-value, value: typed-value}]}` for a
  complete traversal in authoritative storage-key order. Empty and excluded
  observations retain their range/payload support; result keys cannot be
  replaced with fields read from payloads.
- `{kind: "unavailable", subject}` when required observation authority or owner
  is unavailable, or `{kind: "incomplete", subject}` when coverage ends early.
  Neither variant carries complete values or serves as absence/selection proof.

Description fixes the collection context of entry keys. Support records every
inspected dependency, including observations excluded by filtering. Conflicts
arise when corroborating such evidence; they do not retroactively turn a reading
into absence. A consumer must not import unavailable/incomplete readings into
an executable candidate as if they supplied complete observation support.

**Candidate** is `{operation, basis, limits, requests, support, mutations,
ensures, workUsed}`. Requests retain authored group/leaf order for accounting.
A group is `{kind: "group", requests: [...]}`; leaves are `{kind: "get", place}`,
`{kind: "iter", collection, predicate}`, `{kind: "add"/"replace", place, value}`,
`{kind: "remove", place}`, or `{kind: "ensure", reference, value}`. Predicate is
`{kind: "all"}` or `{kind: "equal", value}` in this bounded contract. A request
value is typed data. Predicates add no new code evaluator.

Mutations is a sorted array of add/remove/replace leaves, with at most one leaf
per logical place. Ensures is a sorted array of ensure leaves, with exactly one
typed value per content identity. Duplicate exclusive requests refuse even if
their values match; equal ensures coalesce but keep their performed cost.
Support, normalized proposal, and work count are assertions to independently
reconstruct from the authenticated operation and its observation evidence.
They must not be trusted merely because their encoding is canonical. Every
observation, focus, support subject, or mutation place must belong to the
operation's selected basis/root. Inert names embedded in typed data are not
observation subjects and gain no authority from appearing in a candidate.

**Outcome** is `{operation, basis, status, workUsed}`. Status is
`{kind: "admitted", receipt: typed-value}` or
`{kind: "refused", failure: {category, code, subject}}`. Category is exactly
`domain`, `conflict`, `integrity`, `resource`, `unavailable`, or `contract`.
The operation reference supplies failure source attribution; source-debug spans
are external annotations, not alternate catch sites or identity inputs. Subject
is `{kind: "operation"}`, `{kind: "place", place}`,
`{kind: "collection", collection}`, `{kind: "support", supportKey}`, or
`{kind: "content", reference}`. The semantic contract fixes failure precedence.
Admission receipts require target verification; a serialized `admitted` status
is not itself a receipt authority.

The v1 failure codes and categories are closed:

| Category | Codes |
| --- | --- |
| `domain` | `capacity`, `duplicateMutation`, `notEmpty`, `ownerMissing`, `ownerSurvival`, `alreadyPresent`, `notFound`, `contentMissing` |
| `conflict` | `conflict` |
| `integrity` | `basisMismatch`, `contentIntegrity`, `contentType`, `contentCycle` |
| `resource` | `modelBound`, `workExhausted`, `incomplete` |
| `unavailable` | `unavailable` |
| `contract` | `unsupportedVersion`, `unknownSemantics`, `invalidDigestLength`, `tooManyFeatures`, `unknownFeature`, `nonCanonicalFeatures`, `traversalNeedsSlot`, `semanticsMismatch`, `unsupportedFeature` |

Unknown codes or mismatched category/code pairs refuse. A known missing content
target is a domain obstruction only after complete basis/proposal closure has
established its absence. Failure to obtain content is unavailable, not absence.
An outcome record requires an already established operation and basis. Earlier
envelope/negotiation refusals are boundary errors; a consumer must not invent a
basis or an admitted-status record to serialize them as an operation outcome.

## Bounds and consumer obligations

Before allocation, a decoder enforces its target profile's maximum artifact
bytes, identifier/value bytes, canonical depth, and aggregate element counts.
No consumer may silently treat missing limits as infinity. Per-operation limits
must fit the verified offer; model fixtures use the finite envelope in the
semantic contract (64 requests/members/content nodes, owner/group depth four,
four outgoing content links, 128 mutating admissions). These model constants
are not production storage limits. A target with different representational
ceilings must bind those ceilings in its profile and preserve the same laws.

In order, consumers authenticate the enclosing operation and semantic closure,
check contract/version/feature agreement, validate closed record shapes and
cross-field/bound constraints, validate observations against the selected basis,
reconstruct the normalized proposal and work, and apply the semantic admission
rules. Unknown contract data is a contract refusal, malformed or contradictory
evidence is an integrity failure, incomplete coverage is a resource/coverage
refusal, and ordinary absence remains an established reading result.

The consumer implementation must add byte/digest goldens and hostile decoding
fixtures for each body variant when its codec lands. The reference header
checker alone does not prove those codecs, production schema registration,
authority checking, or execution. This artifact contract supplies their required
shape and meaning; it does not register a provider capability in this PR.

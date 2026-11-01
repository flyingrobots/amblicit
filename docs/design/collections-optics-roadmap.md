# Collections and witnessed optics: implementation rationale

Posture: **planned**, recorded 2026-11-01. This page explains the boundaries of
the implementation plan; it does not define landed language behavior. The live
work owners, acceptance checks and dependency graph are in
[roadmap #288](https://github.com/flyingrobots/amblicit/issues/288).

## The result we want

An author should be able to describe a bounded change to stored entities using
places, observations and proposed changes. Amblicit checks and compiles that
description. Echo observes the required facts and admits the complete change
only when its basis, authority, invariants and resource bounds hold.

Jim's authored rope edit supplies a demanding concrete consumer. Its algorithm
must remain in Amblicit; a native rope planner or a rope-specific Echo intrinsic
would not establish that the general authoring surface works.

The roadmap includes the immediate authored-rope proof and the broader
collections/optics proposal. They meet at an explicit migration proof. General
optic composition is not an assumed prerequisite for proving the first authored
rope operation through the existing generic surface.

| Relationship | Targets |
| --- | --- |
| `refines` | none |
| `supersedes` | none |
| `depends_on` | [Language and Core semantics](../SPEC_edict-language-v1.md), [Lawpack ABI](../SPEC_edict-lawpack-abi-v1.md), [Target profile ABI](../SPEC_edict-target-profile-abi-v1.md) |
| `related` | [Live roadmap](https://github.com/flyingrobots/amblicit/issues/288), [Existing rope acceptance owner](https://github.com/flyingrobots/jedit/issues/296), [Future adapter composition](v2-obligation-closure.md) |

## Proposed contract constraints

The design input is *Edict collections v2: places, payloads and witnessed
optics* (2026-10-22), SHA-256
`fbc898f3237264f25fad1132aee7b05b9c23403cf72a7b64d5cdcb5f3b5aac28`.
It supersedes the older collections proposal as a design input, not the current
language specification. The first implementation step ports the contract and
its conformance cases into repository ownership and settles unresolved
representation choices. Implementers must not depend on a private Reader path.

A place names a membership location. A stable owner can own collections without
being a member of them. Replacing payload must neither copy nor erase owned
structure. Entry keys come from the collection, not from an editable payload.
Canonical aliases of one place remain one mutation target.

An optic description is inert. It grants no authority and performs no read.
Interpreting it produces a reading with a basis and support for what was
observed. Established absence, an unavailable observation, incomplete coverage
and a conflict cannot be silently exchanged. Selection support includes empty
ranges and inspected entries that were excluded by a filter.

A proposed change is a candidate, validated as a whole before application.
Reads use the starting snapshot, including reads after a proposed write.
Capacity follows starting cardinality minus removals plus additions, after
presence and duplicate checks. Owner survival and snapshot `NotEmpty` remain
independent obligations. The initial contract retains the proposal's refusal
of new-owner-plus-child creation in one candidate.

Semantic content identity, target storage identity and root/path occurrence
identity serve different purposes. Shared immutable content must not collapse
two occurrences into one. Complete candidate validation must handle newly
ensured parents and children together, while snapshot reads retain their basis.

Bounds cover actual work, scanned entries and nesting as appropriate; a small
result limit alone does not bound a scan. Coalescing equal ensures does not
refund computation already performed. Pure payload equivalence does not erase
required writes, receipts or history.

## Why the implementation boundaries look this way

The reference model comes before production representation choices. It tests
the proposal's 16 conformance cases and targeted counterexamples independently
of the eventual compiler and runtime. A passing model is not production proof.

Compiler and consumer work may merge separately when the new capability is
versioned, unsupported consumers refuse it, and existing behavior remains
valid. A runtime slice includes its separate verifier, publication artifacts
and real public-source witness. Native tests alone cannot establish that the
public provider can consume the feature.

The stored surface grows through optional slots, bounded traversal and atomic
membership candidates. Immutable content is then enforced using the actual
target identity law. Composition is limited to the demonstrated subset, after
its observations, support and failure rules exist. This avoids choosing a
general optic algebra before the concrete laws are testable.

The two consumer proofs preserve an independent baseline. The first proves the
authored rope operation against all retained obligations. The second migrates
that proven operation to the new collections/optics surface and repeats its
evidence chain. Only then does the roadmap retire raw `CreateNode` from new
authoring profiles, with explicitly versioned historical replay.

Native issue hierarchy groups work. A blocker edge means that another PR must
already be merged for this PR to be correct. Shared files, shared workers,
similar topics and a common parent do not establish that dependency. Every
executable owner must produce a coherent PR and a working intermediate main.

## Source baseline used to make the plan

These are dated inspection results, not fresh runtime validation:

| Boundary | Evidence inspected | Consequence for the plan |
| --- | --- | --- |
| Public compiler | Amblicit `312e60e6a4b548cb70ae91a7020fa76c69569a1b`; `crates/amblicit-cli/src/application_build.rs` and compiler/provider invocation paths | Trace source closure, Core, Target, provider and separate verifier as distinct gates. |
| Fold | [Amblicit #285](https://github.com/flyingrobots/amblicit/pull/285), [#286](https://github.com/flyingrobots/amblicit/pull/286), [Echo #944](https://github.com/flyingrobots/echo/pull/944) | Reuse landed first-order state fold; do not schedule it again. |
| Structured effects | [Amblicit #272](https://github.com/flyingrobots/amblicit/pull/272); Echo candidate `a9eb33de3b06a45024b886349f5e07f304dc8dfe`, `crates/warp-core/src/edict_effects/execution.rs` and `instruction_relation.rs` | Compiler control flow exists; the inspected generic action path still accepts flat let/effect/require instructions. Add the consumer and public proof. |
| Lists | Same Echo candidate; Amblicit bounded literal handling and byte-specific `len`/`at` routing | Publish list values and ordered-read judgments, then add bounded worklist operations. Native candidate evidence does not establish publication. |
| Rope obligations | Jedit `5f8c44b0f4c400fe3c61e26db2c209b311b67419`, [DL-0159](https://github.com/flyingrobots/jedit/blob/5f8c44b0f4c400fe3c61e26db2c209b311b67419/docs/design/0159-edict-rope-representation.md#replacerange-acceptance-across-representations) | Preserve all 40 obligations, mapped as 28 carry and 12 counterpart; specify independent physical/outcome fixtures before claiming acceptance. |

The Echo source pin is a retained list candidate, not a statement that the
branch is reviewed or ready to merge. Refresh all source and component pins
before implementation. Open legacy tickets require reconciliation with merged
evidence; their open status does not prove the corresponding feature is absent.

## Completion boundaries

The rope proof retains exact public compiler/provider artifacts, independent
verification, generic runtime execution, complete observations, deterministic
outcomes and the Tick/WAL/receipt/bounded-reading/recovery chain. Text equality,
generic refusal, an unsupported resource envelope or a Reference-only result
cannot substitute for a retained persistent-rope obligation.

Final Jim.edict observer cutover, transitional host deletion, save/export,
unrestricted optic algebra, `Seq`, incarnation references, cascading removal,
other targets and distributed capacity finality remain separately scoped.
Fine-grained capacity sharding is not required without a demonstrated need;
serialized membership can satisfy the initial semantics.

This planning change adds no executable capability and changes no normative
specification or current-truth topic shelf. Implementation PRs must update the
owning formal specification, coupled schemas, requirement/test-plan evidence
and topic claims as their behavior lands. GitHub remains the sole live tracker.

# Target IR Topic

Status: current HEAD contract.

This chapter describes Edict's target IR generation boundary. Target IR is the
first target-owned artifact after typed Core. It is selected by an explicit
target profile and remains participant-neutral: producing Target IR does not
execute a runtime, admit a bundle, or mutate participant state.

## Current Contract

Unsigned `core.integer.add<U32/U64>` requires independently checked operand
types and an upper-bound proof that its sum fits the declared width. Literal
and nested sums compose bounds; dominating conjunctive comparisons contribute
limits in either direction. Missing or disjunctive proof refuses raw Core with
`InvalidCoreIdentity` and no Target artifact. Source compilation is not proof
at this boundary. Provider and runtime support remain separate. [TIR-REQ-056]

The current target IR implementation is deliberately narrow:

- selected target profile: `echo.dpo@1` or `gitwarp.ref_crdt@1`;
- selected Target IR artifact domain: `echo.span-ir/v1`, explicitly ordered
  `echo.span-ir/v2`, or
  `gitwarp.commit-reducer-ir/v1`;
- selected source/Core shape: source-ordered pure `let` bindings, the first
  supported effectful Core effect node, Echo `require` guard requirements, and
  typed external-action request data;
- selected outcome: a deterministic target-owned review artifact with canonical
  `edict.canonical-cbor/v1` bytes and a reviewed
  `edict.target-ir.artifact/v1` digest;
- selected failure mode: stable structured target-lowering errors before any
  target artifact is emitted.

The `edict_syntax` crate exposes `lower_to_target_ir`,
`TargetIrLoweringFacts`, `TargetLoweringReport`, `TargetIrArtifact`,
`TargetIrSemanticClosure`, `TargetEffectSignatureFact`, `TargetIrPureBinding`,
`TargetIrExternalActionRequest`,
`encode_target_ir_artifact`, `digest_target_ir_artifact`, and stable
`TargetLoweringFailureKind` values. The lowerer consumes an already-built
`CoreModule` and explicit target-lowering facts supplied by the caller. It does
not read target facts from ambient environment, discover runtimes, or fetch
registries.

Because `CoreModule` is publicly constructible, Target first invokes the shared
whole-module Core type-integrity judgment and consumes its opaque borrowed
witness. Invalid or unresolved type references in any named definition—even an
unused one—or any type-bearing graph surface return one
`InvalidCoreIdentity` failure before Target or result-projection artifact
construction. Target-specific graph and authority checks run only after that
shared border succeeds. The border expands named definitions under the
occurrence's real depth budget, so table order or a prior shallow validation
cannot make an over-depth type lowerable. The built-in provider compatibility
seam delegates to this exact path. [TIR-REQ-044] [TIR-REQ-045]

The crate also exposes `BuiltinTargetLowerer`, `BuiltinLowererRequest`, and
`lower_with_builtin_lowerer` as an
in-process migration seam for the existing Echo and git-warp lowerers. Selection
is explicit and bound to the lowerer's target-profile coordinate. A mismatch
rejects before invocation; once matched, the complete direct
`TargetLoweringReport` passes through unchanged. Tests prove the direct and
compatibility paths produce identical Target IR artifacts, canonical bytes, and
digests. This adapter does not resolve provider manifests, load components, or
define general target plugin dispatch. [TIR-REQ-013]

For Target IR with an explicit Core/lawpack semantic closure,
`TargetLoweringReport` also carries one compiler-owned result-projection
artifact per supported intent and a separate per-intent failure map for results
outside the first closed projection language. This does not make projection
support a precondition for general Target IR consumers. The public
application-build route separately requires exactly one admitted projection and
fails closed on any projection failure. Legacy Target IR that intentionally
omits semantic closure retains empty projection and failure maps plus
byte-identical Target IR encoding. The projection artifact is outside the
Target IR digest preimage and has its own canonical schema and domain-framed
identity; its contract is defined by the
[Result Projections topic](../result-projections/README.md).

`TargetIrLoweringFacts::from_lowerability_report` derives the effect-to-intrinsic
lowering table from accepted native lowerability reports.
The derived facts use the target-profile coordinate and operation profile from
the lowerability report, along with the obstruction coordinates proven by that
report. The caller supplies a prevalidated target-profile reference, and the
bridge rejects references whose coordinate does not match the lowerability
report or whose digest is missing or malformed. Repeated identical native effect
selections are coalesced. Rejected lowerability reports cannot build
target-lowering facts. The v0.9 bridge is native-only: it consumes selected
native effect support for the explicitly supported Echo and git-warp target
profiles and does not perform adapter-chain search or general target plugin
dispatch.

Target-lowering facts also carry the operation profiles selected by
lowerability. A Core intent whose `required_operation_profile` is absent from
that explicit set rejects before Target IR is emitted.

For the supported Echo and git-warp slices, each supported Core effect node
becomes a deterministic Target IR step that records:

- the source Core effect coordinate;
- the effect result binding;
- the selected target intrinsic;
- the structured Core input expression;
- sorted obstruction failure keys and their structured obstruction arm values.

Each supported Core external-action request instead becomes one
`externalActionRequests` entry. It preserves the operation, input and settlement
types, schemas, input, scope, basis, budgets, reconciliation law, compiler-owned
binding, deterministic request id, and fixed awaiting-settlement posture. It
does not emit a target step or target intrinsic. Before copying the request, the
lowerer resolves its settlement metadata and requires the binding to be an
`ExternalActionRequest` with a structurally equivalent settlement type in both
directions. [TIR-REQ-031]

For the supported Echo slice, each supported Core `require` node before any
target step becomes a deterministic Target IR requirement that records:

- the requirement id;
- the structured Core predicate;
- a terminal or `continueObstructed` failure disposition;
- the stable obstruction reason kind and canonical reason payload fields.

git-warp does not currently claim Target IR requirement support. A Core module
with `require` nodes selected for git-warp rejects before artifact emission with
`TargetLoweringFailureKind::UnsupportedTargetFeature`.

In `echo.span-ir/v1`, intent-level Target IR requirements are pre-step guards. A Core `require` after
an emitted target step rejects with
`TargetLoweringFailureKind::UnsupportedTargetFeature` before artifact emission.
If the requirement predicate or reason payload references a local produced by an
earlier target step, lowering uses the same stable failure kind with a more
specific step-output-dependency detail. Selecting v1 never silently upgrades
that contract.

## Explicit ordered execution

`echo.span-ir/v2` under `echo.dpo@1` preserves straight-line interleaving of
effects, pure bindings, and guards. It uses `kind: "orderedTargetIrArtifact"`
and requires `executionOrder` on every intent. That list references each id
in `pureBindings`, `steps`, and `requirements` exactly once, in source order.
Instructions remain in their existing tables; the order list does not duplicate
their payloads. The result is evaluated after the ordered sequence succeeds.
[TIR-REQ-050]

For example, an effect `t.step.0`, derived binding `t.binding.0`, and dependent
guard `t.require.0` produce this review field:

```json
{"executionOrder": ["t.step.0", "t.binding.0", "t.require.0"]}
```

Core-to-Target validation still independently checks types, exact local
identities, lawpack signatures, input-only metadata authority, and
producer-before-consumer availability. Canonical encoding additionally rejects
missing, duplicate, foreign, or forward-dependent order entries, inconsistent
local references, order fields under legacy domains, and v2 without an order.
Predicate and obstruction payload dependencies count too. Input references must
be consistent with one another; their declared type is established by Core
validation, not inferred from the order table. Reordering independent valid
instructions changes the artifact digest. The existing Core semantic closure,
basis, constraints, budget, and result are preserved. [TIR-REQ-050]

The published CDDL has a separate ordered root with a required order list.
Basis and pure bindings still require semantic closure. Schema validation
establishes structural shape; the encoder and Core validator establish the
permutation, dependencies, and type authority. External-action requests,
statement branches, and loops are not supported by this first ordered contract.
There is no runtime execution or atomicity implementation in this compiler
change. [TIR-REQ-050]

Selection is explicit in `TargetIrLoweringFacts::target_ir_domain`, the CLI
projection `target.irDomain`, or a validated lawpack adapter's
`acceptedTargetIr`. A provider must independently support the new contract;
changing an adapter coordinate is not evidence of provider support. Existing
v1 artifacts, digest fixtures, and protected pre-step refusals remain unchanged.
The public CLI exposes the order and referenced pure bindings in review data.

The [Jim consumer witness](../../../scripts/consumer-witnesses/README.md) keeps
the original read/guard source body and demonstrates both boundaries: v1
refuses at Target lowering; an experimental v2 selection reaches the pinned
old Echo provider's schema gate and refuses before provider execution or
package publication. It does not repin Jim or claim a released Echo v2 target
contract. [TIR-REQ-050]

## Shared expression and identity validation

Each Target IR intent also preserves an explicit Core basis expression when
present, the Core input constraints, Core evaluation budget, source-ordered
pure bindings, source-ordered requirements, source-ordered effect steps, and
structured Core result expression for the supported slice. Each pure binding
retains its exact compiler local, Core expression, and deterministic intent-local
identity. The lowerer neither evaluates nor duplicates helper calls and
conditionals. Malformed Core with a dangling, conflicting, forward, self, or
duplicate pure binding fails with `InvalidCoreIdentity` before Target IR exists.
Pure conditional predicates independently prove reference closure, compatible
operand types, and fixed-width integer domains at this boundary, including when
a conditional is nested inside string-shape derivation. Supported pure calls,
including `core.string.concat`, contribute their bounded result type to operand
comparison instead of falling through to literal-only inference. String
concatenation accepts valid raw or NFC bounded operands, including mixed
canonicalizations, and derives the compiler-defined raw UTF-8 result with the
checked sum of their maxima. [TIR-REQ-035] Conditional operands also validate
their nested predicate and compute a bounded least-upper-bound for their branch
results. An exact shared named reference is retained; otherwise the result uses
the canonical structural meaning rather than choosing a branch's provenance.
Byte intervals join their lower and upper bounds component-wise. Lists
recursively join their item shapes and take the wider collection maximum, while
equal-key structural records recursively join each field. This `join`
judgment is distinct from predicate `comparable`: a direct comparison is lawful
only when the complete left shape fits the right or the complete right shape
fits the left. It cannot synthesize a byte union or choose a different
compatibility direction for each record field. Structural Core judgments share
the compiler's finite 128-level type-depth boundary and one Core parser/renderer.
The depth theorem is computed over fully expanded acyclic meaning; named edges
cost zero, structural child edges cost one, and contextual depth is checked at
each use rather than cached as a boolean verdict.
Intrinsics and canonical structural records, lists, options, maps, capability
references, and external requests resolve from syntax; any such `core.types` key
rejects as an identity redefinition before artifact construction. [TIR-REQ-027]
[TIR-REQ-029] [TIR-REQ-032] [TIR-REQ-033] [TIR-REQ-037] [TIR-REQ-038]
[TIR-REQ-042] Field projection uses the same structural record inference. A
direct record or record-valued conditional can therefore supply a field without
inventing a name for its base; predicates, branches, and the selected field
still pass the same recursive type checks. [TIR-REQ-036]
Each imported non-intrinsic pure call must also match exactly one helper fact projected
from a validated lawpack export. Those facts expose read-only identity and
signature accessors but cannot be constructed or mutated by an external caller.
Target lowering checks the canonical coordinate, complete non-generic signature,
exact digest-locked owning lawpack, and independently reconstructs the exact
reachable named-type closure from the signature roots. Structural constructors
are traversed transparently and never need fictitious lawpack ownership. Every
named parent or leaf must remain below the exact lawpack and match caller Core;
missing, extra, foreign, or type-definition-substituted helper evidence rejects
as `InvalidCoreIdentity`; fabricated facts are unrepresentable at the public
API. [TIR-REQ-034] Before copying any expression-bearing Core surface, the lowerer
also traverses intent basis and constraints, requirement predicates and reason
payloads, effect inputs and obstruction values, and every external-request
value. Local references must be available in source order, scalar and declared
types must remain valid, and every executable call must pass the same exact
source-function or lawpack-helper check. The current Core obstruction arm's nonempty,
zero-argument call-shaped constructor remains an opaque application value in
that specific position; adding arguments makes it executable and subjects it to
helper authority. Empty `All` and `Any` aggregates reject through this same
predicate judgment on every copied surface, matching the Core wire schema's
one-or-more cardinality. Effect-failure binder types are checked structurally
against their enclosing effect and failure coordinates. None of these checks
recognize application nouns or verbs. [TIR-REQ-025] [TIR-REQ-039] Before the
supported graph judgment runs, one recursive identity preflight rejects empty
identities in every local table and producer class, including obstruction and
loop binders and nested branch blocks. The canonical encoder also
rejects any local identity produced by more than one pure binding, target step,
or external-action request, and reserves the implicit application-input identity
from those explicit producers, so every downstream result reference remains
unambiguous. This records authored basis, preconditions, evaluation limits, pure
computation, guard dispositions, and success-output semantics without resolving
a runtime basis, executing Echo, or admitting a bundle. [TIR-REQ-018]
[TIR-REQ-019] [TIR-REQ-020] [TIR-REQ-021] [TIR-REQ-022] [TIR-REQ-023]
[TIR-REQ-024] [TIR-REQ-026] [TIR-REQ-028] [TIR-REQ-034] [TIR-REQ-040]

Lawpack-aliased effects have an additional typed authority gate. Validated
lawpack preparation is the only constructor for an effect-signature fact; the
fact binds the source alias to the canonical export, exact manifest digest,
non-generic input/output signature, and resolved exported-type closure. The
compiler checks source calls against that signature for early diagnostics, and
Target validation independently requires exactly one matching fact, reconstructs
the same exact reachable named closure used for helpers, and checks an input that
fits the exported input plus an exported output that fits the result binding.
Missing, extra, duplicate, foreign, value-substituted, or type-definition-
substituted evidence rejects before a Target artifact exists. Native
lowerability facts remain a separate non-lawpack path. [TIR-REQ-030]
[TIR-REQ-043] [TIR-REQ-045]

When any intent has an explicit basis or pure binding, the Core module imports a
lawpack, or the Core module imports a requestable capability, the artifact carries a
`TargetIrSemanticClosure`. The closure binds the exact canonical Core
coordinate/digest plus coordinate-keyed, lowercase digest-locked lawpack and
capability sets. Equivalent resource order and duplicate identical references
canonicalize to the same Target IR identity. Conflicting resources, an empty
source Core coordinate, an unidentifiable Core, a basis-bearing artifact without
its closure, or a request operation absent from the capability closure rejects
before Target IR identity exists. [TIR-REQ-015] [TIR-REQ-017]

Canonical Target IR uses an intentional artifact-envelope value model rather
than Rust struct serialization. The reviewed digest is SHA-256 over canonical
CBOR for:

```text
["edict.digest/v1", "edict.target-ir.artifact/v1", <canonical Target IR value>]
```

The canonical value includes the artifact's own domain, digest-locked target
profile resource, non-empty source Core coordinate, optional semantic closure,
sorted intent map, optional explicit basis expressions, input constraints, Core
evaluation budget, source-ordered pure bindings and exact expressions,
source-ordered requirements, requirement predicates and failure dispositions,
source-ordered target steps, source-ordered external requests, sorted
obstruction failure keys and arms, and structured Core result expression. Target
profile and semantic-closure digests are strict artifact
references: missing digests and non-lowercase `sha256:<64 hex>` review strings
reject before hashing.

Reviewed Echo and git-warp Target IR byte/digest goldens live under
`fixtures/target-ir/canonical/`. `cargo xtask target-ir-goldens --check`
regenerates them from executable lowering and canonical encoding, and
`cargo xtask verify` includes that check.

[`edict-target-ir.cddl`](../../abi/edict-target-ir.cddl) defines the
`target-ir-artifact` root from that canonical value shape. The provider contract
pack assembles it with the common and Core rules it references, verifies the
complete rule closure, and checks both reviewed Target IR artifacts through the
compiled root. This is structural wire-schema evidence for valid
lowering-produced artifacts; the lowerer and encoder continue to own semantic
identifier validity and canonical ordering or deduplication of set-like
fields. The schema has separate closed and legacy artifact variants, so an
external artifact carrying an explicit basis cannot validate after its semantic
closure is removed. [TIR-REQ-014] [TIR-REQ-015]

Bundle assembly can now consume a real `TargetIrArtifact` through
`assemble_contract_bundle_from_target_ir`. That path computes
`targetIrDigest` from canonical Target IR bytes and writes the same digest into
the manifest, recomputes the expected semantic closure from the exact supplied
Core, and requires both the artifact closure and bundle lawpack set to match.
Standalone canonical Target IR encoding provides deterministic structural
self-validation; it cannot reconstruct dependencies that a caller erased from
an in-memory artifact. The computed assembly path is the separate
cross-artifact corroboration boundary. The supplied-reference assembly path
remains available for already-digested external artifact graphs, but the
computed Target IR path has no caller-supplied target IR digest field.

Selecting a target profile outside the explicit supported set rejects with
`TargetLoweringFailureKind::UnsupportedTargetProfile`. Selecting an unsupported
Target IR domain rejects with
`TargetLoweringFailureKind::UnsupportedTargetIrDomain`. Selecting Echo without a
digest-locked target-profile reference, or selecting git-warp without a
digest-locked target-profile reference, rejects with
`TargetLoweringFailureKind::UndigestedTargetProfile`. Supplying a Core module
with an unsupported ABI rejects with
`TargetLoweringFailureKind::UnsupportedCoreAbi`. Supplying a Core module with
floating imports rejects with `TargetLoweringFailureKind::UndigestedCoreImport`.
Supplying unsupported Core capability flags rejects with
`TargetLoweringFailureKind::UnsupportedCoreCapability`. Supplying Core nodes
outside the supported pure binding, effect, external-request, and Echo
requirement shapes
rejects with
`TargetLoweringFailureKind::UnsupportedCoreNode`. Supplying a target-specific
Core feature that the selected target does not support rejects with
`TargetLoweringFailureKind::UnsupportedTargetFeature`. Missing or ambiguous
effect lowering facts, non-Echo target intrinsics, missing operation-profile
support, and obstruction keys absent from the selected target facts also reject
before any artifact is emitted. A Core intent with no target-owned pure bindings,
requirements, steps, or external requests, or a Core module with no intents, rejects with
`TargetLoweringFailureKind::NoTargetSteps`. Duplicate target-lowering facts are
ambiguous only when they match an effect used by the Core module being lowered;
unrelated duplicate facts do not block the supported artifact.

## Deferred

The following are not implemented by this slice:

- Echo runtime execution;
- Echo verifier completeness;
- admission generation;
- general target-lowering plugin dispatch;
- git-warp runtime execution, commit object creation, and CRDT reducer
  verification;
- Echo runtime receipts for first-class resumable obstruction strands;
- Echo request admission, adapter execution, and settlement resumption;
- additional target profiles beyond Echo and git-warp;
- v2 chained or composite adapter resolution.

The verification matrix is tracked in [test-plan.md](./test-plan.md).

## List literal boundary

A Core `list` expression fits its declared `List<T, max=N>` only when it has at
most `N` elements and each element independently fits `T` with available
authority. Target checks this without trusting the source compiler: excess
elements, wrong element types, foreign locals and non-list binding types refuse
with `InvalidCoreIdentity` and no artifact. These checks do not claim runtime
list evaluation. [TIR-REQ-058]

## Target pure intrinsic boundary

Target accepts a call to a target pure intrinsic only against facts pinned to
exactly the lowering target profile and a matching digest-locked target import,
with the contract's arity, argument fit and result type. It recomputes each
call's cost itself from operand bounds it establishes: a literal's size, the
larger branch of a conditional, or the resolved type maximum. Helper calls are
charged to the calling intent, with each helper body metered from a fresh
expression depth and call height bounded separately. Unknown coordinates, type
arguments, wrong arity, unaccepted arguments, missing or foreign facts, a
source function claiming an intrinsic coordinate, and budgets below the
recomputed cost refuse with `InvalidCoreIdentity` and no artifact. These checks
do not claim provider or runtime execution. [TIR-REQ-059]

## Byte-length boundary

`core.bytes.length` accepts exactly one operand and one exact operand type
coordinate resolving to bounded structural Bytes, and returns U64. Target
validation independently checks local authority, operand shape, exact type
coordinate, and result compatibility before emitting a pure binding or result
projection. It does not trust the source compiler's earlier checks. Forged
signatures, dangling operands, coordinate substitution, and narrowed result
widths reject with `InvalidCoreIdentity` and no target artifact. Runtime
interpretation remains a separate consumer obligation.

Byte-length type inference reuses the operand's validated coordinate instead of
performing separate authority and type walks. A deterministic test-only counter
bounds repeated validation through nested conditional predicates; public tests
also reject an unavailable local hidden in the innermost length call. These
checks do not claim a bound on every compiler pass. [TIR-REQ-048] [TIR-REQ-049]

## Byte UTF-8 predicate boundary

`core.bytes.isUtf8<OperandType>(bytes)` retains exactly one structural byte
coordinate and one operand, producing Bool. Target validation independently
checks the operand's authority, exact coordinate and result compatibility.
Forged arity, substituted coordinates, foreign locals and non-Boolean binding
results refuse with `InvalidCoreIdentity` and no artifact. A nested slice still
requires the existing totality proof in its evaluation scope. These checks do
not claim provider or runtime UTF-8 execution. [TIR-REQ-055]

## Byte count boundary

`core.bytes.count<OperandType>(bytes, b"x")` retains exactly one structural byte
coordinate, one operand and one constant needle of exactly one byte, producing
U64. Target validation independently checks the operand's authority, exact
coordinate, needle shape and result compatibility. Forged arity, substituted
coordinates, empty, multi-byte or non-constant needles, foreign locals and
non-U64 binding results refuse with `InvalidCoreIdentity` and no artifact.
Target's own unsigned-addition proof bounds the count by the structural operand
maximum and refuses sums that could exceed the width. These checks do not claim
provider or runtime execution. [TIR-REQ-057]

## Byte-slice boundary

`core.bytes.slice<OperandType>(bytes,start,end)` takes one exact structural byte
coordinate and three operands, including two U64 endpoints. Its result is
`Bytes<max=N>` for the resolved operand maximum N. Target inference validates
each operand once per inference call and independently checks local authority,
endpoint widths and the conservative result bound.

A separate Target-owned totality walk requires `start <= end <= len(bytes)`
from exact conjunctive input evidence or unconditional equal/zero relations.
Within an ordered `All`, each child must be total before its predicate becomes
proof for later children. The walker retains borrowed predicate references,
not cloned Core trees. Reordering a partial expression before its proof refuses
with `InvalidCoreIdentity`. This scope is confined to the conjunction: separate
input constraints retain their canonical set encoding and cannot justify one
another or the earlier basis. [TIR-TP-084]
It traverses all expression-bearing intent locations. Basis predicates and each
separate input predicate start without assumptions from other input constraints.
Forged signatures, missing proofs,
foreign locals and incorrect proof scope reject with `InvalidCoreIdentity` and
no artifact. This walk is shared with subtraction, while the Target proof
routine remains independent from the source compiler. [TIR-REQ-051]

Public Core integrity validation rejects graph nesting beyond 128 edges before
Target traversal. The totality walker independently applies that same ceiling,
carrying one depth counter across expression/predicate transitions and nested
blocks rather than resetting it at a node-family boundary. Over-limit totality
checks fail closed. [TIR-REQ-052]

The existing Call schema carries the operation without changing wire shape or
prior canonical identities. This compiler capability supplies no UTF-8 or rope
semantics, runtime slice execution, or byte-copy budget charging. A provider may
accept and package the preserved call while the evaluator still lacks execution
support; a successful build or verifier report does not discharge those consumer
obligations.

## Byte-concatenation boundary

This section owns the implemented byte-concatenation lowering decision.

`core.bytes.concat<LeftType,RightType>(left,right)` carries two exact structural
byte coordinates and ordered operands. Target independently checks both arities,
resolves both byte bounds without unwrapping nominal types, and infers each
operand once per inference call to validate its coordinate and local authority.
The checked sum of maxima yields `Bytes<max=L+R>`; positive minimum and exact
length are not inferred. Overflow, forged signatures, unavailable operands,
or incompatible destination bounds return `InvalidCoreIdentity` with no
artifact. [TIR-REQ-053]

| Relationship | Targets |
| --- | --- |
| `refines` | [EDICT-LANG-LEN-001](../../SPEC_edict-language-v1.md#refined-scalar-types) |
| `supersedes` | none |
| `depends_on` | [`COREIR-REQ-002`](../core-ir/README.md#current-contract), [TIR-REQ-044](#current-contract) |
| `related` | [`CSPINE-REQ-043`](../compiler-spine/test-plan.md#bounded-byte-concatenation), [TIR-REQ-053](./test-plan.md#bounded-byte-concatenation) |

The existing Call encoding carries this generic operation. It does not add
text interpretation, application limits, byte-copy charging, or evaluator
execution. An accepted provider package is separate evidence from runtime
support for the preserved call.

## Unsigned subtraction boundary

The target lowerer recognizes `core.integer.subtract<U32/U64>` with exactly two
same-width operands. A target-owned Core walk independently checks that each
subtraction is total from identical operands, zero, ordered constants, or exact
operand order in conjunctive input predicates. It does not invoke the source
compiler's proof routine. Basis and input predicates are checked without using
those predicates as already-established evidence. Disjunctive and negated
predicates do not authorize subtraction.

This is Target IR validation, not provider acceptance or Echo execution. The
adversarial cases and public application witness are tracked under TIR-REQ-046
and issue #212; producer locks remain unchanged. An isolated Jim application
copy with a guarded deleted-byte-count expression builds successfully through
the pinned provider, while execution through the original Echo evaluator still
returns `UnsupportedProgram`.

Operand type inference validates each nested subtraction once per type walk,
including operands reached through conditionals and record fields. A returned
type coordinate includes integer-literal and local-reference validation;
comparison shape inference reuses that judgment instead of validating the same
subtree again. Test-only visit counters check this work bound without elapsed-time
thresholds. Public lowering tests retain rejection of nested overflowing,
negative-unsigned, and malformed integer literals. The existing canonical
container nesting ceiling still applies. [TIR-REQ-047]


## Source-owned function authority

Source calls resolve against the complete Core function table, separately from
imported lawpack facts. Independent Target checking validates every function's
ordered local frame, binding/result types, exact call signature and totality,
including definitions unused by intents. A source/imported authority collision
refuses. Core preflight rejects cycles and source paths over 128 frames; this
is not a proof of combined imported runtime depth. [TIR-REQ-054]

Calls, ordered arguments and conditional/pre-body placement are copied without
inlining. Any nonempty source-function table forces the semantic closure to bind
the exact complete Core digest, even for a require-only intent without imports
or a basis. Existing `NoTargetSteps` and result-projection restrictions remain.
Provider schema admission and pure/read runtime function execution require the
consumer to select and implement the new published contract explicitly.

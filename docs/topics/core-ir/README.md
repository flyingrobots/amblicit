# Core IR Topic

Status: current HEAD contract.

This chapter describes the Core IR contract that exists today. It began as the
`v0.2.0-alpha.1` semantic model and schema contract; the compiler-spine topic
now owns the initial source-to-in-memory-Core lowerer. This shelf also owns the
reference canonical encoder for Core modules. It is still not a Core digest,
golden-fixture, target-lowering, or admission claim.

## Public Surface

The machine-readable Core schema is [`docs/abi/edict-core.cddl`](../../abi/edict-core.cddl).
Its top-level artifact is `core-module`, and every Core module declares
`apiVersion: "edict.core/v1"`. [COREIR-REQ-001] [COREIR-REQ-008]

Core is downstream of the source AST. The source parser still returns source
AST, not Core; the compiler-spine shelf owns the initial executable lowering
from source AST to in-memory Core.

The `edict_syntax` crate exposes `encode_core_module` for
`edict.canonical-cbor/v1` Core bytes, `decode_canonical_cbor` for canonical byte
validation, `encode_canonical_cbor` for decoded canonical values, and
`digest_core_module` for the domain-separated `edict.core.module/v1` digest. The
public `MAX_CANONICAL_NESTING_DEPTH` is 128; encoding or decoding the next nested
container rejects with stable `CanonicalErrorKind::NestingLimitExceeded`. The
human walkthrough is [canonical-encoding.md](./canonical-encoding.md).
[COREIR-REQ-012] [COREIR-REQ-013] [COREIR-REQ-014] [COREIR-REQ-017]

The crate also exposes `validate_core_module_type_integrity`. A raw
`CoreModule` is an untrusted candidate; successful validation returns an opaque
borrowed `ValidatedCoreModule` witness plus stable structured failure kinds and
paths on rejection. Canonical encoding and digesting, Target lowering, and
public result-projection emission and verification all cross this same border
before producing or accepting an artifact. Named definitions are memoized by
their context-free maximum structural expansion height; each occurrence then
applies its own remaining depth budget, so earlier shallow validation cannot
bless a later deep use. First-time summary construction also bounds structural
descent, so a long acyclic named chain returns `DepthExceeded` before exhausting
the stack. Depth failures identify the checked table or graph occurrence; other
failure kinds retain their specific child paths. [COREIR-REQ-027] [COREIR-REQ-028]

The same validation boundary limits executable graph descent to 128 edges.
Each basis expression, input predicate, and intent body starts at depth zero.
Expression and predicate children consume one edge, including transitions
between those families; entering a nested branch or loop block also consumes
one. A block's nodes and result retain its current depth. Exceeding the limit
returns `DepthExceeded` at the first over-limit structural path, before canonical
or Target traversal. Canonical Core encoding maps that integrity failure to
`UnsupportedValue`; it does not reach the later canonical container walk
that would return `NestingLimitExceeded`. This is a separate budget from semantic type expansion
and canonical CBOR container nesting: passing the graph-depth judgment does
not promise that the encoded containers fit the canonical nesting budget.
[COREIR-REQ-029]

The Core module schema does not embed reviewed golden bytes, exact Core
digests, target IR, or admission bundles. Reviewed Core artifact fixtures live
outside the schema under `fixtures/core/canonical/`. [COREIR-REQ-007]
[COREIR-REQ-014]

## Current Contract

- Core modules carry imports, type definitions, source functions, intents, and required Core
  capabilities. Imports are digest-locked `resource-ref` values, but the Core
  module does not contain its own self-hash field. [COREIR-REQ-001]
  [COREIR-REQ-007]
- Core types cover bounded scalars, records, variants, options, lists, maps,
  capability references, and typed external-action requests. Runtime-sized
  collections remain explicitly bounded at the Core schema boundary. Integer
  type and value identity retains exact width and signedness. Byte payloads
  carry an explicit maximum and may carry a minimum; equal bounds represent an
  exact byte length, while max-only values omit the minimum from canonical Core.
  Every Core type reference classifies without module state as fixed intrinsic,
  canonical self-describing structure, or a name. One shared parser and renderer
  owns bounded strings and bytes, records, options, lists, maps, capability
  references, and external-action requests. Structural record fields are sorted
  identifiers whose child references recurse under the same 128-level bound;
  parse/render round trips are byte-exact. Intrinsic and structural references
  resolve from themselves and may not occur as `core.types` keys. Only names
  enter that table, so `Unit`, `Bool`, numeric intrinsics, and structural syntax
  cannot be shadowed or redefined before canonical identity.
  [COREIR-REQ-002]
  [COREIR-REQ-019] [COREIR-REQ-020] [COREIR-REQ-022] [COREIR-REQ-023]
  [COREIR-REQ-025] [COREIR-REQ-026]
- Structural type compatibility and imported compiler type resolution share one
  finite depth ceiling of 128. A compiler-accepted type chain at that boundary
  remains compatible downstream instead of encountering an earlier Target IR
  cutoff; deeper or cyclic caller-built shapes still fail closed.
  [COREIR-REQ-024]
- The whole-module type-integrity judgment eagerly validates every named
  definition, including unused hash-significant definitions, and recursively
  validates every type reference carried by intents, local tables, producers,
  binders, expressions, requests, predicates, reasons, nested blocks, and
  results. It enforces supported scalar definitions, canonical and resolvable
  children, nominal contract equality, cycle policy, and the shared depth
  ceiling. Depth is measured after acyclic named expansion: named references do
  not consume an edge or reset the budget, each structural child consumes one,
  depth 128 is accepted, and depth 129 rejects independently of table order,
  root order, or prior traversal. Malformed raw Core therefore cannot mint
  canonical bytes, a digest, Target IR, or projection authority.
  [COREIR-REQ-027] [COREIR-REQ-028]
- Core expressions and predicates are separate schema families. Expressions
  compute values; predicates express boolean obligations and input constraints.
  [COREIR-REQ-003]
- Core blocks contain explicit locals, ordered nodes, and a result expression.
  Nodes cover local binding, semantic effects, guards, branches, bounded loops,
  match blocks, and proof obligations. A branch may carry one optional result
  binding; when present, the selected block result becomes that local and the
  binding participates in canonical Core identity. When absent, the canonical
  encoder omits the field, preserving existing statement-only branch bytes and
  digests. [COREIR-REQ-004] [COREIR-REQ-018] [COREIR-REQ-021]
- The Rust Core IR model and reference canonical encoder represent the first
  semantic effect-node shape: binding, effect coordinate, input expression, and
  deterministic obstruction map. They also represent `require` nodes with
  distinct terminal and preserved-obstruction failure arms. [COREIR-REQ-015]
  [COREIR-REQ-016]
- External-action requests are distinct Core nodes, not semantic effects. They
  bind exact operation, schema, scope, basis, budget, and reconciliation data
  plus fixed awaiting-settlement posture. Canonical encoding requires the
  operation resource to remain in the module's capability imports.
  [COREIR-REQ-020]
- Local references are alpha-stable: each `local-ref` carries a compiler-owned
  `id`, normalized `alphaName`, and type reference. Source binder spelling is
  not identity. [COREIR-REQ-005]
- Input constraints are explicit `input-constraint` records containing a source
  coordinate, origin class, and predicate tree. They are not validator coordinate
  strings. [COREIR-REQ-006] [EDICT-CORE-WHERE-HASH-001]
- Intents state the required operation profile as `requiredOperationProfile`.
  An intent may also carry one typed basis expression; when present, that
  expression participates in canonical Core bytes and digest. Verifier reports,
  runtime basis resolution, and target/admission decisions remain external to
  Core. [COREIR-REQ-009] [COREIR-REQ-018]
  [EDICT-CORE-VERIFIED-EXTERNAL-001]
- Source-owned functions use an optional nonempty `functions` map. Each entry
  carries ordered parameters, a return type and an effect-free pure body.
  Ordered immutable bindings have their own lexical frame; public validation
  rejects captures, forward references, malformed binders, recursion and source
  call paths above 128 frames. Empty maps are omitted from canonical encoding;
  local alpha-renaming preserves identity while signature/body/callee changes
  and unused definitions remain hash-significant. [COREIR-REQ-030]
- Edict-authored lawpack pure helper bodies use `core-fn-body`, a pure function
  body shape. They do not reuse the effect-capable `core-block` node algebra.
  [COREIR-REQ-011]
- Schema-shape fixtures prove minimal accepted Core module/intent shapes and
  rejected missing or external-evidence fields against the CDDL declarations.
  [COREIR-REQ-010]
- Core modules can be encoded as deterministic `edict.canonical-cbor/v1` bytes
  from their semantic value tree. Validation decodes canonical bytes and
  requires byte-identical re-encoding, rejecting non-canonical forms such as
  non-minimal integers. Integer encoding additionally rejects values outside
  the exact domain named by their Core width. [COREIR-REQ-012]
  [COREIR-REQ-013] [COREIR-REQ-019]
- Canonical encoding and decoding allow at most 128 nested arrays or maps. The
  root starts at depth zero, so a scalar wrapped in exactly 128 containers is
  valid; depth 129 rejects with `NestingLimitExceeded` before deeper recursive
  traversal. Existing values below the limit retain their bytes and digests.
  [COREIR-REQ-017]
- Core module digests use SHA-256 over the canonical digest frame
  `["edict.digest/v1", "edict.core.module/v1", <Core module value>]` and render
  for review as `sha256:<64 lowercase hex>`. The first reviewed Core golden
  artifacts cover the `bounded-hello` fixture and are regenerated with
  `cargo xtask core-goldens --write`. [COREIR-REQ-014]
- Canonical encoder behavior is covered for map-order independence, mutation
  sensitivity, decode/re-encode stability, primitive integer width stability,
  effect-node mutation sensitivity, require-failure arm mutation sensitivity,
  exact nesting-boundary rejection, digest stability, digest mutation
  sensitivity, and source alpha-renaming invariance. [COREIR-REQ-005]
  [COREIR-REQ-012] [COREIR-REQ-013] [COREIR-REQ-014] [COREIR-REQ-015]
  [COREIR-REQ-016] [COREIR-REQ-017]
- External request coverage proves byte reproducibility, mutation sensitivity,
  exact capability closure, a fixed-seed 32-case identity corpus, and a bounded
  64-request module. [COREIR-REQ-020]

## Deferred

The following are not implemented by the Core IR contract:

- import resolution and resolved type checking;
- full source-to-Core language coverage;
- full CDDL instance validation;
- target-profile lowering;
- bundle/admission artifacts.

Those items belong to the compiler-spine, lowerability, and admission milestones
tracked in the roadmap.

The verification matrix is tracked in [test-plan.md](./test-plan.md).

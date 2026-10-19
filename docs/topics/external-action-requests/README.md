# External-Action Requests

Status: current HEAD contract.

Edict can construct a typed request for one external operation without
performing that operation. The request is deterministic compiler data. Echo
later admits and records it, an authorized adapter performs the boundary
crossing, and a schema-validated settlement may resume the program. This topic
owns request construction only.

## Source Surface

A requestable operation family enters source through a digest-locked
`capability` import:

```edict
use capability workspace.snapshot.observe@1
  digest "sha256:7fcb985591d116a1624716334209dd5bf3948dfd756028dc32104621f9e90f71"
  as snapshot;
```

The alias is callable only in a `request` statement:

```edict
request pending: ExternalActionRequest<Bytes<max=65536>> =
  snapshot(input.payload)
  input schema workspace.snapshot.input@1 digest "sha256:c390651d8975c49ff148a332feba4054a53fa9867e0412c1c303e0771cda1096"
  settlement schema workspace.snapshot.settlement@1 digest "sha256:efff3be35fba0aeeadc59e73fe771cb42213d390a269bbc5b2e24a028bb84832"
  authority input.scope
  basis input.basis
  budget maxSettlementBytes input.maxSettlementBytes maxAttempts input.maxAttempts
  reconcile workspace.snapshot.reconcile@1 digest "sha256:352dab01263aa1a1f01a8ac213be0debef1bf23d7b522dce24527993124315c9";
```

The operation takes one typed input. Scope, basis, and both budgets are ordinary
typed expressions, so their runtime values survive compilation for Echo
admission. Schemas and the reconciliation law are exact digest-locked
resources. [EXTREQ-REQ-001] [EXTREQ-REQ-006]

## Core And Target IR

Core emits `ExternalActionRequest` as a distinct node containing:

- compiler-owned binding identity;
- exact operation resource;
- input and settlement type coordinates;
- exact input and settlement schemas;
- input, authority-scope, and basis expressions;
- maximum settlement bytes and attempt expressions;
- exact reconciliation law;
- fixed `awaitingSettlement` state and `schemaRequired` settlement posture.

The operation resource must equal one `capability` import. Canonical Core
encoding fails closed when that closure is missing. [EXTREQ-REQ-002]
[EXTREQ-REQ-004]

Target lowering copies the node into `externalActionRequests`. It never turns
the operation into a target step or `targetIntrinsic`. The Target IR semantic
closure binds the exact capability resource alongside the source Core and any
lawpacks; canonical Target IR encoding refuses a request whose operation is
absent from that closure. Before artifact emission, Target validation also
requires `settlementType` to resolve and the compiler-owned binding to be an
`ExternalActionRequest` whose settlement is structurally equivalent in both
directions. Caller-authored Core cannot change only the metadata while retaining
the old request binding. [EXTREQ-REQ-003] [EXTREQ-REQ-004]

Equivalent source and compiler facts produce byte-identical Core and Target IR.
Every operation, schema, input, scope, basis, budget, or reconciliation change
participates in canonical identity. [EXTREQ-REQ-005]

## Public Application Build

An `edict.application/v1` manifest selects the request-only route with
`"buildKind": "externalAction"`. Its `externalActionResources` array names the
canonical artifacts for every input schema, settlement schema, and
reconciliation law referenced by the source. The build loads those artifacts
alongside the exact source, complete lawpack dependency set, declarative
adapter, request-profile configuration, and provider-owned target profile. It
then:

1. compiles and lowers through the real lawpack closure;
2. requires at least one typed request and zero callable Target IR steps;
3. rejects supplied lawpacks unreachable from the first/root manifest;
4. requires the source budget selected for each request-only profile to equal
   that profile's exact declared obligation;
5. binds each request operation digest to an exact root-reachable lawpack
   manifest digest without inventing a namespace or version relationship
   between the two independent resource coordinates;
6. decodes each external-action resource as canonical
   `edict.external-action-resource/v1`, validates its closed schema or
   reconciliation meta-contract, and recomputes its
   `edict.external-action-resource/v1` domain-framed identity;
7. requires the configured resource set to equal the Target request closure:
   no missing, substituted, duplicate, disconnected, malformed, or placeholder
   artifact survives;
8. writes the owning encoders' exact `core.cbor` and `target-ir.cbor` bytes.

Publication is a locked pair replacement. A failure restores the previous pair,
and a successful request build removes stale executable-operation package and
verification-report outputs. The route does not invoke a provider component or
perform an external action. [EXTREQ-REQ-009]

Resource resolution grants no performance authority. Schema artifacts describe
canonical request or settlement values. Reconciliation artifacts declare the
terminal postures, required bindings, and effect-free replay rule. They cannot
read, write, spawn, or call a provider component. [EXTREQ-REQ-011]

## Authority Boundary

Request authority is not performance authority:

- a capability alias cannot be invoked as an ordinary semantic effect;
- the current request-family allowlist contains only the domain-specific
  `workspace` root used by `workspace.snapshot.observe@1` and
  `workspace.patch.applyValidated@1`;
- raw `filesystem`, `process`, `network`, Git, GitHub, `model`, and `shell`
  operation families, case variants, abbreviations, and unregistered roots are
  rejected with `UnrequestableExternalOperation`;
- compiling and lowering perform no I/O;
- the compiler/provider component interface gains no callable import;
- dynamic path, ref, basis, budget, adapter, and settlement constraints remain
  Echo admission obligations.

The first admitted family is domain-specific
`workspace.snapshot.observe@1`, not ambient filesystem access.
[EXTREQ-REQ-003] [EXTREQ-REQ-006] [EXTREQ-REQ-007]

## Validated Patch Request

`workspace.patch.applyValidated@1` is the second compiler-owned request family.
Its generated lawpack closure binds the canonical patch-input schema, exact
workspace-root basis class, writable-path-policy authority class, CI-workflow
exclusion policy, settlement schema, postcondition class, and reconciliation
law. The application source carries the patch, authority scope, basis, and
budgets as typed request expressions. Core and Target IR contain one request
and zero callable steps. [EXTREQ-REQ-010]

These declarations bind the validation contract; they do not grant write
authority or validate a live path or workspace basis. Echo must admit the
dynamic request instance and a separately authorized adapter must perform any
mutation. The compiler, lowerer, verifier, and provider-component interface
remain free of filesystem access.

## Waiting And Settlement

The request binding and Target IR request id identify explicit
`awaitingSettlement` program state. No native stack, continuation, callback, or
`async` host frame is serialized. This Edict slice does not define settlement
execution or resumption; Echo owns durable request, claim, settlement, recovery,
and replay semantics.

## Deferred

- Echo admission and durable settlement of the compiler-emitted request;
- bounded workspace-observation adapter execution;
- settlement-driven deterministic resumption;
- Echo execution and settlement of the basis-bound validated patch request;
- Git, GitHub, process, network, timer, and model adapters;
- the autonomous delivery loop.

The verification matrix, fixed seed, and stress bound are in
[test-plan.md](./test-plan.md).

Nested request blocks cross the shared Core integrity boundary before canonical
closure inspection. With atomic operands, graph depth 128 can reach closure
inspection; depth 129 returns Core `DepthExceeded`, mapped to canonical
`UnsupportedValue`. The separate canonical CBOR container ceiling still applies
after integrity and closure validation. [EXTREQ-REQ-004]

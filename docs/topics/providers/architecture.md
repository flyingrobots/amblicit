# Provider Host Architecture

Status: current branch structure for the external provider authority boundary.

## Authority Flow

```text
validated provider manifest
  -> selected component role and exact world identity
  -> resolver-supplied bytes
  -> digest and top-level contract-attestation verification
  -> Wasmtime decode plus exact import/export/type preflight
  -> prepared typed component
  -> validated request proof + identical concrete schema registry authority
  -> invoke once, or replay twice through distinct fresh bounded Stores
  -> typed lower or verify call per Store
  -> bounded result lifting per call
  -> pure canonical/schema/envelope admission per call
  -> sealed provider outcome or sealed replay observation
```

Wasmtime is an enforcement mechanism, never an Edict authority source. The
validated manifest authorizes one digest and world. A resolver obtains bytes, but
the host independently reproduces the digest before parsing them. Structural
component compatibility does not prove the nominal contract: exactly one
top-level custom section named `edict:target-provider-contract` must contain the
selected `edict:target-provider/{lowerer|verifier}@1.0.0` identity. Because the
section is inside the already verified bytes, it is digest-covered.

Standard componentization exposes the protocol's named types through the exact
type-only `edict:target-provider/protocol@1.0.0` instance import. The host permits
that import only when every member is a type. Every callable or unknown import
rejects before instantiation. No WASI dependency, WASI linker, filesystem,
network, environment, clock, randomness, registry, or callback capability is
present.

## Engine And Store

`ProviderComponentHost` owns one Wasmtime 48.0.8 engine. The configuration
enables the component model and fuel, disables epochs, SIMD, relaxed SIMD, tail
calls, memory64, multi-memory, and copy-on-write initialization, canonicalizes
NaNs, and fixes the maximum Wasm stack. Epoch interruption is absent, so this
slice makes no wall-time or watchdog determinism claim.

Every invocation uses `Store::try_new` to create a fresh store containing only a
resource limiter. Generated typed bindings call exactly one `lower` or `verify`
export. A prepared lowerer cannot be passed to the verifier path or vice versa.
The prepared value retains its creating engine, and invocation rejects a
different host engine during authority preflight rather than reclassifying that
host misuse as a provider instantiation failure.
The validated request retains its schema-validator capability, and invocation
requires that object to be the same concrete registry whose manifest equals the
prepared component's manifest. This prevents independently valid authority
objects from being mixed at the final call.

## Replay And Isolation

Replay is a host operation, not a provider assertion. `replay_lowerer` and
`replay_verifier` call the existing invocation path twice with the same prepared
component, opaque validated request, concrete schema authority, and limits.
Because the invocation path creates its store internally, neither run can reuse
guest memory, tables, resources, or instance state from the other.

A completed replay observation is equal only when the complete sealed outcomes
are equal. A rejected observation uses `ProviderHostFailureIdentity`, which
contains the stable kind, phase, and structured validation report while
excluding bounded engine diagnostic prose. Replay mismatch distinguishes a
completed-versus-rejected disposition change, a changed completed outcome, and
a changed stable host-failure identity.

Executable evidence covers concurrent calls through one host and prepared
component, recovery after trap, fuel, memory denial, malformed lifting, and
response rejection, isolation between two prepared providers, repeated
preparation, and equivalent completed and rejected replays. Two independent
test processes also reproduce the reviewed Echo-shaped Target IR bytes and
domain-framed digest through the generic fixture lowerer. The host has no
compiled-component cache, so repeated preparation exercises the current
cache-free path.

## Limit Units

The host keeps distinct limits because their units and replay meaning differ:

| Limit | Unit and enforcement point |
| --- | --- |
| `max_input_bytes` | Checked sum of provider request WIT-logical strings and byte lists before instantiation. |
| `max_output_bytes` | Maximum logical response bytes the validated request may authorize, checked before instantiation. |
| `max_diagnostic_bytes` | Checked provider-authored diagnostic UTF-8 bytes after lifting. |
| `max_wasm_memory_bytes` | Guest linear-memory allocation enforced by the store resource limiter. |
| table/instance/memory counts | Store resource counts enforced by the resource limiter. |
| `max_wasm_fuel` | Deterministic Wasm guest-work fuel. |
| `max_hostcall_bytes` | Wasmtime host-call fuel for guest-to-host list/string lifting; approximately byte-scaled and separate from Edict logical accounting. |
| `max_host_diagnostic_bytes` | Maximum engine diagnostic bytes retained by the host-owned error adapter. |

The pure response validator separately applies the request's exact output count,
diagnostic count, and logical response-byte limits. Post-lift validation alone is
not treated as allocation containment; guest memory and Wasmtime host-call fuel
bound the earlier phases.

## Failure Boundary

`ProviderHostFailureKind` owns stable categories for component digest, decode,
contract, instantiation, input, fuel, resources, response lifting, guest traps,
malformed responses, logical response limits, diagnostics, response admission,
and host invariants. `ProviderHostPhase` identifies configuration, preflight,
compile, instantiate, lower, verify, or response validation. Wasmtime errors are
retained only as bounded opaque diagnostics and do not define public failure
identity. Pure admission failures preserve their structured validation report.
Wasmtime 46 exposes store count-limit exhaustion through a pinned diagnostic
adapter rather than a public typed error; the exact runtime version and a zero
instance-budget regression test ratchet that mapping.

Checked malicious fixtures witness each execution boundary, including an invalid
canonical-ABI result discriminant, an instantiation-only data-segment failure,
and fuel exhaustion from a core start function. The fixture inventory binds
source inputs and component SHA-256 digests. The `provider-runtime-dependencies`
xtask also pins the direct and resolved Wasmtime feature closure and rejects
`wasmtime-wasi` or a second workspace owner.

## Residual Boundary

The modeled guest failure classes return stable host failures and leave later
invocations and the compiler test process usable. This is an in-process native
Wasmtime host, not an operating-system process sandbox: it does not claim
containment from an implementation fault in Wasmtime or trusted Edict host code.
There is also no manifest-backed resolver, package loader, mutable component
cache, browser component runtime, Echo-owned production provider, target runtime
execution, or admission execution in this crate.

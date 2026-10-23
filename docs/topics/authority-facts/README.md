# Authority Facts Topic

Status: current HEAD contract.

This shelf describes Edict's authority-facts loading and canonical byte
boundary. An authority-facts document is a deterministic, digest-bound input
that can provide the first compiler facts already modeled by `CompilerContext`:
operation profiles, profile write-class allowances, effect write classes, and
budgets. Callers may use the explicit JSON review/input form or the normative
`edict.canonical-cbor/v1` representation. [AUTHFACTS-REQ-001]
[AUTHFACTS-REQ-006]

This is not trusted authorship governance. The loader proves that facts came
from explicit files with digest-locked source identity and stable validation
behavior. It does not decide whether a participant trusts the author, reviewer,
identity system, or policy behind those files.

## Public Surface

The public authority-facts surface lives in `amblicit_syntax`:

- `load_authority_facts_file` parses and validates one JSON authority-facts
  document. [AUTHFACTS-REQ-001]
- `encode_authority_facts_cbor` projects a validated document into the frozen
  canonical value shape, and `decode_authority_facts_cbor` reconstructs the
  existing document model only after canonical and structural validation.
  [AUTHFACTS-REQ-006] [AUTHFACTS-REQ-007]
- `digest_authority_facts_document` computes the domain-framed artifact digest
  under `edict.authority-facts/v1`. [AUTHFACTS-REQ-006]
- `load_compiler_context_from_authority_fact_files` loads one or more files and
  merges their facts into a deterministic `CompilerContext`.
  [AUTHFACTS-REQ-002]
- `AuthorityFactsLoadFailureKind` gives stable failure categories for malformed
  files or bytes, non-digest-locked sources, invalid fact coordinates, invalid
  write classes, duplicate facts, malformed canonical shapes, and conflicting
  facts. [AUTHFACTS-REQ-004] [AUTHFACTS-REQ-009]

Authority-facts documents identify their source as either `lawpack` or
`targetProfile`, with a coordinate and `sha256:<64 hex>` digest. The source
identity is evidence binding for the loaded facts; it is not participant trust
policy.

## Canonical ABI

The normative root is `authority-facts` in
[`docs/abi/edict-authority-facts.cddl`](../../abi/edict-authority-facts.cddl).
It is assembled with `edict-common.cddl` for the shared typed SHA-256 digest:

```text
{
  apiVersion: "edict.authority-facts/v1",
  source: { kind, coordinate, digest: ["sha256", 32-byte value] },
  operationProfiles: {
    source-coordinate => { core, allowedWriteClasses: { write-class => null } }
  },
  effectWriteClasses: { effect-coordinate => write-class },
  budgets: { budget-coordinate => { maxSteps, maxAllocatedBytes, maxOutputBytes } },
}
```

The JSON loader remains an explicit review/input surface. Its fact arrays and
embedded fact coordinates project to coordinate-keyed canonical maps; its
`sha256:<64 hex>` source digest projects to typed digest bytes. The binary
decoder reverses that projection into `AuthorityFactsDocument`, normalizing the
review digest to lowercase. [AUTHFACTS-REQ-006] [AUTHFACTS-REQ-007]

Fact map keys are unique by canonical CBOR construction. Allowed write classes
are also a canonical map-set: each class is a key whose value is the `null` unit
marker. Encoding deduplicates the input list into that map, while canonical CBOR
fixes its key order. Reordering source declarations therefore cannot move
canonical bytes or the domain-framed digest. [AUTHFACTS-REQ-008]

## Current Contract

- File-backed profile and budget facts can resolve the `bounded-hello` compiler
  fixture without caller-constructed in-memory context facts.
  [AUTHFACTS-REQ-002]
- File-backed profile write-class allowances and effect write classes
  participate in compiler profile/effect compatibility checks. A write-class
  effect under a read-only loaded profile rejects with
  `ProfileEffectMismatch`. [AUTHFACTS-REQ-003]
- Authority-facts loading is deterministic. The loader consumes the exact file
  paths provided by the caller and does not discover directories, fetch
  registries, read environment configuration, or mutate dependency state.
  [AUTHFACTS-REQ-001]
- Duplicate fact coordinates inside one document reject before a
  `CompilerContext` is returned. Across separate digest-consistent documents,
  an identical fact is harmless while a different value at the same coordinate
  is a conflict. [AUTHFACTS-REQ-004] [AUTHFACTS-REQ-008]
- The canonical ABI rejects non-canonical CBOR before shape interpretation,
  rejects unknown or mistyped fields before semantic validation, and then uses
  the existing coordinate, digest, write-class, and conflict validation path.
  [AUTHFACTS-REQ-007] [AUTHFACTS-REQ-009]
- The reviewed runtime-neutral fixture is regenerated and checked with
  `cargo xtask authority-facts-goldens --write` and `--check`; the full
  `cargo xtask verify` gate runs check mode. [AUTHFACTS-REQ-006]

## Deferred

The following are not implemented:

- full `edict.lawpack/v1` manifest instance validation;
- full `edict.target-profile/v1` file-backed manifest loading;
- intrinsic, obstruction, obligation, adapter, footprint, and cost corpus
  validation beyond the first compiler-context facts;
- author/reviewer provenance validation;
- signatures, trust roots, revocation, registry selection, or Continuum
  participant acceptance policy.

The verification matrix is tracked in [test-plan.md](./test-plan.md).

# Crate Scope Decision

Status: decided for the audit-remediation branch. Implementation is deferred.

## Decision

Edict should eventually split the current overbroad `amblicit-syntax` crate into
layered crates behind an umbrella crate. A simple rename is not the preferred
long-term fix.

The target package shape is:

| Future crate | Intended responsibility |
| --- | --- |
| `amblicit-syntax` | Lexing, parsing, source AST, and syntax-adjacent diagnostics. |
| `amblicit-core` | Surface validation, compiler context, compiler spine, Core IR, canonical Core encoding, and Core digest helpers. |
| `amblicit-targets` | Target-profile conformance, lowerability, and Target IR artifact construction/canonicalization. |
| `amblicit-admission` | Contract-bundle assembly/validation and Edict-owned Gate C request/receipt validation. |
| `amblicit` | Curated umbrella re-exports for the supported public alpha API. |

The dependency direction should be acyclic:

```text
amblicit
  -> amblicit-admission
      -> amblicit-targets
          -> amblicit-core
              -> amblicit-syntax
```

The exact split can be narrower when implemented, but dependency direction must
not invert. Parser or AST code must not depend on admission or bundle types.

## Why Not Rename Only

A rename such as `edict-core` would make the current package name less wrong,
but it would not create compile-time layer boundaries. The audit finding is not
just naming; it is that parser, compiler, target, bundle, and admission surfaces
all live in one crate. Renaming the crate would keep that architecture intact.

## Why Not Split In This Branch

This audit branch is closing several behavior, tooling, and documentation
findings. Moving public APIs across packages would create a large semver and
review surface without changing runtime behavior. It would also interfere with
the ongoing release-train evidence that currently cites `amblicit_syntax` public
items directly.

The package split should be its own pre-1.0 migration slice with:

- an API inventory of every current public re-export;
- a staged compatibility plan for downstream callers;
- dependency-cycle tests or metadata checks;
- topic-shelf updates for every moved public surface;
- release notes that explain whether old crate paths remain as re-exports.

## Current Branch Rule

Until the split lands, new code may continue to live in `amblicit-syntax` only when
it preserves the internal layer order documented in
[`ARCHITECTURE.md`](../../ARCHITECTURE.md):

```text
syntax -> semantic/compiler -> Core/canonical -> targets -> bundles/admission
```

New code must not add reverse dependencies across those conceptual layers.

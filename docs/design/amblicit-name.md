# Amblicit Name Decision

Status: accepted decision with staged implementation. Stages 1 (product prose),
2 (package, library and binary names) and 3 (source extension and editor
grammars) are current. Stage 4 is planned. Until it lands, the names it replaces
remain the current names, and this page lists them.

## Decision

The language and project are named **Amblicit**, pronounced am-BLISS-it. The
name was previously Edict.

- **Slogan:** *Use only as declared.*
- **Description:** lawful computation within explicit bounds.

The word joins *ambit* (the bounds of what something may reach), *licit*
(lawful) and *explicit*. It follows the pattern of *implicit* and *explicit*.

The term **lawpack** does not change. A lawpack is a digest-locked law package
that an Amblicit program imports.

## Reasons

1. An edict is a decree from an authority. An Amblicit operation does not
   command. It proposes a change, and the destination runtime decides whether
   to admit it. Spec invariants I-014 (assurance is evidence, not authority)
   and I-028 (admission evidence is external) state this rule.
2. Other projects use the name Edict. On 2026-10-23, the
   [`edict` crate](https://crates.io/crates/edict) on crates.io was a published
   entity-component-system library, and the most-starred GitHub repository
   named `edict` was
   [`cft0808/edict`](https://github.com/cft0808/edict), a multi-agent
   orchestration system.
3. On 2026-10-23, `amblicit` and `amblicit-lang` were not registered on
   crates.io: the crates.io API returned HTTP 404 for
   `https://crates.io/api/v1/crates/amblicit` and
   `https://crates.io/api/v1/crates/amblicit-lang`. A
   [GitHub repository search for `amblicit`](https://github.com/search?q=amblicit&type=repositories)
   returned no repositories.

## Stages

Each stage is one pull request. Each stage leaves `main` in a working state.

| Stage | Scope | Posture |
| --- | --- | --- |
| 1 | Product prose: the root `README.md`, this note, and one entry in the documentation router that links to this note | Current |
| 2 | Package names, the facade library `amblicit` and the CLI binary `amblicit` | Current |
| 3 | Source extension `.amblicit`, editor grammars and the scope name `source.amblicit` | Current |
| 4 | Wire identifiers `amblicit.*/vN` as new versions | Planned |

Rules for the stages:

- Stage 3 renames file names and editor names only. Values inside canonical
  bytes keep the `edict` spelling until Stage 4: the lawpack export source
  kind `"edict"`, the auxiliary artifact kinds `edict-source` and
  `edict-source-closure`, and the WIT package `edict:target-provider`. Files
  named after a wire identifier, such as `docs/schemas/edict.*.schema.json`
  and `docs/abi/edict-*.cddl`, also keep their names until Stage 4. The
  lawpack build coordination file `.<name>.edict-lawpack-build.lock` keeps
  its name.
- Stage 4 adds new versioned identifiers. Each existing `edict.*/vN`
  identifier keeps its meaning at its existing version. Two names must not
  produce the same canonical bytes.
- Stage 4 changes canonical bytes, digests and golden artifacts. Downstream
  consumers, such as Echo and Hello Echo, change after Stage 4 merges.
- After Stage 1 merged, the GitHub repository was renamed from
  `flyingrobots/edict` to `flyingrobots/amblicit`. GitHub redirects the old
  URLs.
- Other prose, such as the specifications and topic shelves, changes with the
  stage that renames the identifiers it describes.

## Current names

These names are current. Use them in commands, code and artifacts. A surface
with a planned name keeps its current name until the matching stage lands:

| Surface | Current name | Planned name | Stage |
| --- | --- | --- | --- |
| Crates (packages) | `flyingrobots-amblicit`, `amblicit-cli`, `amblicit-syntax`, `amblicit-provider-schema`, `amblicit-provider-host-wasmtime` (previously `flyingrobots-edict`, `edict-cli`, `edict-syntax`, `edict-provider-schema`, `edict-provider-host-wasmtime`) | none, renamed | 2 |
| Facade library | `amblicit` (package `flyingrobots-amblicit`; previously `edict`) | none, renamed | 2 |
| CLI binary | `amblicit` (previously `edict`) | none, renamed | 2 |
| CLI stdin-cap environment variable | `AMBLICIT_CLI_MAX_STDIN_BYTES` (previously `EDICT_CLI_MAX_STDIN_BYTES`) | none, renamed | 2 |
| Source extension | `.amblicit` (previously `.edict`; the CLI default `directoryExtensions` is `[".amblicit"]` and does not fall back to `.edict`) | none, renamed | 3 |
| Editor scope | `source.amblicit` (previously `source.edict`) | none, renamed | 3 |
| Tree-sitter grammar | `amblicit` in `grammars/tree-sitter-amblicit` (previously `edict` in `grammars/tree-sitter-edict`) | none, renamed | 3 |
| TextMate grammar | `grammars/textmate/amblicit.tmLanguage.json`, name `Amblicit` (previously `edict.tmLanguage.json`, name `Edict`) | none, renamed | 3 |
| VS Code/Cursor extension | package `amblicit-vscode`, language id `amblicit` (previously `edict-vscode`, `edict`) | none, renamed | 3 |
| Wire identifiers | `edict.*/vN` | `amblicit.*/vN`, new versions | 4 |
| Repository | `flyingrobots/amblicit` (previously `flyingrobots/edict`; GitHub redirects) | none, renamed | after 1 |

## Relationships

| Relationship | Targets |
| --- | --- |
| `refines` | none |
| `supersedes` | none |
| `depends_on` | [DOCS-REQ-007](../topics/documentation/README.md#durable-decision-discipline) |
| `related` | [Language specification](../SPEC_edict-language-v1.md), [README](../../README.md) |

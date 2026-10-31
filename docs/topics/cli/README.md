# CLI Topic

Status: current HEAD contract for the first public command-line surface.

This chapter describes the Edict CLI stream contract. The CLI is a compiler and
validation boundary, not an interactive pretty-printer. Every input record and
every output record is JSON Lines.

## Public Surface

The `amblicit` binary reads compiler requests from stdin as JSONL records. It emits
only JSONL records on stdout and stderr. Stdin is bounded before request parsing:
the default cap is 8 MiB, and `AMBLICIT_CLI_MAX_STDIN_BYTES` may override that cap
with a positive byte count. Over-limit input is rejected with an `InputTooLarge`
CLI diagnostic and exit `2`. [CLI-REQ-001, CLI-REQ-010]

The binary takes no positional arguments. The only accepted flags are
`--help`/`-h` and `--version`/`-V`, which emit a single `edict.cli.info/v1`
record on stdout and exit `0`; the `help` record carries the usage summary, the
accepted request schema identifiers, and the exit-code contract. Any other
argument is rejected with an `InvalidArguments` diagnostic and exit `2`.
[CLI-REQ-009]

The implemented operations are `build`, `check`, and `project`.

A `build` request contains one settings record and no compiler-input records.
It selects exactly one document: `application` points to an
`edict.application/v1` manifest, while `lawpack` points to an
`edict.lawpack-build/v1` authoring document. `checkOnly` is accepted only with
`lawpack`, and either selected document path must be non-empty.

```json
{"schema":"edict.compiler.settings/v1","type":"compilerSettings","operation":"build","application":"edict.application.json"}
```

### Lawpack Builds

A lawpack build accepts application-owned typed declarations, constructs the
canonical manifest, exports, adapters, local resources, and lowercase digest
sidecars, and sends the exact bytes through the existing public lawpack,
adapter, and complete dependency-graph validators before publication:

```json
{"schema":"edict.compiler.settings/v1","type":"compilerSettings","operation":"build","lawpack":"edict.lawpack.json"}
```

All paths inside the lawpack build document are resolved relative to that
document and confined beneath its directory. The output directory is an
Edict-owned generated tree identified by `edict.lawpack-output.json`. A write
build replaces that complete tree transactionally and therefore removes stale
owned artifacts. It refuses a non-empty unowned directory or a symlinked
ownership index, and it refuses to place one owned output inside another owned
lawpack tree. Write builds acquire shared intent locks for proper ancestors and
an exclusive lock for the output itself, so disjoint sibling outputs remain
parallel while parent/child or identical output footprints conflict. After
coordination, Edict traverses every publication-root component from the
filesystem root without following symbolic links, then pins the output parent as
a capability directory; staging, activation, rollback, and cleanup cannot follow
a later ambient-path replacement. Publication moves never replace an existing
destination; a concurrently installed empty directory is preserved and causes
a typed failure. Write publication is supported on Apple targets, Linux,
Android, and Redox. Windows and other targets without Edict's atomic
no-replace directory-move backend return `LawpackOutputWriteUnsupported` before
reading the build document or mutating its namespace. `checkOnly` remains
available on unsupported non-Windows targets. Windows lawpack builds fail
closed before document I/O in both modes, with `LawpackCheckUnsupported` for
`checkOnly`, because Edict has no stable Windows filesystem-identity backend.
The document directory is the publication
namespace; callers must not concurrently publish overlapping trees from
different document roots. Pure preflight derives fixed artifacts and sidecars and rejects reserved
namespaces, duplicates, ancestor collisions, filesystem NUL, nonportable names,
trailing-dot aliases, overlong paths, and case aliases before output inspection,
coordination, or dependency I/O. Raw output-directory paths use one portable
`/`-separated grammar, reserve internal names case-insensitively, and leave room
for every derived lock filename. Every dependency path component must be real;
Edict opens each one without following symbolic links and retains the accepted
file identity through overlap checks and bounded reading. Existing output
directories and traversed dependency parents are also compared by filesystem
identity, so a case-insensitive alias cannot route an input back under the
replaceable output tree. A check-only build
does not repair the owned artifact tree and
reports `LawpackOutputDrift` unless the complete existing tree is
byte-identical. It creates no directories or lock files and rechecks the
ownership basis after traversal:

```json
{"schema":"edict.compiler.settings/v1","type":"compilerSettings","operation":"build","lawpack":"edict.lawpack.json","checkOnly":true}
```

The authoring format, local-versus-external resource identity, publication
policy, and artifact ownership model are documented in the
[lawpack-authoring guide](../lawpack-authoring/README.md).

### Application Builds

An application manifest names an ordered, explicit source closure, its complete
lawpack closure, the selected target profile and provider package, and the
output directory. The first source is the entry package; up to 63 remaining
sources supply its complete digest-authenticated module dependencies. Both
application routes accept a non-empty ordered lawpack closure whose first
entry is the root. They validate the
complete supplied dependency graph, reject any supplied lawpack unreachable
from that root, compile and lower the source through the root lawpack's
declarative target adapter, and resolve the selected target profile only from
its checked provider-package manifest.

Source loading performs no discovery. Duplicate canonical source paths,
disconnected or substituted modules, missing dependencies and escaped paths
fail before publication. The fixed 1 MiB aggregate source limit applies before
parsing. Surface validation runs on each original module before closure
authentication, so invalid declarations retain their detailed diagnostics.
Parse, linker and compiler diagnostics retain each original file's
canonical path and local byte span, including errors from several modules.

A source without module imports keeps the existing `edict.source/v1` byte-string
artifact. Module builds use the authenticated `edict.source-closure/v1` artifact
and `edict-source-closure` auxiliary kind in both provider invocations. A provider
must support that source form explicitly; older providers refuse it. Source
module support does not expand the existing source-function accounting profile,
which continues to refuse request-bearing values.

The application path is resolved from the process working directory. Every
path inside the manifest is resolved from the manifest's parent directory and
must be non-empty, relative, and free of parent traversal. A minimal manifest
has this shape:

```json
{
  "schema": "edict.application/v1",
  "coordinate": "examples.hello_echo@1",
  "sources": ["src/hello_echo.amblicit"],
  "lawpacks": [{
    "manifest": "vendor/causal-cell/manifest.cbor",
    "exports": "vendor/causal-cell/exports.cbor",
    "adapter": "vendor/causal-cell/adapter.cbor",
    "targetConfiguration": "vendor/causal-cell/echo-operation-configuration.cbor"
  }],
  "target": {
    "profile": "echo.dpo@1",
    "providerPackage": ".build/echo-provider"
  },
  "outputDirectory": ".build/application"
}
```

`buildKind` defaults to `executableOperation`. Setting it to `externalAction`
selects the request-only route explicitly:

```json
{
  "schema": "edict.application/v1",
  "buildKind": "externalAction",
  "coordinate": "examples.workspace_observer@1",
  "sources": ["src/observe-workspace.amblicit"],
  "lawpacks": [{
    "manifest": "vendor/workspace-snapshot/manifest.cbor",
    "exports": "vendor/workspace-snapshot/exports.cbor",
    "adapter": "vendor/workspace-snapshot/adapter.cbor",
    "targetConfiguration": "vendor/workspace-snapshot/request-profile-configuration.cbor"
  }],
  "externalActionResources": [
    {"artifact": "vendor/workspace-snapshot/input-schema.cbor"},
    {"artifact": "vendor/workspace-snapshot/settlement-schema.cbor"},
    {"artifact": "vendor/workspace-snapshot/reconciliation-law.cbor"}
  ],
  "target": {
    "profile": "echo.dpo@1",
    "providerPackage": ".build/echo-provider"
  },
  "outputDirectory": ".build/application"
}
```

The request-only route requires at least one compiler-emitted external-action
request, rejects any callable Target IR step, and requires every request
operation digest to equal one exact root-reachable lawpack manifest digest; the
operation coordinate remains its own independently versioned resource identity.
Every input schema, settlement schema, and reconciliation law must resolve
through `externalActionResources` to one canonical
`edict.external-action-resource/v1` artifact. The build validates the resource
meta-contract and exact domain-framed identity and rejects missing, duplicate,
disconnected, substituted, opaque, non-canonical, or sentinel resources.
The list is bounded to 192 artifacts. Executable-operation builds reject a
non-empty resource list. The source budget must equal the exact obligation
declared by its selected request-only profile. `providerPackage` remains
required because the route loads and verifies its provider manifest and
selected target-profile artifact. Omitting it is `InvalidApplicationConfig`,
but no provider component is invoked. The owning canonical encoders publish:

- `core.cbor`;
- `target-ir.cbor`.

The pair is deterministic and transactionally replaces any previous
application output pair. Switching build kinds removes stale outputs from the
other route. Runtime admission, request execution, settlement, recovery, and
replay remain outside Edict. [CLI-REQ-016]

The executable-operation build invokes the provider's checked lowerer component and its structurally
separate verifier component through the capability-denied provider host. Only
an accepted verification result reaches the output directory. The current Echo
target writes the exact provider-emitted bytes as:

- `executable-operation-package.cbor`;
- `verification-report.cbor`.

Edict does not re-encode either artifact and does not execute the package.
Concurrent writers are excluded, and replacement preserves the previous pair
if either output cannot be published.

For the singleton executable-operation route, target lowering emits
compiler-authored `edict.result-projection/v1` artifacts or explicit per-intent
projection failures. The application build then requires exactly one artifact,
independently verifies it against exact Core and Target IR, and binds its
canonical bytes plus `edict.result-projection.artifact/v1` identity into both
provider semantic-input closures. Providers that implement the previous
six-input closure refuse before publication; Echo #698 owns package inclusion
and runtime consumption of the new seventh input.

Provider diagnostics are fail-closed on this first public build route: any
provider-authored diagnostic rejects publication, independent of its severity.
The terminal status `checked` count is `1` after a successful build because the
request processes one application manifest and its complete referenced closure.
[CLI-REQ-015]

A `check` request accepts:

- inline source code;
- one file path;
- one directory path;
- an ordered list of file paths;
- one glob pattern.

Each input is represented by a JSON object whose `schema` is
`edict.compiler.input/v1`. Raw source code is not a separate text mode; it is
the `source` field of a JSONL input record. [CLI-REQ-002]

A `project` request uses the same input records, including inline `source`
records for dirty editor buffers whose contents do not have to exist on disk.
It emits editor-facing projection records for requested slots:

- lexical syntax spans;
- diagnostics, always as a projection record when requested, even when empty,
  and also when a requested syntax projection cannot be produced because lexing
  fails;
- Core review JSON plus the canonical Core digest;
- Target IR review JSON plus the canonical Target IR digest.

Projection review JSON is display data for tools. The digest fields are
computed from the existing canonical Core and Target IR encoders; the projection
JSON is not a canonical hash contract. When present, review JSON exposes the
hash-significant Core and Target IR basis expressions plus the Target IR
source-Core/lawpack semantic closure, so a digest change is not paired with an
otherwise indistinguishable review payload. Compiler-level source or lowering
failures remain projection data on stdout, so editor adapters can distinguish a
bad buffer from a broken CLI transport. [CLI-REQ-013, CLI-REQ-014]

Compiler settings are represented by a JSON object whose `schema` is
`edict.compiler.settings/v1`. The request is trusted local input by default:
path, directory, path-list, and glob records read files with the caller's
filesystem privileges. Callers that accept untrusted request records can set
`inputRoot` in compiler settings; then every resolved filesystem input must stay
within that root, or the CLI rejects the request with `InputPathOutsideRoot` and
exit `2`. Inline source records are not filesystem reads. [CLI-REQ-003,
CLI-REQ-011]

Amblicit source files use the `.amblicit` extension. A directory input selects
the files whose extension is in `directoryExtensions`, and the default set is
exactly `[".amblicit"]`. The CLI does not fall back to the legacy `.edict`
extension: a directory input selects a `.edict` file only when the caller lists
`.edict` in `directoryExtensions`. Path, path-list, and glob inputs read exactly
the files that the caller names or that the glob pattern matches; they do not
filter by extension. An inline `source` record without `name` reports the
display name `inline.amblicit`. [CLI-REQ-002]

Successful `check` results are emitted to stdout. A successful `build` emits
only its terminal status record to stdout after the accepted artifacts have
been written or checked. `check` compiler diagnostics, build failures, CLI input errors,
and failure status records are emitted to stderr. `project` projection records,
including compiler diagnostics and lowering failures, are emitted to stdout
when the request itself is valid. Both streams use one JSON object per line
with no banners, spinners, blank lines, or direct human prose outside JSON
string fields. [CLI-REQ-001, CLI-REQ-014, CLI-REQ-015]

When a CLI-input failure happens after the requested operation is known, the
diagnostic and terminal status records carry that command. Invalid `project`
settings therefore report `command: "project"` rather than falling back to the
`check` command. [CLI-REQ-006, CLI-REQ-007]

### Application Source Diagnostics

A failed application build emits one `edict.cli.diagnostic/v1` record for each
parser or compiler error on stderr, followed by its existing
`InvalidApplicationSource` or `ApplicationCompilationFailed` summary and one
terminal status record. Each source record retains the originating stage and
kind, its message, and the source span. The summary does not embed a Debug
vector of compiler errors. When ownership cannot be established, each error's
human-readable message remains in the summary without a guessed file location.
The terminal `errors` count includes every source
record and the summary; `checked` stays zero, and the existing build exit code
is `2`. A failed source build writes no output and preserves prior artifacts.
[CLI-REQ-018]

`span.start` and `span.end` are zero-based UTF-8 byte offsets, with an exclusive
end. `sourceLocation` contains the canonical source path, a one-based source
line, and a one-based Unicode-scalar column. An empty EOF span has the location
immediately after the last source character. CRLF advances the source line once.
These coordinates refer to the source loaded for this build. The top-level
`line` field, when present, continues to identify a JSONL request record; it is
not a source line or column. [CLI-REQ-018]

The diagnostic and event schemas now admit `build`; the diagnostic schema also
admits compiler stages and the optional `sourceLocation` object. Check and
project stream shapes are unchanged. Clients that validate build records must
use the matching checked schemas. Schema validation is exercised by the public
binary tests and is not added to the shipped CLI runtime. [CLI-REQ-018]

## Stream Contract Artifacts

Every record family on the CLI boundary has a checked-in JSON Schema. These
schemas are the stable contract for callers; the CLI does not embed a schema
validation engine. The binary rejects compiler input records with fields outside
the closed `edict.compiler.input/v1` schema variants, so callers should treat
the checked-in schemas as the accepted wire shape.

| Record `schema` | Direction | Artifact |
| --- | --- | --- |
| `edict.compiler.settings/v1` | stdin | [`compiler-settings`](../../schemas/edict.compiler-settings.v1.schema.json) [CLI-REQ-003] |
| `edict.compiler.input/v1` | stdin | [`compiler-input`](../../schemas/edict.compiler-input.v1.schema.json) [CLI-REQ-004] |
| `edict.cli.check-result/v1` | stdout | [`cli-check-result`](../../schemas/edict.cli-check-result.v1.schema.json) [CLI-REQ-005] |
| `edict.projection.syntax/v1` | stdout | [`projection-syntax`](../../schemas/edict.projection-syntax.v1.schema.json) [CLI-REQ-013] |
| `edict.projection.diagnostics/v1` | stdout | [`projection-diagnostics`](../../schemas/edict.projection-diagnostics.v1.schema.json) [CLI-REQ-013, CLI-REQ-014] |
| `edict.projection.core/v1` | stdout | [`projection-core`](../../schemas/edict.projection-core.v1.schema.json) [CLI-REQ-013, CLI-REQ-014] |
| `edict.projection.target-ir/v1` | stdout | [`projection-target-ir`](../../schemas/edict.projection-target-ir.v1.schema.json) [CLI-REQ-013, CLI-REQ-014] |
| `edict.cli.diagnostic/v1` | stderr | [`cli-diagnostic`](../../schemas/edict.cli-diagnostic.v1.schema.json) [CLI-REQ-006] |
| `edict.cli.event/v1` | stdout/stderr | [`cli-event`](../../schemas/edict.cli-event.v1.schema.json) [CLI-REQ-007] |
| `edict.cli.info/v1` | stdout | [`cli-info`](../../schemas/edict.cli-info.v1.schema.json) [CLI-REQ-009] |

## Exit Codes

- `0`: request completed successfully. Application `build` wrote accepted
  provider artifacts; lawpack write mode published its complete owned tree;
  lawpack `checkOnly` verified exact existing bytes without repairing them. For
  `project`, this can include compiler diagnostics or Target IR lowering
  failures emitted as projection records.
- `1`: compiler or validation diagnostics were produced for at least one
  source input in the `check` operation.
- `2`: CLI input or usage was invalid, or a build failed, including source
  parsing or compilation. Build failures keep this existing exit code.

## Golden Fixtures

The CLI contract is pinned by a checked-in golden corpus under
[`fixtures/cli/`](../../../fixtures/cli/). Each case is replayed end-to-end
through the binary and its stdout, stderr, and exit code are matched
byte-for-byte. The corpus covers success, parse and semantic rejection,
CLI-input rejection, and the deterministic path, directory, path-list, and glob
expansion paths, including optional root-confinement rejection. [CLI-REQ-008]

## Deferred

The following are not implemented by this first CLI slice:

- general-purpose contract-bundle assembly;
- runtime admission and execution workflows;
- human-pretty output mode;
- Echo execution;
- language-server transport.

The verification matrix is tracked in [test-plan.md](./test-plan.md).

## Signature mismatch diagnostic context

Application build diagnostics include optional `signatureMismatch` context on a compiler `TypeMismatch` at the `typeCheck` stage. It contains `effect`, `position` (`input` or `receipt`), `path`, `expectedType`, and `actualType`. Field segments carry a separate name; list-item and settlement segments carry only their kind. Exactly absent fields use JSON null for the corresponding type. Other errors omit this property. Existing source spans, source locations, summaries, status counts, and failed-output publication rules are preserved.

The diagnostic and projection schemas admit this optional context. Consumers pinned to an older strict schema must update that schema before accepting the new property. Records without the property remain valid. The projection formatter preserves supplied compiler detail; current public projection inputs do not load a lawpack signature closure. No new projection transport or capability is introduced.

Core expression review retains variant constructors as an explicit `kind: variant`
record with the exact `type`, `case` and optional `payload`; absent payload is
omitted. The payload's own review shape is preserved. This is display/projection
of compiler-built Core, not a runtime value codec. [CLI-REQ-013]

## Match review data

Core expression review JSON renders a match as `kind: "match"`, a recursively
rendered `scrutinee`, and an `arms` array. Each arm has `case` and `body`; a
payload-binding arm also has `binder` in the existing local-reference review
shape. This is inspection data, not a runtime execution or provider-acceptance
claim. [CLI-REQ-013]

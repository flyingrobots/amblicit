# Author an Application-Owned Lawpack

This guide shows an application repository how to turn one reviewable JSON
definition into the canonical lawpack artifacts consumed by `edict application
build`. The application owns the vocabulary and declarations. Edict owns schema
checking, canonical CBOR, digest framing, closure validation, and publication.
No provider component or runtime participates in authoring.

## Authoring Workflow

### 1. Create the Review Document

Create an `edict.lawpack-build/v1` document in the application repository. Its
paths are relative to the document, not the process working directory:

```json
{
  "schema": "edict.lawpack-build/v1",
  "outputDirectory": "generated/example-text",
  "lawpack": {
    "schema": "edict.lawpack-authoring/v1",
    "id": "example.text",
    "version": "1",
    "acceptedCoreAbi": ["edict.core/v1"],
    "dependencies": [],
    "exportsCoordinate": "example.text.exports/v1",
    "exports": {
      "types": [{
        "coordinate": "example.text@1.Key",
        "definition": "String<max=64>"
      }],
      "constants": [],
      "pureFunctions": [],
      "effects": [],
      "obstructions": [],
      "operationProfiles": {}
    },
    "targetAdapters": [],
    "verifier": {
      "class": "declarative",
      "ruleset": {
        "id": "example.text.verifier/v1",
        "digest": "sha256:1111111111111111111111111111111111111111111111111111111111111111"
      }
    },
    "compatibility": {
      "id": "example.text.compatibility/v1",
      "digest": "sha256:2222222222222222222222222222222222222222222222222222222222222222"
    },
    "conformanceFixtureCorpus": {
      "id": "example.text.fixtures/v1",
      "digest": "sha256:3333333333333333333333333333333333333333333333333333333333333333"
    },
    "localResources": []
  },
  "dependencyBundles": []
}
```

### 2. Submit the Build Request

Submit the document through the normal JSONL CLI:

```json
{"schema":"edict.compiler.settings/v1","type":"compilerSettings","operation":"build","lawpack":"edict.lawpack.json"}
```

### 3. Inspect the Authored Artifacts

On success, `generated/example-text/` contains:

- `manifest.cbor` and `manifest.sha256`;
- `exports.cbor` and `exports.sha256`;
- each declared adapter and its `.sha256` sidecar;
- each declared local resource, including target configuration, and its
  `.sha256` sidecar;
- `edict.lawpack-output.json`, the ownership and review index for the generated
  directory.

The `.sha256` files are lowercase review strings ending in one newline. The
canonical artifacts carry typed digest bytes internally; review strings are not
their wire representation.

### 4. Check Generated Output

Use `checkOnly` in CI or before committing vendored output:

```json
{"schema":"edict.compiler.settings/v1","type":"compilerSettings","operation":"build","lawpack":"edict.lawpack.json","checkOnly":true}
```

Check-only mode performs the same bounded reads, construction, decoding, and
closure validation, then compares the complete output tree byte for byte through
one pinned directory handle reached by traversing every document-root component
from the filesystem root without following symbolic links, then traversing real
output-parent directories from that retained root capability. The final
output-directory open does not follow a
symbolic link installed after inspection. Before returning success it
retraverses that parent chain from the retained root, verifies that both the
parent and requested output name still denote the pinned filesystem identities
and ownership basis, rejects an ownership index that appeared at the retained
root or any output ancestor, then validates the exact tree a second time through
the reopened handle. Each inspected artifact and child directory is opened
relative to its retained parent without following symbolic links.
It never repairs or changes the owned artifact tree and creates no parent
directories or lock files. This is optimistic two-pass validation, not an atomic
filesystem snapshot; an uncooperative process can still mutate after the final
observation.
Missing, changed, or extra output is `LawpackOutputDrift`.
Missing output ancestors also report `LawpackOutputDrift`; non-directory or
symlinked ancestors report `LawpackOutputOwnershipFailed`. Check-only never
reports `LawpackOutputWriteFailed` because it performs no publication,
including during initial output-path resolution. Its
parent-chain gate rejects every component that is not a normal relative name.

## Resource And Dependency Identity

An Edict helper body is part of the exported semantic content. For example,
changing a valid helper's returned integer from `7` to `8` changes the generated
exports and manifest identities even when its coordinate and signature stay
the same. Rebuild the lawpack, review the new manifest digest, and update the
application's exact source import before building the application. The old pin
rejects with `InvalidApplicationClosure`; it neither replaces prior application
output nor publishes output into a fresh directory. With the new exact pin,
the change reaches compiled Core and supported Target artifacts.
[LAUTH-REQ-008]

An external resource uses an application-supplied exact identity:

```json
{"id":"echo.dpo@1","digest":"sha256:7777777777777777777777777777777777777777777777777777777777777777"}
```

A local resource places reviewable JSON in the authoring document:

```json
{
  "name": "target-config",
  "coordinate": "example.text.target-config/v1",
  "output": "resources/target-config.cbor",
  "value": {"maxBytes": 4096}
}
```

Other declarations refer to it as `{"local":"target-config"}`. Edict converts
the value to canonical CBOR and derives its coordinate-framed identity. JSON
objects of the exact form `{"$edictBytes":"00ff"}` represent canonical byte
strings; floating-point JSON numbers are rejected. Lawpack authoring permits at
most 48 containing arrays or objects around a terminal value; another
container is rejected before recursive conversion. Constant values and Edict
pure-function bodies reserve three of those levels for the exports map, member
array, and member map that enclose them in the canonical artifact. This
authoring-specific limit leaves normal-thread stack headroom while the general
canonical-CBOR profile retains its independent 128-container limit.

Every `dependencies` edge names an id, version, and exact manifest digest. The
matching canonical manifest and exports paths appear in `dependencyBundles`.
The caller must supply the complete transitive closure and nothing disconnected
from the authored root. Edict decodes each dependency, corroborates every pin,
and runs the same complete-graph validator used by application builds. When the
output directory already exists, dependency traversal retains its filesystem
identity and rejects any dependency parent with that identity. This closes
case-insensitive and other filesystem-equivalent aliases that a lexical path
comparison cannot see.

### 5. Apply Publication Policy

The lawpack contract assigns `outputDirectory` exclusively to this build after
its output index is present. The filesystem does not enforce that ownership
against another process that can mutate the publication parent. Among
publishers that honor Edict's footprint locks, a write build stages a complete
sibling directory, preserves the old directory, activates the replacement, and
restores the old directory if activation fails. A later successful build
replaces the whole owned directory, so artifacts removed from the definition
cannot survive as stale output. An
internal transaction or backup name is fixed-length and independent of the
user-selected output component, so every accepted output name remains
publishable within portable component limits. Output-directory components that
could alias Edict's `.edict-lawpack-...` transaction names or hidden
`.*.edict-lawpack-build.lock` coordination files are reserved case-insensitively
and reject before filesystem access. Raw output-directory paths use `/` as their
only separator, reject empty and platform-reserved components, and cap each
component at 229 bytes so its derived lock name remains portable. An output
cannot be nested beneath an ancestor containing another lawpack output index.
Within one document-root publication namespace, Edict acquires shared
intent locks for proper output ancestors in top-down order and an exclusive lock
for the output itself. Sibling output footprints can publish concurrently;
identical and parent/child footprints conflict. Lock-file opens do not follow
symbolic links, so another entry cannot redirect a footprint claim to a
different file identity. The real-directory ancestor
chain is rechecked after intent acquisition. The publication root and output
parent are then pinned as capability directories by traversing every component
from the filesystem root without following symbolic links. Write mode stages
the complete replacement and uses retained capability handles to keep opened
directories confined. The old output is opened, owner-validated, and identified
before its name is moved; the backup name must then reopen to that retained
identity before publication continues. Rollback likewise reopens the restored
destination and requires the retained captured identity before reporting
success; a mismatching destination remains intact as recovery evidence.
Every publication move whose destination must be absent uses an atomic
no-replace operation, so an intervening empty directory is refused rather than
silently overwritten. Apple, Linux, Android, and Redox use their
capability-relative no-replace kernel operation. Windows and any other target
without that backend refuse write publication with
`LawpackOutputWriteUnsupported` before reading the build document or mutating
the publication namespace. On unsupported non-Windows targets, `checkOnly`
remains available because it performs no publication move. Windows lawpack
builds currently fail closed before document I/O in both modes, using
`LawpackCheckUnsupported` for `checkOnly`; Edict does not claim a Windows
transactional publication or filesystem-identity backend.
Production transaction creation is available only through retained exclusive
output authority. Operating-system footprint locks compose with in-process
shared/exclusive exclusion keyed by the retained lock file's filesystem
identity, so alternate spellings of one lock object and parent/child
lock-respecting publishers cannot enter the create/open boundary concurrently
while siblings remain independent. This is the supported cooperating-writer
proof; it does not
turn portable create-then-open into hostile-writer object continuity. No-follow
opens reject symbolic links, but they do not by themselves prove that a real
directory reopened by name is the object Edict previously created.

An uncooperative process with write authority over the publication parent can
rename any namespace entry between portable filesystem calls. Edict therefore
does not promise to restore the old output to its original pathname against
such a process. The supported failure-atomic guarantee is for lock-respecting
publishers in the same document-root namespace. Under uncooperative mutation,
the safety contract is narrower: retained capabilities confine I/O, symbolic
links are never followed, detected substitutes are not deliberately overwritten
or deleted, and the command refuses rather than claiming an unverified commit
or rollback. Exact recovery of a directory whose only name was moved by another
process requires stronger kernel-enforced exclusion or a parent directory the
process cannot mutate.

After activation, the retained transaction identity and its complete artifact
tree must still match the staged authoring result before the successful
post-validation public-name identity rebind commits the replacement. After
exact-tree traversal, the public output name is reopened and must still identify
that staged transaction before the captured backup is removed. If the name
vanished or changed, rollback preserves a substitute under
the transaction recovery name, restores the captured output when the checked
namespace transition permits it, and reports failure. Rollback refuses to
delete an output that appeared concurrently.
Check-only performs no locking or filesystem mutation. Concurrent overlapping
publication from different document roots is outside one namespace and must be
avoided by the caller.

An existing non-empty directory without a valid
`edict.lawpack-output.json` ownership index (schema
`edict.lawpack-output/v1`) is refused rather than deleted. Output paths and
input dependency paths must be confined relative paths, and dependency inputs
must remain outside the owned output tree. Existing symlink traversal is
rejected. Authored files may not collide by using another file as a parent, and
the ownership-index path is reserved for Edict. Edict indexes every emitted
file path and checks each proper ancestor against that set, so duplicate and
file/descendant collisions
remain fail-closed without pairwise growth as application-owned resource sets
expand. Pure preflight derives the fixed manifest and exports, every authored
artifact, every digest sidecar, and the reserved ownership-index namespace, then
rejects duplicates and file/descendant collisions immediately after the build
document is decoded and before output inspection, coordination, or dependency
filesystem I/O. Primary `.cbor` paths reserve the two bytes added when their
extension becomes `.sha256`; derived sidecars are separately validated at the
255-byte component and 1024-byte relative-artifact ceilings. Output components
use bounded ASCII letters, digits, `.`, `_`, and `-`, and reject
Windows device names, filesystem NUL, case-insensitive aliases of Edict's
internal publication names, trailing-dot aliases, overlong components or
relative paths, and platform-specific forbidden punctuation. Only `/` separates
non-empty raw path components; backslashes and repeated separators reject before
host path parsing.

For example, the raw `outputDirectory` grammar produces these results before
publication performs filesystem access:

| Raw `outputDirectory` | Result | Reason |
| --- | --- | --- |
| `generated/Package_1` | accepted | Both non-empty components use the portable character set |
| `generated\\child` | `InvalidLawpackConfig` | A backslash is not a path separator in the raw grammar |
| `generated//child` | `InvalidLawpackConfig` | The repeated separator creates an empty component |
| `generated./child` | `InvalidLawpackConfig` | The first component has a trailing-dot alias |
| 230 ASCII `x` characters | `InvalidLawpackConfig` | The component exceeds the 229-byte limit reserved for its derived lock name |

The index identity must match the lawpack being authored. Edict refuses to
replace or check a tree owned by a different lawpack id or version even when
that tree otherwise carries a structurally valid ownership index.

Definition, dependency, generated-index, and published-artifact reads are
bounded to 1 MiB per file, generated artifacts are refused before publication
if they exceed that same read boundary, and the supplied dependency closure is
bounded to 192 bundles. Raw authoring JSON rejects duplicate object keys before
typed deserialization, so no hash-significant value can silently replace an
earlier review value. Check-only traversal accepts only directories needed by
expected artifacts and rejects an unexpected file before reading its contents,
so drift cannot force accumulation of an arbitrary output tree in memory. A
missing nested output parent is reported as `LawpackOutputDrift` without
creating that parent or a sibling publication lock. A successful check reopens
the parent chain and output after traversal, reports drift if their directory
identities or the output ownership basis changed, and requires a second
exact-tree pass.

A persistent hidden lock file is kept beside each write output and its proper
ancestors. These files are footprint-coordination state, not canonical lawpack
artifacts, and should remain untracked. A process-wide registry keyed by each
retained lock file's filesystem identity composes with those cooperative
operating-system locks; there is no host-wide or filesystem-wide coordinator
that excludes an uncooperative namespace writer. The successful
post-validation public-name identity rebind is the publication commit point.
Failure to remove the hidden previous-tree backup after that boundary does not
turn a committed replacement into a failed command; that backup is best-effort
cleanup state and may be removed by the operator.

## The Five Artifacts That Must Stay Distinct

| Artifact | Owner | Meaning |
| --- | --- | --- |
| Lawpack authoring JSON | Application | Reviewable semantic declarations and exact external pins. |
| Generated lawpack artifacts | Edict encoder | Canonical manifest, exports, adapters, configurations, and derived identities. |
| Application `.edict` source | Application | Executable application law that imports the exact generated manifest digest. |
| Provider package | Target provider | Target profile plus bounded lowering and verification components; unused during authoring. |
| Runtime receipt | Runtime | Evidence about one admitted execution; neither input to nor output from authoring. |

The independent application witness
`external_application_authors_vendors_and_builds_its_own_lawpack` runs outside
the Edict checkout. It authors the workspace-snapshot closure through the
public binary, reproduces the four reviewed canonical artifacts byte for byte,
and then feeds those generated bytes to the public application build. The
fixture proves authoring and compilation, not runtime execution.

## Failure Boundary

Malformed definitions fail before artifact return. Emitted manifest, exports,
and adapter bytes are passed back through `decode_lawpack_bundle`,
`decode_lawpack_adapter`, and `validate_lawpack_dependency_graph` before any
publication. Stable CLI kinds distinguish invalid definitions or digests,
unresolved local resources, invalid canonical values, incomplete adapters,
dependency substitution, path escape or prefix collision, unknown tagged
fields, output ownership, output size, output drift, and publication failure.

Edict does not discover application semantics from schemas or fixtures, invoke
a provider, build an executable package, admit an operation, or create a runtime
receipt on this path.

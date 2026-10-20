# Provider Contract Pack v1

This directory preserves Edict's prior function-free provider contract
publication as exact Rust-neutral bytes for explicitly pinned consumers.

| File | Contract |
| --- | --- |
| `edict-provider-contracts.cddl` | Self-contained CDDL assembled from the reviewed Edict ABI fragments. |
| `manifest.json` | Exact schema identity, contract and domain roots, and the five Edict-owned target-profile resources. |

The manifest's `schema.bytesHex` is the exact CDDL file encoded as lowercase
hex. `schema.rawSha256` is SHA-256 over those raw file bytes. Each resource
likewise carries exact canonical bytes as lowercase hex, a raw byte digest, its
coordinate-framed Edict digest, and reviewed repository/source-path provenance.
The `contracts` array names every published logical root; `domains` is the
subset used for provider artifact-domain dispatch.

All arrays use ascending exact UTF-8 order by their identifying field. The
manifest is a deterministic transport and review artifact, not a second schema
source: consumers must decode `schema.bytesHex`, verify its raw digest, and use
those bytes for CDDL validation. Resource bytes are verified the same way, then
their coordinate-framed digests bind Edict semantic identity. Neither JSON
object key order nor review rendering participates in an Edict semantic digest.

Both artifacts carry the Apache-2.0 license. Consumers receive their bytes as
explicit inputs. The provenance paths are review evidence, not discovery or
filesystem lookup handles.

The current owner generator writes and checks the separately selected
[source-functions-v1 publication](../source-functions-v1/README.md). These
prior CDDL/manifest bytes are retained unchanged; selecting the new publication
requires an explicit downstream contract/digest update.

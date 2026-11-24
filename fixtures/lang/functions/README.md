# Source-owned pure function compiler witnesses

`range-assembly-baseline.amblicit` is copied byte-for-byte from Jim (GitHub
`flyingrobots/jedit`) commit `327ac11c71cc5a9a33c5552be7a6db533bf88f5d`,
`edict/replace-range-probes/range-assembly/RangeAssembly.edict`.
Its SHA-256 is `8e8d2703759fb30cb49b73650ba8ddfd637d68b33b86145661bfca3033c968fc`.

`range-assembly.amblicit` factors that same expression into a called two-argument
source function with an immutable local binding. The existing input/output
bounds, application import and intent contract are preserved. Both are Edict
source specimens; no handwritten Core or Target artifact substitutes for them.

The source-function tests use explicit profile/budget context facts for isolated
compiler checks. The CLI test invokes the public `project` operation. Neither
witness is application-build, Echo runtime, retained-buffer decoding, or rope
mutation evidence. Those require the separate public application and consumer
witnesses in Edict226, Echo752 and Jim296.


`legacy-core.cddl` preserves the Core schema bytes from Edict commit
`80ae9edc2c4ed127e18ea2434c20dd20ed39cadc`, before the source-function table.
The provider-schema test assembles that old root with the unchanged remaining
contracts and proves rejection of real function-bearing Core alongside an
accepted function-free control. It separately validates the new root.

The Docker-only [public application witness](../../../scripts/consumer-witnesses/jedit-source-functions.py)
builds these same two source files against an explicitly pinned old provider.
It uses the original Jim `edict/replace-range` vendored lawpack (digest
`95758c1605894672cc9069fde01bb8b6e11842b053102660c9cd4f4d6f34d64e`), verifies
input/source/binary hashes before and after, and requires the function-free
artifact pair plus function-bearing schema refusal with no output. This remains
application compatibility evidence, not provider function execution evidence.

`function-precondition.amblicit` is copied byte-for-byte from Jedit
(`flyingrobots/jedit`) commit `641f53a3cc53482a929817213d82d92de0b6c8b5`,
`edict/replace-range-probes/compiler-inventory/iterative/function-precondition.edict`.
Its SHA-256 is `8feec28f75b59e2a027f9662900c4e504a5b0e0104f6649120f056f0d2888bdb`.
It is the reproducer for function `where` preconditions (#264): main refused it
at parse with `ExpectedToken`. It now compiles, and its reviewed Core and Echo
Target IR goldens live under `fixtures/core/canonical/function-precondition.*`
and `fixtures/target-ir/canonical/echo-function-precondition.*`.

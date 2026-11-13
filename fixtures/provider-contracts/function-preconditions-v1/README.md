# Function-precondition provider contract publication

This publication adds the optional, nonempty `preconditions` predicate list on a
source function (`core-source-function`) to the
[bounded-state-fold publication](../bounded-state-fold-v1/README.md). An earlier
pack refuses the field with a schema mismatch. Accepting the field in a schema is
not support for it: a consumer must implement the language specification's
Function Preconditions contract or refuse.

This CDDL/manifest pair is now frozen. The generator writes the separate
[pure-intrinsic-catalog publication](../pure-intrinsic-catalog-v1/README.md),
which carries the same Core schema. This pair's `intrinsics-document` root
admits only `edict.target-profile.intrinsics/v1` catalogs; tests reproduce the
pair from the frozen target-profile schema in
[`fixtures/target-profile/pre-catalog-v2-target-profile.cddl`](../../target-profile/pre-catalog-v2-target-profile.cddl).

The pack coordinate remains `edict.provider-contract-pack.cddl@1` and the manifest
API version remains `edict.provider-contract-pack/v1`. Consumers select the exact
CDDL/manifest pair and verify its schema digest; the same coordinate does not make
different publication bytes interchangeable.

# Echo `/v3` target-intrinsics catalog (vendored)

`resource.target-intrinsics-3.cbor` is a byte-for-byte copy of
`crates/echo-edict-provider-lowerer/resources/resource.target-intrinsics-3.cbor`
at echo commit `44098b15` (SHA-256
`75f82f1481719639723612e7bec410fabb88e60157c57ab995b081a8726039fd`).

That commit is on Echo's unmerged draft PR #1029, not on Echo's main. When
#1029 merges, re-vendor the file from Echo's main and update this note and the
test that reads it (`crates/amblicit-syntax/tests/effect_cost_templates.rs`,
`echo_v3_catalog_passes_both_admissions_in_build_order`).

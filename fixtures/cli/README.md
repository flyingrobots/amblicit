# CLI Golden Fixtures

Each subdirectory is one end-to-end golden case for the public `amblicit` CLI. The
`golden_cli_fixtures_replay_exactly` test (in
`crates/amblicit-cli/tests/golden_cli.rs`) replays every case through the compiled
binary and matches stdout, stderr, and the exit code byte-for-byte against the
checked-in goldens.

## Case layout

| File | Required | Meaning |
| --- | --- | --- |
| `request.jsonl` | yes | JSONL request written to the binary's stdin. |
| `expected.stdout.jsonl` | no | Exact expected stdout. Absent means empty. |
| `expected.stderr.jsonl` | no | Exact expected stderr. Absent means empty. |
| `exit` | no | Expected process exit code. Absent means `0`. |
| `env.json` | no | JSON object of environment variables applied while replaying the case. |
| `inputs/…` | no | Source files referenced by `path`, `directory`, `pathList`, or `glob` requests. |

The binary runs with the case directory as its working directory, so any path a
request names resolves to a stable relative path in the emitted records.

## Coverage

- `01-source-ok` — inline source success.
- `02-source-parse-rejection` — parse-stage diagnostic, exit `1`.
- `03-source-semantic-rejection` — semantic-stage diagnostic, exit `1`.
- `04-cli-missing-settings` — CLI-input rejection before compilation, exit `2`.
- `05-path-input-ok` — single file path input.
- `06-directory-expansion-ok` — recursive directory expansion, sorted, non-`.amblicit` files ignored.
- `07-path-list-ok` — ordered path list, request order preserved.
- `08-glob-expansion-ok` — glob expansion, sorted.
- `09-shape-source-ok` — source importing a GraphQL `shape` schema; accepted because import resolution is deferred at the `check` stage.
- `10-input-extra-field` — input record with an unrecognized field is rejected (schema parity: `additionalProperties: false`), exit `2`.
- `11-input-hybrid-kind` — input record mixing fields from two kinds is rejected (schema parity: mutually exclusive kinds), exit `2`.
- `12-input-too-large` — stdin larger than the configured byte limit is rejected before request parsing, exit `2`.
- `13-input-root-outside` — path input resolving outside configured `inputRoot` is rejected before source checking, exit `2`.

## Regenerating a case

After an intentional contract change, regenerate the corpus from the repository
root and review the diff before committing:

```sh
cargo xtask cli-goldens --write
```

Check mode replays the same corpus without writing:

```sh
cargo xtask cli-goldens --check
```

Write mode deletes `expected.stdout.jsonl`, `expected.stderr.jsonl`, or `exit`
when the corresponding stream is empty or the exit code is `0`.

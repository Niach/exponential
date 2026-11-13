# `ecode-core`: eCode's pure core

A pure Rust port of `packages/ecode/src/{forge,checks,pipeline,runner,remote}.ts`
(#EXP-1613): git and CI as plain functions over plain state.

- **forge**: the PR-tree model every provider offers. `hydrate(given)` builds a
  `ForgeState`; `forge(&state, op, &args)` returns the next state, the events
  and the result, or a refusal (`{code, reason}`, state unchanged, no events).
  Ops: `push`, `pr.open`, `pr.list_by_base`, `pr.retarget`, `pr.mergeability`,
  `pr.squash_merge`, `pr.merge_ordered`, `pr.close`, `pr.files`,
  `branch.rebase_onto`.
- **checks**: the rollup over a PR's check rows, the merge gate of the
  required checks, a job's check name.
- **pipeline**: the pipeline file `.exponential/ci.json`: `glob_match`,
  `validate_pipeline` (every error as a JSON pointer + code) and
  `plan_pipeline` (matrix expanded, ordered by `needs`, skipped by `paths` or
  for a fork).
- **runner**: `schedule(&state, op, &args, at)`: enqueue, beat, claim, finish,
  cancel and the clock's `tick` (timeouts, lost runners, queued too long).
- **remote**: `parse_remote`, the scoped credential-helper key, token refresh
  and log redaction.

The enums and constants are generated from `packages/ecode/contract.json`
(`src/contract.generated.rs`; regenerate with
`cd packages/ecode && bun run scripts/generate.ts`, never by hand).

**The rule** (`tests/dependency_gate.rs`): serde and serde_json only; no I/O,
no clock, no async, no unsafe, nothing of Exponential. The crate is server-free
and reusable as is (Rerusted, #RUST-9).

**Tests**: `cargo test -p ecode-core` replays every row and case of
`packages/ecode/fixtures/{forge,checks,pipeline,runner,remote}.json`
(`tests/fixtures.rs`, the semantics of `packages/ecode/src/replay.ts`).

Licence: Apache-2.0

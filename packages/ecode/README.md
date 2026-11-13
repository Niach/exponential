# @exp/ecode

eCode's contract (#EXP-1612, spec #EXP-1613). eCode puts git and CI in one library:

- forge providers: GitHub, and eCode's own;
- own git: refs on the host, packs in the team bucket;
- own CI: a pipeline file, runners on team devices.

It is built server-free first, so a host without any server (Rerusted, #RUST-9) uses it as is. The Exponential server is one optional provider and sync layer.

`contract.json` generates the enums and small rules into four places with `bun run --filter @exp/ecode generate`:

- Rust: `apps/desktop/crates/ecode-core`;
- TypeScript: `src/contract.generated.ts`;
- Swift: ExpCore `ECode`;
- Kotlin: `com.exponential.app.ecode`.

Each `fixtures/*.json` is a spec (its `$comment`) plus cases. `src/` holds the pure TypeScript reference that replays them. These suites replay the same fixtures:

- `ecode-core`, with `cargo test -p ecode-core`;
- the server's `apps/web/src/lib/ecode/ecode-spec.test.ts`, inside the PR gate, which also binds the check enums and the rollup to domain-contract's (#EXP-1610).

After a deliberate spec change, run `bun run --filter @exp/ecode bless` and review the diff: the diff is the spec change.

| fixture | covers |
|---|---|
| `forge.json` | The PR-tree primitives: open with any base, list by base, retarget (no loops), mergeability (a path-wise three-way merge: clean or dirty, plus the paths), squash merge (one commit; the children move onto the base; the head branch goes), ordered multi-merge, close, files, `rebase --onto`, push (fast-forward or forced). Also the capability table. |
| `checks.json` | The rollup over check rows (`none`, `pending`, `success`, `failure`), the merge gate over the required names (`ready`, `waiting`, `blocked`, `no_checks`) and check names for matrix jobs. |
| `pipeline.json` | `.exponential/ci.json`: validation with JSON-pointer errors, the glob rule, and the planner. The planner handles triggers and filters, `needs` order, the matrix, env and secrets, and skips by `paths` or for a fork. |
| `runner.json` | Runner scheduling: claim by labels and slots, needs, timeouts, lost runners, the queued-too-long flag, cancel, and the skip chain under a failure. |
| `remote.json` | Remote URLs (`https://…`, `exp://team/repo`), the credential-helper scope, token refresh and log redaction. |

## Providers and capabilities

A provider declares what it supports in `capability.byProvider`. The UI and the playbook read these flags instead of assuming GitHub:

- **github**: retarget, squash merge, mergeability, native stacks, checks, pipelines (Actions), run logs, required checks, fast-forward, webhooks.
- **ecode**: GitHub's set except webhooks, plus ordered merge, `rebase --onto` on the host and a mirror remote. Its events come from our own bus, or from the host's store when there is no server.

## The traits (crate `ecode`, #EXP-1615)

`ecode-core` is the pure half: serde and serde_json only, no I/O. The `ecode` crate puts the I/O behind five traits, so a host plugs in its own transport and storage:

| trait | does | GitHub | eCode via the Exponential server | eCode without a server |
|---|---|---|---|---|
| `Remote` | where objects go: credentials, fetch, push | https + a token | `exp://` through `git-remote-exp`; packs in the team bucket | a local or p2p transport (e.g. git packs over iroh) |
| `Forge` | the review unit: every op in `forge.json` | pull requests | our rows, merges run by a runner on a team device | review objects in the host's `Store` |
| `Ci` | pipelines, runs, jobs, logs | Actions (read; `runs-on` self-hosted) | our runner, rows in `commit_checks` | our runner, results in the `Store` |
| `Store` | review and run objects; `update_ref(name, expected, new)` = the ONE serialization point | n/a | Postgres (CAS) | the host's rule (e.g. delegates plus a threshold) |
| `SecretSource` | injects secrets into a job's process; secrets are used, never read | n/a | the vault, fetched per job | the OS keyring |

Licence: Apache-2.0.

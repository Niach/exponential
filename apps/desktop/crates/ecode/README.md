# `ecode`: eCode on devices

The I/O half of eCode (#EXP-1615, epic #EXP-1612). `ecode-core` is the pure
spec model; this crate runs the same ops on real repositories and providers.
It is server-free: eDE, Spezl and Rerusted (#RUST-9) link it as is.

## The traits

| trait | does |
|---|---|
| `Remote` | credentials, fetch, push (`GitRemote`: git argv, a helper scoped to exactly the remote, the token in the child's env only) |
| `Forge` | every op of `packages/ecode/fixtures/forge.json`, its refusals as `Error::Refused`, events by polling (`events(cursor)`) |
| `Ci` | plan, runs, jobs, log, rerun, cancel (the trait; feature `runner` runs one job) |
| `Store` | content-addressed review and run objects; `update_ref(name, expected, new)` is the ONE compare-and-swap point (`MemoryStore`, `FileStore`) |
| `SecretSource` | injects secrets into a `Command`; there is no getter (`MapSecrets`) |

`Credentials` is the shape of the server's `repositories.credentials`
(`{remoteUrl, username, token, expiresAt}`) for any https host or `exp://`;
every printable form is redacted.

## `LocalRepo` (feature `repo`)

On gitoxide (`gix` 0.90, `sha1` + `revision` only): open/init, commits from
per-path changes, log, diff, revert (eDE's undo), refs by compare-and-swap,
and the spec's path-wise three-way mergeability. On git argv until gix covers
them: push/fetch (`Remote`), the squash merge (`merge-tree --write-tree` +
`commit-tree`, git ≥ 2.40), `rebase --onto` (a throwaway worktree, every
commit kept) and bundles (gix 0.90 has no bundle support).

## Providers

| feature | provider |
|---|---|
| `local` (default) | `LocalForge`: branches in a `LocalRepo`, PRs and events in a `Store` table |
| `github` | `GithubForge` over REST with a token, `DeviceFlow` for OAuth without a server, events by polling the PR list with an ETag |
| `exponential` | `ExponentialCredentials`: `repositories.credentials`, cached until the refresh margin |

## The runner (feature `runner`, #EXP-1617)

`run_job(sandbox, remote, secrets, job, sink)` runs ONE planned job
(`JobSpec`, built from ecode-core's `PlannedJob`) and returns its conclusion
and step states. Scheduling stays in ecode-core (`schedule`, `runner.json`);
the host claims jobs.

- `Sandbox`: a clean workspace at the job's commit and the shell a step runs
  in. `ProcessSandbox` = a bare mirror per remote plus a detached git worktree
  per job, steps in `bash --noprofile --norc -eo pipefail` (`sh -e` without
  bash). Containers can implement the same trait later.
- The step's env is an allowlist: the sandbox's passthrough names (`PATH`,
  `HOME`, …), the job's planned env, `CI=true`; never the host's whole env.
- Secrets: only the job's declared names, injected through `SecretSource`;
  every log line is redacted with `ecode_core::redact`.
- Timeouts (the job's and each step's) and cancels kill the step's whole
  process group (SIGTERM, 5 s, SIGKILL), grandchildren included.
- `LogSink` gets redacted log text and step transitions, and is polled about
  every 250 ms for a cancel (the host flushes and beats there).

## Tests

- `tests/forge_replay.rs` replays `forge.json` on a real git repository:
  each case's commits are written into a temp repo, every step runs through
  `LocalForge`, and new commits are checked by parent and per-path content.
- `examples/local_flow.rs` (run as `tests/local_flow.rs`): init → branches →
  a PR on a non-default base → retarget → ordered merge, no server.
- `tests/dependency_gate.rs`: no Exponential app crate, gix's features pinned.
- `runner` unit tests: a real checkout, redaction, the env allowlist,
  `continueOnError`, and a cancel that kills a backgrounded grandchild.

Licence: Apache-2.0.

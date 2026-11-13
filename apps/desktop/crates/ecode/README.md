# `ecode`: eCode on devices

The I/O half of eCode (#EXP-1615, epic #EXP-1612). `ecode-core` is the pure
spec model; this crate runs the same ops on real repositories and providers.
It is server-free: eDE, Spezl and Rerusted (#RUST-9) link it as is.

## The traits

| trait | does |
|---|---|
| `Remote` | credentials, fetch, push (`GitRemote`: git argv, a helper scoped to exactly the remote, the token in the child's env only) |
| `Forge` | every op of `packages/ecode/fixtures/forge.json`, its refusals as `Error::Refused`, events by polling (`events(cursor)`) |
| `Ci` | plan, runs, jobs, log, rerun, cancel: types only, E4 (#EXP-1617) implements it |
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

## Tests

- `tests/forge_replay.rs` replays `forge.json` on a real git repository:
  each case's commits are written into a temp repo, every step runs through
  `LocalForge`, and new commits are checked by parent and per-path content.
- `examples/local_flow.rs` (run as `tests/local_flow.rs`): init → branches →
  a PR on a non-default base → retarget → ordered merge, no server.
- `tests/dependency_gate.rs`: no Exponential app crate, gix's features pinned.

Licence: Apache-2.0.

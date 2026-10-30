# exponential-agent-runtime

The single-account agent core shared by **Exponential** and **spezl** (EXP-1426, epic EXP-1420).
It spawns the user's own, unmodified `claude` / `codex` binary over ACP, streams what the agent
does, lists the CLIs' own session history (with their own session names), runs the agent's login
in a PTY and wires MCP servers. One local account, no server.

## Rules

1. **No server coupling.** No tRPC, token store, device id, steer relay or pickup loop. The crate
   depends on no Exponential app crate and no UI toolkit (`tests/dependency_gate.rs`).
2. **Never touch an agent's credentials.** The core spawns the vendor's binary and lets it own its
   login: it never reads, copies, moves or refreshes a token. Anthropic's Claude Code terms forbid
   token intermediation, so `coding/src/claude_oauth.rs` is NOT part of this crate and never will
   be. Exponential's multi-account staging and rotation stay in `coding`.

## Seams

Server coupling becomes traits the hosts implement. Each lands with the first module that needs it.

| Trait | What it answers | Exponential (`coding` + `engine`) | spezl / local |
| --- | --- | --- | --- |
| `EventSink` | where mapped events go | the steer publisher | the voice loop / UI |
| `RunBackend` | report state, reply cards, consume queued messages, answers | `api::session_device` over tRPC | in memory |
| `AccountEnv` | config dir + env for the spawn | multi-account rotation | the CLI's default home |
| `McpProvider` | the MCP servers a run gets | team + device servers | the user's own list |

## What stays in Exponential

The pickup loop, the launcher, worktrees / git / clones / GitHub tokens, `TrpcClient` and
`TokenStore`, the steer relay, account rotation and usage, prune / reaper, remote admin and
`claude_oauth`.

## Extraction order

Stacked PRs, each behaviour-preserving: the old module paths re-export the moved modules, so no
caller changes until a later cleanup.

1. This crate, the dependency gate and the pure leaves: atomic config writes (`api::atomic_file`),
   the lock file, the claude / codex trust seeders, the codex session index, the confinement
   check, the command head, the Linux run scope, the background `Command` constructor
   (`terminal::process`).
2. The event vocabulary (`ActivityEvent` and the frames it needs, the redactor) out of `steer`.
3. Agent identity, detection and the single-account login PTY (no credential moves).
4. The ACP engine core (adapters, mapper, host, session, transport) behind `RunBackend` /
   `EventSink`; `engine` becomes Exponential's adapter.
5. A local backend and a runnable example: one turn plus the session history with your own login,
   no server.

Named per the shared-libs table (`docs/libraries.md`, EXP-1421): crate `exponential-agent-runtime`, release tag `agent-runtime-v*`.

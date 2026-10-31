# emcp

MCP for Exponential's devices and spezl, on [rmcp](https://github.com/modelcontextprotocol/rust-sdk),
the official Rust MCP SDK: a loopback Streamable HTTP server behind a
tool-provider trait, a blocking client, an OAuth client whose credentials live
in a vault the host provides, the **local hub** that puts several MCP servers
behind one endpoint with a grant per run and an audit row per call, **code
mode** (feature `codemode`), and the eMCP contract (`packages/emcp`) replayed
from its fixtures.

Everything is blocking from the host's side: the SDK runs on emcp's own
runtime threads, never under a host's executor. The crate talks to no server
of its own and reads no settings file or env var.

Exponential links it from the `codemode` crate (a re-export of
`emcp::codemode` plus the contract's tool rows), the `api`/`ui` crates (their
calls into the instance's `/api/mcp`), epilot and epilot-screen (their tools);
spezl links it as a git dependency.

## eCore

The API speaks eCore's types (`apps/desktop/crates/ecore`, #EXP-1517), re-exported
as `emcp::ecore` and `emcp::{SessionId, Token, Failure, ErrorCode, DeliveryVerdict}`:
a run's bearer is a `Token` (64 lowercase hex, checked with `parse_bearer`), the
run is a `SessionId`, a refusal is a `Failure`, and `ClientError::delivery()` says
whether a failed call never ran. eCore is emcp's one path dependency: the git
checkout of an `emcp-v*` tag carries it at the same commit.

## Consuming

`publish = false`: consume it as a git dependency on a release tag. Tags are
`emcp-v<version>`, `<version>` being the crate's `version`. The rules are the
git-tag policy in `docs/libraries.md` ("Consuming from another repo").

```toml
[dependencies]
emcp = { git = "https://github.com/Niach/exponential", tag = "emcp-v0.2.0" }
# with code mode:
emcp = { git = "https://github.com/Niach/exponential", tag = "emcp-v0.2.0", features = ["codemode"] }
```

The contract is compiled in with `include_str!` (a git checkout carries the
fixtures; a crates.io package would not):

| Fixture | What it decides |
| --- | --- |
| `packages/emcp/fixtures/exposes.json` | an app's MCP tools as declarative bindings: validation, `${input.x}` resolution, `<app>__<tool>` naming and collisions, the upstream call-time check, the `_meta` provenance |
| `packages/emcp/fixtures/access.json` | who sees and calls which app tool: `mcp_grants`, the member × app policy (no row = `write`) |
| `packages/emcp/fixtures/audit.json` | the `mcp_calls` row: canonical JSON, the args hash, the status rule, 90-day retention |
| `packages/emcp/fixtures/vault.json` | `ev1.<kid>.<iv>.<ciphertext‖tag>`: AES-256-GCM, the AAD binding a value to its row and key |
| `packages/emcp/fixtures/upstream.json` | one member's connection to one upstream: states, transitions, what is callable |
| `packages/emcp/fixtures/hub.json` | the local hub: server names, a run's `<server>__<tool>` list under its grant, routing, the `mcp_calls` row of every call |
| `packages/domain-contract/fixtures/app-actions.json` | which `exponential.<noun>.<op>` actions exist, which are destructive or owner-only |

## Serve, call, authorize

```rust
// Serve: a ToolProvider on 127.0.0.1:<port>/mcp, one bearer token per run.
let info = emcp::ServerInfo { name: "clock".into(), version: "0.1.0".into(), instructions: None };
let server = emcp::Server::serve(info, Arc::new(Clock))?;   // Clock: impl ToolProvider
let run = emcp::SessionId::new("run-1")?;                   // eCore's id rule
let token = server.grant(&run, "EXP-12");                    // an ecore::Token; an earlier one of run-1 dies
// MCP config: { "clock": { "type": "http", "url": server.url(),
//               "headers": { "Authorization": token.header() } } }
server.revoke(&run);                                         // the provider hears ended(&run)

// Call: any Streamable HTTP server, headers resolved by the host.
let upstream = emcp::Upstream { name: "exponential".into(), url, headers };
let client = emcp::Client::connect(&upstream, "my-app", timeout)?;
let result = client.call("exponential_issues", json!({ "op": "get", "id": "EXP-12" }), timeout)?;

// Authorize: OAuth (discovery, DCR, PKCE, refresh), credentials in a Vault.
let authorization = emcp::oauth::begin(&config, vault.clone(), "linear")?;
open_browser(authorization.url());
authorization.finish(&code, &state)?;
let client = emcp::oauth::connect(&config.url, vault, "linear", "my-app", timeout)?;
```

## The local hub

One loopback endpoint over several MCP servers (#EXP-1507). Mount upstreams
(Streamable HTTP, headers resolved) and in-process `ToolProvider`s under a
name; admit each agent run with a grant (`read` | `write` per server, a server
not named = none) and hand it one token. The run lists `<server>__<tool>` for
the tools its grant covers (a tool is `read` iff `readOnlyHint: true`); a call
to anything else is MCP's unknown-tool error. Every `tools/call`, refused ones
included, becomes one `mcp_calls` row (audit.json's columns) handed to your
`AuditSink` (`MemoryAudit`, `JsonlAudit`, or your own).

```rust
let hub = emcp::hub::Hub::serve(info, Arc::new(emcp::hub::JsonlAudit::new(path)))?;
hub.mount("linear", emcp::hub::Mount::Upstream(upstream))?;
hub.mount("computer", emcp::hub::Mount::Provider(Arc::new(Computer)))?;
let grant: emcp::hub::HubGrant = serde_json::from_value(json!({
    "servers": { "linear": "read", "computer": "write" }, "teamId": null, "userId": null,
}))?;
let run = emcp::SessionId::new("run-1")?;
let token = hub.grant(&run, "EXP-12", grant);      // MCP config: hub.url() + token.header()
hub.revoke(&run);
```

## Code mode (feature `codemode`)

`exec` + `describe` on one loopback endpoint: a script the agent writes calls
the run's OTHER MCP servers as `await tools.<server>.<tool>(args)`, in
parallel, and only its output reaches the model. Scripts run on boa in a
worker process (the host's own executable re-run as `__codemode-worker`:
call `emcp::codemode::worker::requested()` / `run_and_exit()` first thing in
`main`), killed at the deadline, a revoke or the memory ceiling. The MCP
gateway (`gateway_grant`) shares the listener: a transparent proxy with a
per-run `GatewayTransform`. A host with tools of its own installs a
`SubjectTable` (`set_subject_table`) so the call log names their subjects.

### The public API

| Item | Role |
| --- | --- |
| `server::ToolProvider` | `tools()`, `call(caller, name, arguments)`, `ended(session_id)`: what the server asks of its host |
| `Server::serve` / `serve_with` | bind `127.0.0.1:0`, serve a provider (plus `RawRoute`s that own their prefix and their auth) |
| `Server::{grant, revoke, revoke_all, admitted, url, base}` | the runs' tokens and the endpoint |
| `Client::connect`, `list_tools`, `call`, `client::call_once` | the blocking client; `ClientError` = `Http(status)`, `Transport`, `Rpc`, `Timeout` |
| `oauth::{begin, Authorization::finish, connect, connected, disconnect}` | an OAuth connection, following `upstream.json` |
| `oauth::{Vault, SealedVault, MemoryVault}` | where credentials live; `SealedVault` seals every value with `contract::vault` |
| `server::ToolProvider::{tools_for, try_call}` | optional: a per-run list, and MCP's unknown-tool error instead of content (`Refusal`) |
| `hub::{Hub, Mount, HubGrant, AuditSink, MemoryAudit, JsonlAudit, AuditRow}` | the local hub |
| `codemode::{grant, gateway_grant, revoke, worker, set_subject_table, …}` | code mode (feature `codemode`) |
| `contract::{exposes, access, audit, vault, upstream, hub, actions}` | the pure contract, one module per fixture |

`tests/fixtures.rs` replays every fixture case; `tests/loopback.rs` serves and
calls over a real socket; `tests/hub.rs` replays hub.json's list and call cases
through a real hub (`cargo test -p emcp --features codemode` runs code mode's
own tests too).

## The dependency rule

No Exponential app crate (`api`, `domain`, `coding`, `ui`, `app`, …) and no
other lib of this repo, gated by `tests/dependency_gate.rs`. The hosts
(codemode, epilot, epilot-screen, `api`, `ui`) depend on emcp, never the
reverse; what code mode needs from Exponential (the contract's tool rows)
comes in through `set_subject_table`. eui-core stays off it: it is published on crates.io, which cannot
carry a git dependency.

## Licence

Apache-2.0.

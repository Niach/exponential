# emcp

MCP for Exponential's devices and spezl, on [rmcp](https://github.com/modelcontextprotocol/rust-sdk),
the official Rust MCP SDK: a loopback Streamable HTTP server behind a
tool-provider trait, a blocking client, an OAuth client whose credentials live
in a vault the host provides, and the eMCP contract (`packages/emcp`) replayed
from its fixtures.

Everything is blocking from the host's side: the SDK runs on emcp's own
runtime threads, never under a host's executor. The crate talks to no server
of its own and reads no settings file or env var.

Exponential links it from codemode and the `api`/`ui` crates (their calls into
the instance's `/api/mcp`); epilot and epilot-screen serve their tools through
it next; spezl links it as a git dependency.

## Consuming

`publish = false`: consume it as a git dependency on a release tag. Tags are
`emcp-v<version>`, `<version>` being the crate's `version`. The rules are the
git-tag policy in `docs/libraries.md` ("Consuming from another repo").

```toml
[dependencies]
emcp = { git = "https://github.com/Niach/exponential", tag = "emcp-v0.1.0" }
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
| `packages/domain-contract/fixtures/app-actions.json` | which `exponential.<noun>.<op>` actions exist, which are destructive or owner-only |

## Serve, call, authorize

```rust
// Serve: a ToolProvider on 127.0.0.1:<port>/mcp, one bearer token per run.
let info = emcp::ServerInfo { name: "clock".into(), version: "0.1.0".into(), instructions: None };
let server = emcp::Server::serve(info, Arc::new(Clock))?;   // Clock: impl ToolProvider
let token = server.grant("run-1", "EXP-12");                 // an earlier token of run-1 dies
// MCP config: { "clock": { "type": "http", "url": server.url(),
//               "headers": { "Authorization": "Bearer <token>" } } }
server.revoke("run-1");                                      // the provider hears ended("run-1")

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

### The public API

| Item | Role |
| --- | --- |
| `server::ToolProvider` | `tools()`, `call(caller, name, arguments)`, `ended(session_id)`: what the server asks of its host |
| `Server::serve` / `serve_with` | bind `127.0.0.1:0`, serve a provider (plus `RawRoute`s that own their prefix and their auth) |
| `Server::{grant, revoke, revoke_all, admitted, url, base}` | the runs' tokens and the endpoint |
| `Client::connect`, `list_tools`, `call`, `client::call_once` | the blocking client; `ClientError` = `Http(status)`, `Transport`, `Rpc`, `Timeout` |
| `oauth::{begin, Authorization::finish, connect, connected, disconnect}` | an OAuth connection, following `upstream.json` |
| `oauth::{Vault, SealedVault, MemoryVault}` | where credentials live; `SealedVault` seals every value with `contract::vault` |
| `contract::{exposes, access, audit, vault, upstream, actions}` | the pure contract, one module per fixture |

`tests/fixtures.rs` replays every fixture case; `tests/loopback.rs` serves and
calls over a real socket.

## The dependency rule

No Exponential app crate (`api`, `domain`, `coding`, `ui`, `app`, …) and no
other lib of this repo, gated by `tests/dependency_gate.rs`. The hosts
(codemode, epilot, epilot-screen, `api`, `ui`) depend on emcp, never the
reverse. eui-core stays off it: it is published on crates.io, which cannot
carry a git dependency.

## Licence

Apache-2.0.

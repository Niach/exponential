# agent-core

The shared **agent loop** for the Exponential desktop apps, ported from
`apps/companion/src`. Built as a C-ABI `cdylib`/`staticlib` and consumed by:

- **macOS** (Swift) — via a clang module map over `include/agent_core.h`.
- **Linux** (Zig) — via `@cImport(@cInclude("agent_core.h"))`.

The C surface is in `src/ffi.rs` and mirrors `include/agent_core.h`. It is
**synchronous and thread-safe**: calls return immediately, work runs on the
core's own background runtime, and every outbound event flows through the single
callback set with `agent_core_set_event_callback`. The host marshals callback
events onto its UI thread (`DispatchQueue.main` / `g_idle_add`).

## Status: M0 (scaffold)

Dependency-free stub so both GUIs can verify they link and receive events. It
stands up the lifecycle (`create`/`start`/`stop`/`free`) and emits a `log`
heartbeat. Setup/identity (`claim_setup`, `github_device_login`, `uninstall`)
and the agent-run bridge (`submit_run_result`, `cancel_run`) return
`ERR_NOT_IMPLEMENTED` until M5/M6.

## Build & test

```bash
cargo build -p agent-core            # produces target/{debug,release}/libagent_core.{so,dylib,a}
cargo test  -p agent-core            # FFI lifecycle smoke test + domain-contract constants
```

## Generated code

`src/domain_contract.rs` is generated from
`packages/domain-contract/contract.json` — do not edit it by hand. Regenerate
with `bun run --filter @exp/domain-contract generate` (emits the Swift, Kotlin,
Rust, and Zig constants together).

## Header generation (later)

`include/agent_core.h` is **hand-maintained** for now. Once M6 adds crates.io
dependencies, wire `cbindgen` (see `cbindgen.toml`) as a `build.rs` step so the
header is generated from `src/ffi.rs` and stays in sync automatically.

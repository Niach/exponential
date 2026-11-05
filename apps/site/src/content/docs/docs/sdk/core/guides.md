---
title: Core guides
description: Use Core's types today through emcp, or from a checkout; regenerate the TypeScript, Swift and Kotlin twins.
---

## Use it today

Core is the `ecore` crate. It has no release tag yet. Two ways in:

- **Through MCP Proxy.** `emcp` re-exports it as `emcp::ecore` and `emcp::{SessionId, Token, Failure, ErrorCode, DeliveryVerdict}`; depend on `emcp` at its tag ([MCP Proxy guides](/docs/sdk/mcp-proxy/guides/)).
- **From a checkout.** A path or git dependency on `apps/desktop/crates/ecore` at a pinned `rev`, until the first tag; then the rules of [Install from a release tag](/docs/sdk/install/) apply.

```rust
use emcp::ecore::{SessionId, Token};

let run = SessionId::new("run-1")?;          // the one id rule
let token = Token::from_header(header);      // Option<Token>: 64 lowercase hex
let value = token.map(|t| t.header());       // back to "Bearer …"
```

## Change a type

The contract is the one source. Edit `packages/ecore/contract.json`, then regenerate all four languages:

```sh
bun run --filter @exp/ecore generate
```

`packages/ecore/fixtures/ecore.json` replays on Rust, TypeScript, Swift and Kotlin, and a drift test fails a stale output.

The contract and the crate in full: [Reference](/docs/sdk/core/reference/).

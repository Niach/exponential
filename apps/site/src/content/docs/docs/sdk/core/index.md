---
title: Core
description: The plain types every module shares (ids, grants, capabilities, the delivery verdict, errors and the event envelope) in Rust, TypeScript, Swift and Kotlin.
---

Core holds the plain types every module shares. Modules call each other over MCP through [MCP Proxy](/docs/sdk/mcp-proxy/); what they pass is spelled here. Apache-2.0.

## Status

Built, unreleased. One contract (`packages/ecore/contract.json`) generates the Rust crate `ecore` and its TypeScript, Swift and Kotlin twins, and one fixture replays on all four. No release tag is cut yet; `emcp` takes `ecore` by path, so an `emcp-v*` checkout carries it.

| Part | Where | State |
|---|---|---|
| **Rust** | `ecore` | built |
| **TypeScript** | `@exp/ecore` (private) | built |
| **Swift** | generated into ExpCore (`ECore`) | built |
| **Kotlin** | generated into the Android app (`com.exponential.app.ecore`) | built |
| **Release** | A tag and published packages | planned |

## The types

| Type | What |
|---|---|
| Ids | `SessionId`, `RunId`, `SpaceId`, `WindowId`, `DeviceId`, one id rule |
| Grants | `Token` (64 lowercase hex, `Authorization: Bearer …`) and `Grant` (session, run, capabilities, expiry) |
| Capabilities | `capture`, `input:person`, `space`, `lend`, `ui`, `system` |
| Delivery verdict | `delivered_verified`, `not_delivered` or `retry_safe` |
| Errors | `Failure { code, message, retryable }`, refusals included |
| Event envelope | `Envelope { v, id, kind, at, source, sessionId?, data }` |

## Types only

No I/O, no async runtime, no network, no UI and no dependency on any other module: serde and serde_json only. A test in the crate (`tests/dependency_gate.rs`) enforces it, plus a source-size budget.

Next: [Guides](/docs/sdk/core/guides/) · [Reference](/docs/sdk/core/reference/)

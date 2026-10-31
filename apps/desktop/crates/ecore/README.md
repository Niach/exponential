# ecore

eCore (#EXP-1504): the plain types every ExponentialOS module shares. Modules
call each other's capabilities over MCP through eMCP; what they pass is
spelled here.

- **ids:** `SessionId`, `RunId`, `SpaceId`, `WindowId`, `DeviceId` (one id rule);
- **grants:** `Token` (64 lowercase hex, `Authorization: Bearer …`) and `Grant`
  (session, run, capabilities, expiry);
- **capabilities:** `capture`, `input:person`, `space`, `lend`, `ui`, `system`;
- **the delivery verdict:** `delivered_verified` / `not_delivered` / `retry_safe`;
- **the error shape:** `Failure { code, message, retryable }`, refusals included;
- **the event envelope:** `Envelope { v, id, kind, at, source, sessionId?, data }`.

The enums, constants and rules come from `packages/ecore/contract.json`, which
also generates the TS (`@exp/ecore`), Swift (`ExpCore` `ECore`) and Kotlin
(`com.exponential.app.ecore.ECore`) twins: `bun run --filter @exp/ecore generate`.
`packages/ecore/fixtures/ecore.json` replays on all four.

## The rule

Types and pure rules only: no I/O, no async runtime, no network, no UI, and
no dependency on any other module (serde and serde_json only).
`tests/dependency_gate.rs` enforces it, plus a source-size budget; raise the
budget only with a reason.

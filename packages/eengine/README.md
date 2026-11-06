# @exp/eengine

The Engine's contract (#VAPP-259, `docs/spec/apps.md` §13.1). The Engine is the stateful runtime around genUI that runs Apps in every host: Exponential, the softcoded app, eDE, Spezl and exported apps. The Apps format and its pure logic stay in genUI's host (`@exponential_app/eui/host`, `eui-core/src/host`). Each `fixtures/*.json` is the spec (its `$comment`) plus the cases. `src/` holds the pure TypeScript reference that replays them. It is the web's twin of the Rust crate `eengine`, which replays the same fixtures.

```ts
import { brokerDecide, consentSheet, lifecycleStep } from "@exp/eengine"
import fixture from "@exp/eengine/fixtures/capabilities.json" with { type: "json" }
```

| fixture | covers |
|---|---|
| `lifecycle.json` | launch, foreground, background, terminate, `online`, deep links (`crossAppTarget`), notification taps; the effects (open, flush, sources, automations, persist, refused) |
| `consent.json` | the sheet's answer, grant writes (only the ConsentUi, only after a gesture), the Vault record, reconciling on a package load, the ask sheet and the Permissions sheet as genUI surfaces |
| `capabilities.json` | the capability catalog (scopes, `ask` vs `picker`, host-only), `<capability>__<tool>` / `<capability>.<tool>` names, the one broker for UI and agent calls, an agent's tool list, the audit row |
| `providers.json` | the `HostConfig` (required and optional providers; no telemetry, no updates), identity kinds (local = "me"), the network policy |
| `outbox.json` | a `$ref` to `domain-contract` `app-records.json` `outbox` (O1, not copied) plus when it applies (sync provider = queue, none = write through) |

Licence: Apache-2.0

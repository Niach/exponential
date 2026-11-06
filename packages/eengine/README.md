# @exponential_app/engine

The Engine's contract (#VAPP-259, `docs/spec/apps.md` §13.1). The Engine is the stateful runtime around genUI that runs Apps in every host: Exponential, the softcoded app, eDE, Spezl and exported apps. The Apps format and its pure logic stay in genUI's host (`@exponential_app/eui/host`, `eui-core/src/host`). Each `fixtures/*.json` is the spec (its `$comment`) plus the cases. `src/` holds the pure TypeScript reference that replays them. It is the web's twin of the Rust crate `eengine`, which replays the same fixtures.

```ts
import { brokerDecide, consentSheet, lifecycleStep } from "@exponential_app/engine"
import fixture from "@exponential_app/engine/fixtures/capabilities.json" with { type: "json" }
```

Inside this monorepo the workspace package is `@exp/eengine` (private, the same entries); every `eengine-v*` tag publishes it to npm (`release/prepare-npm.ts`; Node 18+, Bun, Deno). Swift and Kotlin reach the Rust crate through `eengine-ffi`: SwiftPM `ExponentialEngine`, Maven `at.exponential:engine`.

| fixture | covers |
|---|---|
| `lifecycle.json` | launch, foreground, background, terminate, `online`, deep links (`crossAppTarget`), notification taps; the effects (open, flush, sources, automations, persist, refused) |
| `consent.json` | the sheet's answer, grant writes (only the ConsentUi, only after a gesture), the Vault record, reconciling on a package load, the ask sheet and the Permissions sheet as genUI surfaces |
| `capabilities.json` | the capability catalog (scopes, `ask` vs `picker`, host-only), `<capability>__<tool>` / `<capability>.<tool>` names, the one broker for UI and agent calls, an agent's tool list, the audit row; capabilities v1 (E5): every tool's schemas, the bridge between the broker and a native provider (validation, app files, notification ids and taps) and the `print.pdf` layout, byte for byte |
| `providers.json` | the `HostConfig` (required and optional providers; no telemetry, no updates), identity kinds (local = "me"), the network policy |
| `outbox.json` | a `$ref` to `domain-contract` `app-records.json` `outbox` (O1, not copied) plus when it applies (sync provider = queue, none = write through) |

## Web capabilities

`@exp/eengine/web` is the browser's host side of capabilities v1 (#VAPP-265): `webCapabilities(platform = browserPlatform(), { onTap })` answers the native providers the Engine's bridge calls, each `{ name, call(tool, args) }` returning an MCP call result that matches `capabilities.json` `nativeOutputSchema`. Every browser API sits behind the `WebPlatform` seam (tests use fakes). A capability the browser lacks is left out.

| capability | tools | browser |
|---|---|---|
| `files` | `open`, `save` | `showOpenFilePicker` / `showSaveFilePicker` when present, else a hidden `<input type=file>` (cancel = its `cancel` event) and an `<a download>` blob URL (`saved: true`) |
| `photos` | `pick`, `list`, `get` | the library is the app's OPFS `photos/` dir: `pick` = the image picker, each picked file copied in as `<lastModified>-<name>` (the id; takenAt = lastModified); `list` newest first, cursor = an offset; no OPFS = an empty library |
| `share` | `send` | `navigator.share` (files only when `canShare({files})`); absent without `navigator.share` |
| `print` | `print` | a hidden iframe on a blob URL, `contentWindow.print()`; always `printed: true` (browsers cannot tell) |
| `notifications` | `post`, `cancel` | `Notification` (asks when `default`, `denied` = `failed: notifications blocked`); `tag` = the id; a tap calls `onTap(tap)` + `window.focus()`; absent without `Notification` |

- **User activation:** pickers, the share sheet and print open only inside a user gesture. App UI must call through the broker within the click's task (no awaited round trip first); without one, the call answers `failed: …`.
- **Scheduled notifications** are page timers: one fires only if the page is still open at `at` (no service worker, nothing survives a reload).
- No permission strings: the browser prompts for notifications itself; the others need none.

Licence: Apache-2.0

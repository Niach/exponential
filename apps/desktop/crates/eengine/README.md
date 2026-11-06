# eengine

The **Engine** (#VAPP-259, `docs/spec/apps.md` §13.1): one runtime for Apps in every host: Exponential, the softcoded app, eDE, Spezl and exported apps. The Apps format and its pure logic stay in genUI's host (`eui_core::host`). The Engine is the stateful, I/O-bearing part around it.

- `lifecycle`: an app instance's lifecycle (launch, foreground, background, terminate, online, deep links, notification taps) as one reducer with effects.
- `broker`: the capability catalog and the one broker for app UI and agent calls. The manifest (format 5 `capabilities`) is the ceiling, consent comes next, then the audit row.
- `consent`: the sheet's answer, grant writes (only the ConsentUi, only after a gesture), the Vault record and its reconcile, and the ask and Permissions sheets as genUI surfaces made only of existing components.
- `providers`: `RecordStore`, `BlobStore`, `SyncTransport`, `Identity`, `Vault`, `WindowHost`, `Clock`, `HostZone`, plus `HostConfig`, which has no telemetry and no update field.
- `outbox`: O1's outbox (`app-records.json` `outbox`), moved here from the desktop's `domain`.
- `stores`: `MemoryRecordStore`, `LocalIdentity` (a device uuid, no account), `FsBlobStore` and `MemoryVault`.

Features: `sqlite` (`SqliteRecordStore`, the app-records table `elocal` owns, #EDE-55) and `hub` (capabilities as MCP tools on an in-process MCP Proxy hub, #VAPP-262).

The contract is `packages/eengine` (fixtures plus the TS twin `@exp/eengine`). `tests/fixtures.rs` replays it, and `tests/dependency_gate.rs` keeps app crates and path dependencies out.

Licence: Apache-2.0

# `eengine-ffi`: the Engine for Swift and Kotlin

The UniFFI 0.31 facade over [`eengine`](../eengine) for iOS, macOS and
Android (#VAPP-263), coarse like genUI's
[`exponential-ui-ffi`](../exponential-ui-ffi): an xcframework / an Android
`.so` plus the generated bindings. Swift and Kotlin never re-implement the
Engine (`docs/spec/apps.md` §13.1): they hand it their providers and keep
the painters. No app crate.

## The contract

The spec is `packages/eengine/fixtures` (their `$comment`s); the TypeScript
twin is `packages/eengine/src` (npm `@exponential_app/engine`).

- `Engine(host, identity, providers)`: one per host process.
  - `EngineHost {kind, platform, formFactor, locale, zone?, network, dataDir?}`
    (`kind` = providers.json `hosts`; `network` = `off | upstreamsOnly | any`;
    `dataDir` = where the default stores live, None = in memory).
  - `EngineIdentity` = `member {userId, role?, access?}` | `local {deviceId}` | `peer {key}`.
  - `EngineProviders`: `vault` and `window` are required; `storage`,
    `blobs`, `sync`, `capabilities`, `consent`, `audit`, `clock` are optional.
  - `load(fileJson)` → `AppInstance`: a package file (`{package, signature?}`)
    or a bare package, verified, validated, its grants reconciled.
    `EngineError.load {kind: signature | invalid, issuesJson}` says why not.
- `AppInstance`:
  - `event(eventJson)` → the effects (JSON). `{"type":"launch"}`,
    `foreground`, `background`, `terminate`, `online`, `deepLink`,
    `notificationTap` (lifecycle.json). Open, persist and flush are carried
    out through the providers; sources and automations are the host's.
  - `recordsValue(source)`: a `records:<kind>?…` source's value, the
    outbox laid over the store's rows (None = not a records source).
  - `call(name, argsJson, newIds)` → `CallResult` = `value {json}` |
    `ask {verdictJson, sheet}` | `refused {code, message}`, for
    `records.upsert` / `records.delete` / `records.batch`, `app.navigate`
    and capabilities (`photos.pick`, through the broker).
  - `answerConsent(name, argsJson, verdictJson, answer)`: the person's answer
    to an `ask` (`allow | once | deny | dismiss`).
  - `setGrant(scope, value?)`, `permissionsSheetJson()`, `grantsJson()`,
    `manifestJson()`: the Permissions sheet (consent.json).
  - `outboxJson()`, `pending()`, `queues()`, `flush()`: O1's outbox when
    the host has a `SyncTransport`.
  - Feature `hub`: `agentAccess(run)` → `{url, token, tools}` serves the
    app's capabilities to an on-device agent over MCP Proxy's loopback hub,
    through the same broker; `revokeAgent(run)`.
- `validateHostConfig(configJson)` (providers.json `validateCases`),
  `engineVersion()`, `engineFeatures()`.

The providers, implemented in Swift or Kotlin:

| trait | what | default |
|---|---|---|
| `RecordStore` | an app's rows, batches with `writeId`, history, `subscribe` → `RecordsListener.changed` | SQLite at `<dataDir>/records.sqlite` (feature `sqlite`); in memory without a `dataDir` |
| `BlobStore` | record files | files under `<dataDir>/blobs`; in memory without a `dataDir` |
| `Vault` | secrets, grants, the last screen (Keychain, Keystore) | required |
| `WindowHost` | `present(app, screen, paramsJson, how)`: `replace`, `push`, `sheet` | required |
| `Capability` | a native tool provider under its catalog name: `toolsJson()`, `call(tool, argumentsJson)` (MCP tools and results) | none |
| `ConsentUi` | answers the consent sheet inline (blocks) | none: `call` answers `ask` and the host paints the sheet |
| `SyncTransport` | replays outbox entries (`{"ok":true}` / `{"transport":true}` / `{"code":…}`) | none: writes go to the store |
| `CallAudit` | the broker's audit rows | none |
| `EngineClock` | ISO time | the system clock |

JSON text crosses wherever the Engine's own types are JSON (packages, rows,
batches, events, effects, MCP tools and results, genUI surfaces), as in
genUI's facade. A provider runs on the caller's thread while the instance is
locked: it must not call back into the same `AppInstance` synchronously.
Errors: `EngineError` = `invalid {reason}` | `load {kind, issuesJson}` |
`failure {code, reason}`.

## Features

`sqlite` (default): the SQLite record store. `hub`: MCP Proxy's in-process
hub (tokio, hyper, rmcp). No Voice, P2P or `eui-gen`. The products ship the
defaults; a host that builds the facade itself picks its set with
`EENGINE_FFI_FEATURES="sqlite,hub"` (or `""`) in every script below, and
regenerates the bindings with the same set (the `hub` methods only exist in
a `hub` build).

## Build

```bash
bash apps/desktop/crates/eengine-ffi/generate-bindings.sh  # bindings/swift + bindings/kotlin from the host library
bash apps/desktop/crates/eengine-ffi/build-ios.sh          # out/ExponentialEngineFFI.xcframework (+ bindings)
bash apps/desktop/crates/eengine-ffi/build-android.sh      # out/jniLibs/<abi>/libeengine_ffi.so (+ bindings)
bash apps/desktop/crates/eengine-ffi/run-binding-tests.sh  # the Swift + Kotlin suites against the host dylib
```

Device builds use the workspace's `mobile` profile (LTO, one codegen unit,
stripped), one crate type per build so LTO applies. The generated binding
sources under `bindings/` are COMMITTED (Kotlin ships with
`omit_checksums`, so regenerate after every `lib.rs` change); `out/` is
ignored. Kotlin uses JNA (≥ 5.12).

`tests/facade.rs` runs the facade from Rust with providers written the way
Swift and Kotlin write them; `tests/swift/main.swift` and
`tests/kotlin/FacadeTest.kt` do the same through the bindings and replay
providers.json's `validateCases`.

## Distribution

Every `eengine-v*` tag (`release-eengine.yml`, `docs/libraries.md`) publishes:

- **SwiftPM**: `https://github.com/Niach/exponential-engine-swift`, tagged
  `<version>`, product `ExponentialEngine` (`import ExponentialEngine`;
  iOS 15+, macOS 12+). The xcframework zip is an asset of the
  `eengine-v<version>` release; `release/swift-package.sh` writes the package.
- **Maven Central**: `at.exponential:engine:<version>` (an AAR: the `.so`
  for arm64-v8a, armeabi-v7a, x86_64 + the binding, package
  `at.exponential.engine.ffi`; JNA comes along). `android/` is the Gradle
  library on genUI Compose's toolchain; `release/central-bundle.sh` is the
  Portal bundle.
- **npm**: `@exponential_app/engine`, the contract and the TS twin (`packages/eengine`).

```swift
.package(url: "https://github.com/Niach/exponential-engine-swift", exact: "0.1.0")
```

```kotlin
implementation("at.exponential:engine:0.1.0")
```

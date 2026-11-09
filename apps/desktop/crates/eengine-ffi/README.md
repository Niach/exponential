# `eengine-ffi`: the Engine for Swift and Kotlin

The UniFFI 0.32 facade over [`eengine`](../eengine) for iOS, macOS and
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
| `SyncTransport` | replays outbox entries (`{"ok":true,"answer":…}` / `{"transport":true}` / `{"code":…,"message":…,"invalid":…}`) | none: writes go to the store |
| `CallAudit` | the broker's audit rows | none |
| `EngineClock` | ISO time | the system clock |

JSON text crosses wherever the Engine's own types are JSON (packages, rows,
batches, events, effects, MCP tools and results, genUI surfaces), as in
genUI's facade. A provider runs on the caller's thread while the instance is
locked: it must not call back into the same `AppInstance` synchronously.
Errors: `EngineError` = `invalid {reason}` | `load {kind, issuesJson}` |
`failure {code, reason}`.

## Capabilities (Swift)

`swift/` (#VAPP-265) = the native half of capabilities v1
(`packages/eengine/fixtures/capabilities.json`), ONE SwiftPM module per
capability (#VAPP-268) so an App Store app links a privacy framework only
when it declares the capability (ITMS-90683: a linked API needs its purpose
string): `ExponentialEnginePhotos` (Photos, PhotosUI), `ExponentialEngineFiles`,
`ExponentialEngineShare` (LinkPresentation), `ExponentialEnginePrint`,
`ExponentialEngineNotifications` (UserNotifications), each on
`ExponentialEngineCapabilityCore` (arguments, results, threading, the
presenter), and the umbrella `ExponentialEngineCapabilities`:
`EngineCapabilities.ios(CapabilityConfig(presenter:))` → the five
`Capability`s for `EngineProviders(capabilities:)`. `Package.swift` here is
the same layout over the workspace's `out/` xcframework (the iOS runner,
`apps/runner/ios`, links it by path). The Engine validates,
stores files and shapes app results; these only see native arguments
(bytes = base64 `data`) and answer `nativeOutputSchema`.

| capability | iOS | Info.plist (E7 declares) |
|---|---|---|
| `photos` | `pick` = PHPickerViewController (no permission, HEIC stays `image/heic`); `list`/`get` = PhotoKit, images newest first, id = `localIdentifier`, cursor = an offset token, bytes via PHImageManager (iCloud allowed); library access requested on first use | `NSPhotoLibraryUsageDescription` |
| `files` | `open` = UIDocumentPickerViewController (copies; `accept` → UTTypes); `save` = export a temp file | none |
| `share` | UIActivityViewController; `shared` = its completion | `NSPhotoLibraryAddUsageDescription` (Save Image) |
| `print` | UIPrintInteractionController with the bytes; `printed` = its completion | none |
| `notifications` | UNUserNotificationCenter: authorization on the first post; identifier = the Engine's `<app>/<id>` (posting again replaces, `cancel` removes pending + delivered); `at` ahead → a time-interval (< 1 min) or calendar trigger; the tap JSON rides in `userInfo` | none (runtime prompt) |

- **Threading:** `call` BLOCKS until the person answers; UI is presented on
  the main queue from `presenter` (default: the key window's top view
  controller). Never call a capability on the main thread: it answers
  `failed: main thread` (the Engine calls from its worker).
- **Taps:** set `UNUserNotificationCenter.current().delegate` at launch; in
  `didReceive`, `NotificationTaps.tapJson(response:)` → `app.event(eventJson:)`
  (nil for a dismiss or a foreign notification).
- **macOS:** `EngineCapabilities.macOS()` = notifications; photos, files,
  share, print answer `unavailable` (no AppKit seams yet).
- Every platform call sits behind a seam (`Seams.swift`); the suite
  (`tests/swift-capabilities`, in `run-binding-tests.sh swift`) replays the
  fixture against fakes and typechecks the UIKit half for iOS 15.
- Limits: iOS keeps at most 64 pending notifications; a library change
  between `list` pages can shift the offset cursor.

## Capabilities (Kotlin)

The AAR also ships the native capabilities v1 (#VAPP-265,
`packages/eengine/fixtures/capabilities.json`), package
`at.exponential.engine.capabilities` (`android/src/main/kotlin`):

```kotlin
val host = ComponentActivityHost(applicationContext) { currentActivity } // the foreground ComponentActivity
val capabilities = AndroidCapabilities.all(host) // photos, files, share, print, notifications → EngineProviders.capabilities
AndroidNotifications.configure(applicationContext, R.drawable.ic_notification, "Reminders") // optional
// launch activity, onCreate + onNewIntent:
NotificationTaps.fromIntent(intent)?.let { tap -> instance.event(tap) }
```

**Threading:** a call BLOCKS until the person is done (picker, chooser,
print dialog, permission prompt): the Engine calls on its worker thread, the
UI work is posted to the main thread over the host's ActivityResultRegistry
and waited on. Never call a capability on the main thread: every tool
answers `failed: main thread` there. A pending call fails (`failed: activity
gone`) when its activity is destroyed (a rotation included, unless the host
declares `android:configChanges`).

| capability | Android |
|---|---|
| `photos` | `list`/`get`: MediaStore.Images, newest first (DATE_TAKEN, then DATE_MODIFIED), id = the content URI, cursor = an offset token; `pick`: the Photo Picker (`PickVisualMedia` / `PickMultipleVisualMedia`) |
| `files` | SAF `ACTION_OPEN_DOCUMENT` (accept → MIME types; an unknown extension widens to every type), `ACTION_CREATE_DOCUMENT` |
| `share` | an `ACTION_SEND` chooser; a file through the library's `ShareFileProvider` (read-only, URI grants, cache files pruned after a day); `shared` = the chooser's chosen-component callback (Copy and similar chooser actions read as false) |
| `print` | PrintManager: a PDF as is, a PNG/JPEG fitted onto a page; `printed` = the job queued/started/completed |
| `notifications` | NotificationManagerCompat on channel `exponential-engine`, tag = the id, numeric id = FNV-1a of it; a future `at` = an inexact `setAndAllowWhileIdle` alarm; records in SharedPreferences (`cancelled` = a pending one, or a shown one still in the shade); a tap opens the launch activity with the tap JSON verbatim as an extra |

`toolsJson()` = the fixture's native `tools` (description + the app-facing
`inputSchema`), generated into `ToolCatalog.kt` and locked by its test.

The library manifest merges its components into the app: `ShareFileProvider`
(`${applicationId}.exponential.engine.share`), `ShareChosenReceiver`,
`NotificationAlarmReceiver`, `NotificationBootReceiver` (none exported).
The PERMISSIONS are the app's (E7 declares them; the library declares none):

| capability | permissions |
|---|---|
| `photos` | `READ_MEDIA_IMAGES` (API 33+), `READ_MEDIA_VISUAL_USER_SELECTED` (API 34+, partial access), `READ_EXTERNAL_STORAGE` with `android:maxSdkVersion="32"`; `pick` needs none |
| `files`, `share`, `print` | none |
| `notifications` | `POST_NOTIFICATIONS` (API 33+, asked on the first post); `RECEIVE_BOOT_COMPLETED` (re-arms scheduled ones after a reboot; without it they wait for the next app update) |

`cd android && ../../../../../packages/exponential-ui-compose/gradlew -p . testDebugUnitTest`
runs the pure parts on the plain JVM (fakes behind `PhotoLibrary`,
`DocumentPicker`, `ShareSheet`, `Printer`, `NotificationPoster`,
`AlarmScheduler`, `NotificationStore`; no `.so`) against the fixture's
`nativeOutputSchema` and `bridgeCases`.

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

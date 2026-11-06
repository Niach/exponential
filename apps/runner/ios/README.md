# The iOS runner (#VAPP-268, E6c)

The prebuilt shell an exported app runs in on iPhone and iPad: SwiftUI +
`ExponentialUI` (genUI's SwiftUI host) + the Engine (`eengine-ffi`, built from
the workspace). The contract is `packages/eengine/fixtures/runner.json` (its
`$comment`) and `docs/spec/apps.md` §13.3. The runner has NO chrome of its
own: the window is the genUI surfaces the Engine builds (`app`, `consent`,
`permissions`, `toast`). No OTA, no update channel, no telemetry, no account.

iOS can't be repacked: a linked privacy API needs its purpose string
(ITMS-90683). So the runner is a TEMPLATE: `make-project.sh` emits an Xcode
project per bundle that links ONLY the declared capability modules
(`ExponentialEnginePhotos`, `-Files`, `-Share`, `-Print`, `-Notifications`,
see `apps/desktop/crates/eengine-ffi/Package.swift`), then `xcodebuild`.

| path | what |
|---|---|
| `Sources/RunnerKit` | the glue (SwiftPM, tested with `swift test`): the bundle (`app/` → `runner.json` + `package.app.json`), the Keychain Vault + the device uuid, the `RunnerSession` (ONE `AppInstance`: lifecycle, deep links, notification taps, every function call, consent, Permissions, the toast, the form factor) and the `RunnerPainter` it drives |
| `App/` | the SwiftUI app (not a SwiftPM target): `RunnerModel` = the painter on one `ExponentialHost`; `RunnerApp` = scene events, `onOpenURL`, the window at its full height (the page scrolls inside the shell) |
| `make-project.sh` | bundle + capabilities → `project.yml` (XcodeGen), `Info.plist` (bundle id/name/version + URL scheme from `runner.json`, purpose strings from the package's `capabilities[].why`), `RunnerCapabilities.swift` (only the declared modules), the bundle at `<name>.app/app`; `--build simulator|device` |
| `sample/app` | a small bundle with an `ask` scope (`photos.read`): the build check and the screenshots |
| `check-build.sh` | builds the sample twice for the simulator and checks what is linked (`otool -L`) and declared (`Info.plist`) |
| `../../../scripts/lanes/runner-ios.sh` | THE MAC BUILD LANE: the xcframeworks (when missing) + `make-project.sh` under the `ios` lane lock |

```bash
# the xcframeworks (simulator slice; aarch64-apple-ios for a device)
EENGINE_APPLE_TARGETS="aarch64-apple-darwin aarch64-apple-ios-sim" bash apps/desktop/crates/eengine-ffi/build-ios.sh
EXPONENTIAL_UI_APPLE_TARGETS="aarch64-apple-ios-sim" bash apps/desktop/crates/exponential-ui-ffi/build-ios.sh
swift test --package-path apps/runner/ios          # RunnerKit against the real engine (macOS slice)
bash apps/runner/ios/check-build.sh                 # the generated project builds; links only what it declares
# one app (what E7's `exponential bundle --target ios` runs, here or on a remote Mac):
scripts/lanes/runner-ios.sh <owner> --bundle <app dir> --out <dir> --capabilities photos,share --build device
```

- **Capabilities → modules:** `photos.read` → photos (Photos, PhotosUI;
  `NSPhotoLibraryUsageDescription` = its `why`), `files.read`/`files.write`
  → files, `share.send` → share (LinkPresentation;
  `NSPhotoLibraryAddUsageDescription`, the sheet's Save Image), `print.pdf`
  → print, `notifications.post` → notifications (UserNotifications; the tap
  delegate). `camera.capture`, `clipboard.*`, `location.read` and
  `http.fetch` have no iOS native yet: the broker answers `unavailable`.
- **Signing:** `--build device` signs with `RUNNER_DEVELOPMENT_TEAM` when
  set, else leaves the `.app` unsigned for E7 to sign. A `runner.json` id
  with `_` becomes `-` in the bundle id (the URL scheme keeps the id).
- **Threading:** every Engine call runs on one serial worker queue (a
  capability blocks it while its picker is up); the painter runs on main.
- **Storage:** `Application Support/<config id>/` (`records.sqlite`,
  `blobs/`); the Vault = the Keychain (service = the config id,
  `AfterFirstUnlockThisDeviceOnly`).

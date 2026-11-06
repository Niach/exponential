# The Android runner

#VAPP-267 (E6b, epic #VAPP-259): the prebuilt APK/AAB template an exported app
runs in. The contract is `packages/eengine/fixtures/runner.json` (its
`$comment` is the spec) and `docs/spec/apps.md` §13.3. This project only wires
the platform to it:

- **Painting:** genUI's Compose host (`packages/exponential-ui-compose`,
  `ExponentialHost`). Every pixel is a genUI surface: `app` (the open
  screen, inside `runner_package`'s shell), `consent`, `permissions`, `toast`.
  The runner draws no UI of its own; the window background is the theme's.
- **Engine:** `at.exponential:engine` (`apps/desktop/crates/eengine-ffi/android`):
  ONE `AppInstance` per process. Identity `local` (the device uuid is
  created once under the Vault key `eengine/runner/device`), host kind
  `exported`, network from `runner.json`, records in SQLite and files in
  `FsBlobStore` under `files/apps/<id>/`. The Vault is `KeystoreVault`
  (AES-GCM values under a non-exportable Android Keystore key). The
  capabilities are E5's `AndroidCapabilities.all`.
- **Workspace only:** both libraries come in as Gradle INCLUDED BUILDS
  (`settings.gradle.kts`), substituted for their Maven coordinates. Nothing
  is resolved from a published Engine or eUI package.

No account, no telemetry, no update channel. Nothing here logs: R8 strips
`android.util.Log` calls (`proguard-rules.pro`). There is no analytics,
crash or Firebase SDK, and the package is never fetched: it is read from the
APK's assets.

## Layout

| file | what |
|---|---|
| `app/src/main/assets/app/` | THE BUNDLE (`runner.json` + `package.app.json`). The template ships a placeholder Notes app (two screens, a `photos.read` ask scope). E7 replaces the directory. |
| `RunnerBundle.kt` | reads the bundle: `runner.json` first (any issue = failure `config`), then the package (absent = `missing`) |
| `RunnerController.kt` | the glue: load (failures `signature` / `invalid`), the shell per form factor, lifecycle events, every app call through `instance.call` (Ask = surface `consent`, Refused = `toast`), `runner.navigate`, `runner.permissions`, `consent.toggle`, `consent.answer`, `records:` sources |
| `ComposePainter.kt` | `RunnerPainter` over `ExponentialHost` + the window composable |
| `RunnerActivity.kt` | the one activity: create = `launch` (or the cold `deepLink` / `notificationTap`), start = `foreground`, stop = `background`, finishing = `terminate`, a new intent = a link or a tap. It handles its own configuration changes (a pending picker survives a rotation). `wide` from 768 dp (genUI `md`). |
| `RunnerLinks.kt` | `<runner.json id>://<screen>?<params>` → the `deepLink` event |
| `KeystoreVault.kt` | the Vault |

## What E7 (`exponential bundle`) patches

| where | what |
|---|---|
| `app/src/main/assets/app/` | the bundle: `runner.json` + `package.app.json` |
| `applicationId` (`app/build.gradle.kts`, placeholder `at.exponential.runner.template`) | `runner.json` `id` |
| `versionName` / `versionCode` | `runner.json` `version` |
| `manifestPlaceholders["runnerScheme"]` | `runner.json` `id` (the deep-link scheme) |
| `res/values/strings.xml` `runner_label` | `runner.json` `name` |
| `res/mipmap*/ic_launcher` (`android:icon` / `android:roundIcon`; the template's is a placeholder vector) | the package icon, `mipmap/` + one PNG per density |
| `AndroidManifest.xml` between `E7:PERMISSIONS` and `/E7:PERMISSIONS` | the permissions below, for the package's declared capabilities only |

E7 compiles nothing: `scripts/runner-kits/android.sh` keeps this build's
merged manifest, compiled resources, their ids (`--emit-ids`, so the relink
keeps the ids R8 inlined into the dex), dex and native libs, and
`exponential bundle` relinks them per app (`apps/desktop/crates/eengine-bundle`
README, "Android").

The template declares NO permissions, and the Engine AAR declares none
either. E7 maps the capability manifest (E5, `eengine-ffi/README.md`
"Capabilities (Kotlin)"):

| capability | `<uses-permission>` |
|---|---|
| `photos` | `android.permission.READ_MEDIA_IMAGES` (API 33+), `android.permission.READ_MEDIA_VISUAL_USER_SELECTED` (API 34+), `android.permission.READ_EXTERNAL_STORAGE` with `android:maxSdkVersion="32"`. `pick` alone needs none (the Photo Picker). |
| `files`, `share`, `print` | none |
| `notifications` | `android.permission.POST_NOTIFICATIONS` (API 33+, asked on the first post), `android.permission.RECEIVE_BOOT_COMPLETED` (re-arms scheduled ones after a reboot) |
| `runner.json` `network` ≠ `off` | `android.permission.INTERNET` |

The components the AAR merges in (`ShareFileProvider`, the notification
receivers) need no patching: their authority derives from `applicationId`.

## Build and test

```bash
bash apps/desktop/crates/eengine-ffi/build-android.sh       # the Engine .so (ABIS=arm64-v8a for one)
bash apps/desktop/crates/exponential-ui-ffi/build-android.sh  # genUI's .so
cd apps/runner/android
../../../packages/exponential-ui-compose/gradlew -p . :app:assembleDebug      # or :app:assembleRelease (R8) / :app:bundleRelease
../../../packages/exponential-ui-compose/gradlew -p . :app:testDebugUnitTest  # JVM: needs `cargo build -p eengine-ffi --release --features cli`
../../../packages/exponential-ui-compose/gradlew -p . :app:connectedDebugAndroidTest  # on a device / emulator
```

The JVM suite (`app/src/test`) replays runner.json's `configCases`,
`failureCases`, `toastCases` and the bundle layout. It runs the template's
own bundle and drives the controller against the real Engine (the host
dylib through JNA) with a fake painter: launch, form-factor change,
navigate, records writes re-resolving bound sources, the consent round trip
(allow, deny), refused calls, Permissions toggles, deep links, notification
taps and background/terminate. Gate runs it (`runner-android` in
`.github/workflows/test.yml`). `RunnerActivityTest` (instrumented) runs the
bundled app on a device: the first screen paints and a note written through
the Engine shows in the list.

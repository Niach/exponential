# eengine-bundle

E7 (#VAPP-270): `exponential bundle` (the CLI subcommand lives in `crates/cli`, `commands/bundle.rs`). A signed package, one SVG icon and `bundle.toml` become native binaries on the prebuilt runners (`packages/eengine/fixtures/runner.json` `bundle`, `docs/spec/apps.md` §13.3). The contract is `packages/eengine/fixtures/bundle.json`; its `$comment` is the spec and `tests/fixture.rs` replays it.

```bash
scripts/runner-kits.sh --out /tmp/kits macos web          # the runner kits, from the workspace
exponential bundle photobook.app.json --icon icon.svg --config bundle.toml \
  --target macos,web --runners /tmp/kits --out dist
```

Nothing is compiled per app except iOS (the Mac lane links only the declared capability modules). No updater, no update feed, no telemetry, no account: a new package version is a rebundle.

| module | what |
|---|---|
| `prepare.rs` | the package file + bundle.toml → `Prepared`, or the first refusal (`notJson`, `signature`, `unsigned`, `catalog`, `capability`, `invalid`, `config`) |
| `kit.rs` | the runner kit of a target: `--runners <dir>/<target>` or the pinned `eengine-v<version>` release asset (sha256-checked); a kit without `NOTICES.txt` is refused |
| `manifest.rs` | the declared scopes → Android permissions, iOS modules + purpose strings, macOS sandbox entitlements, the Linux `.desktop` entry, the Windows URL protocol key, the web manifest |
| `icons.rs` | the SVG rendered at every size a target asks for |
| `targets/<target>.rs` | one target: copy the kit, write `app/`, the icons and the platform manifest, package, sign |

## Signing

With the author's own keys from `bundle.toml` `[signing.<target>]` (we never hold them). Without them a target signs ad-hoc or not at all and says so (`signing` in the fixture lists each target's inputs).

## Android (`--target android`)

Nothing is compiled per app: the kit is the template's release build taken apart, and the bundler relinks it with the Android SDK's build-tools.

**The kit** (`scripts/runner-kits/android.sh`, `apps/runner/android` `:app:assembleRelease`): `kit.json` (`templateId`, `compileSdk`), the merged `AndroidManifest.xml` as text (still on the template id, with the `E7:PERMISSIONS` markers), `resources.zip` (the merged aapt2 `.flat` files, app + libraries), `resource-ids.txt` (every entry of the release APK's resource table, `aapt2 dump resources`, in `--stable-ids` form: R8 inlined these ids into the dex; the bundler moves them onto the config id's package), `payload.zip` (the rest of the release APK: `classes*.dex`, `lib/<abi>/*.so`, the Java resources, `assets/dexopt`), `NOTICES.txt`.

**Per app:** the manifest gets the config id in place of the template id (applicationId, the deep-link scheme `<id>://`, the FileProvider authority, the receiver permission) and `manifest::android_permissions` between the markers; an overlay compiles `runner_label` (= `name`) and `mipmap{,-mdpi…-xxxhdpi}/ic_launcher.png` (`icons.android`); `aapt2 link --stable-ids` (`versionName` = `version`, `versionCode` = major × 1 000 000 + minor × 1 000 + patch). The .apk = the linked resources + the payload + `assets/app/` + `assets/NOTICES.txt`, `zipalign -P 16` (16 KiB pages for the .so; `-p` before build-tools 35), `apksigner sign` (v1–v3). The .aab = the same base module in proto format (`aapt2 link --proto-format`), `bundletool build-bundle`, `jarsigner`.

**The author's prerequisites:** `ANDROID_HOME` with `build-tools` (35+) and `platforms;android-<compileSdk>` (36), a JDK (`JAVA_HOME`: keytool, jarsigner, and java for apksigner/bundletool), `BUNDLETOOL` = `bundletool-all-<version>.jar` (or `bundletool` on PATH). Building the kit also needs the NDK (`cargo-ndk`) for the two native libraries.

**Signing** (`[signing.android]` of bundle.toml): `keystore` (.jks/.p12; relative = to the working directory), `alias`, `store-password`, `key-password` (default = the store password); `EXPONENTIAL_ANDROID_STORE_PASSWORD` / `EXPONENTIAL_ANDROID_KEY_PASSWORD` override the file, so passwords need not live in it. They reach apksigner and jarsigner through the environment, never argv. Without a keystore the run generates a throwaway debug key (PKCS12, `androiddebugkey`) and says the outputs are for testing only; a new key each run, so a rebundle installs over the previous one only after an uninstall.

`tests/android.rs` runs the whole target on a small synthetic kit (aapt2 dump badging, apksigner verify, jarsigner -verify, bundletool dump manifest); it skips, saying why, without ANDROID_HOME, JAVA_HOME and BUNDLETOOL.

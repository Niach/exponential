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

## `--target web`

Kit: `scripts/runner-kits/web.sh` = `apps/runner/web`'s `bun run build` (`dist/`) + `NOTICES.txt` (from `apps/runner/web/public/NOTICES.txt`) beside `index.html`. Output: `{name}-{version}-web/`, a static PWA directory any static host serves, over a copy of the kit:

- `app/` replaced (the package byte for byte + `runner.json`);
- `manifest.webmanifest` (`manifest::web_manifest`), `icons/icon-192.png` + `icon-512.png` and `favicon.ico` (16/32/48) from the SVG, the `<title>` = the app's name;
- `glyphs.json` pruned: the runner draws Lucide glyphs only from this file (no icon set in its JS), the kit carries every glyph, and the export keeps those named by any string of the package or of the runner's shell (runner.json, consent.json); a name outside it paints genUI's missing-icon fallback;
- `sw.js`'s `CACHE` and `FILES` lines recomputed over the final files (sha256 over every path + bytes), so the export precaches itself and a rebundle is a new cache;
- `NOTICES.txt`. Nothing to sign (`signing.web`).

Size (gzip -9, the album sample): main JS 542,219 → 392,945 bytes (+ 1,004 for its `glyphs.json`); the whole export 1,300,938 → 1,100,203 (sqlite-wasm's unused Worker1 script is dropped from the build too).

`tests/web.rs` bundles the album on a stand-in kit; `EENGINE_BUNDLE_WEB_KIT=<kits>/web cargo test -p eengine-bundle --test web` runs it on the real one.

## Linux and Windows

`scripts/runner-kits.sh --out <dir> linux windows` fills the kits:

- **linux**: `<arch>/exponential-runner` (profile `runner`; x86_64 on an x86_64 host, aarch64 when its rust target and `aarch64-linux-gnu-gcc` are installed, or `EXP_RUNNER_BIN_<ARCH>=<release asset>`), `<arch>/appimage-runtime` (the AppImage type2 runtime, pinned tag + sha256) and its `appimage-runtime.NOTICES.txt`.
- **windows**: `exponential-runner.exe`, built on Windows only (gpui compiles its shaders with the SDK's `fxc.exe`, so no cross build from Linux), or `EXP_RUNNER_EXE=<release asset>`.

`EXP_RUNNER_NOTICES=<file>` stands in for the runner's committed `NOTICES.txt`.

| output (per arch) | layout |
|---|---|
| `{id}_{version}_{arch}.deb` (`amd64`, `arm64`) | written here (ar + tar.gz, root-owned, no dpkg): `/usr/bin/<id>`, `/usr/share/<id>/app/`, `/usr/share/applications/<id>.desktop` (`Exec=<id> %u`, the `x-scheme-handler/<id>`), `/usr/share/icons/hicolor/<n>x<n>/apps/<id>.png`, `/usr/share/doc/<id>/NOTICES.txt` |
| `{name}-{version}-{arch}.AppImage` (`x86_64`, `aarch64`) | the kit's runtime + a squashfs (zstd) of the AppDir: `<id>` (the runner) beside `app/`, `AppRun` → `<id>`, `<id>.desktop`, `<id>.png` + `.DirIcon`, the hicolor icons, `NOTICES.txt` (the runner's + the runtime's). No update information. An AppImage registers no link scheme until a desktop integration tool installs its `.desktop` |
| `{name}-{version}-setup.exe` | NSIS, per user (no elevation) into `%LOCALAPPDATA%\Programs\<name>`: `<name>.exe` (the runner), `app\`, `app.ico` (rendered from the SVG at 16 to 256 px), `NOTICES.txt`, a Start-menu shortcut, `HKCU\Software\Classes\<id>` (`"<exe>" "%1"`), the Apps-and-features entry and `uninstall.exe` (which removes all of it; the app's data under `%APPDATA%` stays) |

**Tools the author installs:** `mksquashfs` (squashfs-tools) for the AppImage, NSIS 3 (`makensis`, Linux packages `nsis`) for the installer, `osslsigncode` to sign it. Each can be named by `MKSQUASHFS`, `MAKENSIS`, `OSSLSIGNCODE`. The `.deb` needs nothing.

```bash
cargo test -p eengine-bundle --test linux_windows    # stub kits; EENGINE_BUNDLE_RUNNERS=<kits> for real ones
```

Without `mksquashfs` or `makensis` the test checks the refusal names the tool; `EENGINE_BUNDLE_REQUIRE=linux,windows` makes them required. `check-eengine-runner-windows.yml` builds the real Windows kit, bundles the sample, lists the setup.exe with `7z l`, installs it silently, checks the files and the protocol key, runs the app 20 s and uninstalls it.

## Signing

With the author's own keys from `bundle.toml` `[signing.<target>]` (we never hold them). Without them a target signs ad-hoc or not at all and says so (`signing` in the fixture lists each target's inputs).

## macOS (`--target macos`)

Kit (`scripts/runner-kits/macos.sh`, on a Mac): `exponential-runner` (crate `eengine-runner`, profile `runner`, `MACOSX_DEPLOYMENT_TARGET=13.0`; universal via lipo when `aarch64-apple-darwin` and `x86_64-apple-darwin` are both installed, else the host arch) + `NOTICES.txt` (`apps/desktop/crates/eengine-runner/NOTICES.txt`, `EXP_RUNNER_NOTICES` overrides the path).

Outputs:

- `{name}.app`: `Contents/MacOS/<name>` (the runner), `Contents/Resources/app/` (`package.app.json`, `runner.json`), `Contents/Resources/AppIcon.icns` (the SVG at 16–1024 px), `Contents/Resources/NOTICES.txt`, and `Contents/Info.plist` (`CFBundleIdentifier` = the config id, name, version, `CFBundleURLTypes` with the scheme, `LSMinimumSystemVersion` 13.0, `NSHighResolutionCapable`). It is signed with the hardened runtime and the sandbox entitlements of the declared scopes (`manifest::macos_entitlements`). Under the App Sandbox the runner's data lands in `~/Library/Containers/<id>/Data/Library/Application Support/<id>/` (`records.sqlite`, `files/`).
- `{name}-{version}.dmg`: hdiutil (UDZO, the .app + an `/Applications` link). Without hdiutil (a Linux host) only the .app is written, and the CLI says so.

`[signing.macos]` in bundle.toml (all optional; none = ad-hoc `codesign -s -`, which runs on the bundling Mac only):

| key | what |
|---|---|
| `identity` | a Developer ID Application identity in the keychain (`codesign --sign`) |
| `p12_file`, `p12_password` | the certificate as a .p12, signed with `rcodesign` (any host, no keychain) |
| `notary_api_key` | rcodesign's App Store Connect API key JSON (`rcodesign encode-app-store-connect-api-key`: `issuer_id`, `key_id`, `private_key`). The .dmg is notarized (`rcodesign notary-submit --staple`, else `xcrun notarytool submit --wait` + `stapler`) and the .app is stapled. Needs an identity or a p12 |

```bash
cargo test -p eengine-bundle   # on a Mac this bundles /usr/bin/true end to end: layout, codesign --verify, entitlements, the .dmg
```

## `--target ios`

The one target compiled per app, on a Mac (Xcode 26+, `brew install xcodegen`). The kit (`scripts/runner-kits/ios.sh`) is the runner TEMPLATE at its workspace paths: `apps/runner/ios` (App/, RunnerKit, `make-project.sh`), `eengine-ffi`'s Swift package + `ExponentialEngineFFI.xcframework`, `exponential-ui-swift` + `ExponentialUIFFI.xcframework` (device + simulator slices, built by `build-ios.sh` when missing), `bundle.json` and `NOTICES.txt`. `targets/ios.rs` hands `make-project.sh` the modules (`manifest::ios_modules`) and purpose strings (`manifest::ios_purposes`, ITMS-90683): ONE mapping, bundle.json `permissions.ios`, which the script also reads for its own defaults and checks both ways (a linked privacy module has its purpose string, an unlinked one none). Plus the AppIcon (1024 px, opaque: the SVG over the runner background), `NOTICES.txt` (at `<name>.app/NOTICES.txt`) and `app/`.

Output `{name}-{version}.ipa` = `Payload/<name>.app` zipped, built for the device (`aarch64-apple-ios`):

| `[signing.ios]` | result |
|---|---|
| none (and no `DEVELOPMENT_TEAM` in the environment) | unsigned `.ipa`: re-sign it with your distribution identity |
| `team = "ABCDE12345"` | automatic signing: `xcodebuild archive -allowProvisioningUpdates` + `-exportArchive` (method `app-store-connect`, TestFlight-ready); needs the team's account in Xcode, or the API key below |
| `+ profile = "<name or UUID>"`, `identity = "Apple Distribution"` (default) | manual signing with an installed App Store provisioning profile and the distribution certificate in the keychain |
| `+ api_key_path`, `api_key_id`, `api_issuer_id` | automatic signing with an App Store Connect API key (`-authenticationKey*`), no Xcode account |

`EENGINE_BUNDLE_IOS_SIMULATOR=1` also writes `{name}-{version}-simulator.app` (a development check, never shipped). `apps/runner/ios/check-bundle.sh [workdir] [--launch shot.png]` runs it all end to end: the kit, the album sample to an ipa, the device + simulator products checked (`otool -L` links Photos/PhotosUI and nothing undeclared, Info.plist has exactly the photo-library purpose string, NOTICES.txt, AppIcon), and with `--launch` a simulator of its own (install, launch, screenshot, shut down, delete; under the `ios` lane).

- **Linux:** unsigned (distributions sign their repositories, not packages).
- **Windows:** osslsigncode signs the app's `.exe` before packing, then the setup.exe:

  ```toml
  [signing.windows]
  pfx = "certs/codesign.pfx"           # relative to the working directory
  password_env = "WINDOWS_PFX_PASSWORD" # or password = "…"
  timestamp_url = "http://timestamp.digicert.com"   # optional
  ```

  The password reaches osslsigncode through a file (`-readpass`), never argv. The uninstaller stays unsigned. The alternative is Azure Trusted Signing: bundle unsigned, then sign the setup.exe with the Trusted Signing GitHub action or `signtool` + its dlib (endpoint, account, certificate profile).

## Android (`--target android`)

Nothing is compiled per app: the kit is the template's release build taken apart, and the bundler relinks it with the Android SDK's build-tools.

**The kit** (`scripts/runner-kits/android.sh`, `apps/runner/android` `:app:assembleRelease`): `kit.json` (`templateId`, `compileSdk`), the merged `AndroidManifest.xml` as text (still on the template id, with the `E7:PERMISSIONS` markers), `resources.zip` (the merged aapt2 `.flat` files, app + libraries), `resource-ids.txt` (every entry of the release APK's resource table, `aapt2 dump resources`, in `--stable-ids` form: R8 inlined these ids into the dex; the bundler moves them onto the config id's package), `payload.zip` (the rest of the release APK: `classes*.dex`, `lib/<abi>/*.so`, the Java resources, `assets/dexopt`), `NOTICES.txt`.

**Per app:** the manifest gets the config id in place of the template id (applicationId, the deep-link scheme `<id>://`, the FileProvider authority, the receiver permission) and `manifest::android_permissions` between the markers; an overlay compiles `runner_label` (= `name`) and `mipmap{,-mdpi…-xxxhdpi}/ic_launcher.png` (`icons.android`); `aapt2 link --stable-ids` (`versionName` = `version`, `versionCode` = major × 1 000 000 + minor × 1 000 + patch). The .apk = the linked resources + the payload + `assets/app/` + `assets/NOTICES.txt`, `zipalign -P 16` (16 KiB pages for the .so; `-p` before build-tools 35), `apksigner sign` (v1–v3). The .aab = the same base module in proto format (`aapt2 link --proto-format`), `bundletool build-bundle`, `jarsigner`.

**The author's prerequisites:** `ANDROID_HOME` with `build-tools` (35+) and `platforms;android-<compileSdk>` (36), a JDK (`JAVA_HOME`: keytool, jarsigner, and java for apksigner/bundletool), `BUNDLETOOL` = `bundletool-all-<version>.jar` (or `bundletool` on PATH). Building the kit also needs the NDK (`cargo-ndk`) for the two native libraries.

**Signing** (`[signing.android]` of bundle.toml): `keystore` (.jks/.p12; relative = to the working directory), `alias`, `store-password`, `key-password` (default = the store password); `EXPONENTIAL_ANDROID_STORE_PASSWORD` / `EXPONENTIAL_ANDROID_KEY_PASSWORD` override the file, so passwords need not live in it. They reach apksigner and jarsigner through the environment, never argv. Without a keystore the run generates a throwaway debug key (PKCS12, `androiddebugkey`) and says the outputs are for testing only; a new key each run, so a rebundle installs over the previous one only after an uninstall.

`tests/android.rs` runs the whole target on a small synthetic kit (aapt2 dump badging, apksigner verify, jarsigner -verify, bundletool dump manifest); it skips, saying why, without ANDROID_HOME, JAVA_HOME and BUNDLETOOL.

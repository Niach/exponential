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

- **Linux:** unsigned (distributions sign their repositories, not packages).
- **Windows:** osslsigncode signs the app's `.exe` before packing, then the setup.exe:

  ```toml
  [signing.windows]
  pfx = "certs/codesign.pfx"           # relative to the working directory
  password_env = "WINDOWS_PFX_PASSWORD" # or password = "…"
  timestamp_url = "http://timestamp.digicert.com"   # optional
  ```

  The password reaches osslsigncode through a file (`-readpass`), never argv. The uninstaller stays unsigned. The alternative is Azure Trusted Signing: bundle unsigned, then sign the setup.exe with the Trusted Signing GitHub action or `signtool` + its dlib (endpoint, account, certificate profile).

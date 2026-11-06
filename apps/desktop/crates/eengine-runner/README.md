# eengine-runner

The desktop runner (#VAPP-266, E6): `exponential-runner`, the prebuilt shell an exported app runs in on macOS, Windows and Linux. E7's `exponential bundle` copies it next to an `app/` directory. The contract is `packages/eengine/fixtures/runner.json`; its `$comment` is the spec.

The runner has no chrome of its own. It paints four genUI surfaces with eui-gpui's `ExponentialHost`, bottom to top: `app`, `consent`, `permissions` and `toast`. Nothing else is painted. It ships with no updater, no telemetry and no account. Nothing installs a logger, and `log`'s `release_max_level_off` compiles every log macro out of release builds.

## How it runs

1. **Bundle** (`bundle.rs`). `app/` is looked up at `<name>.app/Contents/Resources/app` on macOS and at `<install dir>/app` on Windows. On Linux it is `<exe dir>/app`, else `<exe dir>/../share/<config id>/app`; that second location is used only when its `runner.json` names that id. Each failure paints in place of `app`:
   - no directory, or no `package.app.json`: `missing`
   - `runner.json` unreadable or with issues: `config`
   - a package file that is not JSON: `invalid`
2. **Instance** (`runner.rs`). There is one Engine `AppInstance`:
   - identity `local`: the device uuid is created once under the Vault key `eengine/runner/device`
   - host kind `exported`, and `network` from the config
   - storage under the platform's app-data directory, per config id (`storage.rs`): `records.sqlite` (`SqliteRecordStore`) and `files/` (`FsBlobStore`)
   - capabilities from `eengine-desktop`

   A load error paints `signature` or `invalid`.
3. **Painter** (`main.rs`). It installs `runner_package(pkg, formFactor)` and re-installs it, re-applying the open screen, when the window crosses `md` (768). `app` is laid out at the window's height (`SurfaceView::set_fill_height`), so the shell fills the window and pages scroll inside it.
4. **Lifecycle**. These events drive lifecycle.json:
   - launch, with the first deep link instead when there is one
   - window activation for `foreground` and `background`
   - quit for `terminate`
   - deep links `<config id>://<screen>?<params>`: argv on Linux and Windows, `on_open_urls` on macOS
   - notification taps

   A plain launch that opens nothing opens the derived package's first screen.
5. **Calls**. `records.*` and `app.navigate` run on the main thread. Every capability tool of the manifest goes through the broker on a worker thread, because the natives block on their dialogs. The answer is applied as follows:
   - `Ask` paints `consent`, and `consent.answer` answers it.
   - `Refused` paints `toast`.
   - A refused link shows lifecycle.json's `copy.refused`.

   The runner handles these events itself: `runner.navigate`, `runner.permissions`, and `consent.toggle` (granted → revoke, else granted, then `permissions` is repainted).

## The Vault

`vault.rs` keeps the Vault in the platform keystore under the config id:

- **macOS:** Keychain generic passwords (service = the id).
- **Linux:** the Secret Service over D-Bus (gnome-keyring, KWallet), `plain` session, default collection, attributes `application` + `key`.
- **Windows:** Credential Manager generic credentials (`<id>/<key>`).

If the keystore is unavailable (no Secret Service, or a locked collection that would prompt), the Vault falls back to `vault.json` (0600) in the app-data directory. A read tries the keystore first, then the file.

## Theme and mode

The theme is the package's `theme` (a built-in id or a theme JSON), else Exponential's. The mode follows the OS appearance, live.

## Tests

```bash
cargo test -p eengine-runner      # headless: bundle, vault, events, the instance, the dependency gate
```

`tests/dependency_gate.rs` walks Cargo.lock from the runner. It fails on:

- any app crate (`ui`, `app`, `sync`, `updater`…)
- any log, crash or analytics sink
- any self-update crate

CI coverage:

- **Gate** (`test.yml`): runs the tests on Linux.
- **`check-eengine-runner-windows.yml`** (informational): builds the runner on `windows-latest`, runs the tests, and smoke-starts the release binary.
- **`release-eengine.yml`**: builds the release assets: a macOS universal binary (lipo arm64 + x86_64), Linux x86_64 + arm64, and Windows x86_64.

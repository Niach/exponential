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

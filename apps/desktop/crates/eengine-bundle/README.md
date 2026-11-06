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

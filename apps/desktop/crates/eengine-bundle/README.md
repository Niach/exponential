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

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

## Signing

With the author's own keys from `bundle.toml` `[signing.<target>]` (we never hold them). Without them a target signs ad-hoc or not at all and says so (`signing` in the fixture lists each target's inputs).

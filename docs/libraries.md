# The library family: names (EXP-1421)

Decided 2026-10-29 for the epic #EXP-1420. The **product names** (eUI, eTalk, ePilot,
eNet, eApps) are what copy, docs and the front page say. The **package names**
stay in the `exponential-*` family on every registry: eUI is already published
under it (npm `@exponential_app/ui*`, crates.io `exponential-ui`,
Maven `at.exponential:ui-compose*`, SwiftPM `ExponentialUI`), and the bare short
names are taken (crates.io `enet`; npm `epilot`, `enet`, `eui`), so a mixed
scheme would be the only alternative. Nothing published gets renamed, so no
compatibility re-exports are needed.

The brand stays Exponential (no Softcoded/sUI names anywhere). Everything is
Apache-2.0 like eUI unless the lib's own issue says otherwise; spezl (MIT)
consumes them as dependencies.

## Table

| product | Rust crates (`apps/desktop/crates/<name>`) | npm (`@exponential_app/…`) | Maven (`at.exponential:…`) | SwiftPM product (repo) | release tag | docs |
|---|---|---|---|---|---|---|
| **eUI** | `exponential-ui`, `exponential-ui-ffi` (not published, ships inside Swift/Kotlin), `exponential-ui-gpui` (#EXP-1429) | `ui`, `ui-react`, `ui-solid`, `ui-element` | `ui-compose`, `ui-compose-primitives` | `ExponentialUI`, `ExponentialUIPrimitives` (`Niach/exponential-ui-swift`) | `ui-v*` | exponential.at/ui/ |
| **eTalk** | `etalk` (was `talk`), `etalk-engine` (`voice`), `etalk-crispasr` (`crispasr`), `etalk-codec` (`call-codec`), `etalk-wakeword` (#EXP-1424), `etalk-gpui` (tray, push to talk, overlay, call UI; #EXP-1422); `etalk-ffi` not yet | `talk` | `talk-compose` | `ExponentialTalk` (`Niach/exponential-talk-swift`) | `etalk-v*` | exponential.at/talk/ |
| **ePilot** | `epilot` (was `computer`), `epilot-screen` (`screen`) | none (desktop only) | none | none (desktop only) | `epilot-v*` | exponential.at/pilot/ |
| **eNet** | `exponential-net`, `exponential-net-ffi`, `exponential-net-wasm` | `net` (the wasm build, relay-only in the browser) | `net` | `ExponentialNet` (`Niach/exponential-net-swift`) | `net-v*` | exponential.at/net/ |
| **eApps** | `exponential-apps` | `apps` | `apps` | `ExponentialApps` (`Niach/exponential-apps-swift`) | `apps-v*` | exponential.at/apps/ |
| agent-runtime | `exponential-agent-runtime` (from `coding` + `engine`, #EXP-1426) | none | none | none | `agent-runtime-v*` | in the repo README only |

**Deviation (2026-10-30):** the eTalk and ePilot extractions (#1348, #1331)
renamed their crates `etalk*` / `epilot*` and tag `etalk-v*` / `epilot-v*`
instead of the `exponential-talk*` / `exponential-pilot*` / `talk-v*` /
`pilot-v*` planned here. They are git dependencies (`publish = false`: the
fixtures are compiled in with `include_str!`), so no registry name is spent;
the table records what is merged. Renaming them back to the `exponential-*`
family before a first release is open.

Every name above was checked free on 2026-10-29 (crates.io API, `npm view`,
Maven Central) except the eUI ones, which we already own.

## Rules for the extractions

- Each extraction renames its OWN crates per this table (directory = crate
  name, `lib` name = the snake_case of it) and keeps the Exponential app
  building in the same PR.
- A crate meant for consumers outside the monorepo has no path dependency on
  an app crate (`ui`, `domain`, `sync`, `api`, `app`); app coupling goes
  behind a trait the app implements.
- Bindings follow eUI: one `-ffi` crate (UniFFI) per lib that has mobile
  consumers; Swift ships an xcframework through a `Niach/exponential-<lib>-swift`
  distribution repo, Kotlin a Maven Central bundle under `at.exponential`.
- One tag prefix per lib (`<lib>-v<semver>`), never `v*` (the web image) or
  the native app prefixes.
- Docs: one page per lib in the `/ui/` site's style (`apps/marketing/ui`), and
  one entry per lib in the front page's library section.
- gpui consumers (Exponential desktop, spezl desktop) pin the same zed/gpui and
  gpui-component revisions: see `apps/desktop/crates/exponential-ui-gpui/README.md`.

## Published state (2026-10-30)

| product | released | how |
|---|---|---|
| eUI | 0.1.1 (master is 0.2.0, unreleased) | tag `ui-v<version>` → `release-ui.yml`: npm, crates.io (`exponential-ui`), Maven Central, SwiftPM; `exponential-ui-gpui` rides the tag as a git dependency. 0.2.0 waits for `exponential-ui.yml` to go green again (round-5 baselines) |
| eTalk | never | crates at the desktop workspace version; no `etalk-v*` tag yet |
| ePilot | never | crates at the desktop workspace version; no `epilot-v*` tag yet |
| eNet | never | `exponential-net{,-ffi,-wasm}` 0.1.0, MIT OR Apache-2.0; no `net-v*` tag, no release workflow |
| eApps | inside eUI | the package format v2 is in `@exponential_app/ui/host` and the `exponential-ui` crate on master (released with eUI 0.2.0); no `exponential-apps` crate yet |
| agent-runtime | never | `exponential-agent-runtime` 0.1.0, `publish = false` |

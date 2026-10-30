# The library family: names (EXP-1421)

Decided 2026-10-29 for the epic #EXP-1420. The **product names** (eUI, eTalk, ePilot,
eNet, eApps) are what copy, docs and the front page say. The **package names**
stay in the `exponential-*` family on every registry: eUI is already published
under it (npm `@exponential_app/ui*` 0.1.1, crates.io `exponential-ui` 0.1.1,
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
| **eTalk** | `exponential-talk` (today `talk`), `exponential-talk-voice` (`voice`), `exponential-talk-asr` (`crispasr`), `exponential-talk-codec` (`call-codec`), `exponential-talk-gpui` (tray, push to talk, overlay, call UI; #EXP-1422), `exponential-talk-ffi` | `talk` | `talk-compose` | `ExponentialTalk` (`Niach/exponential-talk-swift`) | `talk-v*` | exponential.at/talk/ |
| **ePilot** | `exponential-pilot` (today `computer`), `exponential-pilot-screen` (`screen`) | none (desktop only) | none | none (desktop only) | `pilot-v*` | exponential.at/pilot/ |
| **eNet** | `exponential-net`, `exponential-net-ffi`, `exponential-net-wasm` | `net` (the wasm build, relay-only in the browser) | `net` | `ExponentialNet` (`Niach/exponential-net-swift`) | `net-v*` | exponential.at/net/ |
| **eApps** | `exponential-apps` | `apps` | `apps` | `ExponentialApps` (`Niach/exponential-apps-swift`) | `apps-v*` | exponential.at/apps/ |
| agent-runtime | `exponential-agent-runtime` (from `coding` + `engine`, #EXP-1426) | none | none | none | `agent-runtime-v*` | in the repo README only |

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

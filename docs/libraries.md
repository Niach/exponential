# The library family: names (EXP-1421)

Decided 2026-10-29 for the epic #EXP-1420, renamed 2026-10-30 (#EXP-1438).
The **product names** (eUI, eTalk, ePilot, eNet, eApps) are what copy, docs and
the front page say. The **package names** follow the same short scheme
(`eui*`, `etalk*`, `epilot*`, `enet*`) on every registry, and so do the tags
(`eui-v*`, `etalk-v*`, `epilot-v*`, `enet-v*`). The bare crates.io names `eui`
and `enet` are taken, so the core crates are `eui-core` and `enet-core`.

eUI 0.1.x shipped under the old `exponential-*` names (npm `@exponential_app/ui*`,
crates.io `exponential-ui`, Maven `at.exponential:ui-compose*`, tag `ui-v*`).
0.2.0 is the first release under the new names: the release workflow deprecates
the old npm packages, and `exponential-ui` 0.1.1 stays on crates.io, superseded.

Kept on purpose: the UniFFI/wasm binding crates (`exponential-ui-ffi`,
`exponential-net-ffi`, `exponential-net-wasm`; never published on their own).
Their lib names are baked into the generated bindings and the native library
files. Also kept: the Swift modules `ExponentialUI` / `ExponentialUIPrimitives`
from the distribution repo `Niach/exponential-ui-swift`, the Kotlin packages
`at.exponential.ui.*` and the `packages/exponential-ui*` directories (none of
them is a registry name).

The brand stays Exponential (no Softcoded/sUI names anywhere). Everything is
Apache-2.0 like eUI unless the lib's own issue says otherwise; spezl (MIT)
consumes them as dependencies.

## Table

| product | Rust crates (`apps/desktop/crates/<name>`) | npm (`@exponential_app/…`) | Maven (`at.exponential:…`) | SwiftPM product (repo) | release tag | docs |
|---|---|---|---|---|---|---|
| **eUI** | `eui-core` (crates.io), `eui-gpui` (git dependency on the tag, #EXP-1429), `exponential-ui-ffi` (not published, ships inside Swift/Kotlin) | `eui`, `eui-react`, `eui-solid`, `eui-element` | `eui-compose`, `eui-compose-primitives` | `ExponentialUI`, `ExponentialUIPrimitives` (`Niach/exponential-ui-swift`) | `eui-v*` → `release-eui.yml` | exponential.at/ui/ |
| **eTalk** | `etalk`, `etalk-engine`, `etalk-crispasr`, `etalk-codec`, `etalk-wakeword`, `etalk-gpui` (git dependencies on the tag); `etalk-ffi` not yet | not yet | not yet | not yet | `etalk-v*` → `release-etalk.yml` | exponential.at/talk/ |
| **ePilot** | `epilot`, `epilot-screen` (git dependencies on the tag) | none (desktop only) | none | none (desktop only) | `epilot-v*` → `release-epilot.yml` | exponential.at/pilot/ |
| **eNet** | `enet-core` (crates.io), `exponential-net-ffi`, `exponential-net-wasm` (not published yet) | `enet` (the wasm build, planned) | `enet` (planned) | `ExponentialNet` (planned) | `enet-v*` → `release-enet.yml` | exponential.at/net/ |
| **eApps** | inside `eui-core` for now | inside `@exponential_app/eui/host` | | | rides `eui-v*` | exponential.at/apps/ |
| agent-runtime | `exponential-agent-runtime` (from `coding` + `engine`, #EXP-1426) | none | none | none | `agent-runtime-v*` | in the repo README only |

eTalk and ePilot are git dependencies (`publish = false`: the fixtures are
compiled in with `include_str!` from outside the crate), so their release is the
tag plus a GitHub release; no registry name is spent.

Every new name was checked free on 2026-10-30: crates.io `eui-core`, `eui-gpui`,
`enet-core`, `etalk`, `epilot`, and npm `@exponential_app/eui*`, `@exponential_app/enet`,
`@exponential_app/etalk`.

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
  gpui-component revisions: see `apps/desktop/crates/eui-gpui/README.md`.

## Published state (2026-10-30)

| product | version | registry | tag |
|---|---|---|---|
| eUI | 0.2.0 | npm `@exponential_app/eui{,-react,-solid,-element}`, crates.io `eui-core`, Maven Central `at.exponential:eui-compose{,-primitives}`, SwiftPM `Niach/exponential-ui-swift`; `eui-gpui` = git dependency on the tag | `eui-v0.2.0` |
| eTalk | 0.1.0 | GitHub release (git dependencies on the tag) | `etalk-v0.1.0` |
| ePilot | 0.1.0 | GitHub release (git dependencies on the tag) | `epilot-v0.1.0` |
| eNet | 0.1.0 | crates.io `enet-core` (the bindings are not published yet) | `enet-v0.1.0` |
| eUI (old name) | 0.1.1 | npm `@exponential_app/ui*` (deprecated), crates.io `exponential-ui`, Maven `at.exponential:ui-compose*` | `ui-v0.1.1` |
| agent-runtime | never | `exponential-agent-runtime` 0.1.0, `publish = false` | |

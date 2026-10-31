# The library family: names (EXP-1421)

Decided 2026-10-29 for the epic #EXP-1420, renamed 2026-10-30 (#EXP-1438).
The **product names** (eUI, eTalk, ePilot, eNet, eApps, eMCP, eAgent, eCore) are what
copy, docs and the front page say; eAgent's crate is `eagent` (renamed from
`exponential-agent-runtime` by #EXP-1505). The **package names** follow the same short scheme
(`eui*`, `etalk*`, `epilot*`, `enet*`) on every registry, and so do the tags
(`eui-v*`, `etalk-v*`, `epilot-v*`, `enet-v*`). The bare crates.io names `eui`
and `enet` are taken, so the core crates are `eui-core` and `enet-core`.

eUI 0.1.x shipped under the old `exponential-*` names (npm `@exponential_app/ui*`,
crates.io `exponential-ui`, Maven `at.exponential:ui-compose*`, tag `ui-v*`).
0.2.0 is the first release under the new names: the release workflow deprecates
the old npm packages, and `exponential-ui` 0.1.1 stays on crates.io, superseded.

Kept on purpose: the UniFFI/wasm binding crates (`exponential-ui-ffi`,
`exponential-net-ffi`, `exponential-net-wasm`; never published on their own;
`etalk-ffi` follows the new scheme and is not published either).
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
| **eTalk** | `etalk`, `etalk-engine`, `etalk-crispasr`, `etalk-codec`, `etalk-wakeword`, `etalk-gpui` (git dependencies on the tag), `etalk-ffi` (not published, ships inside Swift/Kotlin) | `etalk` (the contract + fixtures, `packages/etalk`) | `etalk` | `ExponentialTalk` (`Niach/exponential-talk-swift`) | `etalk-v*` → `release-etalk.yml` | exponential.at/talk/ |
| **ePilot** | `epilot`, `epilot-screen` (git dependencies on the tag) | none (desktop only) | none | none (desktop only) | `epilot-v*` → `release-epilot.yml` | exponential.at/pilot/ |
| **eNet** | `enet-core` (crates.io), `exponential-net-ffi`, `exponential-net-wasm` (not published yet) | `enet` (the wasm build, planned) | `enet` (planned) | `ExponentialNet` (planned) | `enet-v*` → `release-enet.yml` | exponential.at/net/ |
| **eApps** | inside `eui-core` for now | inside `@exponential_app/eui/host` | | | rides `eui-v*` | exponential.at/apps/ |
| **eAgent** | `eagent` (from `coding` + `engine`, #EXP-1426; renamed from `exponential-agent-runtime` in #EXP-1505, which stays one release as a re-export shim; git dependency on the tag) | `@exp/eagent` (the orchestration spec: fixtures + a pure TS reducer, `packages/eagent`, #EXP-1508; private for now) | none | none | `eagent-v*` → `release-eagent.yml` (still accepts `agent-runtime-v*` for one release) | exponential.at/agent/ |
| **eMCP** | `emcp` (on rmcp, for the devices and spezl, #EXP-1492; the local hub and code mode (feature `codemode`), #EXP-1507; git dependency on the tag) | `emcp` (the contract + fixtures, `packages/emcp`, #EXP-1490; private for now) | none | none | `emcp-v*` → `release-emcp.yml` | exponential.at/mcp/ |
| **eCore** | `ecore` (types only, serde; #EXP-1504; git dependency) | `@exp/ecore` (the contract + generated TS, `packages/ecore`; private for now) | generated into the Android app (`com.exponential.app.ecore`) | generated into ExpCore (`ECore`) | none yet | exponential.at/core/ |

**eDE** (#NEW-10, ExponentialOS's desktop) is not a library: its own Cargo
workspace in `apps/ede` (`ede-compositor`, `ede-session`), in progress, no
release. The vision is the blog post exponential.at/blog/exponentialos/.

eTalk, ePilot, eMCP and eAgent are git dependencies (`publish = false`: the
fixtures are compiled in with `include_str!` from outside the crate), so their
release is the tag plus a GitHub release; no crates.io name is spent.

## Consuming from another repo (the git-tag policy, #EXP-1420)

How spezl (or anyone) depends on the `publish = false` crates:

1. **Pin a release tag, nothing else.** `etalk = { git = "https://github.com/Niach/exponential", tag = "etalk-v0.2.3" }`.
   Never `branch`, never master; a `rev` only to try an unreleased fix, and
   only until the next tag. Every crate of one lib uses the SAME tag.
2. **One tag = one version of every crate of the lib.** The release workflow
   refuses a tag that disagrees with any of the crates' `version`; tags are
   cut from master after the bump PR merged, and a tag is never moved or
   deleted (consumers' `Cargo.lock` pins its commit).
3. **Semver, 0.x style.** A breaking API change bumps the minor (`0.2` →
   `0.3`), anything else the patch; the lib's CHANGELOG / GitHub release says
   what broke. A gpui revision change is breaking for the gpui crates
   (`etalk-gpui`, like `eui-gpui`).
4. **No path escapes the checkout.** A consumable crate's path dependencies
   are crates of its OWN lib only, never an app crate (`ui`, `theme`, `domain`,
   `sync`, `api`, `app`, …): gated per crate (`tests/dependency_gate.rs` in
   `eagent` and `etalk-gpui`). Fixtures come in with
   `include_str!` from `packages/`, which a git checkout carries.
   **Another lib comes in through ITS release tag** (#EXP-1460): `etalk-gpui`
   takes `eui-core`/`eui-gpui` with `git = "https://github.com/Niach/exponential",
   tag = "eui-v…"` and `epilot`/`epilot-screen` at `epilot-v…`, and
   `epilot`/`epilot-screen` take `emcp` at `emcp-v…` (#EXP-1492); a path
   dependency would resolve inside the etalk tag's checkout and hand a
   consumer a second eui-gpui whose types never unify with its own. The one
   exception is eCore (`ecore`, types only, no tag yet): `emcp` takes it by
   path (#EXP-1517), so it resolves inside the emcp tag's checkout. The
   desktop workspace's `[patch."https://github.com/Niach/exponential"]` builds
   them from the local crates. A consumer of `etalk-gpui` pins eUI and ePilot
   at the tags `etalk-gpui`'s Cargo.toml names (same URL, same tag): Cargo
   then sees one source and keeps one copy.
5. **gpui consumers copy two things** (a dependency's `[patch]` and lock never
   reach its consumers): the ztracing patch and the zed pin, exactly as
   `apps/desktop/crates/eui-gpui/README.md` "gpui revisions" says. The
   non-gpui crates (`etalk`, `etalk-codec`, `etalk-engine`, `epilot`,
   `epilot-screen`, `emcp`, `eagent`) need neither.
6. **Proven per release.** Each lib's release workflow runs
   `scripts/libs/git-consumer-smoke.sh <lib> <commit>`: a scratch crate outside
   this workspace depends on every crate of the lib through `git` + the commit,
   with the patch and the lock copied, and must build. eTalk's run also pins
   eUI and ePilot at the tags `etalk-gpui` names, like a consumer of all
   three, and fails on a second copy of any crate of this repo
   (`cargo tree -d`). Run it locally before tagging.

| lib | tag | crates under it | workflow |
|---|---|---|---|
| eTalk | `etalk-v*` | `etalk`, `etalk-codec`, `etalk-engine`, `etalk-crispasr`, `etalk-wakeword`, `etalk-gpui` (+ npm `@exponential_app/etalk`, SwiftPM `ExponentialTalk`, Maven `at.exponential:etalk`) | `release-etalk.yml` |
| ePilot | `epilot-v*` | `epilot`, `epilot-screen` | `release-epilot.yml` |
| eMCP | `emcp-v*` | `emcp` | `release-emcp.yml` |
| eAgent | `eagent-v*` (+ `agent-runtime-v*` for one release) | `eagent`, `exponential-agent-runtime` (the shim, one release) | `release-eagent.yml` |

Every new name was checked free on 2026-10-30: crates.io `eui-core`, `eui-gpui`,
`enet-core`, `etalk`, `epilot`, and npm `@exponential_app/eui*`, `@exponential_app/enet`,
`@exponential_app/etalk`.

## Migrating: agent-runtime → eAgent (#EXP-1505)

The agent runtime crate `exponential-agent-runtime` is now `eagent` (lib
`eagent`, tags `eagent-v*`, workflow `release-eagent.yml`). For ONE release:

- the old crate ships next to it as a shim, `pub use eagent::*;`, at the same
  version and under the same tag, so `exponential-agent-runtime = { git = …,
  tag = "eagent-v…" }` and every `exponential_agent_runtime::…` path still build;
- the release workflow still runs on an `agent-runtime-v*` tag.

To move: `eagent = { git = "https://github.com/Niach/exponential", tag = "eagent-v<version>" }`
in place of the old dependency, then `exponential_agent_runtime::` → `eagent::`. The
release after that removes the shim crate and the old tag pattern.

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

## Published state (2026-10-31)

| product | version | registry | tag |
|---|---|---|---|
| eUI | 0.3.0 | npm `@exponential_app/eui{,-react,-solid,-element}`, crates.io `eui-core`, Maven Central `at.exponential:eui-compose{,-primitives}`, SwiftPM `Niach/exponential-ui-swift`; `eui-gpui` = git dependency on the tag | `eui-v0.3.0` |
| eTalk | 0.2.3 | GitHub release (git dependencies on the tag), npm `@exponential_app/etalk`, SwiftPM `Niach/exponential-talk-swift` (product `ExponentialTalk`), Maven Central `at.exponential:etalk` (`etalk-ffi` inside both) | `etalk-v0.2.3` |
| ePilot | 0.1.0 | GitHub release (git dependencies on the tag) | `epilot-v0.1.0` |
| eNet | 0.1.0 | crates.io `enet-core` (the bindings are not published yet) | `enet-v0.1.0` |
| eUI (old name) | 0.1.1 | npm `@exponential_app/ui*` (deprecated), crates.io `exponential-ui`, Maven `at.exponential:ui-compose*` | `ui-v0.1.1` |
| agent-runtime (now eAgent) | 0.1.0 | GitHub release (git dependency on the tag) | `agent-runtime-v0.1.0` |
| eMCP | 0.2.0 | GitHub release (git dependency on the tag) | `emcp-v0.2.0` |
| eCore | unreleased | none (built: `ecore` + the generated TS/Swift/Kotlin) | none |

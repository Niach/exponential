# The Exponential SDK: names (EXP-1421)

Decided 2026-10-29 for the epic #EXP-1420, renamed 2026-10-30 (#EXP-1438) and
2026-11-05 (Danny). The product is one **Exponential SDK** with the modules
genUI, Motion, Voice, P2P, Computer Use, MCP Proxy, Agents, Core, Apps, Engine and Code: that is
what copy, docs and the front page say. The Agents crate is `eagent` (renamed from
`exponential-agent-runtime` by #EXP-1505). The module was called ACP until
2026-11-05; ACP now only ever means the Agent Client Protocol, written in full in copy. The **package names** keep their short
scheme (`eui*`, `etalk*`, `epilot*`, `enet*`, `emcp`, `eagent`, `ecore`, `eengine*`, `ecode*`) on every
registry, and so do the tags (`eui-v*`, `etalk-v*`, `epilot-v*`, `enet-v*`, `eengine-v*`, `ecode-v*`).
The Engine's crate is `eengine` because `engine` is taken by the ACP engine crate. The
bare crates.io names `eui` and `enet` are taken, so the core crates are `eui-core`
and `enet-core`. Motion ships inside the genUI packages (`eui-motion`,
`@exponential_app/eui/motion`).

genUI 0.1.x shipped under the old `exponential-*` names (npm `@exponential_app/ui*`,
crates.io `exponential-ui`, Maven `at.exponential:ui-compose*`, tag `ui-v*`).
0.2.0 is the first release under the new names: the release workflow deprecates
the old npm packages, and `exponential-ui` 0.1.1 stays on crates.io, superseded.

Kept on purpose: the UniFFI/wasm binding crates (`exponential-ui-ffi`,
`exponential-net-ffi`, `exponential-net-wasm`; never published on their own;
`etalk-ffi` and `eengine-ffi` follow the new scheme and are not published either).
Their lib names are baked into the generated bindings and the native library
files. Also kept: the Swift modules `ExponentialUI` / `ExponentialUIPrimitives`
from the distribution repo `Niach/exponential-ui-swift`, the Kotlin packages
`at.exponential.ui.*` and the `packages/exponential-ui*` directories (none of
them is a registry name).

The brand stays Exponential (no Softcoded/sUI names anywhere). Everything is
Apache-2.0 like genUI unless the lib's own issue says otherwise; spezl (MIT)
consumes them as dependencies.

## Table

| module | Rust crates (`apps/desktop/crates/<name>`) | npm (`@exponential_app/…`) | Maven (`at.exponential:…`) | SwiftPM product (repo) | release tag | docs |
|---|---|---|---|---|---|---|
| **genUI** | `eui-core` (crates.io), `eui-gpui` (git dependency on the tag, #EXP-1429), `eui-motion` (Motion's engine: Lottie, the dialect, the display list, the clock and state machines, #EXP-1584; git dependency on the tag), `eui-motion-wasm` (not published, its build ships inside `@exponential_app/eui`), `exponential-ui-ffi` (not published, ships inside Swift/Kotlin; carries `MotionPlayer`) | `eui` (incl. `eui/motion`, the Motion WASM), `eui-react`, `eui-solid`, `eui-element` | `eui-compose`, `eui-compose-primitives` | `ExponentialUI`, `ExponentialUIPrimitives` (`Niach/exponential-ui-swift`) | `eui-v*` → `release-eui.yml` | exponential.at/sdk/genui/ |
| **Voice** | `etalk`, `etalk-engine`, `etalk-crispasr`, `etalk-codec`, `etalk-wakeword`, `etalk-gpui` (git dependencies on the tag), `etalk-ffi` (not published, ships inside Swift/Kotlin) | `etalk` (the contract + fixtures, `packages/etalk`) | `etalk` | `ExponentialTalk` (`Niach/exponential-talk-swift`) | `etalk-v*` → `release-etalk.yml` | exponential.at/sdk/voice/ |
| **Computer Use** | `epilot`, `epilot-screen` (git dependencies on the tag) | none (desktop only) | none | none (desktop only) | `epilot-v*` → `release-epilot.yml` | exponential.at/sdk/computer-use/ |
| **P2P** | `enet-core` (crates.io), `exponential-net-ffi`, `exponential-net-wasm` (not published yet) | `enet` (the wasm build, planned) | `enet` (planned) | `ExponentialNet` (planned) | `enet-v*` → `release-enet.yml` | exponential.at/sdk/p2p/ |
| **Apps** | inside `eui-core` (the format and its pure logic: validation, query, CEL, policy, transitions, migrations, exposes); no `eapps` crate (decided 2026-11-05, #VAPP-259) | inside `@exponential_app/eui/host` | inside `eui-compose` | inside `ExponentialUI` | rides `eui-v*` | exponential.at/sdk/apps/ |
| **Engine** | `eengine` (#VAPP-261: AppInstance, lifecycle, sources, dispatch, outbox, stores, local identity; the capability broker on an in-process `emcp` hub, #VAPP-262; the modules behind cargo features `sqlite` and `hub`; git dependency on the tag), `eengine-ffi` (UniFFI, #VAPP-263: `Engine` + `AppInstance`, the providers as foreign traits; not published, ships inside Swift/Kotlin) | `engine` (the contract, fixtures and pure TS twin, `packages/eengine`, #VAPP-260; the workspace package `@exp/eengine` stays private) | `engine` | `ExponentialEngine` (`Niach/exponential-engine-swift`) | `eengine-v*` → `release-eengine.yml` | exponential.at/sdk/apps/ |
| **Agents** | `eagent` (from `coding` + `engine`, #EXP-1426; renamed from `exponential-agent-runtime` in #EXP-1505, which stays one release as a re-export shim; git dependency on the tag; `provider::AgentProvider` = one way to start a run: `AcpAgent`, `LocalModel` behind the `local-model` feature, `RemoteSession`, tools only as a hub grant, #VAPP-264) | `@exp/eagent` (the orchestration spec: fixtures + a pure TS reducer, `packages/eagent`, #EXP-1508; private for now; its Rust port `eagent-hub-core` + WASM `eagent-hub-core-wasm`, #EXP-1509, replay the same fixtures) | none | none | `eagent-v*` → `release-eagent.yml` (still accepts `agent-runtime-v*` for one release) | exponential.at/sdk/agents/ |
| **MCP Proxy** | `emcp` (on rmcp, for the devices and spezl, #EXP-1492; the local hub and code mode (feature `codemode`), #EXP-1507; git dependency on the tag) | `emcp` (the contract + fixtures, `packages/emcp`, #EXP-1490; private for now) | none | none | `emcp-v*` → `release-emcp.yml` | exponential.at/sdk/mcp-proxy/ |
| **Core** | `ecore` (types only, serde; #EXP-1504; git dependency) | `@exp/ecore` (the contract + generated TS, `packages/ecore`; private for now) | generated into the Android app (`com.exponential.app.ecore`) | generated into ExpCore (`ECore`) | none yet | exponential.at/sdk/core/ |
| **Code** | `ecode-core` (#EXP-1613: the pure core: the forge's PR-tree model, checks, the pipeline file, runner scheduling, remotes; serde only, no I/O, no dependency on Exponential) and `ecode` (#EXP-1615: the `Remote`/`Forge`/`Ci`/`Store`/`SecretSource` traits, `LocalRepo` on gix (push, squash merge, `rebase --onto` and bundles on git argv), the providers `local` (default: a file `Store` + `LocalRepo`, replays `forge.json` on a real repo), `github` (REST, a token or the device flow, polling) and `exponential` (`repositories.credentials`) behind features; no Exponential app crate; `coding` uses its credentials; the runner follows in #EXP-1617) (git dependencies on the tag) | `@exp/ecode` (the contract, fixtures and pure TS reference, `packages/ecode`; private for now) | generated into the Android app (`com.exponential.app.ecode`) | generated into ExpCore (`ECode`) | `ecode-v*` (none cut yet) | exponential.at/sdk/code/ |

**The Engine's module map** (#VAPP-259, `docs/spec/apps.md` §13.1). Modules never depend on the Engine; the Engine depends on them, each optional behind a cargo feature; hosts depend on the Engine (or on single modules, as today); painters stay per platform.

```
 hosts:  Exponential web · desktop · iOS · Android │ softcoded app │ eDE │ Spezl │ exported apps
            │ (web: TS twin)        └──────── eengine-ffi (UniFFI) / Rust ────────┘
 ┌──────────────────────────── Engine (eengine) ────────────────────────────┐
 │ AppInstance · lifecycle · screens/router · sources · action dispatch ·  │
 │ outbox · consent · capability broker · HostConfig                       │
 └──┬───────┬─────────┬────────────┬─────────────┬───────┬────────┬────────┘
  Apps    genUI    MCP Proxy     Agents         Voice    P2P   Computer Use
    └───────┴─────────┴──── Core (ecore) ───────┴───────────────┘
```

**Exponential Desktop** (#NEW-10, `apps/ede`, internally eDE) is not an SDK module: its own Cargo
workspace in `apps/ede` (`exponential-compositor`, `exponential-session`, `exponential-ui` = the
genUI-on-disk runtime and its supervisor, `exponential-ctl`), crates of the desktop
workspace (`apps/desktop/crates/ede-protocol` = the control plane every client links,
`apps/desktop/crates/ede-shell` = `exponential-shell`, the gpui shell, #EDE-51; the
`uicore-*` crates it is built on, #EDE-50: `uicore-model` = the pure shell reducer
replaying `packages/uicore`'s fixtures, `uicore-data` = what the documents bind,
`uicore-gpui` = the ShellHost (layer-shell surfaces and popups, or windows),
`uicore-docs` = the factory documents compiled in; `ide = shell + uicore`,
`ede = compositor + uicore`) and the
contract `@exp/ede` (`packages/ede`: the surface header, the Tier C rules and
the apply machine as fixtures + a TS reference, replayed by `exponential-ui`) and
`@exp/uicore` (`packages/uicore`, #EDE-46: the shell reducer the IDE and eDE both
host, its selectors and the shared geometry as fixtures + a TS reference, replayed
by the `uicore-model` crate; `parity.md` = the IDE-to-eDE region inventory), in
progress, no release.

Voice, Computer Use, MCP Proxy, Agents and the Engine are git dependencies (`publish = false`: the
fixtures are compiled in with `include_str!` from outside the crate), so their
release is the tag plus a GitHub release; no crates.io name is spent.

## Consuming from another repo (the git-tag policy, #EXP-1420)

How spezl (or anyone) depends on the `publish = false` crates:

1. **Pin a release tag, nothing else.** `etalk = { git = "https://github.com/Niach/exponential", tag = "etalk-v0.2.4" }`.
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
   `epilot`/`epilot-screen` take `emcp` at `emcp-v…` (#EXP-1492), and
   `eengine` takes `eui-core` and `emcp` at their tags (#VAPP-261); a path
   dependency would resolve inside the etalk tag's checkout and hand a
   consumer a second eui-gpui whose types never unify with its own. The one
   exception is Core (`ecore`, types only, no tag yet): `emcp` (#EXP-1517)
   and `eagent` (#VAPP-264) take it by path, so it resolves inside the
   tag's own checkout. The
   desktop workspace's `[patch."https://github.com/Niach/exponential"]` builds
   them from the local crates. A consumer of `etalk-gpui` pins genUI and Computer Use
   at the tags `etalk-gpui`'s Cargo.toml names (same URL, same tag): Cargo
   then sees one source and keeps one copy.
5. **gpui consumers copy two things** (a dependency's `[patch]` and lock never
   reach its consumers): the ztracing patch and the zed pin, exactly as
   `apps/desktop/crates/eui-gpui/README.md` "gpui revisions" says. The
   non-gpui crates (`etalk`, `etalk-codec`, `etalk-engine`, `epilot`,
   `epilot-screen`, `emcp`, `eagent`, `eengine`) need neither.
6. **Proven per release.** Each lib's release workflow runs
   `scripts/libs/git-consumer-smoke.sh <lib> <commit>`: a scratch crate outside
   this workspace depends on every crate of the lib through `git` + the commit,
   with the patch and the lock copied, and must build. Voice's run also pins
   genUI and Computer Use at the tags `etalk-gpui` names, like a consumer of all
   three, and fails on a second copy of any crate of this repo
   (`cargo tree -d`). Run it locally before tagging.

| lib | tag | crates under it | workflow |
|---|---|---|---|
| Voice | `etalk-v*` | `etalk`, `etalk-codec`, `etalk-engine`, `etalk-crispasr`, `etalk-wakeword`, `etalk-gpui` (+ npm `@exponential_app/etalk`, SwiftPM `ExponentialTalk`, Maven `at.exponential:etalk`) | `release-etalk.yml` |
| Computer Use | `epilot-v*` | `epilot`, `epilot-screen` | `release-epilot.yml` |
| MCP Proxy | `emcp-v*` | `emcp` | `release-emcp.yml` |
| Agents | `eagent-v*` (+ `agent-runtime-v*` for one release) | `eagent`, `exponential-agent-runtime` (the shim, one release) | `release-eagent.yml` |
| Engine | `eengine-v*` | `eengine` (features `sqlite`, `hub`; + npm `@exponential_app/engine`, SwiftPM `ExponentialEngine`, Maven `at.exponential:engine`, all three from `eengine-ffi` and `packages/eengine`) | `release-eengine.yml` |
| Code | `ecode-v*` | `ecode-core`, `ecode` (#EXP-1612; Rerusted #RUST-9 consumes them) | `release-ecode.yml` (with the first tag) |

Every new name was checked free on 2026-10-30: crates.io `eui-core`, `eui-gpui`,
`enet-core`, `etalk`, `epilot`, and npm `@exponential_app/eui*`, `@exponential_app/enet`,
`@exponential_app/etalk`; and on 2026-11-06 npm `@exponential_app/engine`, Maven
`at.exponential:engine` and the repo name `Niach/exponential-engine-swift`.

## Migrating: agent-runtime → Agents (#EXP-1505)

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
- Bindings follow genUI: one `-ffi` crate (UniFFI) per lib that has mobile
  consumers; Swift ships an xcframework through a `Niach/exponential-<lib>-swift`
  distribution repo, Kotlin a Maven Central bundle under `at.exponential`.
- One tag prefix per lib (`<lib>-v<semver>`), never `v*` (the web image) or
  the native app prefixes.
- Docs: one docs section per module on exponential.at (`apps/site/src/content/docs/docs/sdk/<module>/`), and
  one entry per module in the front page's SDK section.
- gpui consumers (Exponential desktop, spezl desktop) pin the same zed/gpui and
  gpui-component revisions: see `apps/desktop/crates/eui-gpui/README.md`.

## Published state (2026-11-02)

| module | version | registry | tag |
|---|---|---|---|
| genUI | 0.21.0 (narrow-phone Timeline/Table, overlay layers, #IDENT links in Markdown, $string checks; VAPP-250/252, EXP-1691) | npm `@exponential_app/eui{,-react,-solid,-element}`, crates.io `eui-core`, Maven Central `at.exponential:eui-compose{,-primitives}`, SwiftPM `Niach/exponential-ui-swift`; `eui-gpui`, `eui-motion` = git dependencies on the tag | `eui-v0.15.2` |
| Voice | 0.2.3 | GitHub release (git dependencies on the tag), npm `@exponential_app/etalk`, SwiftPM `Niach/exponential-talk-swift` (product `ExponentialTalk`), Maven Central `at.exponential:etalk` (`etalk-ffi` inside both) | `etalk-v0.2.4` |
| Computer Use | 0.1.0 | GitHub release (git dependencies on the tag) | `epilot-v0.1.0` |
| P2P | 0.1.0 | crates.io `enet-core` (the bindings are not published yet) | `enet-v0.1.0` |
| genUI (old `ui` packages) | 0.1.1 | npm `@exponential_app/ui*` (deprecated), crates.io `exponential-ui`, Maven `at.exponential:ui-compose*` | `ui-v0.1.1` |
| Agents | 0.2.0 (`exponential-agent-runtime` = its one-release re-export shim) | GitHub release (git dependency on the tag) | `eagent-v0.2.0` |
| agent-runtime (old name of Agents) | 0.1.0 | GitHub release (git dependency on the tag) | `agent-runtime-v0.1.0` |
| MCP Proxy | 0.2.0 | GitHub release (git dependency on the tag) | `emcp-v0.2.0` |
| Core | unreleased | none (built: `ecore` + the generated TS/Swift/Kotlin) | none |
| Code | unreleased | none (built: `ecode-core` + `@exp/ecode` + the generated Swift/Kotlin, #EXP-1613) | none |

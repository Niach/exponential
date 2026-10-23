# Releasing Exponential UI

One tag, every registry: push `ui-v<semver>` and
`.github/workflows/release-ui.yml` builds, verifies and publishes. Each
registry step is gated on its secret; without it the job still runs and
uploads what it would have published as a workflow artifact.

| registry | what | secret(s) | first-time setup |
|---|---|---|---|
| npm | `@exponential-at/ui`, `@exponential-at/ui-react`, `@exponential-at/ui-solid`, `@exponential-at/ui-element` | `NPM_TOKEN` (automation token) | create the `@exponential-at` org on npmjs.com |
| crates.io | `exponential-ui` | `CARGO_REGISTRY_TOKEN` | claim the crate name with the first publish |
| SwiftPM | `ExponentialUI` (the xcframework zip on the GitHub release + the tagged repo `Niach/exponential-ui-swift`) | `SWIFT_DIST_TOKEN` (contents:write on that repo) | create the empty public repo |
| Maven Central | `at.exponential:ui-compose`, `:ui-compose-primitives` | `MAVEN_CENTRAL_USERNAME` + `MAVEN_CENTRAL_PASSWORD` (a Central Portal user token), `SIGNING_KEY` (ASCII-armoured GPG private key) + `SIGNING_PASSWORD` | verify the `at.exponential` namespace (DNS TXT on exponential.at) |

Not published: `exponential-ui-ffi` (it ships INSIDE the Swift and Kotlin
artifacts) and `exponential-ui-gpui` (`publish = false`: gpui and
gpui-component are git dependencies, which crates.io refuses; it publishes
once both have matching crates.io releases).

## Cutting a release

1. Bump the version everywhere it is written: `packages/exponential-ui*/package.json`,
   `apps/desktop/crates/exponential-ui{,-ffi,-gpui}/Cargo.toml` (the workflow
   refuses a tag that does not match the core crate), and move each package's
   `CHANGELOG.md` `Unreleased` section under the version. The Compose
   artifacts need no bump: the workflow passes the tag's version as
   `-PuiVersion` (`ORG_GRADLE_PROJECT_uiVersion`); the `0.1.0` in
   `ui-compose/build.gradle.kts` and `primitives/build.gradle.kts` is only the
   local default. The Swift distribution package is generated per release.
2. Green: `.github/workflows/exponential-ui.yml` (all six renderers
   conformant, Solid and the web component at parity with React) on the release commit.
3. `git tag ui-v0.2.0 && git push origin ui-v0.2.0` (one tag per push, see the
   repo's release notes on tag pushes).

## What the jobs do

- **npm**: `bun packages/exponential-ui/release/prepare-npm.ts <version> dist-npm`
  builds each package (`build-npm.ts`: one ESM bundle per entry, dependencies
  external, JSON inlined, `tsc` declarations; `build:css` for the web
  renderers' stylesheets; Solid's JSX through Vite + `vite-plugin-solid`; the
  element's `dist/standalone.js` through its `scripts/build-standalone.ts`) and stages a
  PUBLISHABLE package.json (no `private`, entry points on `dist/`, `workspace:*` pinned to the version, LICENSE + NOTICE).
  The workspace package.json files stay private with `main` on the sources.
  Entries: `@exponential-at/ui` `.`, `./web`, `./web/primitives` (the
  primitives' class recipes, FEED-131); `@exponential-at/ui-react` and
  `@exponential-at/ui-solid` each `.`, `./primitives` and `./styles.css` (the
  compiled primitives sheet, byte-identical in both).
  Locally: `bun samples/exponential-ui/pack-local.ts` packs the same tarballs.
- **crates**: `cargo publish -p exponential-ui` (`--dry-run` without the token).
- **swift**: `build-ios.sh` (device arm64, simulator arm64 + x86_64, macOS
  arm64 + x86_64, all profile `mobile`) → `packages/exponential-ui-swift/release/zip-xcframework.sh`
  (zip + `swift package compute-checksum`) → `make-release-package.sh`
  writes the distribution repo (`Package.swift` with
  `.binaryTarget(url:checksum:)` on this release's zip); `swift-dist` pushes
  it after the GitHub release exists and tags it `<version>`.
- **maven**: `build-android.sh` (three ABIs) → `packages/exponential-ui-compose/release/central-bundle.sh`
  (signed when the key is set, `.md5`/`.sha1` per file, zipped Maven layout)
  → `POST central.sonatype.com/api/v1/publisher/upload`.
- **release**: the GitHub release `ui-v<version>` (never `latest`: the desktop
  owns that) with the xcframework zip, the Maven bundle, the npm tarballs,
  LICENSE, NOTICE and `SHA256SUMS`.

Licences: everything is Apache-2.0; the vendored A2UI schemas and the
json-render wording carry their `VENDORED` rows in
`packages/licenses/curated/supplement.ts` (`bun run notices` regenerates the
NOTICES; `docs/third-party-licences.md` is the policy).

## Sizes (VAPP-106)

`bun run --filter @exponential-at/ui size` (= `bun packages/exponential-ui/release/size.ts`)
measures what the SDK ships and `release/sizes.json` is the number on
record; `release/size-budget.json` is the gate: `size.ts <section> --check`
fails anything over its budget, and `exponential-ui.yml` runs it per job on
what that job built (`npm` in the React job, `xcframework` in the Swift
job, the x86_64 `.so` in the Compose emulator job, `cargo bloat` in the
Rust job; `release-ui.yml` gates every ABI and every slice before
publishing).

| section | what | how |
|---|---|---|
| `npm` | every entry of `build-npm.ts`, bundled MINIFIED (the release ships them readable + source-mapped): the entry plus the chunks it imports, bytes and gzip; each web renderer's `styles.css` as built; the element's `standalone.js` (everything inlined). Also the root-entry split: a renderer entry that bundles the described catalog, the prompt, the schema emitters or a fixture fails | `bundle()` from `build-npm.ts` into a temp dir (Bun; Vite for the Solid package and the standalone file) |
| `xcframework` | each slice's static library and its thin archs (`lipo`) | after `build-ios.sh` |
| `android` | each ABI's `libexponential_ui_ffi.so`, the AAR when Gradle built one | after `build-android.sh` |
| `crate` | `cargo bloat --crates` on the core's `bench` example (the top crates by `.text`), the host build of the facade | `cargo install cargo-bloat` |

What the native numbers rest on (VAPP-106): the device builds compile the
facade as ONE crate type (`cargo rustc --crate-type staticlib|cdylib`),
because cargo skips the profile's LTO when a target also builds an rlib, and
`build-ios.sh` strips a copy of each archive (`strip -S -x`: the debug info
and local symbols of the prebuilt std objects) before `lipo`; the `.so` is
stripped by the profile. The crate's `builtin-themes` (default on) and
`descriptions` (off) features gate the embedded data; the device builds keep
the defaults (the Swift and Compose conformance suites paint `playful`).

The root-entry split (VAPP-106): `@exponential-at/ui` ships the runtime
catalog (`generated/core.runtime.json`, no descriptions or examples); the
prompt, the schema emitters (+ the vendored A2UI basic catalog), the
conformance suite (+ every fixture) and the host API are `./prompt`,
`./schema`, `./conformance` and `./host`. The web layer the React and Solid
renderers import (`./web`, `./web/primitives`), both renderers' entries and
the element's (`.`, `./define`, `standalone.js`) are renderer entries too: `size.ts npm` fails (`--check` or not) when one of
them bundles the described catalog, the prompt, the schema emitters or a
fixture (`SPLIT_RULES`, `release/size.test.ts`); the primitives' constants
that a fixture locks are written in code and held equal by a test.

Updating the record: `size.ts <section> --write` rewrites that section of
`sizes.json`; a change that shrinks something lowers its budget in the same
commit (the ratchet never loosens by accident), a change that must grow
something raises the budget and says why in the PR. Native budgets come
from a macOS build; Linux hosts (CI's Rust job) report without a budget.


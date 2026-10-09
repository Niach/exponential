# Releasing Exponential UI

One tag, every registry: push `ui-v<semver>` and
`.github/workflows/release-ui.yml` builds, verifies and publishes. Each
registry step is gated on its secret; without it the job still runs and
uploads what it would have published as a workflow artifact.

| registry | what | secret(s) | first-time setup |
|---|---|---|---|
| npm | `@exponential-at/ui`, `@exponential-at/ui-react` | `NPM_TOKEN` (automation token) | create the `@exponential-at` org on npmjs.com |
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
   refuses a tag that does not match the core crate), `uiVersion` in
   `packages/exponential-ui-compose/gradle.properties`.
2. Green: `.github/workflows/exponential-ui.yml` (all four renderers
   conformant) on the release commit.
3. `git tag ui-v0.2.0 && git push origin ui-v0.2.0` (one tag per push, see the
   repo's release notes on tag pushes).

## What the jobs do

- **npm**: `bun packages/exponential-ui/release/prepare-npm.ts <version> dist-npm`
  builds each package (`build-npm.ts`: one ESM bundle per entry, dependencies
  external, JSON inlined, `tsc` declarations; `build:css` for the React
  stylesheet) and stages a PUBLISHABLE package.json (no `private`, entry
  points on `dist/`, `workspace:*` pinned to the version, LICENSE + NOTICE).
  The workspace package.json files stay private with `main` on the sources.
  Locally: `bun samples/exponential-ui/pack-local.ts` packs the same tarballs.
- **crates**: `cargo publish -p exponential-ui` (`--dry-run` without the token).
- **swift**: `build-ios.sh` → `packages/exponential-ui-swift/release/zip-xcframework.sh`
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

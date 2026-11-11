# Release actions (shared with spezl)

EXP-1430: the release pipeline's reusable parts, as composite actions. Other
repos (Niach/spezl) call them as
`Niach/exponential/.github/actions/<name>@<sha>`; Exponential's own
`build-*.yml` call `./.github/actions/<name>` after checkout. Composite actions,
not reusable workflows: a reusable workflow called from another repo checks out
the CALLER, so it cannot reach the scripts it needs; an action carries them
(`github.action_path`). Pin a sha, never `@master`.

| Action | Does | Used by |
|---|---|---|
| `macos-app` | binary(ies) → `.app` (universal via `lipo`, `LSUIElement` for tray apps, extra Frameworks) → Developer ID sign → notarized + stapled `.dmg`; ad-hoc `.zip` without a cert | `desktop-build.yml` |
| `windows-sign` | Azure Artifact Signing of `.exe`s + `signtool verify /pa`; unsigned + warning without credentials | `desktop-build.yml` |
| `updater-feed` | `SHA256SUMS.txt` + `SHA256SUMS.txt.minisig` (ed25519) for the self-updaters; `append` for a late asset | `build-desktop.yml`, `build-cli.yml` |
| `apple-signing` | p12 → temp keychain, outputs the identity (iOS) | `build-ios.yml` |
| `testflight-upload` | `.ipa` → TestFlight (internal by default) | `build-ios.yml` |
| `play-upload` | signed `.aab` → a Play track (never production), changelogs from fastlane metadata | `build-android.yml` |
| `cargo-fetch` | restores the crates `seed-cargo-registry.yml` saves on master (hosted Linux, #EXP-1692), sets cargo's network settings, then `cargo fetch --locked` with retries (EXP-1689) | every job that runs cargo |
| `setup-bun` | `oven-sh/setup-bun`, falling back to the same Bun from npm's registry when github.com's release download fails | every job that runs bun |
| `gradle-dist` | downloads every wrapper's Gradle zip itself (retrying every error, its distributionUrl then a mirror) and points the wrapper at the local file, so a services.gradle.org / github.com outage never reaches the wrapper's own unretried download; the committed `distributionSha256Sum` verifies it | every job that sets up JDK 21 |
| `apt-mirror` | `apt-get update` off the Azure mirror: `archive.ubuntu.com` when Canonical resolves, else every source (security too) on `mirrors.edge.kernel.org`, then `ftp.halifax.rwth-aachen.de`; every later apt download of the job retries | every apt step on a GitHub-hosted runner; `updater-feed`'s minisign fallback |
| `bump-versions` | config-driven version + build-number bump, prints the release tags | `bump-versions.yml` |

Each action's README lists its inputs. The secrets and variables they read
are in the table in `../workflows/README.md`.

## Example: a tray-first desktop app in another repo

```yaml
- uses: Niach/exponential/.github/actions/macos-app@<sha>
  with:
    binaries: |
      target/aarch64-apple-darwin/release/spezl
    app-name: spezl
    bundle-id: app.spezl.desktop
    executable: spezl
    info-plist: packaging/Info.plist
    entitlements: packaging/spezl.entitlements
    icon: assets/icon-macos.svg
    lsuielement: true
    version: ${{ steps.ver.outputs.version }}
    build: ${{ github.run_number }}
    dmg-name: spezl.dmg
    cert-p12: ${{ secrets.MACOS_CERT_P12 }}
    cert-password: ${{ secrets.MACOS_CERT_PASSWORD }}
    notary-key-id: ${{ secrets.NOTARY_KEY_ID }}
    notary-issuer-id: ${{ secrets.NOTARY_ISSUER_ID }}
    notary-key: ${{ secrets.NOTARY_KEY }}
# … in the release job, after downloading the artifacts into dist/:
- uses: Niach/exponential/.github/actions/updater-feed@<sha>
  with:
    dir: dist
    signing-key: ${{ secrets.UPDATE_SIGNING_KEY }}
    public-key: ${{ vars.UPDATE_SIGNING_PUBKEY }}
```

The app's updater uses the `updater` crate (`apps/desktop/crates/updater`):
build with `EXP_UPDATE_PUBKEY=${{ vars.UPDATE_SIGNING_PUBKEY }}` and every
update must carry a valid `SHA256SUMS.txt.minisig`.

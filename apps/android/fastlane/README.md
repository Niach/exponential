fastlane documentation
----

# Installation

Make sure you have the latest version of the Xcode command line tools installed:

```sh
xcode-select --install
```

For _fastlane_ installation instructions, see [Installing _fastlane_](https://docs.fastlane.tools/#installing-fastlane)

# Available Actions

## Android

### android build

```sh
[bundle exec] fastlane android build
```

Build a release App Bundle (.aab) + APK for the production flavor. Signed when RELEASE_STORE_FILE is set; UNSIGNED fallback otherwise.

### android closed

```sh
[bundle exec] fastlane android closed
```

Build + upload the production .aab to the Play closed testing track.

### android internal

```sh
[bundle exec] fastlane android internal
```

Build + upload the STAGING flavor (.aab) to the at.exponential.staging Play INTERNAL testing track. The app record must already exist in the Play Console (the Play API cannot create apps) and the service account must have access. Signs from the RELEASE_* env like `build`; needs SUPPLY_JSON_KEY. Override the rollout state with PLAY_RELEASE_STATUS (default 'completed' = immediately live to testers).

### android screenshots

```sh
[bundle exec] fastlane android screenshots
```

Capture Play Store screenshots on a booted emulator via screengrab. Needs the seeded local backend running (see fastlane/Screengrabfile); override the instance URL with SCREENGRAB_INSTANCE_URL. Raw captures land in fastlane/screenshots-raw/ — follow up with `bun run screenshots:store` (repo root) before sync_store, which composites the Play set (EXP-580).

### android styleguide_screenshots

```sh
[bundle exec] fastlane android styleguide_screenshots
```

Capture the STYLEGUIDE reference screenshots (EXP-566) on a booted emulator: 11 sg_* shots of the plain app surfaces, into fastlane/styleguide-screenshots/. Same seeded local backend as `screenshots` (no steer relay needed); override the instance URL with SCREENGRAB_INSTANCE_URL. Config lives in fastlane/Screengrabfile-styleguide.

### android production

```sh
[bundle exec] fastlane android production
```

Promote the current closed-testing build to the production track (no new binary). Defaults version_code to the newest changelogs/<vc>.txt; override with version_code:NN.

### android sync_store

```sh
[bundle exec] fastlane android sync_store
```

Upload listing metadata + freshly generated screenshots to Play WITHOUT touching binaries or track states. Run `fastlane screenshots` first — the phoneScreenshots dir is gitignored (EXP-348), so this lane pushes whatever the last screengrab run produced. supply's metadata path must anchor to an EXISTING release, so the lane targets the closed track (the only track with releases so far) and defaults version_code to the newest changelogs/<vc>.txt; override with `fastlane sync_store version_code:NN`.

----

This README.md is auto-generated and will be re-generated every time [_fastlane_](https://fastlane.tools) is run.

More information about _fastlane_ can be found on [fastlane.tools](https://fastlane.tools).

The documentation of _fastlane_ can be found on [docs.fastlane.tools](https://docs.fastlane.tools).

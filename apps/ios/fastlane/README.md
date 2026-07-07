fastlane documentation
----

# Installation

Make sure you have the latest version of the Xcode command line tools installed:

```sh
xcode-select --install
```

For _fastlane_ installation instructions, see [Installing _fastlane_](https://docs.fastlane.tools/#installing-fastlane)

# Available Actions

## iOS

### ios build

```sh
[bundle exec] fastlane ios build
```

Regenerate the Tuist project and build a signed App Store archive (.ipa).

### ios screenshots

```sh
[bundle exec] fastlane ios screenshots
```

Regenerate the Tuist project and capture App Store screenshots (snapshot).

Needs the seeded local backend (apps/web/scripts/seed-screenshots.ts) at

http://localhost:5173 — override with SNAPSHOT_INSTANCE_URL.

### ios beta

```sh
[bundle exec] fastlane ios beta
```

Build + upload to TestFlight (pilot). Internal-only by default (fast: skips the

processing wait). `fastlane beta external:true` also distributes to the external

group (TESTFLIGHT_EXTERNAL_GROUP, default 'External Testers') — this waits for

build processing, attaches the What-to-Test changelog (TESTFLIGHT_CHANGELOG env,

falling back to metadata/en-US/release_notes.txt), and auto-submits the build for

Beta App Review. The external group + TestFlight Test Information must already

exist in App Store Connect (see docs/release-ios.md → External TestFlight).

### ios sync_store

```sh
[bundle exec] fastlane ios sync_store
```

Upload listing metadata + screenshots to App Store Connect WITHOUT building.

Metadata lives in fastlane/metadata/, screenshots in fastlane/screenshots/.

### ios release

```sh
[bundle exec] fastlane ios release
```

Build + upload to App Store Connect (deliver). Does NOT auto-submit for review.

----

This README.md is auto-generated and will be re-generated every time [_fastlane_](https://fastlane.tools) is run.

More information about _fastlane_ can be found on [fastlane.tools](https://fastlane.tools).

The documentation of _fastlane_ can be found on [docs.fastlane.tools](https://docs.fastlane.tools).

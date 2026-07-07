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

### android screenshots

```sh
[bundle exec] fastlane android screenshots
```

Capture Play Store screenshots on a booted emulator via screengrab. Needs the seeded local backend running (see fastlane/Screengrabfile); override the instance URL with SCREENGRAB_INSTANCE_URL.

### android production

```sh
[bundle exec] fastlane android production
```

Promote the current closed-testing build to the production track (no new binary).

----

This README.md is auto-generated and will be re-generated every time [_fastlane_](https://fastlane.tools) is run.

More information about _fastlane_ can be found on [fastlane.tools](https://fastlane.tools).

The documentation of _fastlane_ can be found on [docs.fastlane.tools](https://docs.fastlane.tools).

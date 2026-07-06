# Releasing the Android app

The Android app (`apps/android`) ships to Google Play as a signed **App Bundle** (`.aab`).
Releases are driven by **fastlane** lanes (`apps/android/fastlane/`) that build the signed
bundle and push it to Play via `supply`. This doc covers the one-time keystore + service-
account setup and the per-release two-liner.

> The lanes run **locally on a Mac** (or any machine with the Android SDK + JDK 17). CI
> wiring is optional sugar on top, not a prerequisite. `signingConfigs` are wired in
> `app/build.gradle.kts` and the `build` lane falls back to an **unsigned** artifact when
> no keystore env is present, so it works before the keystore exists.

## Build flavors

Two product flavors share one codebase (mirrors the iOS Tuist targets):

| Flavor       | applicationId            | Default cloud            | Play listing                     |
|--------------|--------------------------|--------------------------|----------------------------------|
| `production` | `at.exponential`         | `app.exponential.at`     | the public store listing         |
| `staging`    | `at.exponential.staging` | `next.exponential.at`    | internal-testing only (co-installs) |

Only `production` goes to the public track. `staging` is for internal testing and can
co-install next to production because of the `.staging` applicationId suffix.

## 1. One-time: generate the upload keystore

Google Play App Signing manages the *app signing key* for you; you only hold an **upload
key**. Generate it once and keep it secret + backed up (losing it means re-registering an
upload key with Google support):

```bash
keytool -genkeypair -v \
  -keystore exponential-upload.jks \
  -alias exponential-upload \
  -keyalg RSA -keysize 2048 -validity 10000 \
  -storetype JKS
```

`keytool` prompts for a store password, a key password, and a distinguished name (CN, O,
etc.). Record all of these in your password manager. Store `exponential-upload.jks`
**outside the repo** (it is not — and must never be — committed).

## 2. Wire the keystore into the build

The release `signingConfig` reads four values from **gradle properties or environment
variables** — a gradle property (`-P…` / `gradle.properties`) takes precedence over the
environment variable of the same name (see `releaseProp()` in `app/build.gradle.kts`). When
`RELEASE_STORE_FILE` is unset the release build is produced **unsigned** (so CI stays green
before a keystore exists).

| Property / env var        | Meaning                              |
|---------------------------|--------------------------------------|
| `RELEASE_STORE_FILE`      | absolute path to the `.jks` keystore |
| `RELEASE_STORE_PASSWORD`  | keystore (store) password            |
| `RELEASE_KEY_ALIAS`       | key alias (`exponential-upload`)     |
| `RELEASE_KEY_PASSWORD`    | key password                         |

### Local signed build

```bash
cd apps/android
./gradlew :app:bundleProductionRelease \
  -PRELEASE_STORE_FILE=$HOME/keys/exponential-upload.jks \
  -PRELEASE_STORE_PASSWORD=... \
  -PRELEASE_KEY_ALIAS=exponential-upload \
  -PRELEASE_KEY_PASSWORD=...
```

Or export them as env vars (handy for `~/.gradle/gradle.properties`, which is per-machine
and outside the repo — a good place for the local values):

```properties
# ~/.gradle/gradle.properties  (NOT the repo's apps/android/gradle.properties)
RELEASE_STORE_FILE=/Users/you/keys/exponential-upload.jks
RELEASE_STORE_PASSWORD=...
RELEASE_KEY_ALIAS=exponential-upload
RELEASE_KEY_PASSWORD=...
```

### CI

The `android-v*` tag workflow (`.gitea/workflows/build-android.yml`) reads the same four
names from repo **secrets** (`RELEASE_STORE_FILE`, `RELEASE_STORE_PASSWORD`,
`RELEASE_KEY_ALIAS`, `RELEASE_KEY_PASSWORD`). The keystore itself must be present on the
runner at `RELEASE_STORE_FILE`; the simplest path is to base64 the `.jks` into a secret and
decode it into place in a pre-build step (add that step when you provision the keystore).
Until the secrets exist, CI uploads **unsigned** `.aab`s + APKs — sign them locally before
distributing.

## 3. One-time: fastlane + Play service account

fastlane's `supply` uploads to Play with a **service-account JSON** (no interactive login):

1. Play Console → **Setup → API access** → link/create a Google Cloud project → **create a
   service account** → grant it the **Release manager** role → create a **JSON key**.
2. Save the JSON **outside the repo** and point `SUPPLY_JSON_KEY` at it (`fastlane/*.json`
   is gitignored as a safety net).
3. Install the toolchain (Ruby 3.x recommended):

   ```bash
   cd apps/android
   bundle install          # installs the pinned fastlane from apps/android/Gemfile
   ```

The very **first** upload for a brand-new app must be done **by hand** in the Play Console
(opt into **Play App Signing**, let Google generate the app signing key — your keystore
becomes the *upload* key). Once the app record and the closed testing track exist, every
subsequent release goes through the lanes below.

## 4. Cut a release (the two-liner)

1. Bump `versionCode` (monotonic integer) and `versionName` in
   `apps/android/app/build.gradle.kts`. Play rejects a re-used `versionCode`.
2. Build + upload to the closed testing track:

   ```bash
   cd apps/android
   bundle exec fastlane closed
   ```

   `closed` runs `build` (signed `.aab` + APK for the `production` flavor — signing from
   the `RELEASE_*` env/props of §2) then `supply`-uploads the `.aab` as a **draft** on the
   closed track (default Play closed track = API name `beta`; override a custom track via
   `PLAY_TRACK`). Release notes come from `fastlane/metadata/.../changelogs/<versionCode>.txt`.
3. Smoke-test the closed build with the Google-Groups testers, then promote it to
   production:

   ```bash
   bundle exec fastlane production
   ```

   `production` promotes the current closed-testing release to the production track (no
   new binary) and pushes the versioned store listing metadata.

### Lanes

| Lane | Does |
|------|------|
| `build`      | Signed `.aab` + APK for the `production` flavor (unsigned fallback when `RELEASE_STORE_FILE` unset). |
| `closed`     | `build` → `supply` upload to the **closed** testing track (draft; `PLAY_TRACK` overrides the track name, default `beta`). Needs `SUPPLY_JSON_KEY`. |
| `production` | Promote **closed → production** + push listing metadata. Needs `SUPPLY_JSON_KEY`. |

### CI (optional)

The `android-v*` tag workflow (`.gitea/workflows/build-android.yml`) still builds the
`.aab`s + APKs as release artifacts. Wiring it to run `bundle exec fastlane closed`
(with `SUPPLY_JSON_KEY` + the `RELEASE_*` secrets injected on the runner) is optional — the
lanes are designed to run from a local Mac first.

## 5. Store listing asset checklist

Everything below is required before the production listing can be submitted (Play Console →
*Store presence*):

- [ ] App name (30 chars), short description (80), full description (4000)
- [ ] App icon 512×512 PNG (32-bit, with alpha)
- [ ] Feature graphic 1024×500 PNG/JPEG
- [ ] Phone screenshots — at least 2 (max 8), 16:9 or 9:16, min 320 px, max 3840 px
- [ ] 7-inch and 10-inch tablet screenshots (if declaring tablet support)
- [ ] Categorization: app category + tags
- [ ] Contact details (email; website `https://exponential.at`)
- [ ] **Privacy policy URL** — from the marketing site (`https://exponential.at/privacy`)
- [ ] Data safety form (what data is collected/shared; auth email, feedback content)
- [ ] Content rating questionnaire
- [ ] Target audience & ads declaration
- [ ] Set pricing (free) + countries

Listing **text** is version-controlled under `apps/android/fastlane/metadata/android/en-US/`
(`title.txt`, `short_description.txt`, `full_description.txt`, `changelogs/default.txt`) and
`supply` pushes it on `internal`/`production`. Only the **binary assets** (icon, feature
graphic, screenshots) still need to be dropped into `metadata/.../images/` (or entered in the
Console) — add them there and they ride the lanes too.

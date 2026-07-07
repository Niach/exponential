# Releasing the iOS app

The iOS app (`apps/ios`) is a Tuist-generated Xcode project. Releases are driven by
**fastlane** lanes (`apps/ios/fastlane/`) layered on Tuist — `gym` archives, `pilot` pushes
to TestFlight, `deliver` uploads for App Store review. This doc is the exact, copy-pasteable
pipeline.

> The lanes run **locally on a Mac** with Xcode + Tuist. No CI Mac runner is required for
> launch. The Xcode workspace is not committed (Tuist-generated), so the `build` lane runs
> `tuist generate` first. The Xcode-Organizer manual flow (§ "Manual fallback") still works
> if you ever need it.

## Targets & schemes

Tuist (`apps/ios/Project.swift`) defines two shippable app targets:

| Scheme                | Target        | bundleId                | Default cloud         |
|-----------------------|---------------|-------------------------|-----------------------|
| `Exponential`         | production    | `at.exponential`        | `app.exponential.at`  |
| `Exponential-Staging` | staging       | `at.exponential.staging`| `next.exponential.at` |

Only **`Exponential`** goes to the App Store. `Exponential-Staging` is for local/TestFlight
internal testing and co-installs (distinct bundleId). Both archive from the `release`
configuration. The development team is `V6W7BVCSM8` (`DEVELOPMENT_TEAM` in `Project.swift`).

Version is centralized: `appMarketingVersion` (`CFBundleShortVersionString`) and
`appBuildVersion` (`CFBundleVersion`) at the top of `Project.swift`. The ShareExtension
inherits both, so bumping there covers every target (a `CFBundleVersion` mismatch between the
app and its extension is rejected at upload).

## One-time setup

1. **Apple Developer Program** membership (team `V6W7BVCSM8`).
2. **App Store Connect** record for `at.exponential` (name "Exponential", primary language,
   bundle ID registered under Certificates, Identifiers & Profiles).
3. **App Store Connect API key** (Users & Access → **Integrations** → App Store Connect
   API → generate). Download the `.p8` once, save it **outside the repo** (`fastlane/*.p8`
   is gitignored), and set three env vars the lanes read:

   ```bash
   export ASC_KEY_ID=XXXXXXXXXX
   export ASC_ISSUER_ID=xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx
   export ASC_KEY_PATH=$HOME/keys/AuthKey_XXXXXXXXXX.p8
   ```

4. **Signing**: not yet bootstrapped — the `match` stanza in the `Fastfile` is commented
   out pending the account-setup decision (masterplan §13: `match` in a private git repo
   vs Developer-portal-managed profiles). Until then the lanes rely on Xcode *Automatically
   manage signing* with team `V6W7BVCSM8`, which provisions the Distribution certificate +
   App Store profiles. To switch to `match`, uncomment its stanza in `fastlane/Fastfile`
   and seed the repo once with `bundle exec fastlane match appstore`.
5. **fastlane toolchain** (Ruby 3.x recommended):

   ```bash
   cd apps/ios
   bundle install          # installs the pinned fastlane from apps/ios/Gemfile
   ```

6. **Push (APNs)**: the app uses Firebase Cloud Messaging; ensure the APNs key is uploaded to
   the Firebase project and the `aps-environment` entitlement is `production` for the release
   build (`Exponential.entitlements`).

## Cut a release (the two-liner)

1. **Bump the version** in `apps/ios/Project.swift`:
   - `appMarketingVersion` → e.g. `0.3.1` (user-visible; can repeat across builds)
   - `appBuildVersion` → monotonically increasing integer, **unique per upload** (App Store
     Connect rejects a re-used build number for the same marketing version). The
     ShareExtension inherits both, so this one bump covers every target.

2. **Ship to TestFlight** (regenerate → archive → upload):

   ```bash
   cd apps/ios
   bundle exec fastlane beta
   ```

   `beta` runs `build` (`tuist generate --no-open` then `gym` — Release archive of the
   `Exponential` scheme, `app-store` export) and `pilot`-uploads to TestFlight using the
   ASC API key from `ASC_*`. Install via TestFlight on a device and smoke-test.

3. **Submit for App Store review** (uploads the build + listing metadata; does **not**
   auto-submit):

   ```bash
   bundle exec fastlane release
   ```

   `release` runs `build` then `deliver` with `submit_for_review: false`, so you do the
   final "Submit for Review" tap in App Store Connect after the metadata/screenshot checks
   below.

### Lanes

| Lane | Does |
|------|------|
| `build`   | `tuist generate` → `gym` archive (`Exponential`, Release, `app-store`) → `build/Exponential.ipa`. |
| `beta`    | `build` → `pilot` upload to TestFlight. Needs `ASC_KEY_ID`/`ASC_ISSUER_ID`/`ASC_KEY_PATH`. |
| `release` | `build` → `deliver` upload to App Store Connect (`submit_for_review: false`). Same ASC env. |
| `screenshots` | `tuist generate` → `snapshot`: drives `ExponentialUITests/StoreScreenshots` on iPhone 17 Pro Max + iPad Pro 13-inch (M5) against a seeded local backend → `fastlane/screenshots/en-US/`. See *Store screenshots* below. |

> `gym` requires a **signed** archive — do not pass `CODE_SIGNING_ALLOWED=NO` (that flag is
> only for the simulator parity check in MEMORY). Xcode managed signing (team
> `V6W7BVCSM8`) resolves the Distribution certificate + profile, unless you enable the
> `match` stanza in the `Fastfile`.

## Store screenshots (automated)

`fastlane screenshots` captures the six store shots (board, issue detail, comments,
new issue, search, inbox) by signing into the real app from a UI test
(`ExponentialUITests/StoreScreenshots.swift`). Prereqs, from the repo root:

```bash
bun run backend:up                                  # Postgres + Electric
bun dev                                             # web dev server on :5173
cd apps/web && bun run seed:screenshots             # demo user + "Acme" workspace
cd ../ios && fastlane screenshots                   # both simulators, en-US
```

Notes:
- The seed script (`apps/web/scripts/seed-screenshots.ts`) is idempotent and prints
  the demo login. It recreates the demo **users** each run on purpose: the vite dev
  bridge strips the `electric-*` headers from the shape proxies, so shapes can never
  advance past their snapshot in local dev — fresh user/workspace ids force fresh
  shapes with fresh snapshots. Re-run it right before capturing.
- The Snapfile **erases both simulators** first (a leftover keychain session would
  skip the sign-in flow) and overrides the status bar (9:41, full battery).
- The instance URL defaults to `http://localhost:5173`; override with the
  `SNAPSHOT_INSTANCE_URL` launch environment variable if needed.
- Use the Homebrew `fastlane` (`brew install fastlane`), not `bundle exec` — the
  committed `Gemfile.lock` pins a bundler version the system Ruby doesn't ship.

## Manual fallback (Xcode Organizer)

If the lanes are unavailable, the hand flow still works: `cd apps/ios && tuist generate`,
open `Exponential.xcworkspace`, select the `Exponential` scheme + *Any iOS Device*, then
**Product → Archive** → **Window → Organizer → Archives** → **Distribute App → App Store
Connect → Upload**. (Or export the `.ipa` and upload with **Transporter.app** — App Store
uploads never go through `notarytool`; that is macOS-only.) The build then appears in
**TestFlight** after processing; answer **export compliance** ("no" custom crypto → exempt,
standard HTTPS/TLS only) and add it to an internal testing group to install.

## App Store submission checklist

App Store Connect → the app version → complete before **Submit for Review**:

- [ ] Bumped `appMarketingVersion` + unique `appBuildVersion`, matched across app + extension
- [ ] Build selected from TestFlight/processed builds
- [ ] **What's New** (release notes) text
- [ ] App name, subtitle, promotional text, description, keywords
- [ ] **Support URL** and **Marketing URL** (from the marketing site, `https://exponential.at`)
- [ ] **Privacy Policy URL** — `https://exponential.at/privacy` (required)
- [ ] **App Privacy** "nutrition label": declare data collected (auth email, feedback
      content, diagnostics) and whether linked to identity / used for tracking (no tracking)
- [ ] Screenshots for the required device sizes (upload the largest; ASC scales down):
      - **6.9"** iPhone (1320×2868 or 2868×1320) — required
      - **6.5"** iPhone (1242×2688 / 1284×2778) — required if not covered by 6.9"
      - **13"** iPad Pro (2064×2752) — required if the app supports iPad
- [ ] App icon 1024×1024 (in the asset catalog; ASC pulls it from the build)
- [ ] Age rating questionnaire
- [ ] Category (Primary/Secondary)
- [ ] Pricing (free) + availability (countries)
- [ ] Sign-in info for the review team if login is required (a demo account)
- [ ] Export compliance answered

Listing metadata + screenshots can live under `apps/ios/fastlane/metadata/` /
`apps/ios/fastlane/screenshots/` and `deliver` (the `release` lane) uploads them, replacing
the hand-entry above. Seed the tree with `bundle exec fastlane deliver init` once, then keep
it version-controlled. The archive/upload steps are already `bundle exec fastlane beta`
(TestFlight) and `bundle exec fastlane release` (App Store).

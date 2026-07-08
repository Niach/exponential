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

   That default is **internal-only** (App Store Connect team members). To also push the
   build to external testers, run `bundle exec fastlane beta external:true` — see
   *External TestFlight* below.

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
| `beta`    | `build` → `pilot` upload to TestFlight (internal-only, skips the processing wait). `beta external:true` additionally waits for processing, distributes to the external group, and submits for Beta App Review. Needs `ASC_KEY_ID`/`ASC_ISSUER_ID`/`ASC_KEY_PATH`. |
| `release` | `build` → `deliver` upload to App Store Connect (`submit_for_review: false`). Same ASC env. |
| `sync_store` | `deliver` with `skip_binary_upload` — pushes `fastlane/metadata/` (listing texts, categories, review info) + `fastlane/screenshots/` to App Store Connect without building. Same ASC env. |
| `screenshots` | `tuist generate` → `snapshot`: drives `ExponentialUITests/StoreScreenshots` on iPhone 17 Pro Max + iPad Pro 13-inch (M5) against a seeded local backend → `fastlane/screenshots/en-US/`. See *Store screenshots* below. |

> `gym` requires a **signed** archive — do not pass `CODE_SIGNING_ALLOWED=NO` (that flag is
> only for the simulator parity check in MEMORY). Xcode managed signing (team
> `V6W7BVCSM8`) resolves the Distribution certificate + profile, unless you enable the
> `match` stanza in the `Fastfile`.

Three hard-won signing/upload gotchas (all hit on the first real upload, 2026-07-07):

- **Cloud signing needs an Admin-role ASC key** — with a lesser role, export fails with
  `Cloud signing permission error`. The `build` lane passes the key via
  `-allowProvisioningUpdates -authenticationKey*` xcargs.
- **Do NOT let Apple's cloud signer sign the binary if the account name has an umlaut**:
  it writes the designated requirement with NFD-decomposed characters while the cert CN is
  NFC, so validation fails with `Invalid Signature … not properly signed` (error 90035) for
  every framework. Fix: keep a **local** Apple Distribution cert in the login keychain
  (`fastlane cert --api_key_path <json> --development false`) — xcodebuild then signs
  locally and the DR matches. Verify before uploading:
  `codesign --verify --deep --strict -v Payload/Exponential.app` must say
  *satisfies its Designated Requirement*.
- **Upload from a release Xcode, not a beta** — App Store Connect rejects beta-SDK builds
  (error 90534). If `xcode-select -p` points at Xcode-beta, run the lanes with
  `DEVELOPER_DIR=/Applications/Xcode.app/Contents/Developer`.

## External TestFlight

External testers (anyone with an email invite or the public link, up to 10k) get builds via

```bash
cd apps/ios
bundle exec fastlane beta external:true
```

which uploads, **waits for build processing** (unlike the internal path — pilot can only
assign external groups to a processed build), distributes to the external group, and
auto-submits the build for **Beta App Review**. The What-to-Test changelog comes from the
`TESTFLIGHT_CHANGELOG` env var, falling back to
`fastlane/metadata/en-US/release_notes.txt`. The group name defaults to
`External Testers`; override with `TESTFLIGHT_EXTERNAL_GROUP`.

One-time setup in App Store Connect (the lane does **not** create these):

1. **Create the external group** — the app → TestFlight → sidebar **External Testing → +**
   → name it `External Testers` (or set `TESTFLIGHT_EXTERNAL_GROUP` to whatever you pick).
   Optionally enable the group's **public link** to recruit testers without collecting
   emails; testers can otherwise be added by email on the group page.
2. **Fill in Test Information** (TestFlight → Test Information): beta app description,
   feedback email, and the **Beta App Review contact + demo sign-in account** — the first
   external submission is rejected without them. Reuse the review info from
   `fastlane/metadata/review_information/`.

Review expectations: the **first** external build goes through a full Beta App Review
(typically up to ~24–48h); subsequent builds of the same marketing version are usually
approved automatically within minutes. Bumping `appMarketingVersion` triggers a fresh
review. Internal groups are never review-gated, so `fastlane beta` (without
`external:true`) stays the fast smoke-test path. Export compliance never blocks
distribution — `ITSAppUsesNonExemptEncryption` is already declared `false` in
`Project.swift` (standard HTTPS/TLS only).

## Store screenshots (automated)

`fastlane screenshots` captures the five store shots (board, issue detail, comments,
project switcher with the v7 typed projects, inbox) by signing into the real app from a UI test
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

Listing metadata lives under `apps/ios/fastlane/metadata/` (name, subtitle, description,
keywords, promo text, release notes, support/marketing/privacy URLs, categories, copyright,
review information incl. sign-in instructions for the review team) and is version-controlled.
`fastlane sync_store` uploads it together with `fastlane/screenshots/` — no build needed —
so most checklist items above are covered by editing the files and re-running the lane.
Still manual in App Store Connect: App Privacy nutrition label, age rating questionnaire,
pricing/availability, review contact **phone number**, export compliance, and the final
"Submit for Review" tap. The archive/upload steps are `fastlane beta` (TestFlight) and
`fastlane release` (App Store binary + metadata).

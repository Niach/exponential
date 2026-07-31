# Deploys & releases

The operations runbook. `CLAUDE.md` carries only the summary — everything an
operator needs lives here.

All services run on Coolify (`coolify.home.straehhuber.com`, Hetzner
`46.225.140.133`). **Coolify is home-LAN-only — there are no auto-redeploy
webhooks.** After a green Actions run, deploy from a LAN machine:

```bash
coolify deploy uuid <uuid>
```

## Service inventory

| Service | Domain | uuid | Notes |
| --- | --- | --- | --- |
| Web cloud | `app.exponential.at` | `hzoe7vty1rzjypyymsaqw2w6` | dockerimage `ghcr.io/niach/exponential-web:latest`; Postgres `hqc1ofbam3x5kyxjexwj1oio`, Electric `s12y6uvto3utdsan5mrkhjjp`; attachments in Hetzner bucket `exponential` |
| Marketing | `exponential.at` | `bh4vnu32zwiu0bw6nf8d7yt8` | public-source clone; build `cd apps/marketing && bun run build`, start `npx -y serve apps/marketing/dist -l 80` (serve auto-loads the `serve.json` copied into `dist/`; a repo-relative `--config` path resolves against the served dir and crashes serve) |
| Push relay | `push.exponential.at` | `escnmp723si2642q1vcrmnqt` | builds `Dockerfile.push-relay`; holds `FIREBASE_SERVICE_ACCOUNT_JSON` |
| Steer relay | `steer.exponential.at` | `wxb6j3l0m01ogonvj5bxodum` | dockerimage `ghcr.io/niach/exponential-steer-relay:latest`; holds `STEER_RELAY_SECRET` (must match web env) **and `TRUST_PROXY=true`** |
| Staging web | `next.exponential.at` | `i2h9ozcemp70yigkf8jylaq2` | same web image; Postgres `mu6of6u8vul17sycib40zax8`, Electric `x80j1jdcf6zmviyh18d9b8iq`, bucket `exponentialnext`; Creem test mode |
| Staging steer relay | `steer-next.exponential.at` | `3reo2ipnx9m6srogc4h1xcow` | mirrors prod |

`TRUST_PROXY=true` is mandatory on both relays behind Traefik — per-IP rate
limits key on `X-Forwarded-For`.

The web image is the SAME artifact for cloud, staging and the self-host
distribution (`selfhost/` + `INSTALL.md`), so the ghcr package must stay
PUBLIC. Self-hosters pin semver tags.

`/api/health` gates the web HEALTHCHECK (DB-backed); the push relay exposes
`/healthz`. DNS is on Cloudflare, zone-only gray-cloud (keeps Traefik HTTP-01
working).

## Web

`build-web.yml` publishes `ghcr.io/niach/exponential-web` on master pushes and
`v*` tags, multi-arch.

## Android

Tag `android-vX.Y.Z` → one production APK + a Play bundle attached to a GitHub
Release (`make_latest: false`, so `releases/latest` stays desktop-owned).
Signed when the keystore secrets are set. The Play upload itself is manual, via
fastlane — see [release-android.md](release-android.md).

## iOS

No CI. Local fastlane only — see [release-ios.md](release-ios.md).

## Desktop releases

Tag `desktop-v*` (or workflow dispatch) → `build-desktop.yml`:

1. **Codegen-drift guard** — regenerates the committed Rust files and fails on
   any diff.
2. Two channels × three OSes. The channel is a compile-time cargo feature:
   `production`, and `staging` (`--features staging`, app id
   `at.exponential.staging`, pointed at `next.exponential.at`).

Per-OS artifacts:

- **Linux** — AppImage (glibc floor: ubuntu-22.04 / 2.35).
- **macOS** — a `.app` bundle (required for the `exponential://` scheme),
  shipped as a notarized `.dmg` when the `MACOS_CERT_P12` / `NOTARY_*` secrets
  are set, otherwise an ad-hoc `.zip`.
- **Windows** — a raw `.exe` only (it is both the download link and the updater
  asset), with HKCU self-registration. Marked `continue-on-error` in CI.

Both desktop builds self-register the `exponential://` handler at startup
(`app::desktop_integration` plus a single-instance socket on Linux;
`LSSetDefaultHandlerForURLScheme` on macOS).

Production artifacts go to a GitHub Release with SHA256SUMS and
`make_latest: true`.

**Self-update (EXP-22)** — checked at launch and every 4h
(`crates/ui/src/update.rs`; staging never self-updates) → `crates/updater`
downloads, verifies against SHA256SUMS, swaps per-OS, and restarts gpui. A
missing asset (including an unsigned macOS build), a dev or non-AppImage run,
or an unwritable install all degrade to a browser-link banner.

Still manual: Linux `.deb` / tarball, and a signed Windows MSI.

## Release-time checklist (not automated)

- **Changelog (EXP-164)** — every user-facing release PREPENDS a
  `ChangelogEntry` to `apps/web/src/lib/changelog.ts` (fresh id, ISO date,
  title, one-line summary, short GFM body). The head id re-surfaces the sidebar
  "What's new" card; `changelog.test.ts` enforces the conventions.
- **GitHub App** — webhook Active
  (`${BETTER_AUTH_URL}/api/webhooks/github`, secret `GITHUB_WEBHOOK_SECRET`);
  `installation*` events arrive automatically; subscribe **Pull request**.
  Claim flow: callback `${BETTER_AUTH_URL}/api/integrations/github/callback`,
  client secret set, "Redirect on update" ticked, "Request user authorization"
  UNCHECKED. Permissions: `workflows` write + **Org → Members read-only**
  (EXP-363: claims require CONTROL — login match for a user, active membership
  for an org, fail-closed `orgperm` until accepted; collaborators get
  `notowner` + an install link). Cached tokens keep their mint-time permissions
  for ~1h. Staging and prod need separate Apps.
- **Deep links (EXP-92)** — web serves
  `/.well-known/apple-app-site-association` (static, team `V6W7BVCSM8`) and
  `assetlinks.json` (from `ANDROID_APP_LINK_FINGERPRINTS`; 404 when unset — use
  the **Play App Signing key** SHA-256, NOT the upload keystore). iOS needs
  Associated Domains on both App IDs.
- **Cloud env** — sign-up is Google-only: `AUTH_PASSWORD_ENABLED=false`,
  `GOOGLE_LOGIN_ENABLED=true`, `AUTH_SIGNUP_ENABLED` unset. Signup and login are
  ONE merged `/auth/login`; `/auth/register` redirects. SES stays configured for
  transactional mail.

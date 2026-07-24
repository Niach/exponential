# Exponential

Real-time issue tracker.

## Tech Stack

- **Framework**: TanStack Start (React 19, TanStack Router, TanStack React DB)
- **Database**: PostgreSQL 17 via Drizzle ORM (`snake_case` casing)
- **Real-time**: ElectricSQL (shape proxy pattern via `@tanstack/electric-db-collection`)
- **Auth**: Better Auth (email/password + OIDC via `genericOAuth` plugin, session-based, `tanstackStartCookies` plugin)
- **API**: tRPC v11 (`authedProcedure`, `generateTxId` for Electric sync)
- **UI**: shadcn/ui on Tailwind v4 (OKLCH zinc palette, dark theme forced via `html.dark`)
- **Date Picker**: `react-day-picker` + `date-fns` via shadcn `Calendar` component
- **Infrastructure**: Docker Compose — Postgres:54321, Electric:30000, Garage:3900 (S3-compatible), Caddy:3000 (HTTP/2 reverse proxy; `Caddyfile` is gitignored — copy from `Caddyfile.example`, which is configured for Electric long-poll timeouts), optional steer-relay:4002 (`--profile steer`)
- **Package Manager**: bun

## Monorepo Layout

```
exponential/
├── apps/
│   ├── web/        # TanStack Start app (the issue tracker)
│   ├── push-relay/ # Standalone push notification relay (Hono/Bun, separately deployed)
│   ├── steer-relay/# Standalone remote-start + live-terminal-steer WebSocket hub (Bun; device presence + session rooms in memory)
│   ├── marketing/  # Marketing site (Vite + React, deployed via Coolify)
│   ├── ios/        # Native SwiftUI iOS app (Tuist + GRDB) — self-contained (ExpCore/ExpUI become iOS-only frameworks; the legacy ExponentialMac target still exists pending its planned deletion)
│   ├── android/    # Native Kotlin / Jetpack Compose app
│   └── desktop/    # Cross-platform desktop IDE (Rust: gpui + gpui-component + alacritty_terminal; embedded `claude` coding sessions; trunk git IDE — auto-clone, source-control panel, files rail, issue Changes tab)
├── packages/
│   ├── db-schema/          # Drizzle schema + shared zod/domain types
│   ├── design-tokens/      # OKLCH→sRGB theme tokens → Compose/SwiftUI/Rust
│   ├── domain-contract/    # contract.json — canonical enum values; emits per-language constants
│   ├── electric-protocol/  # Electric SQL shape protocol fixtures
│   ├── steer-ticket/       # Shared HS256 steer-ticket sign/verify (web mints, relay verifies)
│   ├── widget/             # Embeddable feedback widget (Preact + snapDOM); builds loader.js/widget.js into apps/web/public/widget/v1/
│   └── tsconfig/           # Shared TS configs
├── docker-compose.yaml
├── Caddyfile
├── Dockerfile              # Builds the web app image; build context = repo root
├── Dockerfile.push-relay   # Builds the push relay image; build context = repo root
├── Dockerfile.steer-relay  # Builds the steer relay image; build context = repo root
└── package.json    # bun workspaces, dispatcher scripts
```

Workspace package names: `@exp/web`, `@exp/push-relay`, `@exp/steer-relay`, `@exp/marketing`, `@exp/db-schema`, `@exp/design-tokens`, `@exp/domain-contract`, `@exp/electric-protocol`, `@exp/steer-ticket`, `@exp/widget`, `@exp/tsconfig`. (The desktop app `apps/desktop` is a Rust Cargo workspace, not a bun workspace.)

**Client parity:** all four clients (web, iOS, Android, desktop) sync the same **fourteen** Electric shapes (teams, boards, issues, labels, issue_labels, users, team_members, team_invites, comments, attachments, notifications, issue_events, issue_subscribers, **coding_sessions**) — proxy count == shape count == 14. `repositories`, `actions`, `user_notification_prefs`, `email_deliveries`, the helpdesk tables (`support_threads`/`support_messages`), and the widget tables are **server-only (tRPC), never synced**. **Vocabulary (EXP-180):** the product says **team** and **board** EVERYWHERE — copy, URLs (`/t/$teamSlug/boards/$boardSlug/issues/$id`), code identifiers, DB tables/columns (`teams`, `boards`, `team_id`, `board_id`), tRPC routers (`teams`, `boards`, `teamMembers`, `teamInvites`), shape proxy names, and MCP tools (`exponential_teams_*`, `exponential_boards_*`). The old workspace/project vocabulary and the legacy `/w/`+`/projects/` URL forms are DEAD (no redirects — the rename shipped pre-users as a clean cut); do not reintroduce them. Boards have no types — the old dev/tasks/feedback split is gone; `repository_id` is NULLABLE (FK restrict) and coding features gate purely on repo PRESENCE. **Nothing is anonymously readable (EXP-180 removed public boards):** every shape is member-only — anonymous requests resolve to the impossible-match sentinel, there is no public tRPC surface, and attachment byte reads require membership. The only anonymous endpoints are the widget (`/api/widget/*` — the sole stranger write path), the helpdesk reporter magic-link routes (`/api/support/*` + `/support/$token`), invites, and auth. All board-scoped shapes are TEAM-scoped for members with a STATIC trash predicate (REV2-5): `team_id IN (member teams) AND board_deleted_at IS NULL`, where `board_deleted_at` is a trigger-maintained mirror of the parent board's `deleted_at` (denormalized onto issues + comments/attachments/issue_events/issue_subscribers/coding_sessions/issue_labels, fanned out by `propagate_board_deleted_at` on trash/restore). A trashed board's children still drop out of member sync for the 48h trash window — but as incremental move-out deltas, NOT a where-clause change: the per-user board-id lists that used to be embedded in these where clauses rotated all 8 board-scoped shape identities for every member on any board create/trash and forced full cross-team resyncs. Shape identities now only rotate on actual team-membership changes. Batch coding_sessions rows (`board_id IS NULL`, span boards) and issue-less notifications keep a NULL `board_deleted_at` and always sync. The notifications shape is fully static per user — `user_id = me AND board_deleted_at IS NULL`, no membership lists at all (safe: the fan-out membership-filters recipients at delivery time; already-delivered rows outlive a membership like received email). Issue-less notification rows (helpdesk `support_reply`) carry an app-written SYNCED `team_id` so every client's inbox can route them to the right team's Support surface (all four clients render issue-less support_reply rows as per-team Support inbox groups; issue-anchored rows leave `team_id` NULL). The app stays **noindex** everywhere: the `__root.tsx` meta plus an ungated `X-Robots-Tag: noindex` on every response from `server-bun.ts` (self-hosted instances must not be indexed either); `/robots.txt` deliberately ALLOWS crawling (`Disallow: /api/` only) so crawlers can SEE the noindex; there is no sitemap. Marketing owns the indexed surface — `apps/marketing/public/robots.txt` + a `dist/sitemap.xml`/`llms.txt` generated from the `PAGES` manifest in `src/lib/seo.ts` by `scripts/prerender.tsx`. Permissions are membership-only (`use-team-permissions.ts` and its native mirrors); every member moderates and handles support. Signups get **no** team (EXP-188 removed the auto-created personal team): the first-run flow is create-or-join — the onboarding wizard on web/iOS/Android (and the desktop zero-team empty state) offers "Create a team" (name → `teams.create`, open to EVERY authed user; creator becomes owner; cloud free tier capped by `FREE_OWNED_TEAMS_CAP`) or "Join a team" (paste an invite link; `teamInvites.accept` stamps `onboardingCompletedAt`). `teams.getDefault` is the NON-CREATING default-team resolver (oldest non-feedback membership or null — it replaced `teams.ensureDefault`, so bump `CLIENT_MIN_VERSION_*` when deploying past old native builds); an owner may delete ANY of their teams including the last one, and team invites optionally carry a synced `email` (the server mails the invite link). The bootstrap feedback team is identified by `getFeedbackTeamId()` (slug `feedback`), never by a flag, and stays undeletable. **Billing (Creem) and the admin console are intentionally web-only** — native clients show no billing UI (store-policy safe). **The desktop app is the only client that runs coding sessions and publishes to the steer relay** (PTY mirror + the member-only scrubbed activity channel — nothing coding-related is ever visible outside the team). When changing enum values in `packages/db-schema/src/domain.ts`, also update `packages/domain-contract/contract.json` and run `bun run --filter @exp/domain-contract generate` to refresh the Swift / Kotlin / Rust constants.
**Batch coding runs (EXP-106 — replaced the deleted releases feature):** the release entity is gone (no `releases` table/shape/router, no `issues.release_id`, no release UI on any client, no `release_added`/`release_removed` event types, no `exponential_release_pr_open` MCP tool). Multi-issue coding survives as **batch runs, desktop-only** (EXP-201: any agent — claude/codex/pi): the ONE unified Start-coding dialog (`crates/ui/src/start_coding_dialog.rs` — always a searchable multi-issue picker; an agent tab strip plus per-agent Model/Effort pickers, ultracode switch + plan-mode checkbox [Claude-only], skip-permissions checkbox [claude+codex — OFF = the agent's guarded auto mode]; defaults from settings are per-AGENT with NO issue/batch split (EXP-206 — Claude: plan ON/ultracode OFF/skip OFF; Codex: skip OFF; the settings pane groups them as agent tabs, and the dialog's agent tab strip offers only the doctor-installed agents); ONE repository per run) launches a plain single-issue session for 1 checked issue and a BATCH session for 2+ (the bulk-select bar's "Start coding" action opens the same dialog pre-checked). A batch run is deliberately loose: ONE agent session on ONE pushed branch `exp/batch-<id8>` (`coding/src/batch_launcher.rs`; 8 lowercase hex chars so `parseIssueIdentifierFromBranch` can never mis-link it), a prompt listing all issues (`batch_prompt.rs` — issues may overlap, Claude organizes the work itself; no per-issue subagent defs, no `--agents`, no per-issue worktrees/PRs, no dependency waves), ending in ONE combined PR via `exponential_pr_open` with `issueIds` + `head` — the server links EVERY listed issue to that PR (same repo enforced), and the webhook/poller resolve a PR to ALL its linked issues by exact `pr_url`, so merging the combined PR completes them all. `coding_sessions` rows are issue-scoped OR batch-scoped (`issue_id`/`board_id` NULLABLE; `codingSessions.start` takes exactly one of issueId/teamId — the teamId form inserts the issueless batch row); web steer of batch runs is deferred.

**Description / comment markdown contract:** `issues.description` and `comments.body` are plain `text` columns holding **GFM** markdown — the single interchange contract across web (TipTap + tiptap-markdown), iOS (cmark-gfm), desktop (Rust — pulldown-cmark or comrak, GFM), and Android (from-scratch block editor in `ui/markdown/`, commonmark-java; byte-parity locked by its test suite). Supported, round-trippable features: bold, italic, strikethrough, inline code, headings (H1–H3 editable), bullet/ordered lists, **task lists** (`- [ ]`/`- [x]`), blockquote, code blocks, links, **block/full-width inline images**, **@mentions**, and **#issue mentions**. **Underline is intentionally unsupported** (no GFM representation — it does not round-trip); tables, slash commands, and image resize are intentionally out of scope. **Mentions** are written as `@<email>` in the markdown source (the single interchange form — round-trips as plain GFM text); the server resolves `@email` to team members and fires `issue_mention` notifications + auto-subscribes them (`apps/web/src/lib/integrations/mentions.ts`). Clients render a known member's `@email` as a name pill; editors on all four clients offer an @-autocomplete that inserts the plain `@email` form. **Issue mentions** are written as the plain GFM text `#<IDENTIFIER>` (e.g. `#EXP-42`) — same interchange principle as `@email`: no new markdown syntax, no schema impact (an inline `#` is never a heading). Clients render the token as a clickable/tappable issue pill ONLY when it resolves to a synced issue the viewer can see in the SAME team (token contract in `apps/web/src/lib/issue-refs.ts` + TipTap decorations in `lib/issue-ref-extension.ts`/`issue-ref-provider.tsx`; iOS `IssueRefLookup.swift`; Android `ui/markdown/IssueRefs.kt`; desktop `crates/ui/src/markdown`); unknown identifiers stay plain text. Typing `#` in any issue-description/comment editor offers same-team issues (identifier + title-substring match) and inserts the plain `#<IDENTIFIER>` text — all four clients. Server-side, `resolveIssueRefs` (`lib/integrations/mentions.ts`) resolves refs team-scoped but fires NO notifications yet (deliberate — it is the anchor point for a future "referenced-in" signal). Embedded images are always stored as the relative form `![alt](/api/attachments/{id})`; the server canonicalizes image URLs to relative on save (`canonicalizeMarkdownImageUrls` in `apps/web/src/lib/storage/issue-attachments.ts`), and clients resolve to absolute only at fetch time. The iOS editor is block-based: `IssueEditorModel` (an `@Observable`) owns `[ContentBlock]` as the single source of truth and derives markdown only at save; image upload is atomic (all-or-nothing) and concurrent. Attachments carry probed `width`/`height` so clients can pre-size and avoid layout shift.

## Commands

All commands run from the repo root unless noted.

```bash
bun install                        # Install workspace deps (run from root)
bun run backend                    # Start the LOCAL backend: docker compose up -d + web dev server (app at localhost:3000 via Caddy)
bun run ios                        # Start iOS: tuist generate (opens Xcode; run the Exponential scheme) — Mac-only
bun run android                    # Start Android: install productionDebug + launch on the connected device/emulator
bun dev                            # Start web dev server (apps/web, localhost:5173)
bun run dev:marketing              # Start marketing dev server (apps/marketing)
bun run dev:push-relay             # Start push relay dev server (apps/push-relay, localhost:4001)
bun run start:push-relay           # Start push relay in production mode
bun run dev:steer-relay            # Start steer relay dev server (apps/steer-relay, localhost:4002)
bun run start:steer-relay          # Start steer relay in production mode
bun run test:steer-relay           # Steer relay + ticket unit/integration tests
bun run build                      # Build widget + web + marketing (widget MUST build before web)
bun run build:web                  # Build only the web app
bun run build:widget               # Build the embeddable widget into apps/web/public/widget/v1/
bun run dev:widget                 # Watch-build the widget (pairs with `bun dev`; test page at /widget/v1/demo.html)
bun run test:widget                # Run widget package unit tests
bun run typecheck                  # Typecheck the web app
bun run test                       # Run web vitest unit tests
bun run test:e2e                   # Run web playwright e2e tests
bun run migrate                    # Apply Drizzle migrations
bun run migrate:generate           # Generate migrations from schema changes
bun run psql                       # psql shell into local DB

bun run backend:up                 # docker compose up -d (Postgres + Electric + Garage + Caddy)
bun run backend:down               # docker compose down
bun run backend:clear              # docker compose down -v (wipe volumes)
bun run storage:init               # one-time Garage bootstrap (after backend:up); prints S3 access/secret key

bun run lint                       # ESLint fix across workspaces
bun run format                     # Prettier format

bun run android:build              # ./gradlew :app:assembleProductionDebug in apps/android
bun run android:install            # ./gradlew :app:installProductionDebug

bun run dev:desktop                # gpui IDE against LOCAL backend (EXP_INSTANCE_URL=http://localhost:3000)
bun run build:desktop              # cargo build --release -p app (production channel)
bun run appimage:desktop           # build + package a Linux AppImage (production)
bun run test:desktop               # cargo test (apps/desktop workspace)
bun run clean:desktop              # cargo clean (EXP-76: run on zed/gpui rev bumps — cargo never GCs stranded artifacts)

bun run --filter @exp/domain-contract generate   # Regenerate iOS + Android + desktop enum constants
bun run --filter @exp/design-tokens generate     # Regenerate Android + iOS + desktop theme tokens
```

You can also run a script directly inside a workspace: `bun --filter @exp/web <script>` or `cd apps/web && bun run <script>`. The desktop commands shell out to `cargo` (like `android:build` shells out to gradle) — `apps/desktop` is a Rust Cargo workspace, so plain `cargo` from `apps/desktop/` works too. Its two generated Rust files (`crates/domain/src/contract.generated.rs`, `crates/theme/src/tokens.generated.rs`) are committed; re-run the generators above only when `contract.json` / `tokens.json` change.

## Deploys

All deploy targets run on Coolify (`coolify.home.straehhuber.com`, Hetzner host `46.225.140.133`) — staging and prod only.

- **Web cloud (`app.exponential.at`)** — Coolify app `exponential-web` (uuid `hzoe7vty1rzjypyymsaqw2w6`), a *dockerimage* app pulling `ghcr.io/niach/exponential-web:latest`. The image is built by `.github/workflows/build-issues-web.yml` on every push to `master` and on `v*.*.*` / `v*.*.*-dev` tags. **Coolify is home-LAN-only, so there is no auto-redeploy webhook** — after a green Actions run, redeploy manually from a LAN-connected machine: `coolify deploy uuid hzoe7vty1rzjypyymsaqw2w6` (or click "Deploy" in the Coolify UI). Backed by `exponential-postgres` (uuid `hqc1ofbam3x5kyxjexwj1oio`) and `exponential-electric` (uuid `s12y6uvto3utdsan5mrkhjjp`); attachments live in Hetzner Object Storage bucket `exponential` at `nbg1.your-objectstorage.com`.
- **Marketing (`exponential.at`)** — Coolify app `exponential-marketing` (uuid `bh4vnu32zwiu0bw6nf8d7yt8`). Public-source app cloning `https://github.com/Niach/exponential.git`, base directory `/`, build `cd apps/marketing && bun run build`, start `npx -y serve apps/marketing/dist -l 80`. **No auto-redeploy** — Coolify is home-LAN-only so the webhook doesn't reach it. Manual: `coolify deploy uuid bh4vnu32zwiu0bw6nf8d7yt8` from a LAN-connected machine.
- **Push relay (`push.exponential.at`)** — Coolify app `exponential-push-relay` (uuid `escnmp723si2642q1vcrmnqt`). Public-source app cloning `https://github.com/Niach/exponential.git` and building `Dockerfile.push-relay` (context `.`). Holds the `FIREBASE_SERVICE_ACCOUNT_JSON` env var. Same manual-deploy rule: `coolify deploy uuid escnmp723si2642q1vcrmnqt`.
- **Steer relay (`steer.exponential.at`)** — Coolify app `exponential-steer-relay` (uuid `syytl3ahhc7q6d2a7573m90s`), mirroring the push-relay setup: public-source app cloning the repo and building `Dockerfile.steer-relay` (context `.`). Holds `STEER_RELAY_SECRET` (matches the web apps' env; web apps also set `STEER_RELAY_URL=https://steer.exponential.at`) **and `TRUST_PROXY=true`** — mandatory behind Coolify/Traefik: it keys the relay's per-IP WebSocket rate limits by the proxy-appended `X-Forwarded-For` address; without it every connect worldwide shares one fallback bucket (see `apps/steer-relay/.env.example`). Manual deploy like the others: `coolify deploy uuid syytl3ahhc7q6d2a7573m90s`. Unset `STEER_RELAY_URL` on a web app disables remote start/steer gracefully.
- **Staging cloud (`next.exponential.at`)** — Coolify app `exponential-next-web` (uuid `i2h9ozcemp70yigkf8jylaq2`), same *dockerimage* from `ghcr.io/niach/exponential-web:latest`. Backed by `exponential-next-postgres` (uuid `mu6of6u8vul17sycib40zax8`) and `exponential-next-electric` (uuid `x80j1jdcf6zmviyh18d9b8iq`); attachments in Hetzner Object Storage bucket `exponentialnext`. Has Creem test-mode billing enabled. Deploy: `coolify deploy uuid i2h9ozcemp70yigkf8jylaq2`.
- **Staging steer relay (`steer-next.exponential.at`)** — Coolify app uuid `mtvt7ikksp1gua3ivk3lefti`, mirroring the prod steer relay (public-source app building `Dockerfile.steer-relay`, context `.`). Holds the staging `STEER_RELAY_SECRET`, which must match `exponential-next-web`'s env (the staging web app sets `STEER_RELAY_URL=https://steer-next.exponential.at`), plus `TRUST_PROXY=true` (same mandatory-behind-Traefik requirement as the prod relay). Manual deploy: `coolify deploy uuid mtvt7ikksp1gua3ivk3lefti`.
- **Android**: tag `android-vX.Y.Z` triggers `.github/workflows/build-android.yml` — builds ONE production release APK + ONE production Play bundle (EXP-68: debug/staging artifacts are no longer published) and publishes them to a GitHub Release. Release artifacts are signed when the `ANDROID_KEYSTORE_B64` + `RELEASE_STORE_PASSWORD`/`RELEASE_KEY_ALIAS`/`RELEASE_KEY_PASSWORD` GitHub secrets are set, otherwise unsigned. Play uploads run locally via fastlane (`apps/android/fastlane`, needs `SUPPLY_JSON_KEY`).
- **Desktop**: tag `desktop-v*` (or manual `workflow_dispatch`) triggers `.github/workflows/build-desktop.yml` — a codegen-drift guard (regenerates the committed Rust files and fails on diff) then builds **two channels × three OSes** (macOS arm64, Linux x86_64, Windows x86_64 — the `windows-latest` legs are `continue-on-error` so they never block a release): `production` (→ `app.exponential.at`) and `staging` (→ `next.exponential.at`, built with `--features staging`, distinct app id `at.exponential.staging` so both can coexist). Channel is a compile-time cargo feature (`ui`'s `CLOUD_INSTANCE`, `app::channel`), the Rust analog of iOS `AppConstants.isStaging`. Linux artifacts are **AppImages** (`apps/desktop/scripts/build-appimage.sh` via linuxdeploy — bundles non-core libs incl. `libxkbcommon-x11`, leaves the GPU/GL/Wayland driver stack to the host; glibc floor = the runner's, currently ubuntu-22.04 → 2.35); macOS ships a proper **`.app` bundle** (`apps/desktop/scripts/build-macos-app.sh` wraps the release binary with `assets/packaging/Info.plist`) — the raw binary is no longer shipped, because macOS only routes the `exponential://` scheme to a Launch-Services-registered bundle that declares `CFBundleURLTypes`. macOS signing is **secret-gated**: with `MACOS_CERT_P12`/`MACOS_CERT_PASSWORD` + `NOTARY_KEY_ID`/`NOTARY_ISSUER_ID`/`NOTARY_KEY` set, the `.app` is Developer-ID signed (hardened runtime) and shipped as a **notarized + stapled `.dmg`** (`build-macos-dmg.sh`); without them CI falls back to an ad-hoc-codesigned `.zip`. Windows ships ONLY the raw `Exponential-<channel>-x86_64-windows.exe` (EXP-68: the `.zip` twin is gone — the raw exe is both the download link and the self-updater's asset, which swaps the running exe directly); its `exponential://` handler + single instance self-register in HKCU at first launch (no installer; a signed MSI/MSIX is future work). The app **self-registers the `exponential://` deep-link handler at startup on both desktops**: Linux via `app::desktop_integration` (writes a `.desktop` + `mimeapps.list` default pointing at `$APPIMAGE`/`current_exe()`, since gpui's Linux backend never invokes `on_open_urls`) with a `UnixDatagram` single-instance socket (`app::single_instance`, mirrors Zed's `open_listener`) forwarding the callback into the running window; macOS via `app::macos_integration` (`LSSetDefaultHandlerForURLScheme` re-asserts the bundle as the default `exponential:` handler each launch — macOS auto-registers the launched `.app` from its Info.plist and delivers `exponential://` to the running instance via `on_open_urls`, so no single-instance socket is needed). The production-channel artifacts are published to a GitHub Release (SHA256SUMS, `make_latest: true` — Android releases set `make_latest: false` so `releases/latest/download/…` marketing links and the in-app updater stay desktop-owned). The app **self-updates from that release** (EXP-22): the update check (`crates/ui/src/update.rs` — at launch and every 4h while running, EXP-68; staging channel never checks) pairs with the gpui-free engine in `crates/updater` — click-to-update banner → streaming download → `SHA256SUMS.txt` verify → swap (Linux: atomic rename over `$APPIMAGE`; Windows: `self-replace` on the raw `.exe` asset; macOS: mount the `.dmg` + rsync over the bundle) → "Restart to update" via gpui `set_restart_path`/`restart`. The plan is built from the CHECKED release's own asset list, so a platform whose asset is missing degrades to the browser-link banner — which is also the macOS gate: unsigned releases ship the ad-hoc `.zip`, not the `.dmg`, so macOS stays banner-only until the signing secrets land. Dev builds / non-AppImage runs / unwritable installs degrade the same way (`updater::capability`). Still manual: Linux `.deb`/tarball and a signed Windows installer (see checklist).

### Release-time checklist (not automated)

- **In-app changelog (EXP-164)**: every user-facing release PREPENDS one `ChangelogEntry` to `CHANGELOG` in `apps/web/src/lib/changelog.ts` (fresh never-reused `id`, ISO `date`, `title`, single-line `summary`, short GFM `body`) — the head entry's id keys the dismissable "What's new" card in the web sidebar footer (per-device localStorage `exp.changelogSeenId`), so a new head entry is what re-surfaces the card; `changelog.test.ts` enforces the conventions.
- **Android signing/uploads**: `signingConfigs` is wired in `app/build.gradle.kts` and CI signs release artifacts when the keystore secrets are set (see the Android deploy bullet above) — what stays manual is the Play upload, run locally via fastlane (`apps/android/fastlane`, needs `SUPPLY_JSON_KEY`; docs/release-android.md).
- **Desktop (Rust/gpui) distribution**: Linux AppImages, raw Windows `.exe`s (EXP-68: no more `.zip` twin), and macOS `.app` bundles are all built automatically by `build-desktop.yml` (both channels), and macOS Developer-ID signing + notarization runs in CI **when the `MACOS_CERT_P12`/`NOTARY_*` secrets are configured** (otherwise ad-hoc `.zip` fallback). The signing secrets ARE configured (2026-07-11: Developer ID Application cert + notary API key; local copies in `~/keystores/developer-id-application.*`), so tagged releases ship the notarized `.dmg` and macOS auto-update is live. Still manual/missing: Linux `.deb`/tarball and a signed Windows MSI/MSIX. URL-scheme registration is handled at runtime by the app's self-registration (Linux `.desktop`/`mimeapps.list`; macOS `LSSetDefaultHandlerForURLScheme`; Windows HKCU), not by a system installer.
- **iOS distribution**: no CI pipeline — releases run locally via fastlane (`apps/ios/fastlane`: build/screenshots/beta/release lanes; docs/release-ios.md).
- **GitHub App settings**: the App's **webhook must be Active** (URL `${BETTER_AUTH_URL}/api/webhooks/github`, secret = `GITHUB_WEBHOOK_SECRET`) — `installation` and `installation_repositories` events are delivered to GitHub Apps AUTOMATICALLY (they never appear in the "Subscribe to events" list); only **Pull request** needs explicit subscription. The server syncs `github_installations` from installation created/unsuspend/suspend/deleted and flags/heals `repositories.inaccessible_at` from repo-selection changes. For the team claim flow: set the OAuth **Callback URL** to `${BETTER_AUTH_URL}/api/integrations/github/callback`, generate a **client secret** (env `GITHUB_APP_CLIENT_ID`/`GITHUB_APP_CLIENT_SECRET` on the Coolify web apps), tick **"Redirect on update"** under the Setup URL, and leave **"Request user authorization (OAuth) during installation" UNCHECKED** (the claim flow triggers OAuth itself; enabling it would divert the install redirect to the callback with a wrong-purpose state). **The App HAS the `workflows` write permission** (granted 2026-07-16 during EXP-112, reversing the EXP-73 item-4 stance) — coding/release runs may push branches touching `.github/workflows/*` through the installation token. Two operational gotchas when changing App permissions: the installation owner must ACCEPT the permission update on the installation (GitHub prompts for it), and the server caches installation tokens until GitHub's real expiry (~1h) — a cached token keeps its mint-time permissions, so a fresh grant only takes effect after the cached token expires. Staging and prod each need their own App (a GitHub App has a single Setup URL).
- **Mobile deep links (EXP-92)**: the web serves `/.well-known/apple-app-site-association` (static — team `V6W7BVCSM8`, both bundle ids) and `/.well-known/assetlinks.json` (from `ANDROID_APP_LINK_FINGERPRINTS`; 404 when unset) so `/t/*/boards/*/issues/*` + `/invite/*` links open the native apps (the legacy `/w/` and `/projects/` URL forms are dead — EXP-180 clean cut). Manual: set `ANDROID_APP_LINK_FINGERPRINTS` on the prod + staging Coolify web apps with the **Play App Signing key** SHA-256 (Play Console → App integrity — the local upload keystore is NOT the shipping cert); iOS needs the Associated Domains capability on both App IDs (automatic signing adds it on the next archive — regenerate stale fastlane profiles if signing fails).
- **Cloud launch env**: sign-up is **Google-only** — set `AUTH_PASSWORD_ENABLED=false`, `GOOGLE_LOGIN_ENABLED=true` + `GOOGLE_CLIENT_ID`/`GOOGLE_CLIENT_SECRET`, and leave `AUTH_SIGNUP_ENABLED` unset/false (Google sign-in auto-creates accounts; signup and login are ONE merged `/auth/login` page — `/auth/register` is always a pure redirect to it, and the page's create-account toggle only shows when `signupEnabled` from `buildAuthConfig` is true). `AWS_SES_REGION` + `AWS_ACCESS_KEY_ID`/`AWS_SECRET_ACCESS_KEY`/`EMAIL_FROM` stay configured for transactional mail (Amazon SES — EXP-108 replaced Resend), but the password reset/verification flows are inert without password auth. Staging (`next.exponential.at`) already runs this posture.
- `/api/health` gates the web Docker HEALTHCHECK (DB-backed; Electric reported but non-gating). The push relay's is `/healthz`.

DNS for `exponential.at` is on Cloudflare (zone-only, gray-cloud A records → Hetzner host) so Traefik's Let's Encrypt HTTP-01 challenge keeps working.

After schema changes, always: `bun run migrate:generate && bun run migrate`

Custom SQL triggers must be applied manually after migrations:

```bash
docker exec -i exponential-postgres-1 psql -U postgres -d exponential < apps/web/src/db/out/custom/0001_triggers.sql
```

## Web App Structure (`apps/web/`)

High-level map (not exhaustive — trust the filesystem over this list):

```
apps/web/src/
├── components/
│   ├── ui/                       # shadcn components — always use these over raw HTML
│   ├── team/                     # Sidebar, mobile topbar, settings sections (general/members/labels/boards/billing), plan-comparison
│   ├── onboarding/               # First-run wizard (board → optional GitHub repo connect; team auto-created)
│   ├── inbox/                    # Notifications inbox view
│   ├── issue-editor/, issue-properties/, issue-row-menu/, comment-rows/
│   ├── issue-list.tsx, issue-detail-view.tsx, issue-timeline.tsx, issue-search-sheet.tsx
│   ├── issue-filter-bar.tsx, issue-filter-popover.tsx, active-filter-pills.tsx
│   ├── create-issue-dialog.tsx, create-board-dialog.tsx, create-team-dialog.tsx
│   ├── diff-view.tsx, agent-session.tsx (EXP-63: custom-rendered steer/activity view over the steer relay — no xterm)
│   └── github-repo-picker.tsx, subscribe-toggle.tsx, …
├── db/                           # schema.ts (re-exports @exp/db-schema + auth-schema), connection.ts, out/ (migrations + custom/0001_triggers.sql)
├── hooks/                        # use-session, use-team-data, use-my-issues-data, use-board-view-data, use-team-permissions, …
├── lib/
│   ├── auth/                     # Better Auth: index.ts (server), client.ts (fetchSessionOnce), config.ts, membership.ts, policies.ts, shape-where.ts, app-user.ts
│   ├── collections.ts            # Electric collection definitions (all use snakeCamelMapper)
│   ├── shape-route.ts            # createShapeRouteHandler — shared auth-gated shape proxy builder
│   ├── filters.ts                # IssueFilters, tab presets, matchesFilters()
│   ├── trpc.ts / trpc-client.ts  # tRPC server setup / client hooks
│   ├── trpc/                     # Routers: issues, boards, teams, labels, issue-labels, comments, notifications, subscriptions, team-members, team-invites, users, push-tokens, integrations, billing, admin, onboarding, repositories, coding-sessions, widgets, helpdesk, steer, mcp-grants
│   ├── steer.ts                  # Pure core of the steer router: ticket claims, perm mapping, relay HTTP calls
│   ├── email.ts / email-unsubscribe.ts  # Single outbound-mail sender (Amazon SES or SMTP; no-op when neither) + signed unsubscribe tokens
│   ├── notification-email-policy.ts / notification-email-digest.ts  # Push-first notification email: push fires immediately on create, email is a DIGEST of notifications still unread at sweep time — DAILY by default since EXP-227, hourly as the legacy opt-in (atomic emailed_at claim, one email per user, sweep scheduled from server-bun.ts; no per-event notification emails)
│   ├── integrations/             # mentions, notifications, fcm, activity, github-app, github-pr, pr-sync, subscriptions
│   └── storage/                  # S3 attachments: issue-attachments, issue-image-upload, image-dimensions, cleanup
├── routes/
│   ├── _authenticated/           # account/notifications (email prefs), onboarding, admin/*, integrations/github/installed (GitHub App lives in team settings → Repositories)
│   ├── t/$teamSlug/              # route.tsx (layout), index, my-issues/, inbox/, support/ (helpdesk inbox), settings/, boards/$boardSlug/ (index + issues/$issueIdentifier full-page detail) — no legacy /w/ or /projects/ forms
│   ├── auth/login.tsx, auth/register.tsx, auth/consent.tsx (MCP OAuth scope picker), invite/$token.tsx
│   ├── api/shapes/               # 14 Electric shape proxies (see Patterns)
│   ├── api/trpc/$.ts             # appRouter
│   ├── api/auth/$.ts, api/auth-config.ts, api/mcp.ts, api/webhooks/github.ts
│   ├── api/attachments/$attachmentId.ts, api/issues/$issueId/images.ts
│   └── api/mobile-oauth-start.ts / -return.ts, api/integrations/github/setup.ts, api/email/unsubscribe.ts, api/health.ts
├── router.tsx, start.tsx (defaultSsr: false), server.ts / server-bun.ts
└── styles.css                    # Tailwind v4 + shadcn dark theme (zinc OKLCH)
```

## Database

### Key Conventions

- Better Auth user IDs are `text` (not UUID) — all FKs to users must be `text` type
- All app tables use UUID PKs via `gen_random_uuid()`
- All tables have `created_at` / `updated_at` timestamps (with timezone)
- Sort order fields use `doublePrecision` (float8) for fractional indexing
- Rich text fields (issue description, comment body) are plain `text` GFM markdown
- Due date uses `date` type (no time component)

### Tables

`teams`, `boards` (repo optional — `repository_id` nullable, FK restrict), `issues`, `labels`, `issue_labels`, `comments`, `attachments`, `coding_sessions` (issue XOR batch XOR action subjects — action rows carry `action_id` [FK actions, set null] + the `action_name` display snapshot), `repositories`, `actions` (EXP-253 — per-team markdown action prompts), `github_installations`, `github_installation_links` (team ↔ installation claims), `team_members`, `team_invites`, `fcm_tokens`, `notifications`, `issue_subscribers`, `issue_events`, `support_threads`/`support_messages` (standalone helpdesk tickets — server-only), `user_notification_prefs`, `email_deliveries`, `email_bounces` (per-address SES bounce/complaint feedback — server-only, admin console), `widget_configs`, `widget_submissions`, `mcp_grants` + Better Auth tables (users, sessions, accounts, verifications, apikeys, oauth_applications/access_tokens/consents)

### Key Issue Fields

`id`, `boardId`, `number`, `identifier`, `title`, `description` (text, GFM), `status`, `priority`, `assigneeId`, `creatorId` (NULLABLE, FK `set null` — widget-filed issues have no user creator), `source` (`user`/`widget` enum, default `user`), `dueDate`, `sortOrder`, `completedAt`, `archivedAt`, `createdAt`, `updatedAt`, `duplicateOfId` (self-FK, pairs with status `duplicate`), plus PR fields `prUrl`/`prNumber`/`prState`/`branch`/`prMergedAt` (one PR per issue — usually one `exp/<IDENTIFIER>` branch; the issues of a batch coding run share ONE combined PR on `exp/batch-<id8>`, so several issues may carry the same `prUrl`). Key board fields: `name`/`slug`/`prefix`/`color`/`icon`, nullable `repositoryId`, plus trash fields `deletedAt` + `isProtected` (see Board trash below). Key team fields: `name`/`slug`/`iconUrl`, server-only `compTier`, and synced `helpdeskEnabled` (the team-level helpdesk switch every client gates its Support entry on)

### Enums

`issue_status` (backlog/todo/in_progress/in_review/done/cancelled/duplicate — `in_review` is the coding-flow parking spot: `pr_open` flips linked issues to it, the PR merge completes them to `done`), `issue_priority` (none/urgent/high/medium/low), `issue_source` (user/widget — where an issue came from; widget rows pair with a null creator), `notification_type` (incl. pr_opened/pr_merged/support_reply), `team_member_role` (owner/member), `pr_state`, `coding_session_status` (running/in_review/ended — `in_review` = PR open, terminal alive awaiting review, EXP-194), `issue_event_type` (incl. board_moved), `subscriber_source` (incl. widget_reporter) — canonical values live in `packages/domain-contract/contract.json`. Support-thread `status` (open/resolved) and message `direction`/`visibility` are documented varchars (server-only vocabulary in `domain.ts`, not the contract)

### Custom Triggers (0001_triggers.sql)

- `generate_issue_number()` — auto-increments `number` per board, sets `identifier` as `{prefix}-{number}`
- `update_updated_at()` — auto-updates `updated_at` on all tables
- `populate_issue_label_team_id()` / `populate_issue_child_team_id()` — denormalize `team_id` onto issue_labels / issue_subscribers / issue_events / coding_sessions so member shape filters stay team-scoped (the child populates no-op when `issue_id` IS NULL — batch-scoped coding_sessions rows carry an explicitly-written team_id and a NULL board_id)
- `populate_issue_child_board_id()` — denormalizes `board_id` + `board_deleted_at` onto comments / attachments / issue_events / issue_subscribers / coding_sessions / issue_labels / notifications (fires on INSERT and on `UPDATE OF board_id`, so `issues.move` re-derives both)
- `populate_issue_board_context()` — denormalizes `team_id` + `board_deleted_at` onto issues from the parent board (REV2-5: makes the issues shape team-scoped and trash-aware without per-user board-id lists)
- `propagate_board_deleted_at()` — fans a board's `deleted_at` out to every child row's `board_deleted_at` mirror on trash/restore; the `update_updated_at` triggers on those tables carry a WHEN guard so the fan-out never bumps `updated_at`

## Patterns

### Electric Shape Proxies

Each synced table gets a shape proxy in `apps/web/src/routes/api/shapes/`, built with the shared `createShapeRouteHandler` (`lib/shape-route.ts`). The proxy authenticates the request, then forwards to Electric. Every proxy pins a server-side `columns` allowlist that clients cannot widen (REV2-5 made this universal for the board-scoped shapes) — `issue-subscribers` uses this to EXCLUDE the reporter `email` column from sync (widget-reporter PII stays server-only; the resolution-email path reads the DB directly), `notifications` excludes `emailed_at` (the email-digest sweep's claim stamp — delivery bookkeeping, not inbox state), `board_id`, and `board_deleted_at` (trash-scoping bookkeeping the where clause filters on — Electric evaluates `where` server-side, so a shape may filter on a column its allowlist excludes), the 8 board-scoped shapes all exclude the REV2-5 scoping columns (`board_deleted_at` everywhere, plus `team_id` on `issues` — native schemas don't carry them), `users` is pinned to exactly the 6 columns every client stores (`id,name,email,image,created_at,updated_at` — the server-only billing/admin/onboarding columns must NEVER sync: native schemas don't have them, and a partial update touching an unknown column used to brick native sync loops; `is_agent` was removed entirely when the synthetic widget-user pattern was dropped), `teams` pins its contract list (incl. `helpdesk_enabled`; the server-only `comp_tier` never syncs), and `boards` pins its full contract column list (incl. `deleted_at` + `is_protected`, the client trash/protection signals). When adding a server-only column to a synced table, add it BEHIND the allowlist — never let it reach the wire. Client collections in `apps/web/src/lib/collections.ts` point to these proxy URLs. There is one proxy per synced table — teams, boards, issues, labels, issue-labels, users, team-members, team-invites, comments, attachments, notifications, issue-events, issue-subscribers, and coding-sessions (14 total, matching the 14 synced shapes). Every shape is member-only: anonymous requests resolve to the impossible-match sentinel.

Proxies are hardened three ways: (1) proxied responses always carry `cache-control: private, no-store` + `vary: authorization, x-api-key, cookie` — Electric's upstream `public, max-age=604800` must never reach auth-gated clients (it poisoned macOS URLCache with cross-auth snapshots); (2) a request presenting token credentials (`Authorization`/`x-api-key`) that fail to resolve gets an explicit 401 instead of degrading to the anonymous where clause (cookie-only requests still fall back anonymously — the web collection layer has no 401 recovery and the router guard re-auths on navigation); (3) `buildWhereClause` sorts id lists so the same id set always yields byte-identical SQL — the where clause is part of Electric's shape identity, and heap-order flips were rotating shape handles into native-client 409 loops; on top of that, REV2-5 keeps membership id lists OUT of the board-scoped where clauses entirely (team-scoped + the static `board_deleted_at IS NULL` predicate via `buildTeamScopedChildWhere`), so shape identities survive board create/trash/restore and only rotate on real team-membership changes. The proxy additionally bounds concurrent initial-snapshot (`offset=-1`) forwarding with a FIFO semaphore (`electric-proxy.ts` — snapshot bodies are fully buffered in Bun memory for Traefik-safe framing, so a cold-start herd must queue instead of ballooning the heap; live long-polls are never gated).

### Board trash (48h soft delete)

`boards.delete` is a SOFT delete: it stamps `deleted_at` (owner-only; refuses `is_protected` rows). Trashed boards vanish from all read surfaces via the trash-aware helpers in `lib/auth/membership.ts`, from the boards shape via its static `"deleted_at" IS NULL` where suffix, and their children from the board-scoped shapes via the trigger-fanned `board_deleted_at` mirror (REV2-5 — incremental move-out deltas, no shape-identity rotation) — but keep their `(team_id, slug)` reservation; `boards.restore` / `boards.listDeleted` (owner-only) power the web-only "Pending deletion" card in team settings (renders nothing when empty). The purge sweep (`lib/board-trash.ts`, started from `server-bun.ts`, retention constant in `@exp/db-schema/domain`) hard-deletes after 48h and reclaims the attachment blobs from S3 — as do `teams.delete` and `admin.deleteWorkspace`'s successor (collect `attachments.storage_key` in-tx before the cascade, `deleteStorageObjects` after commit). `is_protected` is bootstrap-stamped onto the dogfood Exponential board (non-deletable, non-archivable — server-guarded in `boards.delete`/`update`; clients disable the affordance from the synced flag). Widgets router mutations (`create`/`update`/`delete`/`list`) are team-owner-only, as is the team helpdesk toggle (`teams.update({helpdeskEnabled})` — enabling is Pro-gated). `use-team-permissions.ts` exposes `isOwner` + capability booleans — the contract the native apps mirror (owner-only controls are HIDDEN for non-owners on all clients, and every destructive native action confirms first).

### Electric Collections

All collections in `apps/web/src/lib/collections.ts` use `columnMapper: snakeCamelMapper()` from `@electric-sql/client` to map Postgres `snake_case` columns to JS `camelCase`. Without this, `useLiveQuery` `where` filters silently fail. Use `undefined` (not `false`) to skip a query; use `and()`/`or()` from `@tanstack/react-db` instead of JS `&&`/`||`.

### Auth Guard

`_authenticated.tsx` uses `beforeLoad` with `throw redirect()` to gate routes. The session is fetched once via `fetchSessionOnce()` (`lib/auth/client.ts`) and cached to avoid re-fetching on every navigation.

### tRPC + Electric Sync

Mutations go through tRPC. `generateTxId` captures the Postgres transaction ID so the client can wait for Electric to sync the write before updating the UI. Routers are modular in `apps/web/src/lib/trpc/` and combined in `api/trpc/$.ts` as `appRouter`.

### MCP OAuth consent scoping

Human MCP clients (Claude etc.) authenticate against `/api/mcp` via the Better Auth `mcp` plugin's OAuth flow. Every `mcp/authorize` request is pre-flighted by `lib/auth/mcp-authorize-guard.ts` (via the `/api/auth/$` route): unknown/stale client registrations and redirect-URI mismatches get an actionable HTML error page (better-auth's default silently bounces to `/`), and `prompt=consent` is forced so the flow always lands on `/auth/consent` — a team/board multi-select ("Everything" or per-team/per-board) persisted to `mcp_grants` (one row per user+client, upserted on re-consent) by `trpc.mcpGrants.grantAndConsent` BEFORE the code is minted. The MCP tool layer (`lib/mcp/scope.ts` + checks in `lib/mcp/tools.ts`) confines OAuth tokens to the granted scope; a token with NO grant row gets nothing. OAuth access tokens are accepted ONLY at `/api/mcp` (`resolveSession` no longer resolves them, so shapes/tRPC reject them); session cookies and personal `expu_` api keys keep full membership access there. The login page resumes an interrupted authorize by navigating back to the authorize URL and drops the plugin's `oidc_login_prompt` cookie (`lib/auth/oauth-resume.ts`) so the after-hook can't hijack the sign-in fetch into a mixed-content redirect.

### Issue List UX

- Issues are displayed in a CSS grid layout: `grid-cols-[24px_72px_24px_1fr_auto]` (priority, identifier, status, title, labels+due date)
- Rows are clickable to open the edit dialog; priority/status dropdowns use `stopPropagation` to prevent row click
- Empty status groups are hidden from the list
- Due dates display with a `CalendarDays` icon on the right side of rows

### Issue Filtering

- `apps/web/src/lib/filters.ts` defines `IssueFilters` (statuses, priorities, labelIds), tab presets (all/active/backlog), and `matchesFilters()`
- `IssueFilterBar` provides tab navigation and a filter popover button
- `IssueFilterPopover` offers multi-category drill-down filtering (status, priority, labels)
- `ActiveFilterPills` shows removable pills for active filters below the filter bar

### Issue Detail / Edit

- Issue rows navigate to the full-page detail route (`boards/$boardSlug/issues/$issueIdentifier`); the editor receives a live `issue` from Electric (stays fresh without refetching)
- Title and description use local state, save on blur if changed
- Status, priority, labels, and due date mutate immediately via tRPC
- Labels use `trpc.issueLabels.add` / `trpc.issueLabels.remove` for toggle behavior
- `completedAt` is auto-managed by the update mutation based on status changes

### Create Issue Dialog

- Supports title, description, status, priority, labels, and due date
- "Create more" checkbox keeps the dialog open and resets fields after creation
- Due date uses shadcn `Calendar` in a `Popover`

### UI Conventions

- All UI elements must use shadcn/ui components — no raw HTML `<input>`, `<button>`, `<textarea>`, `<label>` elements
- Borderless inputs inside dialogs: use `Input`/`Textarea` with `border-none shadow-none focus-visible:ring-0`
- Icon-only triggers in dropdowns: use `Button variant="ghost"` with `h-5 w-5 p-0`

## Environment Variables (.env)

```
DATABASE_URL                  # Postgres connection string (db name: exponential)
BETTER_AUTH_SECRET            # 32+ char secret for session signing
BETTER_AUTH_URL               # App base URL (http://localhost:5173)
BETTER_AUTH_TRUSTED_ORIGINS   # Comma-separated allowed origins
ELECTRIC_URL                  # Electric service URL (http://localhost:30000)
ELECTRIC_SOURCE_ID            # Electric Cloud source id (optional; unset for local/self-hosted)
ELECTRIC_SECRET               # Electric Cloud source secret (optional)
S3_ENDPOINT                   # S3-compatible storage URL (Garage default: http://localhost:3900)
S3_ACCESS_KEY                 # S3 access key (created by `bun run storage:init`)
S3_SECRET_KEY                 # S3 secret key (created by `bun run storage:init`)
S3_BUCKET                     # S3 bucket for attachments (default: exponential-attachments)
S3_REGION                     # S3 region label (default: garage)
AUTH_PASSWORD_ENABLED         # Enable email/password login (default: true)
AUTH_SIGNUP_ENABLED           # Public password sign-up ('true'/'false'; default: on in dev, OFF in production)
AWS_SES_REGION                # Amazon SES region (e.g. eu-central-1) — enables password reset + email verification (unset = email flows off)
AWS_ACCESS_KEY_ID             # AWS credentials for SES (IAM user with ses:SendEmail; standard AWS env chain)
AWS_SECRET_ACCESS_KEY         # AWS secret key for SES
EMAIL_FROM                    # Verified sender, e.g. "Exponential <notifications@exponential.at>" (domain verified in SES)
EMAIL_REPLY_TO                # Monitored default Reply-To on every outbound email (per-send replyTo wins; unset = none)
SMTP_HOST                     # SMTP alternative to SES for ALL outgoing mail (self-host; SES wins when both set)
SMTP_PORT                     # SMTP port (default 587)
SMTP_USER                     # SMTP auth user (optional)
SMTP_PASS                     # SMTP auth password (optional)
SMTP_SECURE                   # 'true' for implicit TLS (port 465)
OIDC_PROVIDERS                # JSON array of OIDC providers — the primary OIDC mechanism (see .env.example)
# Legacy single-provider OIDC (used only when OIDC_PROVIDERS is unset):
AUTH_OIDC_ENABLED             # Enable legacy single-provider OIDC (default: false)
OIDC_CLIENT_ID                # OAuth2 client ID
OIDC_CLIENT_SECRET            # OAuth2 client secret
OIDC_DISCOVERY_URL            # OIDC discovery endpoint URL
OIDC_PROVIDER_ID              # Provider ID for Better Auth (default: authentik)
GOOGLE_CLIENT_ID              # Google OAuth client ID (required for Google login)
GOOGLE_CLIENT_SECRET          # Google OAuth client secret
GITHUB_APP_ID                 # GitHub App numeric ID (connect from team settings → Repositories; server mints per-repo installation tokens; installations are claimed PER TEAM via github_installation_links — rows mirror in from the setup redirect, the OAuth claim callback, and installation webhooks)
GITHUB_APP_SLUG               # GitHub App URL slug (builds the install link)
GITHUB_APP_PRIVATE_KEY        # GitHub App PEM private key, base64-encoded (base64 -w0 app.private-key.pem)
GITHUB_APP_CLIENT_ID          # GitHub App OAuth client ID — team "Connect GitHub" claim flow (transient user token, never stored)
GITHUB_APP_CLIENT_SECRET      # GitHub App OAuth client secret (unset ⇒ connect falls back to the install-page round-trip)
GITHUB_WEBHOOK_SECRET         # GitHub App webhook HMAC secret (cloud PR-merge detection; App webhook → ${BETTER_AUTH_URL}/api/webhooks/github)
SES_WEBHOOK_SECRET            # Shared secret for the SES bounce/complaint SNS webhook (SNS HTTPS subscription → ${BETTER_AUTH_URL}/api/webhooks/ses?secret=…; unset = bounce tracking off)
GITHUB_POLLING                # 'true' to run the outbound merge cron (self-hosted behind NAT, unreachable by webhook); decoupled from SELF_HOSTED
GOOGLE_LOGIN_ENABLED          # Show "Sign in with Google" on login/register (default: false)
APPLE_CLIENT_ID               # Sign in with Apple *Services ID* (App Store guideline 4.8 — required alongside Google login on iOS)
APPLE_PRIVATE_KEY             # SIWA .p8 key, base64-encoded — server mints the ≤6-month client-secret JWT fresh at every boot
APPLE_KEY_ID                  # Key ID of the SIWA .p8 key
APPLE_TEAM_ID                 # Apple Developer team ID
APPLE_CLIENT_SECRET           # Optional static ES256 client-secret JWT — wins over key-based minting when set
APPLE_LOGIN_ENABLED           # Show "Sign in with Apple" on all clients (default: false)
APPLE_APP_BUNDLE_IDENTIFIER   # Optional: app bundle id for native SIWA idToken exchange (at.exponential)
SELF_HOSTED                   # 'true' for self-hosted (disables billing, unlocks plan limits)
CREEM_API_KEY                 # Creem billing API key (cloud-only)
CREEM_WEBHOOK_SECRET          # Creem webhook signing secret (cloud-only)
CREEM_PRO_PRODUCT_ID          # Creem product ID for Pro ($5/seat/mo, billed yearly only)
CREEM_BUSINESS_PRODUCT_ID     # Creem product ID for Business monthly ($10/seat/mo)
CREEM_BUSINESS_YEARLY_PRODUCT_ID # Creem product ID for Business yearly
PUSH_RELAY_URL                # URL of the push-relay service (e.g. https://push.yourapp.com)
PUSH_RELAY_SECRET             # Shared secret between web app and push relay
STEER_RELAY_URL               # URL of the steer relay (unset = remote start/steer off; LAN URLs fine — all connections dial OUT)
STEER_RELAY_SECRET            # Shared HS256 secret: web mints steer tickets, relay verifies (must match the relay process env)
ANDROID_APP_LINK_FINGERPRINTS # Comma-separated SHA-256 cert fingerprints for /.well-known/assetlinks.json (Android App Links; unset = 404 and links open in the browser)
SECURITY_HEADERS_ENABLED      # 'true' to emit CSP/HSTS etc. from the Bun server
BUN_CONFIG_MAX_HTTP_REQUESTS  # Bun RUNTIME cap on simultaneous outbound fetches (default 256; read by Bun at process start, not app code). Baked as 65336 into the web Docker image — 14 Electric long-poll proxies per synced client saturate the default at ~18 clients and stall ALL other outbound fetches (REV2-6)
INITIAL_ADMIN_EMAILS          # Comma-separated emails auto-promoted to global admin at startup
CLIENT_MIN_VERSION_ANDROID    # Min Android client version — below it tRPC/shape requests answer HTTP 426 (blocking update screen); unset = gate off (fail-open). ALL CLIENT_*_VERSION_* values are marketing versions (versionName / CFBundleShortVersionString / desktop tag, e.g. 0.13.3), NEVER versionCode or build number
CLIENT_MIN_VERSION_IOS        # Min iOS client version (same 426 gate)
CLIENT_MIN_VERSION_DESKTOP    # Min desktop client version (same 426 gate)
CLIENT_LATEST_VERSION_ANDROID # Informational latest Android version (GET /api/version + 426 body; no blocking)
CLIENT_LATEST_VERSION_IOS     # Informational latest iOS version
CLIENT_LATEST_VERSION_DESKTOP # Informational latest desktop version
WIDGET_RATE_LIMIT_PER_KEY_HOURLY # Widget submit limit per public key (default 60/h, burst 10 via WIDGET_RATE_LIMIT_KEY_BURST)
WIDGET_RATE_LIMIT_PER_IP_HOURLY  # Widget submit limit per client IP (default 60/h, burst 5 via WIDGET_RATE_LIMIT_IP_BURST)
WIDGET_RATE_LIMIT_PER_EMAIL_HOURLY # Widget submit limit per reporter address (default 6/h, burst 3 via WIDGET_RATE_LIMIT_EMAIL_BURST) — anti mail-bombing
CONTACT_EMAIL_TO              # Recipient of POST /api/contact (marketing contact form; default support@exponential.at; 503 when no email transport is configured)
```

(The relay processes have their own env — see `apps/push-relay/.env.example` and `apps/steer-relay/.env.example`: the push relay reads `FIREBASE_SERVICE_ACCOUNT_JSON`, and BOTH relays read `TRUST_PROXY=true`, mandatory whenever a reverse proxy fronts them — Coolify/Traefik included — so per-IP rate limits key on the real client address instead of one shared fallback bucket.)

Dead env vars: `GOOGLE_CALENDAR_ENABLED` and `DOGFOOD_REPO` are still set in the live Coolify envs but are read by **no code** (calendar sync is fully excised; `DOGFOOD_REPO` survives only in bootstrap-cloud.ts comments) — safe to delete from the envs, do not re-document or re-introduce.

## Integrations

### Coding flow (v2 — "Start coding" launcher)

The old Rust `agent-core` runtime, the companion daemon, the synthetic desktop-agent identity (`agent_registrations`, `role=agent`, `expk_` keys), the `agent_runs` plan/approval state machine, and the `assigned-issues` shape are **all deleted**. The coding flow is a thin **launcher inside the desktop IDE** (`apps/desktop`, Rust): resolve the issue's repo from the team `repositories` registry (tRPC) → mint a session-gated JIT GitHub-App installation token → create a git worktree + `exp/<IDENTIFIER>` branch with ambient git auth via a repo-local credential helper (EXP-73: `origin` stays the bare URL; the token lives in `<clone>/.git/exp-git-credentials` with a no-downgrade guard, kept fresh by an expiry-scheduled refresher — the server returns GitHub's REAL `expires_at`, and every writer including the git-bar sync worker goes through `coding::git_credentials::ensure`, so a stale cached token can never clobber a fresh one mid-run) → write `.exp-mcp.json` pointing at the web `/api/mcp` (authenticated with the user's personal `expu_` Better Auth apikey; EXP-98: deliberately NOT named `.mcp.json` — claude's project-approval dialog scan of a cwd `.mcp.json` is unconditional and ignores `--mcp-config`/`--strict-mcp-config`, so the config rides `--mcp-config .exp-mcp.json` instead, and the launcher deletes stale launcher-written `.mcp.json` files from pre-fix worktrees) → spawn the chosen agent CLI in the embedded terminal (alacritty_terminal-backed), seeded with a plan-first prompt. **EXP-201: three selectable agents** (`coding/src/agent.rs` `CodingAgent`, contract `codingAgent` + per-agent model/effort lists; a savable default agent + per-agent path/model/effort in settings): **claude** (default — `--permission-mode auto` guarded default, `--dangerously-skip-permissions` via the skip checkbox, `--permission-mode plan` for plan mode; `.exp-mcp.json` MCP), **codex** (OpenAI Codex CLI — explicit Auto preset `--sandbox workspace-write --ask-for-approval on-request -c sandbox_workspace_write.network_access=true` or `--dangerously-bypass-approvals-and-sandbox`; MCP via `-c mcp_servers.*` argv overrides with the `expu_` key riding ONLY the spawn env `EXP_MCP_TOKEN`), and **pi** (pi.dev — no permission system, no native MCP: the launcher writes a `.exp-pi-mcp.ts` extension (`coding/src/pi_bridge.rs`) loaded via `-e` that bridges the exponential MCP tools over streamable HTTP; url+token ride `EXP_MCP_URL`/`EXP_MCP_TOKEN`). Ultracode + plan mode stay Claude-only; the doctor probes all three but only gates the SELECTED agent (git always required), and the steer `online` frame advertises the installed agents so remote Start-coding pickers (web/iOS/Android) only offer what the chosen device can run (`steer.startSession` validates agent + per-agent vocabulary server-side; absent agent = claude, keeping old clients byte-compatible). The agent commits, pushes, and opens its own PR via the MCP `open_pr` tool (the server opens + links the PR through the GitHub App). Local deps are `git` + whichever agent CLIs you use — never `gh`. A slim synced `coding_sessions` row (`running`/`in_review`/`ended`) powers the cross-client "coding now" badge. The person coding is the **real signed-in user**. The single plan of record is `docs/masterplan.md` (v5 release — §2 locked decisions L19–L31, §3 per-seat billing, §12 execution P0–P5, §13 release checklist); superseded deep specs live in `docs/archive/` (masterplan-v2 server/relay, masterplan-v3 gpui desktop, masterplan-v4 project=repo + git IDE).

**Billing (per-seat)**: subscriptions bind to a TEAM (`creem_subscriptions.team_id` + `seats`; checkout via `trpc.billing.createSeatCheckout` with Creem `units` — units is the authoritative seat count, never client metadata). ONE subscription per team: `createSeatCheckout` refuses when an active team-bound subscription exists; seat changes go through `billing.updateSeats` and tier/cadence switches through `billing.changePlan`, both mutating the EXISTING Creem subscription (update/upgrade endpoints) with `update_behavior: proration-none` — Creem's proration is measurably broken on increases (charges new+old instead of the delta; evidence + flip-when-fixed constant in `apps/web/src/lib/billing/creem-subscriptions.ts`), so changes apply immediately and bill at the next renewal. Free = 1 seat, 250MB attachment storage/team, 1 widget config (EXP-180); Pro = $5/seat/mo yearly-only, 2GB, 3 widget configs, helpdesk; Business = $10/seat/mo (monthly or yearly), 10GB, unlimited widgets, helpdesk. The in-app plan comparison additionally shows a display-only Enterprise "Contact us" column (SSO/OIDC coming soon, SLA) — not a purchasable `PlanTier`. Unlimited boards/repos/coding sessions on EVERY tier; push + steer never plan-gated; over-seat teams only block new invites (never lock members out). The helpdesk (team-level `helpdeskEnabled`) is Pro-gated; widget creation is capped per tier, never fully gated. `SELF_HOSTED=true` ⇒ every FEATURE limit unlocked (billing off, no seat/storage/widget caps) — that env var is a product switch only, and says nothing about the licence: the repo is source-available under the Exponential Small Team License 1.0 (`/LICENSE`), which makes self-hosting free only while your company and its affiliates have fewer than 10 total people; 10 or more needs a commercial licence (support@exponential.at). Never call the project open source, and never build a code gate for the headcount cap — it is contract-only. Default-branch rows are resolved live from GitHub at connect time and healed on `repositories.list`/`installationToken` (backfill: `bun run backfill:default-branches` in apps/web) — never assume `main`.

**Actions (EXP-253 — replaced run configs)**: `actions` (DB, per team, markdown prompt `body` ≤64KB, optional `repository_id` SET NULL) is server-only — tRPC CRUD (`actions.*`; member list/get, owner writes) + 4 MCP tools (`exponential_actions_*`), never Electric-synced (proxy count stays 14). Runs are `coding_sessions` rows (`action_id` + `action_name` snapshot) executed LOCALLY by the desktop as interactive Claude sessions (Claude-only v1, model/effort options) on the repo's trunk clone (autopulled) or a scratch dir (`<data_dir>/actions/<id>/`) — no worktree/branch/PR, no server-side secrets ever (actions run under the user's own device auth; CI secrets stay in GitHub Actions). Every run is gated by the per-device `sha256(body)` trust prompt (`action_trust` in `run_trust.sqlite`, fail-closed, re-checked against the freshly fetched body; remote starts foreground the dialog). Remote start rides the steer rails: devices advertise `caps: ["actions"]` in the `online` frame and `steer.startSession({actionId, deviceId, model?, effort?})` strictly gates on it. All four clients have an Actions surface (web `t/$teamSlug/actions` + desktop rail tool window: list/run/create/edit; iOS/Android: view + run). The "Describe with Claude" creator is the ONE claude_task variant with a scoped `.exp-mcp.json` (conflict-fix tasks stay MCP-less); the 3 default actions ship as TEMPLATES in the New-action flow, never seeded rows. The desktop IDE is master-only + autopull (EXP-253): no TopBar (boards are rail icons; team switching in the account menu), no branch switching/commit/push (view-only editor — changes arrive via PRs; the one escape hatch is Discard-and-reset to origin/default), and the git bar became the headless `trunk_sync` engine with a status badge on the Source Control rail icon. Mobile apps ship **full onboarding**: a server-gated first-run wizard (`onboardingCompletedAt` from `lib/auth/onboarding.ts`) that starts with the EXP-188 create-or-join team step (create: name → `teams.create`; join: paste invite link → `teamInvites.accept`, which completes onboarding and skips the board step) followed by guided create-first-board — optional repo picker + inline GitHub App connect (browser/Custom Tab install hop) — plus regular in-app board creation, team creation, and repo management; there is still no account-level Integrations menu (GitHub connect lives in the repo-picker/team-settings flow); action EDITING is web/desktop-only (mobile = view + run). Mobile store deploys use fastlane (`apps/android/fastlane`, `apps/ios/fastlane`; docs/release-android.md, docs/release-ios.md). Desktop releases publish to GitHub Releases on `desktop-v*` tags (production channel only, SHA256SUMS; macOS notarization secret-gated) and the app self-updates from `releases/latest` (EXP-22 — see the Deploys → Desktop bullet; staging builds never check). Do NOT run `bun run lint` — its --fix corrupts `typeof import()` sites (EXP-13).

### Embeddable Feedback Widget

A marker.io-style widget third parties paste into their sites via a GA-style async `<script>` snippet. Source lives in `packages/widget` (Preact in a shadow root + `@zumer/snapdom` for client-side screenshots); the build emits two classic IIFE scripts into `apps/web/public/widget/v1/` — `loader.js` (snippet target: command queue, floating button, config prefetch) and `widget.js` (lazy-loaded panel + capture), plus `demo.html` for manual testing. The widget API is `window.ExponentialWidget`: `init({key})`, `identify({email,name,userId})`, `setCustomData({...})`, `open()`, `close()`. Screenshots are captured client-side (snapDOM, viewport-cropped, WebP→PNG→JPEG ladder, never blocks submission on failure); marker.io's server-side rendering approach was researched and deliberately not used (capture sits behind a `CaptureEngine` interface in `packages/widget/src/capture/` if that ever changes). Screenshots can be **annotated** in a full-screen editor (rectangle / free line / arrow, fixed red, undo/clear): shapes live in image-pixel space (`packages/widget/src/annotate/`), stay editable across editor reopens, and are flattened into the uploaded image on submit (`flattenAnnotations` re-runs the encode ladder) — the server only ever sees one plain image.

Server side: two server-only tables (`widget_configs` with the public `expw_` key + domain allowlist, `widget_submissions` with reporter email/page/env metadata — NOT Electric-synced, read via the `widgets` tRPC router), two CORS-handling public routes (`/api/widget/config`, `/api/widget/submit` — plain file routes; CORS/origin/rate-limit/honeypot helpers in `apps/web/src/lib/widget/`). Widget **modes** are `feedback`/`support`/both (`form_config.modes`; absent = feedback-only). Feedback mode files an ordinary issue onto the config's target board (`widget_configs.board_id`, NULLABLE — required iff modes include feedback; FK `set null` so trashing the board degrades feedback instead of deleting the config). Support mode (EXP-180) files a STANDALONE helpdesk ticket — a `support_threads` row + opening `support_messages` row, NO issue — into the team's Support inbox; it gates on the team-level `teams.helpdesk_enabled` switch + the Pro+ plan (`assertCanUseHelpdesk`), re-checked per submit. The reporter's only credential is an emailed magic link (deterministic HMAC over the thread id — `lib/helpdesk/token.ts`; nothing secret at rest); members reply/note/close/reopen via the `helpdesk` tRPC router (team-scoped `listThreads`, thread-status close/reopen, and `escalate` which files a linked issue onto a chosen board via `linked_issue_id`). Members are notified via the issue-less `support_reply` notification fan-out (`fireAndForgetSupportThreadNotify` → `deliverToWorkspace`-style insert with `issue_id NULL`). Widget feedback issues have **no user creator**: `issues.creator_id` is NULLABLE (`ON DELETE SET NULL`) and widget-filed rows carry `creator_id = NULL` + `issues.source = 'widget'` (the enum default is `user`); clients key a "Feedback widget" origin off `source`. The old synthetic per-widget `isAgent` user is GONE (the `users.is_agent` column, the `widget_configs.widget_user_id` column, and `lib/widget/widget-user.ts` were all removed). Feedback submissions create the issue + screenshot attachment (also null `uploader_id`) + submission row in ONE transaction; support submissions create the thread + submission row (`widget_submissions.support_thread_id`). Rate limiting is in-process token buckets (`WIDGET_RATE_LIMIT_*` env; `SUPPORT_RATE_LIMIT_*` for the reporter routes). Managed in team settings → "Feedback widget" (owner-only, copy-paste snippet; the team Helpdesk toggle card lives on the same page). Dogfood: cloud bootstrap creates the `Exponential App` config on the feedback team with ONE bootstrap board — Exponential (slug `exponential`, prefix `EXP`, private + protected, repo-backed) — flips the team's `helpdesk_enabled` on, un-protects the retired legacy `support` board (its historical ticket-issues stay), one-shot-heals config modes to `[feedback, support]` (only while `form_config.modes` is absent), and comps the feedback team to `business` (only while `comp_tier` IS NULL). The dogfood mount (`FeedbackWidgetProvider` in the team layout) shows the floating launcher like any customer site, pinned `bottom-right` because the app sidebar occupies bottom-left (EXP-163); the sidebar FeedbackButton ("Feedback & support") opens the same widget and renders NOTHING when no widget is configured (self-hosted — the dogfood `expw_` key is domain-allowlisted to exponential.at/app.exponential.at, and there is no legacy `/feedback` redirect anymore).

Dev-mode gotcha: the nitro-alpha dev bridge renders any app 404-status response as a connect `Cannot GET/POST` HTML page and strips custom response headers — both dev-only; production (`server-bun.ts`/srvx) passes responses through untouched.

## Style Conventions

- Template literals for strings (backticks, not quotes)
- Functional components, no class components
- shadcn/ui components in `src/components/ui/` — always use these over raw HTML elements
- Icons from `lucide-react`
- Business logic components in `src/components/` (not in `ui/`)

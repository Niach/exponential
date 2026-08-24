# shots/ — the committed screenshot store (EXP-566)

Every product view, photographed on every platform, committed as optimized webp:
`shots/<view-id>/<platform>.webp` plus a generated `shots/index.json`. The view
inventory lives in `packages/view-catalog/views.json`; the styleguide app
(`bun run dev:styleguide`) browses this store, and the marketing docs embed it
directly. **This directory deliberately supersedes EXP-348** ("screenshots stay
out of git") — the writer only rewrites a file when the image visibly changed
(pixel-diff tolerance per view), so a run over an unchanged product leaves
`git status shots/` empty.

## The one command

```bash
bun run shots                       # capture DEFAULT_PLATFORMS (web, web-mobile, desktop, ios, android)
bun run shots -- --platform web,web-mobile,desktop   # what the automation runs unattended
bun run shots -- --views board,issue-detail          # subset of views
bun run shots -- --skip-seed --skip-relay            # reuse an already-seeded stack
bun run shots -- --force            # rewrite even visually-unchanged files
bun run shots -- --dry-run          # decide + report, touch nothing
bun run shots -- --prune            # delete store files the catalog no longer claims
bun run shots -- --up               # docker compose --profile steer up -d first
```

The orchestrator (`packages/shots/src/capture-all.ts`) preflights everything it
needs, seeds the demo team, starts the steer-relay stub, runs each platform's
capture lane, imports the native outputs, and writes the store + index. It exits
non-zero if any in-scope view failed, after finishing everything else.

## Prerequisites

- Backend stack: `docker compose --profile steer up -d` (postgres, electric,
  garage, caddy, steer-relay). **Caddy is load-bearing** — captures go through
  `https://localhost:3000` (h2), because Electric's long-polls starve Chromium's
  HTTP/1.1 connection limit.
- The web app serving on `:5173`. `bun dev` works where dev mode works; on
  machines where it doesn't, build once (`cd apps/web && bun run build`) and run
  `PORT=5173 bun .output/server/index.mjs` with the `.env` vars exported.
- `apps/web/.env` as a REAL file (dotenvx injects nothing through a symlink)
  with at least `DATABASE_URL`, `BETTER_AUTH_*`, `ELECTRIC_URL`,
  `STEER_RELAY_URL=ws://localhost:4002`, `STEER_RELAY_SECRET` matching the
  compose relay (`dev-steer-secret` by default).
- iOS: Xcode + the Snapfile's simulators (iPhone 17 Pro Max, iPad Pro 13-inch)
  and `bundle install` under `apps/ios`. Run lanes with `LC_ALL=en_US.UTF-8`.
- Android: an English-locale phone emulator booted, exactly one device attached.
  For the emulator the relay URL must be the host LAN IP — the orchestrator
  resolves that itself via `ipconfig getifaddr en0`.
- Desktop: a release binary (`cd apps/desktop && cargo build --release -p app`;
  found via `$CARGO_TARGET_DIR/release/exp-desktop` or the workspace target dir,
  override with `--app-binary`), plus a one-time macOS **Screen Recording**
  grant for the terminal running the capture (the preflight probes it and
  prints instructions when missing).

## Per-platform lanes

| platform   | lane                                                                 |
| ---------- | -------------------------------------------------------------------- |
| web        | `cd apps/web && bun run capture:views -- --form-factor web`          |
| web-mobile | same, `--form-factor web-mobile` (390×844@3x, mobile layout)         |
| desktop    | `packages/shots/src/capture-desktop.ts` — launches the gpui app per view via the `EXP_DEV_*` overrides and `screencapture`s the window; views marked `manual` in the catalog are skipped (fill them with `--manual <view-id>` while the wanted state is on screen) |
| ios        | `cd apps/ios && bundle exec fastlane screenshots && bundle exec fastlane styleguide_screenshots` |
| android    | `cd apps/android && bundle exec fastlane screenshots && bundle exec fastlane styleguide_screenshots` |
| ipad       | captured by the iOS store lane; opt in with `--platform ipad`         |

Raw PNGs land in the gitignored `.shots-raw/<platform>/<view-id>.png`; only the
encoded webp store is committed.

## How the diff-skip works

The candidate is encoded to webp first, then compared against the committed
webp (encoded-vs-encoded, so encoder noise never counts): decode both to RGBA,
`pixelmatch` with a loose per-pixel threshold, and keep the existing file when
the changed fraction is within the view's `diffTolerance` (default 0.005 —
absorbs the seed's relative timestamps). `index.json` is re-derived from disk
with sorted keys and no timestamps, so it only changes when an image does.

## The automation

The team action **“Refresh app screenshots”** (trigger: `pr_merged`) runs
`bun run shots -- --platform web,web-mobile,desktop` on its bound device,
sanity-checks the diff, and pushes a `shots/`-only commit directly to master.
Native (ios/android/ipad) refreshes stay a manual pre-release step — unattended
simulator lanes are too slow and flaky. If `bun run test:shots` fails after a
merge, the just-merged PR added a route/view the catalog doesn't know: add the
`views.json` entry (or an `excludedRoutes` row) first.

## Troubleshooting

- **502 from https://localhost:3000** — nothing on `:5173`; start the web app.
- **Steering views show “Reconnecting…”** — the relay stub isn't running or its
  banner never appeared; check `STEER_RELAY_URL`/`SECRET` against the compose
  relay, and remember the emulator needs the LAN-IP form.
- **401s from capture login** — re-run `cd apps/web && bun run seed:screenshots`
  (it tears down and rebuilds the demo team; also confirm
  `BETTER_AUTH_TRUSTED_ORIGINS` includes `https://localhost:3000`).
- **Flat/dark desktop shots** — Electric hadn't synced before the shutter; the
  capturer retries once with extra delay, but a slow first run may need a rerun
  of just that platform.
- **`screencapture` permission errors** — grant Screen Recording to the
  terminal app in System Settings → Privacy & Security, then rerun.

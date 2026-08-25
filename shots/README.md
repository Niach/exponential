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
bun run shots -- --since auto        # only the views the diff since the last refresh can have moved
bun run shots -- --since origin/master~5   # …or since any ref
bun run shots -- --skip-seed --skip-relay            # reuse an already-seeded stack
bun run shots -- --force            # rewrite even visually-unchanged files
bun run shots -- --dry-run          # decide + report, touch nothing
bun run shots -- --write-only       # re-encode .shots-raw/ into the store; drive nothing
bun run shots -- --repos-root /Users/Shared/Exponential/repos   # where the desktop lane finds clones
bun run shots -- --prune            # delete store files the catalog no longer claims
bun run shots -- --up               # docker compose --profile steer up -d first
```

The orchestrator (`packages/shots/src/capture-all.ts`) preflights everything it
needs, seeds the demo team, starts the steer-relay stub, runs each platform's
capture lane, imports the native outputs, and writes the store + index. It exits
non-zero if any in-scope view failed, after finishing everything else.

## Only what the diff touched

`--since <ref>` (or `--since auto`, meaning "the last commit that touched
`shots/`") narrows the run to the `<view, platform>` pairs the changed files can
actually have moved, and skips a whole lane whose set comes out empty — a
web-only PR never launches the desktop app. When nothing is in scope the run
prints "Nothing to capture" and exits before preflight, in seconds.

The mapping lives in `packages/shots/src/affected.ts` and is deliberately
FAIL-SAFE: a path narrows to specific views only when the repo proves the
connection, and anything else widens to every view of every platform it could
belong to. Web is precise (route → `routeTree.gen.ts` → transitive import graph
of `apps/web/src`, type-only imports erased); desktop matches `crates/ui/src`
module names against view ids and drive values and widens on anything else; the
native lanes are whole-platform. A changed `views.json` entry and a view with no
stored image yet are always in scope.

Ask without capturing anything:

```bash
bun run shots:affected -- --since auto --platform web,web-mobile,desktop
bun run shots:affected -- --since HEAD~3 --json
```

It prints the per-lane view lists plus every path that widened a lane and why —
which is also how you find a rule that needs teaching (`IGNORED`, `BROAD`).

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
- Desktop: for the repo-backed views (`files`, `source-control`, `terminal`,
  `start-coding`, `settings-worktrees`) the machine needs the demo board's
  repository actually cloned, plus `git` and a signed-in agent CLI on PATH —
  the launcher gates its Start button on the doctor, so without one the shot
  carries a red "claude not found on PATH" footer. The app reads clones straight
  off disk at `<repos_root>/<owner>/<name>`, so nothing has to talk to GitHub:
  point `--repos-root` at a directory holding `acme/mobile-app`. Two panes
  (Tools, Worktrees) render that path verbatim — keep it username-free
  (`/Users/Shared/Exponential/repos`) so the committed store carries no
  developer's home directory.
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

Raw PNGs land in the gitignored `.shots-raw/<platform>/<view-id>.png`; only the
encoded webp store is committed.

## How a view is driven

**Web** views are a route plus an anchor, optionally preceded by a named recipe
from `apps/web/scripts/lib/view-recipes.ts` (open a popover, a dialog, a tab).
`web.auth` picks the identity: `demo` (the seeded team owner, the default),
`anonymous` for `/auth/*`, or `newcomer` — a second seeded account that is
verified but owns nothing, which is the only way `/onboarding` and
`/invite/$token` render instead of redirecting.

**Desktop** views are driven entirely through the app's DEV-ONLY `EXP_DEV_*`
family, one `drive` per view:

| drive | env | opens |
| ----- | --- | ----- |
| `tool`     | `EXP_DEV_TOOL`                      | a rail tool window (board, inbox, reviews, support, files, source-control) |
| `screen`   | `EXP_DEV_SCREEN`                    | a centre screen (`settings`, `actions`, `getting-started`, `issue:<id>`, `pr:<id>`) |
| `settings` | `EXP_DEV_SCREEN` + `EXP_DEV_SETTINGS` | one settings section |
| `dialog`   | `EXP_DEV_DIALOG`                    | one dialog, fired once from the render path after the state it needs resolves |
| `manual`   | —                                   | nothing: capture it by hand with `--manual <view-id>` |

A view may add `desktop.env` on top for the cases that are a drive PLUS a flag —
`EXP_DEV_FILTER=1` for the board's filter popover, `EXP_DEV_OPEN_SHELL=1` for a
docked terminal, `EXP_DEV_ACTIONS_TAB` for the Actions screen's tab.

Values may carry `$placeholders` resolved against the seeded database by
`bun run screenshots:ids`: `$APP-5` and friends are issue identifiers,
`$thread`, `$action`, `$device`, `$automation`, `$board` and `$team` name one
well-known seeded row each. An unresolvable placeholder SKIPS the view rather
than photographing whatever the app fell back to.

## How the diff-skip works

The candidate is encoded to webp first, then compared against the committed
webp (encoded-vs-encoded, so encoder noise never counts): decode both to RGBA,
`pixelmatch` with a loose per-pixel threshold, and keep the existing file when
the changed fraction is within the view's `diffTolerance` (default 0.005 —
absorbs the seed's relative timestamps). `index.json` is re-derived from disk
with sorted keys and no timestamps, so it only changes when an image does.

## The automation

The team action **“Refresh app screenshots”** (trigger: `pr_merged`) runs
`bun run shots -- --since auto --platform web,web-mobile,desktop` on its bound
device, sanity-checks the diff, and pushes a `shots/`-only commit directly to
master. `--since auto` is what keeps the unattended run cheap: it re-captures
only the views the merged PR can have moved, and most merges cost one lane or
none at all.
Native (ios/android) refreshes stay a manual pre-release step — unattended
simulator lanes are too slow and flaky. If `bun run test:shots` fails after a
merge, the just-merged PR added a route/view the catalog doesn't know: add the
`views.json` entry (or an `excludedRoutes` row) first.

## Troubleshooting

- **502 from https://localhost:3000** — nothing on `:5173`; start the web app.
- **Steering shows “Couldn't get a viewer ticket” / “Disconnected”** — the web
  server is advertising a `ws://<LAN-IP>` relay while captures load the page
  over `https://localhost:3000`. Browsers treat that as mixed content and block
  the socket; `ws://localhost` is exempt because localhost counts as a
  trustworthy origin. Set `STEER_RELAY_URL=ws://localhost:4002` for the browser
  lanes — the LAN IP is only needed for the ANDROID emulator, and the
  orchestrator substitutes it for that lane itself.
- **Steering views show “Reconnecting…”** — the relay stub isn't running or its
  banner never appeared; check `STEER_RELAY_URL`/`SECRET` against the compose
  relay, and remember the emulator needs the LAN-IP form.
- **401s from capture login** — re-run `cd apps/web && bun run seed:screenshots`
  (it tears down and rebuilds the demo team; also confirm
  `BETTER_AUTH_TRUSTED_ORIGINS` includes `https://localhost:3000`).
- **Flat/dark desktop shots** — Electric hadn't synced before the shutter; the
  capturer retries once with extra delay, but a slow first run may need a rerun
  of just that platform.
- **`shape proxy` preflight fails / every desktop shot is skeletons and empty
  states** — you are on the wrong Node. On **Node 26**, `fetch` over a unix
  socket returns the right status and body with ZERO headers (TCP is fine — an
  undici regression), and nitro's dev worker is reached over exactly such a
  socket. So `bun dev` serves every TanStack Start route with its headers
  stripped: no `content-type` (Start throws `expected content-type header to be
  set` in the browser), no `set-cookie` (the web lane cannot log in), and no
  `electric-handle`/`electric-offset` — the shape cursor, without which no
  client ever syncs. The JSON is correct, so the app looks alive and just stays
  empty. Use the Node pinned in `.tool-versions`:
  `PATH="/opt/homebrew/opt/node@24/bin:$PATH" bun dev` — `vite.config.ts` now
  refuses to start dev on a known-broken major. The built app is unaffected
  (it runs under Bun) and is always a valid fallback:
  `cd apps/web && bun run build && PORT=5173 bun --env-file=.env .output/server/index.mjs`.
  The run also checks this in one request before opening a window, rather than
  photographing forty empty shells and exiting 0.
- **`screencapture` permission errors** — grant Screen Recording to the
  terminal app in System Settings → Privacy & Security, then rerun.

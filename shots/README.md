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
bun run shots                       # capture every platform (web, web-mobile, desktop, ios, android)
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

`--since <ref>` (or `--since auto`, meaning "the last commit that CAPTURED the
store" — the last `chore(shots): refresh app screenshots`, not merely the last
commit to touch `shots/`, because a feature PR that deletes a retired lane's
webps or lands one new view photographs nothing else, EXP-667) narrows the run to the `<view, platform>` pairs the changed files can
actually have moved, and skips a whole lane whose set comes out empty — a
web-only PR never launches the desktop app. When nothing is in scope the run
prints "Nothing to capture" and exits before preflight, in seconds.

The mapping lives in `packages/shots/src/affected.ts` and is deliberately
FAIL-SAFE: a path narrows to specific views only when the repo proves the
connection, and anything else widens to every view of every platform it could
belong to. Web is precise (route → `routeTree.gen.ts` → transitive import graph
of `apps/web/src`, type-only imports erased); desktop matches `crates/ui/src`
module names against view ids and drive values and widens on anything else; the
native lanes match a Swift/Kotlin file's basename minus its role suffix
(`IssueDetailView` → `issue-detail`) against view ids and shot names, then its
DIRECTORY against a view family (`UI/Support` → the support views), and widen
the whole platform for shared code (ExpCore/ExpUI, themes, icons, the fastlane
lanes). A changed `views.json` entry and a view with no stored image yet are always in
scope.

The one carve-out inside that shared-code rule is the native DATA layer —
shape polling, socket reconnect, replay staging, follow-pin policy (EXP-670).
It cannot draw, because every capture photographs a freshly synced client, so
`IGNORED` drops it instead of widening two simulator lanes. Presence
RENDERING (`SessionDevicePresentation`, `DeviceRows`) and the three files that
keep rendered copy beside the socket lifecycle are deliberately excluded from
that carve-out and still widen; the list in `affected.ts` says which and why.

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
- The `sign-in` view is the CLOUD card — Google and Apple above the password
  form — so the instance has to advertise them. Placeholder values are enough
  (nothing signs in through them; the lanes use the password form):

  ```sh
  GOOGLE_CLIENT_ID=placeholder GOOGLE_CLIENT_SECRET=placeholder GOOGLE_LOGIN_ENABLED=true
  APPLE_CLIENT_ID=placeholder  APPLE_CLIENT_SECRET=placeholder  APPLE_LOGIN_ENABLED=true
  ```

  Without them the run DROPS `sign-in` from the web/web-mobile/desktop lanes and
  says so, rather than committing a bare password box under that name.
- `GITHUB_TOKEN` for the `review-diff` view. Its diff is fetched live from
  GitHub, and the anonymous limit is 60 requests an hour for the whole machine —
  once the web lane has spent it the desktop lane photographs a 403. Any token
  with public-repo read makes the lane deterministic.
- The native STYLEGUIDE lanes need the relay stub running too
  (`cd apps/web && bun run screenshots:desktop`, or let `bun run shots` start
  it): `sg_machine-settings` and the `sg_start-coding-*` shots render off the
  demo user's OWN registered device, which the stub announces — the seed does
  not plant it.
- iOS: Xcode + the Snapfile's simulators (iPhone 17 Pro Max, iPad Pro 13-inch)
  and `bundle install` under `apps/ios`. Run lanes with `LC_ALL=en_US.UTF-8`.
- Android: an English-locale phone emulator booted, exactly one device attached.
  For the emulator the relay URL must be the host LAN IP — the orchestrator
  resolves that itself via `ipconfig getifaddr en0`. The run also disables the
  device's `autofill_service` for the duration and restores it afterwards
  (EXP-665): Android's "Save password to Google Password Manager?" dialog is a
  SYSTEM window that steals focus the moment the lane signs in, and Espresso's
  next interaction then dies somewhere unrelated to the cause.
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

Both native lanes take a `shots:<id,id,…>` option (`bundle exec fastlane
styleguide_screenshots shots:sg_board,sg_reviews`). Navigation still runs; a
snapshot outside the list is simply not taken. `bun run shots` computes that list
from the catalog whenever the run is scoped, and skips a lane whose list came out
empty.

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
| `login`    | — (no session injected)             | the pre-login card, on its own throwaway data dir |
| `onboarding` | `EXP_DEV_ONBOARDING`              | the first-run wizard, signed in as the team-less NEWCOMER on its own data dir, WITHOUT `EXP_SKIP_ONBOARDING` |
| `manual`   | —                                   | nothing: capture it by hand with `--manual <view-id>` |

A view may add `desktop.env` on top for the cases that are a drive PLUS a flag —
`EXP_DEV_FILTER=1` for the board's filter popover, `EXP_DEV_BOARD_ID` to point
the rail at a board other than the last-visited one, `EXP_DEV_SELECT` to
pre-select the rows the bulk bar needs, `EXP_DEV_SEARCH_QUERY` to open the
palette with a query already typed, `EXP_DEV_OPEN_SHELL=1` for a docked
terminal, `EXP_DEV_ACTIONS_TAB` for the Actions screen's tab, `EXP_DEV_LOGIN`
for which state the login card starts in. Every one of them is documented in
`packages/shots/src/desktop-env.md`.

The desktop app tells the capturer when it is photographable (EXP-633): with
`EXP_DEV_READY_FILE` set it writes a marker once a window exists, the session is
synced, every Electric shape is past its first `up-to-date` and any requested
dialog has opened, and logs what it is still blocked on every five seconds until
then. `anchorDelayMs` is therefore a POST-ready settle — the last frame or two of
layout — not a sync wait, which is why the values in the catalog are a few
hundred milliseconds rather than seconds.

Values may carry `$placeholders` — in the drive value AND in `desktop.env` —
resolved against the seeded database by `bun run screenshots:ids` (which is a
thin printer over `apps/web/scripts/lib/demo-ids.ts`): `$APP-5` and friends are
issue identifiers, and `$thread`, `$action`, `$device`, `$automation`, `$board`,
`$emptyBoard` and `$team` name one well-known seeded row each. `web.route` adds
`$supportToken`, the reporter magic link for the seeded helpdesk thread — a
CREDENTIAL, resolved lazily and never printed. An unresolvable placeholder SKIPS
the view rather than photographing whatever the app fell back to.

## How the diff-skip works

The candidate is encoded to webp first, then compared against the committed
webp (encoded-vs-encoded, so encoder noise never counts): decode both to RGBA,
`pixelmatch` with a loose per-pixel threshold, and keep the existing file when
the changed fraction is within the view's `diffTolerance` (default 0.005 —
absorbs the seed's relative timestamps). `index.json` is re-derived from disk
with sorted keys and no timestamps, so it only changes when an image does.

The tolerance cannot tell a re-rendered "22 hr. ago" from a redesigned chip of
the same size, so a small real change CAN land as `kept` (EXP-658: it did, on
`issue-comments/ios`). Two guards: every kept shot that differed at all is
listed after the summary table with its fraction against the tolerance
(`0.0041 of 0.0050 (82%)  ← near miss` at 50% and above), and views whose
subject is a small element carry a tighter per-view `diffTolerance` in
`views.json` (`issue-comments` is 0.001, `search` 0.0005). Read that block in
the run log before trusting a `kept`; `--force` rewrites a shot the tolerance
swallowed.

**A `--views` run writes every shot that differs at all** (EXP-670). The share
of a tolerance is a weak signal and must not be read as one: in a single run a
reviews-queue reorder read 95% of its tolerance, a whole new "Pending invites"
section read 50%, and a page of nothing but drifting timestamps read 74%.
Bounding box and pixel density were measured as tie-breakers and rejected —
they overlap just as badly. So when you name the views yourself the tolerance
stops deciding: anything that moved a pixel is written, written shots under
tolerance are marked `← under tolerance, eyeball it`, and YOU revert what the
diff cannot explain. That is the trade the narrowed path wants — one extra
`git checkout` beats a stale screenshot nobody notices. `--since` alone does
NOT do this (it routinely resolves to forty views, and writing every timestamp
flicker across all of them is the 200-file diff this writer exists to prevent),
and `--force` differs too: it rewrites even byte-identical shots.

`SCREENSHOT_FREEZE_NOW` (epoch ms or an ISO timestamp) pins the seed's clock so
absolute dates and ordering stop moving between runs. It is opt-in and `bun run
shots` never sets it: it does nothing for the relative labels each CLIENT renders
against the real clock, and a stale frozen instant reddens every due date and
takes every device offline.

## Store slides and their pop-out rects

The App Store / Play listing images are a separate product from this store: the
compositor in `apps/marketing/scripts/store/` takes the eight raw `store`-lane
captures, bezels them, adds a headline, and lifts one element out of the screen
as a floating card. That last part needs a RECT, and it resolves in three tiers
(`store-crops.ts`):

1. a `pop-<shot>.json` sidecar next to the raw capture — form-keyed
   (`ios-phone` / `ios-tablet` / `android-phone`),
2. the committed `HAND_RECTS` fallback, measured by eye and prone to going stale,
3. nothing — the slide still renders, just flat.

Sidecars are the accurate tier, because the UI test knows exactly where the
element it just photographed was. `PopRects.swift` / `PopRects.kt` record it as
they run, and

```bash
bun run screenshots:pop-sidecars -- --platform ios      # or android
```

merges those recordings into the form-keyed files the compositor reads, keeping
any form this run did not measure and deleting the inputs it consumed. Both
Fastfiles call it after their capture lane, non-fatally.

To tune a rect by hand instead, run the compositor with `--debug-crops`: it
writes each raw with every candidate rect stroked and labelled, so a number can
be nudged and re-checked in one pass. Sidecars are NOT committed (EXP-348 keeps
store screenshots out of git) — only `HAND_RECTS` is.

## The automation

The team action **“Refresh app screenshots”** (trigger: `pr_merged`) runs on its
bound device and pushes a `shots/`-only commit straight to master, then
redeploys the styleguide site.

What makes it affordable is that it asks first. `bun run shots:affected --since
auto` maps the merged diff to `<view, platform>` pairs; a lane whose list comes
out empty is never started, and a diff that moved nothing at all ends the run in
seconds. All FIVE lanes are in play — web, web-mobile, desktop, ios and android
— not just the browser ones: the native suites take `shots:<ids>` subsets, so an
iOS-only PR costs the two or three simulator shots it actually moved instead of
forty. There is no tablet lane: the iPad frames the App Store wants come out of
the iOS `screenshots` lane and go straight to the ASO compositor, never here.

A lane whose DEVICE is not available is skipped, not failed: no booted emulator
means no android lane, no Xcode means no ios lane, and the run still refreshes
everything else and says what it left behind. The same goes for the redeploy —
a missing or unreachable `coolify` CLI is reported, never fatal.

The action may narrow further with `--views` when a BROAD rule widened a lane
more than the change warrants (a tRPC router touched by a one-line fix widens
every web view), and it states its reasoning when it does. Narrowing is a
judgement call about a fail-safe default, so it is written down rather than
silently applied. Because it narrowed by hand, that run also writes every shot
that differs at all rather than trusting the tolerance — see the diff-skip
section.

**The action files no issues.** It runs on every merge, so anything it filed
per-run arrived at merge cadence and outran what anyone could fix; a rule that
belongs in `affected.ts` or a tolerance that needs tuning is a PR, not a
ticket. When a run hits something it cannot decide, it says so in its summary
and stops — a human reading that summary can open an issue if it deserves one.

If `bun run test:shots` fails after a merge, the just-merged PR added a
route/view the catalog does not know: add the `views.json` entry (or an
`excludedRoutes` row) first. And every capture is eyeballed before it is
committed — a wrong screen under the right filename passes every gate there is,
so "no skeleton rows, no empty states that should have data" is part of the
review, not of the tooling.

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
- **`never signalled ready within 90s`** — the desktop app came up but never
  reached a photographable state, so the capturer refused to press the shutter
  (EXP-633). Almost always the stack, not the app: a stopped `electric`
  container, an instance that was never seeded, or a dev server serving shapes
  without their control headers (see the Node bullet below). Reproduce it with
  the exact env the failure printed plus `EXP_DEV_READY_FILE=/tmp/ready.json` —
  the app logs `waiting: …` every five seconds and names the shapes that never
  went live.
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

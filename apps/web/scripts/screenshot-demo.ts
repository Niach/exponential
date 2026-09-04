/**
 * Constants shared by the two store-screenshot scripts (EXP-393).
 *
 * `seed-screenshots.ts` runs its seed at import time, so `screenshot-desktop.ts`
 * cannot pull the demo identity out of it — these live here instead, in one
 * place, because the two scripts have to agree: the desktop script looks the
 * demo user up by email, and the device it announces has to carry the same
 * label the seeded coding sessions do or the picker and the session byline name
 * two different machines.
 */
export const DEMO_EMAIL = `demo@exponential.at`
export const DEMO_PASSWORD = `screenshots-demo`
export const DEMO_NAME = `Alex Carter`
export const TEAM_SLUG = `acme`

/**
 * The seeded board with no issues on it — the `board-empty` view on every
 * client. Named here because three places have to agree on it: the seed that
 * creates it, `lib/demo-ids.ts` which resolves `$emptyBoard` to its uuid for
 * the desktop lane, and the native suites that switch boards by title.
 */
export const EMPTY_BOARD_SLUG = `launch-marketing`

/** The demo user's machine, in both the seeded sessions and the relay presence. */
export const DEMO_DEVICE_LABEL = `Alex's MacBook Pro`

/**
 * The steer deviceId that machine announces (`devices.device_id`, and the
 * `online` frame's). Shared because the seeded automations bind their runner to
 * it — an automation names the MACHINE, not a row uuid, so the seed and the
 * stand-in desktop have to agree or every automation row renders "Unknown
 * machine".
 */
export const DEMO_DEVICE_ID = `screenshot-demo-desktop`

/**
 * The team-shared build server's steer device id. Pinned for the same reason as
 * the desktop's: `screenshots:prune-devices` deletes every device row that is
 * NOT one of these, so the capture lane's throwaway registrations cannot pile
 * up in the demo team.
 */
export const DEMO_SERVER_DEVICE_ID = `screenshot-demo-server`

/**
 * Marketing versions the two machines report. The seeded team server sits one
 * minor behind the desktop on purpose: a fleet where every machine is on the
 * same build never shows the version pill doing anything.
 */
export const DEMO_DEVICE_VERSION = `0.14.22`
export const DEMO_SERVER_VERSION = `0.13.11`

/**
 * A SECOND identity that owns nothing (EXP-566): verified, but with a null
 * `onboardingCompletedAt` and no membership anywhere. It exists so the two
 * pre-team views can be photographed at all — signed in as the demo user, both
 * of them redirect straight to a board.
 *
 *   /onboarding      the create-or-join wizard every signup lands in
 *   /invite/<token>  the landing page a teammate opens from an invite link
 *
 * Accepting the invite would consume it AND complete onboarding, so the
 * capturer must never press the button: it photographs the offer and moves on.
 * The seed re-mints both the user and the invite on every run, which is what
 * keeps the token below valid despite being a constant.
 */
export const NEWCOMER_EMAIL = `newcomer@exponential.at`
export const NEWCOMER_PASSWORD = `screenshots-newcomer`
export const NEWCOMER_NAME = `Jordan Reyes`

/**
 * The STARTER (EXP-725): the third seeded identity, verified and the OWNER of
 * exactly one team that has NO boards, invites or devices, with
 * `onboardingCompletedAt` NULL. The flag stays NULL on its own — the lazy
 * backfill in lib/auth/onboarding.ts only stamps it once a board exists — so
 * signing in resumes the first-run wizard past the team step, which is how
 * the invite and devices steps are photographed on every lane (`?step=` on
 * the web, `EXP_DEV_ONBOARDING=invite|devices` on the desktop, the natives'
 * test hooks). Nothing is ever submitted as this identity: a board would
 * complete onboarding, an invite would take a seat.
 */
export const STARTER_EMAIL = `starter@exponential.at`
export const STARTER_PASSWORD = `screenshots-starter`
export const STARTER_NAME = `Sam Okafor`
export const STARTER_TEAM_NAME = `Bluebird Labs`
export const STARTER_TEAM_SLUG = `bluebird-labs`

/** The unconsumed invite the `invite-accept` view is captured on. */
export const DEMO_INVITE_TOKEN = `screenshots-demo-invite`

/**
 * Fixed expiry for the two PENDING invites the `settings-members` view
 * photographs (EXP-668).
 *
 * Every other seeded date is relative to "now", which is what makes the demo
 * read like a live team — but the pending-invite rows render their expiry as an
 * ABSOLUTE date (`Expires 9/1/2026`), so a relative offset moved it by a day
 * every time the seed ran and rewrote the shot for nothing. These two rows are
 * the only place an absolute future date reaches a pixel, so they are the only
 * place worth pinning; `DEMO_INVITE_TOKEN` stays relative because the
 * `invite-accept` view only tests whether it is expired and never prints it.
 *
 * Far enough out that it stays unexpired for a long time, and
 * `screenshot-demo.test.ts` fails while there is still plenty of runway rather
 * than the day the seed starts producing expired invites.
 */
export const DEMO_PENDING_INVITE_EXPIRY = {
  /** `DEMO_INVITE_TOKEN` — the `invite-accept` view's invite, ALSO listed here. */
  demo: new Date(`2027-06-05T12:00:00Z`),
  /** The mailed invite (`priya@northwind.dev`). */
  mailed: new Date(`2027-06-01T12:00:00Z`),
  /** The bare shareable link, no email on the row. */
  link: new Date(`2027-06-03T12:00:00Z`),
} as const

/**
 * The due dates on the four issues that carry one (EXP-669).
 *
 * A due date renders as an ABSOLUTE `MMM d` chip on all five clients, so a
 * relative offset moved it every calendar day and rewrote `board`,
 * `issue-detail`, `issue-comments` and `my-issues` on the first refresh after
 * midnight. Unlike a comment's relative label there is no bucket wide enough
 * to hide in: the only stable absolute date is a fixed one.
 *
 * Spaced 1 / 2 / 4 / 7 days apart the way the relative offsets were, so the
 * board's due column keeps the same order and spread, and parked in the same
 * era as `DEMO_PENDING_INVITE_EXPIRY` so ONE bump moves every pinned date in
 * this file. No client prints a year (web `formatDate`, iOS/Android `MMM d`,
 * desktop `format_short_date` discards it outright), so a date two years out
 * is pixel-identical in shape to next week's.
 *
 * They must all stay in the FUTURE: `dueDateTone` is a three-way rule and an
 * upcoming date is the muted one every current shot shows. `screenshot-demo.test.ts`
 * fails on `MIN_RUNWAY_DAYS` of headroom, long before one goes overdue and
 * reddens a row.
 *
 * Note this deliberately costs one thing: at `inDays(1)` the deep-links issue
 * rendered the literal "Tomorrow" on iOS and Android, which those two
 * special-case. A fixed date cannot say that, so the mobile chip now reads
 * `Jun 8` like everywhere else. Churning `board` on web and desktop every
 * night was the worse trade.
 */
export const DEMO_DUE_DATES = {
  /** `Push notification deep links open the wrong tab` — the soonest. */
  deepLinks: `2027-06-08`,
  /** `Dark mode contrast pass across settings`. */
  darkMode: `2027-06-09`,
  /** `Reduce cold start below 800 ms` — APP-5, the showcase issue. */
  coldStart: `2027-06-11`,
  /** `Improve empty states with illustrations` — the furthest out. */
  emptyStates: `2027-06-14`,
} as const

/**
 * The two settings surfaces that print a PAST instant verbatim.
 *
 * Same failure as `DEMO_DUE_DATES`, mirrored: a relative offset (`daysAgo(9)`,
 * or Better Auth stamping `now` at mint) renders as an absolute date, so the
 * date moved every calendar day and rewrote `settings-boards` and
 * `settings-api-keys` on the first refresh after midnight — twice on
 * 2026-09-01, for no product change. These are the only two past dates any
 * view prints; everything else past is a relative label.
 *
 * Fixed instants, and unlike the future-dated pins there is nothing to keep
 * runway for: a board archived long ago and a key minted long ago read exactly
 * right however old they get. They only have to stay in the PAST, which
 * `screenshot-demo.test.ts` checks.
 */
export const DEMO_PINNED_PAST_DATES = {
  /** `settings-boards` → "Archived Mar 2, 2026" on the Design System card. */
  boardArchived: new Date(`2026-03-02T09:00:00Z`),
} as const

/**
 * The demo user's two personal API keys, as the `settings-api-keys` view
 * prints them: `${start}… · created ${date} · last used never`.
 *
 * BOTH printed fields are unstable at mint. Better Auth generates a random
 * credential, so `start` (its visible first chars) differs every seed, and it
 * stamps `created_at` with the seed's own clock, so the date moves with the
 * calendar. The row ORDER was a third: the list is `desc(createdAt)` and two
 * keys minted in the same loop landed on timestamps a few milliseconds apart,
 * close enough to flip. All three rewrote the view for nothing.
 *
 * So the seed mints through Better Auth exactly as `users.mintPersonalApiKey`
 * does — the row stays a genuine hashed credential — and then overwrites these
 * two DISPLAY columns. `start` no longer matches the raw key, which costs
 * nothing: the key was already discarded unread at mint, and auth resolves a
 * presented key by hashing it against `key`, never by `start`.
 *
 * Listed newest LAST, and seeded in this order, so the rendered list reads
 * top-down as the reverse of this array.
 */
export const DEMO_API_KEYS = [
  {
    name: DEMO_DEVICE_LABEL,
    start: `expu_a`,
    createdAt: new Date(`2026-03-02T09:12:00Z`),
  },
  {
    name: `Claude Code (MCP)`,
    start: `expu_k`,
    createdAt: new Date(`2026-03-04T16:40:00Z`),
  },
] as const

/**
 * How long ago each comment on the showcase issue (APP-5) was posted, in hours
 * (EXP-669).
 *
 * These are pinned for the same reason as the invite expiries above, but
 * against a different mechanism. A comment renders a RELATIVE label the client
 * computes against its own clock, so what reaches the pixel is not the seeded
 * offset — it is the offset PLUS however long passed between the seed and the
 * shutter. That gap is not a constant: it is a minute or two for a narrowed
 * `--views issue-comments` run, most of an hour by the time the web-mobile
 * lane reaches this view in a full refresh, and longer still for the native
 * styleguide lanes that run last. With the old sub-day offsets the label was
 * hour-granular and a bucket is one hour wide, so the same unchanged thread
 * photographed "22 hours ago" one run and "23 hours ago" the next, rewriting
 * every issue-comments shot for nothing.
 *
 * A DAY-granular label is the fix, because that bucket is wide enough to
 * swallow the gap. It is only 12 hours wide across the fleet, though, not 24:
 * web rounds (date-fns `formatDistanceToNowStrict`, so "1 day" is 24-36h)
 * while iOS, Android and desktop floor on calendar boundaries ("1 day" is
 * 24-48h). Their boundaries therefore fall every 12 hours, and an offset only
 * survives the gap if it sits just ABOVE one — which is also where the two
 * rules agree on what to print, so the same thread reads the same on all five
 * platforms in the gallery.
 *
 * `screenshot-demo.test.ts` checks both rules against
 * `COMMENT_LABEL_RUNWAY_HOURS`; edit these numbers only if it still passes.
 */
export const DEMO_SHOWCASE_COMMENT_HOURS_AGO = {
  /** Mira opens with the profiling numbers. */
  mira: 50,
  /** Jonas picks up the snapshot cache. */
  jonas: 49,
  /** The demo user (Alex) posts the CI results. */
  demo: 26,
  /** Sofia's @mention + #issue-ref reply, the newest in the thread. */
  sofia: 25,
} as const

/**
 * The LAST event of the scripted steering transcript: the unanswered question
 * the steering screenshot is composed around.
 *
 * Both UI tests wait on a fragment of it before capturing, because an empty
 * feed still renders the feed container — the container's test tag alone
 * happily photographs a "Reconnecting…" screen when the relay is unreachable.
 * It has to be the LAST event, not the first: the feed is a bottom-anchored
 * lazy list, so everything above the fold is never composed and no UI query can
 * see it. Keep the fragment the tests quote (`Lazy-load the markdown editor
 * too`) intact.
 */
export const DEMO_FEED_QUESTION = `Cold start is at 740ms (target <800ms). Lazy-load the markdown editor too, or open the PR with what we have?`

/**
 * The helpdesk thread the `support-reporter` view is captured on.
 *
 * That view is the ANONYMOUS magic-link page (`/support/<token>`), so it is
 * addressed by a token minted for ONE thread — and the catalog anchors the shot
 * on the subject line. Seed, id lookup and manifest anchor therefore all have
 * to quote the same string, which is why it lives here rather than inline in
 * the seed's thread list.
 */
export const SUPPORT_REPORTER_THREAD_TITLE = `Can't sign in on the iPad app`

/**
 * What the demo desktop reports about its three agent CLIs (EXP-733): the
 * account each one is signed in as and the usage windows it last read.
 *
 * A real desktop collects both locally and ships them on register/heartbeat
 * (EXP-484) into `devices.agent_accounts` / `agent_usage`; every client then
 * renders the same rows off the synced row — the Agents section of machine
 * settings (web, desktop, iOS, Android), the session usage strip beside a
 * running run, and the mobile Usage sheet. Before this the stand-in desktop
 * (`screenshot-desktop.ts`) registered neither, so every one of those
 * surfaces photographed its empty fallback ("Sign-in status unknown", no
 * limits) and the store could not gate drift on them at all.
 *
 * The numbers are PINNED, the stamps are not. `fetchedAt` has to be within
 * `USAGE_FRESH_MS` (15 min) of the CLIENT's clock or the cards dim into an
 * "as of …" line, and a reset renders as a live countdown (`resets in 3h
 * 32m`) computed against that same clock — so the stub stamps both relative
 * to ITS "now" on every heartbeat, like the real desktop. `resetsIn` is a
 * duration for that reason, and it carries `COUNTDOWN_PAD_SECONDS` of slack
 * past the label it is meant to print: the shutter fires up to one heartbeat
 * (30 s) after the stamp, and without the pad the minute would already have
 * ticked over to `3h 31m` by the time anyone looked.
 *
 * Claude's three windows are the SAME numbers the desktop's own DEV stub
 * (`ui/src/device_settings.rs` `dev_agent_status`, `EXP_DEV_AGENT_ACCOUNT`)
 * bakes into `settings-agents`, so the two places a capture shows this
 * machine's claude limits agree. pi has no usage surface of its own — its
 * Anthropic OAuth provider answers the same endpoint claude does — so it
 * reports claude's windows under the provider caption pi actually prints.
 * `screenshot-demo.test.ts` checks the pad against the countdown rule.
 */
export const COUNTDOWN_PAD_SECONDS = 45

export interface DemoUsageWindow {
  key: string
  label: string
  percent: number
  /** Seconds from the stamp to the reset; null = the window reports none. */
  resetsIn: number | null
}

export interface DemoAgentStatus {
  signedIn: boolean
  email?: string
  plan?: string
  windows: DemoUsageWindow[]
}

const CLAUDE_WINDOWS: DemoUsageWindow[] = [
  { key: `session`, label: `5h`, percent: 73, resetsIn: 3 * 3_600 + 32 * 60 },
  { key: `weekly`, label: `Week`, percent: 24, resetsIn: 30 * 3_600 },
  { key: `model:fable`, label: `Fable`, percent: 38, resetsIn: 30 * 3_600 },
]

export const DEMO_AGENT_STATUS: Record<`claude` | `codex` | `pi`, DemoAgentStatus> = {
  claude: {
    signedIn: true,
    email: DEMO_EMAIL,
    plan: `max`,
    windows: CLAUDE_WINDOWS,
  },
  codex: {
    signedIn: true,
    email: DEMO_EMAIL,
    plan: `plus`,
    windows: [
      { key: `session`, label: `5h`, percent: 41, resetsIn: 2 * 3_600 + 5 * 60 },
      { key: `weekly`, label: `Week`, percent: 57, resetsIn: 4 * 86_400 + 9 * 3_600 },
    ],
  },
  pi: {
    signedIn: true,
    plan: `anthropic (oauth)`,
    windows: CLAUDE_WINDOWS,
  },
}

/**
 * The two jsonb payloads for `devices.agent_accounts` / `agent_usage`, stamped
 * against `now` — call it on EVERY heartbeat, never once at boot, or the
 * numbers go stale 15 minutes into the run.
 */
export function demoAgentReport(now: Date): {
  agentAccounts: Record<
    string,
    { signedIn: boolean; email?: string; plan?: string; checkedAt: string }
  >
  agentUsage: Record<
    string,
    {
      fetchedAt: string
      stale: boolean
      windows: Array<{
        key: string
        label: string
        percent: number
        resetsAt: string | null
      }>
    }
  >
} {
  const stamp = now.toISOString()
  const agentAccounts: ReturnType<typeof demoAgentReport>[`agentAccounts`] = {}
  const agentUsage: ReturnType<typeof demoAgentReport>[`agentUsage`] = {}
  for (const [agent, status] of Object.entries(DEMO_AGENT_STATUS)) {
    agentAccounts[agent] = {
      signedIn: status.signedIn,
      ...(status.email ? { email: status.email } : {}),
      ...(status.plan ? { plan: status.plan } : {}),
      checkedAt: stamp,
    }
    agentUsage[agent] = {
      fetchedAt: stamp,
      stale: false,
      windows: status.windows.map((window) => ({
        key: window.key,
        label: window.label,
        percent: window.percent,
        resetsAt:
          window.resetsIn === null
            ? null
            : new Date(
                now.getTime() + (window.resetsIn + COUNTDOWN_PAD_SECONDS) * 1000
              ).toISOString(),
      })),
    }
  }
  return { agentAccounts, agentUsage }
}

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

/**
 * The three Better Auth identities' user ids, pinned (EXP-913).
 *
 * Every avatar paints its fallback hue from an FNV hash of the user id
 * (`@exp/ui` avatar-color.ts), so a seed that let Better Auth mint fresh ids
 * re-rolled the demo, newcomer and starter colors on every run and rewrote
 * ~40 native shots per refresh for nothing. The seed writes the user and its
 * credential account directly under these ids instead (the teammates were
 * already pinned: `demo-mira` & co). Changing one re-shoots every view that
 * shows that avatar.
 */
export const DEMO_USER_ID = `screenshot-demo-alex`
export const NEWCOMER_USER_ID = `screenshot-demo-newcomer`
export const STARTER_USER_ID = `screenshot-demo-starter`

/**
 * The clock the seeded identities' `users.timezone` is pinned to (EXP-913).
 *
 * The column is claimed ONCE by whichever client signs in first (`onlyIfUnset`,
 * _authenticated.tsx), so `settings-account` printed the web lane's browser
 * zone on one run and another lane's host zone on the next. Pinned to the
 * capture browser's own `timezoneId` (scripts/lib/capture-web.ts), so the
 * picker's option list and its selected value agree on every lane.
 */
export const DEMO_TIMEZONE = `Europe/Berlin`

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
 * The seeded attachments' upload instants (EXP-913), for the same reason: the
 * `settings-storage` view prints each row's date absolutely (web
 * `storage-section.tsx`, desktop `settings/storage.rs`), so `daysAgo(n)` moved
 * every row by a day per calendar day. Same newest-first order the relative
 * offsets had; all far past the orphan sweep's 24h grace, which the
 * deliberately orphaned `cacheSketch` relies on.
 */
export const DEMO_ATTACHMENT_DATES = {
  cacheHitRate: new Date(`2026-02-16T09:00:00Z`),
  auditPdf: new Date(`2026-02-20T09:00:00Z`),
  contrastSweep: new Date(`2026-02-21T09:00:00Z`),
  sysdiagnose: new Date(`2026-02-24T09:00:00Z`),
  voiceoverLabels: new Date(`2026-02-26T09:00:00Z`),
  cacheSketch: new Date(`2026-02-27T09:00:00Z`),
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
  // EXP-1054: the launcher's `Device: <hostname>` shape, so the Security page
  // lists it under Login sessions and the MCP key under API keys.
  {
    name: `Device: ${DEMO_DEVICE_LABEL}`,
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
 * EXP-1293 (Threads v2): the session tree's capture-addressable rows, pinned
 * fixed v4 UUIDs so a route can name them BEFORE the seed runs and a reseed is
 * byte-stable. A thread IS a root `sessions` row (kind `thread`); a team agent
 * a root of kind `agent`; children are sub-sessions (`parent_id`).
 *
 *   DEMO_THREAD_ID   the `thread` view: "Reduce cold start below 800 ms"
 *                    (APP-5 + APP-10 + FB-3), waiting on a person through its
 *                    child.
 *   DEMO_SESSION_ID  the `session` / `steering` views: that thread's child
 *                    "Profile the launch path" (running, needs input). The
 *                    raw session view's feed is relay-fed by
 *                    `screenshot-desktop.ts`, keyed by this id. Kept at the
 *                    value the EXP-740 steered session had, so every lane's
 *                    `/sessions/<id>` address survives the rename.
 *   DEMO_AGENT_ID    the `agent-channel` / `settings-team-agent` views: the
 *                    Triage agent (NEW-2: an `agents` row).
 */
export const DEMO_THREAD_ID = `5d7a2c90-3e41-4b8f-9c16-8a2e4f0b7d13`
export const DEMO_SESSION_ID = `7c1f0a52-6f3c-4e19-9a2b-2d5d0f7c8e41`
export const DEMO_AGENT_ID = `6530c2d7-ed10-46b0-ac66-c78020fd73d9`
/** EXP-1323: the cold-start thread's ENDED child (started from its suggested
 *  tasks card, its end = the `Report from …` line). */
export const DEMO_REPORT_SESSION_ID = `2b8e6f1c-4d7a-4e3b-9c5f-1a6d8e2f7b40`
/** EXP-1341: the cold-start thread's UNNAMED child holding two assigned issues. */
export const DEMO_GROUPED_SESSION_ID = `5c2d8a1e-7f3b-4c6d-8e9a-0b1c2d3e4f51`

/**
 * The session the relay stub publishes its scripted feed for (the `steering`
 * view's anchor is that feed's closing question). Since EXP-1293 it IS the
 * thread's child session — an alias, not a second row.
 */
export const DEMO_STEERED_SESSION_ID = DEMO_SESSION_ID

/** The other two team agents (the rail's Agents section reads Triage ·
 *  Designer · Release, by `sort_order`). */
export const DEMO_AGENT_IDS = {
  triage: DEMO_AGENT_ID,
  designer: `671f2227-1090-4a41-87ec-0ab2138358de`,
  release: `efb92a14-ee7c-4da6-ab07-6697d706ee62`,
} as const

/** S-3 (agent-settings.json `assistant`): the demo user's Assistant in the
 *  demo team (role `assistant`, the brand mark, on the demo desktop): the
 *  `settings-assistant` view's subject and the demo user's voice agent. */
export const DEMO_ASSISTANT_ID = `8f4e2b71-3c9d-4a5e-b6f0-1d2c3e4f5a6b`

/** NEW-2: the agents above are `agents` DEFINITION rows; each runs as the
 *  demo user's CONVERSATION session with it (a root with `agent_id`), keyed
 *  the same way. DEMO_AGENT_ID stays the agent (the `/agents/$agentId` and
 *  settings routes); the Triage conversation holds the channel's messages. */
export const DEMO_AGENT_CONVERSATION_IDS = {
  triage: `b05f0c49-f063-4b5c-a60e-69b1f7032a74`,
  designer: `ad6eb9f5-a99b-4b52-9b3f-0aeaa40314c5`,
  release: `109e239a-2b7a-4a9f-b7e1-200858f803c3`,
} as const
export const DEMO_AGENT_CONVERSATION_ID = DEMO_AGENT_CONVERSATION_IDS.triage

/** #EXP-1456: the demo user's conversation with their Assistant: one waiting
 *  question (a decision, #EXP-1439) and the inbox row its push wrote, so the
 *  `inbox` view shows the session-routed row and its tap opens the
 *  Assistant's page. */
export const DEMO_ASSISTANT_CONVERSATION_ID = `3d7a91c4-52e8-4f0b-9a16-c84e2b5d7f30`
export const DEMO_ASSISTANT_QUESTION = `Ship the board snapshot cache today, or wait for the editor bundle fix?`

/**
 * The other thread roots, one per Threads-list state:
 *
 *   groupByAssignee  APP-14, running + busy, its open PR STACKED on
 *                    batchLabels' open PR (Running, "stack").
 *   sortReports      FB-9, running, spawned by the Triage agent (the
 *                    agent channel's "started a thread" card opens it).
 *   offlineSpike     no issue, PRIVATE (not shared), running (the lock).
 *   batchLabels      APP-15, idle, an open PR ready to merge + a Guide
 *                    (the `guide` view's thread).
 *   scrollRestore    APP-16, ended, its PR merged (Recent, "merged"). Its
 *                    `ended_at` is the re-clock anchor (lib/demo-reclock.ts).
 */
export const DEMO_THREAD_IDS = {
  coldStart: DEMO_THREAD_ID,
  groupByAssignee: `9d3d6bab-3fe7-40bd-b8e7-b5eaef137aaf`,
  sortReports: `be4bcc34-92de-4ea0-a87f-c4f1d160348c`,
  offlineSpike: `373e1d40-91a5-4b4e-9cd9-0a5cd82df514`,
  batchLabels: `f30a7f02-f45e-44a5-ab23-9d2d916d6f3a`,
  scrollRestore: `39cb3777-33f9-4201-84e4-220da9cd0bce`,
} as const

/** The thread the `guide` view opens on its Guide tab (`?tab=guide`). */
export const DEMO_GUIDE_THREAD_ID = DEMO_THREAD_IDS.batchLabels

/** The thread whose PR carries the REAL GitHub diff (APP-14's, stacked on
 *  batchLabels): the `review-diff` and `guide-section` pages under its Guide
 *  (EXP-1308: APP-15's Guide draws no Changes row, as on iOS + Android). */
export const DEMO_DIFF_THREAD_ID = DEMO_THREAD_IDS.groupByAssignee

/**
 * The seeded `pull_requests` rows (EXP-1293: PR state lives there, never on
 * `issues`). `groupByAssignee` is the real GitHub PR (REVIEW_PR_URL in the
 * seed) stacked on `batchLabels`.
 */
export const DEMO_PULL_REQUEST_IDS = {
  batchLabels: `0187423a-5120-426a-a403-5880abc5473d`,
  groupByAssignee: `d321ca2f-1ebc-4cd8-8c37-8cc79aac62e5`,
  scrollRestore: `1d43befb-d260-420e-a7d5-854decaf32da`,
  heicFix: `3310b3c1-dda2-49d4-a555-d119faec9a22`,
  thumbnails: `2e73ddc0-c684-4ee3-8401-f202bd28cd19`,
} as const

/**
 * The Triage agent's channel as the `agent-channel` view photographs it.
 * Message rows print a TIME OF DAY (`09:02`), so their instants are pinned
 * like `DEMO_PINNED_PAST_DATES`: a relative stamp would move the printed
 * minute on every seed. 07:02Z = 09:02 in DEMO_TIMEZONE (CEST).
 */
export const DEMO_AGENT_CHANNEL_TIMES = {
  report: new Date(`2026-10-12T07:02:00Z`),
  threadStarted: new Date(`2026-10-12T07:02:20Z`),
  request: new Date(`2026-10-12T07:14:00Z`),
  reply: new Date(`2026-10-12T07:14:40Z`),
} as const

/** The person message the `agent-channel` view anchors on. */
export const DEMO_AGENT_REQUEST = `Label the Android ones too, and check the board every hour.`

/**
 * VAPP-110: the `apps` / `app` views. The Team dashboard app (three
 * versions, the demo user's EDITOR SESSION with it holding the chat) and a
 * second app with no version yet. `$appSlug` resolves to DEMO_APP_SLUG.
 */
export const DEMO_APP_ID = `c4b1d7e2-5a90-4f3c-8e6d-2b7a9c0f1d35`
/** VAPP-122: the `app` view's subject is the Working hours app (records). */
export const DEMO_APP_SLUG = `working-hours`
/** The Team dashboard app (three versions, no editor session): the list's second app. */
export const DEMO_APP_DASHBOARD_ID = `d5c2e8f3-6b01-4a4d-9f7e-3c8b0d1e2f46`
export const DEMO_APP_DASHBOARD_SLUG = `team-dashboard`
export const DEMO_APP_DASHBOARD_VERSION_IDS = [
  `4c9b5d1f-2e7a-4f0b-bd6c-8a5e0b2f4d37`,
  `5d0c6e2a-3f8b-4a1c-ce7d-9b6f1c3a5e48`,
  `6e1d7f3b-4a9c-4b2d-df8e-0c7a2d4b6f59`,
] as const
/** VAPP-122: the Working hours app's records (the demo user's October, two
 *  September days, a settings record, two of Jonas's days). */
export const DEMO_APP_RECORD_IDS = [
  `7f2e8a4c-5b0d-4c3e-8a9f-1d8b3e5c7a60`,
  `8a3f9b5d-6c1e-4d4f-9b0a-2e9c4f6d8b71`,
  `9b4a0c6e-7d2f-4e5a-ac1b-3f0d5a7e9c82`,
  `0c5b1d7f-8e3a-4f6b-bd2c-4a1e6b8f0d93`,
  `1d6c2e8a-9f4b-4a7c-ce3d-5b2f7c9a1e04`,
  `2e7d3f9b-0a5c-4b8d-df4e-6c3a8d0b2f15`,
  `3f8e4a0c-1b6d-4c9e-8f5f-7d4b9e1c3a26`,
  `4a9f5b1d-2c7e-4d0f-9a6a-8e5c0f2d4b37`,
  `5b0a6c2e-3d8f-4e1a-ab7b-9f6d1a3e5c48`,
  `6c1b7d3f-4e9a-4f2b-bc8c-0a7e2b4f6d59`,
  `7d2c8e4a-5f0b-4a3c-cd9d-1b8f3c5a7e60`,
  `8e3d9f5b-6a1c-4b4d-de0e-2c9a4d6b8f71`,
  `9f4e0a6c-7b2d-4c5e-ef1f-3d0b5e7c9a82`,
  `0a5f1b7d-8c3e-4d6f-8a2a-4e1c6f8d0b93`,
] as const
export const DEMO_APP_EMPTY_ID = `e8f2a6c1-3d47-4b9e-a1c5-7f0b2d8e4a62`
export const DEMO_APP_EDITOR_SESSION_ID = `a7d3f9b4-2c61-4e8a-b5f0-9d1c6e3a7b48`
export const DEMO_APP_VERSION_IDS = [
  `1f6e2a8c-9b4d-4c7e-8a3f-5d2b7e9c1a04`,
  `2a7f3b9d-0c5e-4d8f-9b4a-6e3c8f0d2b15`,
  `3b8a4c0e-1d6f-4e9a-ac5b-7f4d9a1e3c26`,
  `4c9b5d1f-2e7a-4f0b-bd6c-8a5e0b2f4d38`,
  `5d0c6e2a-3f8b-4a1c-ce7d-9b6f1c3a5e49`,
] as const
/** The editor chat prints times of day: pinned like the agent channel's. */
export const DEMO_APP_CHAT_TIMES = {
  request: new Date(`2026-10-12T08:05:00Z`),
  first: new Date(`2026-10-12T08:06:10Z`),
  change: new Date(`2026-10-12T08:12:00Z`),
  second: new Date(`2026-10-12T08:13:05Z`),
  polish: new Date(`2026-10-12T08:20:00Z`),
  third: new Date(`2026-10-12T08:21:30Z`),
  team: new Date(`2026-10-12T08:30:00Z`),
  fourth: new Date(`2026-10-12T08:31:40Z`),
  post: new Date(`2026-10-12T08:40:00Z`),
  fifth: new Date(`2026-10-12T08:41:20Z`),
} as const

/**
 * The comment the steered session's scripted `exponential_comments_create`
 * call posted (EXP-1259, the `comment-call-card` view): the seed stamps this id
 * on an `mcp`-sourced comment by the demo user, and the stand-in desktop's
 * settle names it in its preview, so the card renders the REAL synced row.
 * It sits on the image-cache issue, never the showcase APP-5, whose thread is
 * photographed by `issue-detail` / `issue-comments` on every platform.
 */
export const DEMO_AGENT_COMMENT_ID = `3e8a1c7d-2b4f-4d96-8a15-9c0e7f2b6d43`

/** How long before the seed's clock the agent's comment was posted. */
export const DEMO_AGENT_COMMENT_MINUTES_AGO = 20

/**
 * The action the `action-page` view is photographed on ("Nightly test
 * triage"), pinned like the sessions above: the action page is a route
 * (`actions/$actionId`). EXP-1293: actions are saved prompts only; triggers
 * moved onto agents (DEMO_AGENT_TRIGGER_ID).
 */
export const DEMO_ACTION_ID = `2b6e4f1c-8a3d-4c7e-9f52-6d1a0b3c7e85`

/** The Triage agent's triggers (the settings page's Triggers rows: a cron
 *  schedule and an event), pinned so a reseed writes the same jsonb. */
export const DEMO_AGENT_TRIGGER_ID = `4c9a7e2b-1d56-4f38-a0c4-8e2b5d7f1a63`
export const DEMO_AGENT_EVENT_TRIGGER_ID = `fa1ef2a0-5312-43d1-8c0a-60317e391c25`

/**
 * The one seeded `blocks` edge, by seed issue title: `[blocker, blocked]`.
 * It puts the blocks rail/badge on two board rows, so every board view shows
 * it.
 */
export const DEMO_BLOCKS = [
  `Offline queue for issue edits`,
  `Quick-add issue from the home screen widget`,
] as const

/**
 * The widget-filed issue the `support-reporter` view is captured on (SLOP-4:
 * a widget submission IS an issue; the reporter's conversation is its
 * reporter-audience comments).
 *
 * That view is the ANONYMOUS magic-link page (`/support/<token>`), so it is
 * addressed by a token minted over ONE issue id — pinned here like the
 * session and action ids, so `demo-ids.ts` can mint the token without a
 * lookup — and the catalog anchors the shot on the title. Seed, id lookup
 * and manifest anchor therefore all have to quote the same string.
 */
export const DEMO_WIDGET_ISSUE_ID = `5d2c8e4a-7b1f-4e63-9a0d-3c6f1b8e2d74`
export const WIDGET_REPORT_TITLE = `Can't sign in on the iPad app`
export const WIDGET_REPORTER_NAME = `Emma Fischer`
export const WIDGET_REPORTER_EMAIL = `emma@lumenlabs.io`

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
 * machine's claude limits agree.
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
  /** EXP-792 (EXP-747 B5): the profiles the machine holds for this agent —
   * the FIRST is the active one and mirrors the top-level fields. Absent =
   * the agent reports only its ambient login. */
  profiles?: DemoAgentProfile[]
}

export interface DemoAgentProfile {
  id: string
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

/**
 * EXP-792 (EXP-747 C6): the demo desktop's SECOND claude profile — a work
 * account beside the ambient one — so the usage page photographs two rows
 * for one agent on one machine. Its windows sit below the ambient ones on
 * purpose: the attention-first sort keeps the fuller (active) profile on top.
 */
const CLAUDE_WORK_WINDOWS: DemoUsageWindow[] = [
  { key: `session`, label: `5h`, percent: 12, resetsIn: 4 * 3_600 + 10 * 60 },
  { key: `weekly`, label: `Week`, percent: 9, resetsIn: 5 * 86_400 + 2 * 3_600 },
]

const CODEX_WINDOWS: DemoUsageWindow[] = [
  { key: `session`, label: `5h`, percent: 41, resetsIn: 2 * 3_600 + 5 * 60 },
  { key: `weekly`, label: `Week`, percent: 57, resetsIn: 4 * 86_400 + 9 * 3_600 },
]

export const DEMO_AGENT_STATUS: Record<`claude` | `codex`, DemoAgentStatus> = {
  claude: {
    signedIn: true,
    email: DEMO_EMAIL,
    plan: `max`,
    windows: CLAUDE_WINDOWS,
    profiles: [
      {
        id: `system`,
        signedIn: true,
        email: DEMO_EMAIL,
        plan: `max`,
        windows: CLAUDE_WINDOWS,
      },
      {
        id: `work`,
        signedIn: true,
        email: `alex@northwind.dev`,
        plan: `team`,
        windows: CLAUDE_WORK_WINDOWS,
      },
    ],
  },
  codex: {
    signedIn: true,
    email: DEMO_EMAIL,
    plan: `plus`,
    windows: CODEX_WINDOWS,
    profiles: [
      {
        id: `system`,
        signedIn: true,
        email: DEMO_EMAIL,
        plan: `plus`,
        windows: CODEX_WINDOWS,
      },
    ],
  },
}

/**
 * The two jsonb payloads for `devices.agent_accounts` / `agent_usage`, stamped
 * against `now` — call it on EVERY heartbeat, never once at boot, or the
 * numbers go stale 15 minutes into the run.
 */
interface DemoReportUsage {
  fetchedAt: string
  stale: boolean
  windows: Array<{
    key: string
    label: string
    percent: number
    resetsAt: string | null
  }>
}

interface DemoReportProfile {
  id: string
  signedIn: boolean
  email?: string
  plan?: string
  active: boolean
  checkedAt: string
  usage: DemoReportUsage
}

export function demoAgentReport(now: Date): {
  agentAccounts: Record<
    string,
    {
      signedIn: boolean
      email?: string
      plan?: string
      checkedAt: string
      /** EXP-792: present only for agents that declare profiles. */
      profiles?: DemoReportProfile[]
    }
  >
  agentUsage: Record<string, DemoReportUsage>
} {
  const stamp = now.toISOString()
  const stampUsage = (windows: DemoUsageWindow[]): DemoReportUsage => ({
    fetchedAt: stamp,
    stale: false,
    windows: windows.map((window) => ({
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
  })
  const agentAccounts: ReturnType<typeof demoAgentReport>[`agentAccounts`] = {}
  const agentUsage: ReturnType<typeof demoAgentReport>[`agentUsage`] = {}
  for (const [agent, status] of Object.entries(DEMO_AGENT_STATUS)) {
    agentAccounts[agent] = {
      signedIn: status.signedIn,
      ...(status.email ? { email: status.email } : {}),
      ...(status.plan ? { plan: status.plan } : {}),
      checkedAt: stamp,
      // EXP-792: the first profile is the active one; each carries its own
      // usage the same way a real device reports it (`clampAgentAccounts`).
      ...(status.profiles
        ? {
            profiles: status.profiles.map((profile, index) => ({
              id: profile.id,
              signedIn: profile.signedIn,
              ...(profile.email ? { email: profile.email } : {}),
              ...(profile.plan ? { plan: profile.plan } : {}),
              active: index === 0,
              checkedAt: stamp,
              usage: stampUsage(profile.windows),
            })),
          }
        : {}),
    }
    agentUsage[agent] = stampUsage(status.windows)
  }
  return { agentAccounts, agentUsage }
}

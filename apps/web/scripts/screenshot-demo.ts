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

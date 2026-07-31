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

/** The demo user's machine, in both the seeded sessions and the relay presence. */
export const DEMO_DEVICE_LABEL = `Alex's MacBook Pro`

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

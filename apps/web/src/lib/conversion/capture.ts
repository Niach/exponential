import { db } from "@/db/connection"
import { auth } from "@/lib/auth"
import { dailyAnonymousId } from "@/lib/conversion/anonymous"
import {
  extractAttributionParams,
  externalReferrer,
  shouldCaptureLanding,
  shouldCaptureReturnVisit,
} from "@/lib/conversion/attribution"
import {
  conversionTrackingEnabled,
  recordConversionEvent,
} from "@/lib/conversion/events"
import {
  TokenBucketLimiter,
  clientIpFromRequest,
} from "@/lib/widget/rate-limit"

// The landing row is deduped by the daily anonymous id — a hash of ip+ua — so
// one IP rotating user-agent strings mints a fresh identity per request and
// inserts unbounded rows. This bucket bounds that: 60 landing captures per IP
// per hour (burst 60), hardcoded — analytics is not worth an env knob. Only
// analytics rows are dropped, never the request; a busy shared NAT may
// under-count visitors, which is the accepted trade for a bounded table.
const LANDING_PER_IP_HOURLY = 60

let landingLimiter: TokenBucketLimiter | null = null

function getLandingLimiter(): TokenBucketLimiter {
  landingLimiter ??= new TokenBucketLimiter({
    capacity: LANDING_PER_IP_HOURLY,
    refillPerHour: LANDING_PER_IP_HOURLY,
  })
  return landingLimiter
}

// Server-bun landing tap (EXP-362): pure side effect, no response mutation,
// never blocks the request. One row per anonymous visitor per UTC day — the
// unique landing index plus the daily-rotating anonymous id do the dedupe,
// so repeat pageviews collapse into the first landing (first-touch
// attribution properties win).
export function captureLanding(req: Request): void {
  try {
    // Cloud-only (EXP-362): self-hosted instances collect no analytics.
    if (!conversionTrackingEnabled()) return
    if (!shouldCaptureLanding(req)) return
    const anonymousId = dailyAnonymousId(req)
    // Without a proxy-attested IP (bare dev server) there is no stable
    // visitor identity — recording NULL-id rows would bypass the dedupe
    // index entirely, so skip.
    if (!anonymousId) return
    if (!getLandingLimiter().tryTake(clientIpFromRequest(req)).ok) return
    const url = new URL(req.url)
    const params = extractAttributionParams(url)
    const referrer = externalReferrer(req)
    void recordConversionEvent(db, {
      name: `landing`,
      anonymousId,
      properties: {
        ...params,
        ...(referrer ? { referrer } : {}),
        path: url.pathname,
      },
    })
  } catch (err) {
    console.error(`[conversion] landing capture failed:`, err)
  }
}

// In-process dedupe so a user's page loads after the first of the day cost
// neither a session lookup nor an insert round-trip. The partial unique
// index (uniq_conversion_events_return_visit_daily) is the real guarantee
// across restarts and replicas; this map only saves round-trips. Cleared on
// UTC day rollover so it stays bounded by the day's active users.
const returnVisitSeen = new Map<string, string>()
let returnVisitDay = ``

// Signed-in daily-activity counterpart to captureLanding (EXP-522): one
// `return_visit` row per user per UTC day, recorded fire-and-forget from the
// same server-bun tap. Session resolution (a DB lookup) runs only on document
// GETs that carry a session cookie — SPA shell loads, not API traffic.
export function captureReturnVisit(req: Request): void {
  try {
    // Cloud-only (EXP-362): self-hosted instances collect no analytics.
    if (!conversionTrackingEnabled()) return
    if (!shouldCaptureReturnVisit(req)) return
    const day = new Date().toISOString().slice(0, 10)
    if (day !== returnVisitDay) {
      returnVisitDay = day
      returnVisitSeen.clear()
    }
    const path = new URL(req.url).pathname
    void (async () => {
      const session = await auth.api.getSession({ headers: req.headers })
      const userId = session?.user.id
      if (!userId || returnVisitSeen.get(userId) === day) return
      returnVisitSeen.set(userId, day)
      // `day` rides properties so the partial unique index can key on it —
      // created_at date expressions would need an AT TIME ZONE cast the
      // existing index conventions avoid.
      await recordConversionEvent(db, {
        name: `return_visit`,
        userId,
        properties: { day, path },
      })
    })().catch((err) => {
      console.error(`[conversion] return-visit capture failed:`, err)
    })
  } catch (err) {
    console.error(`[conversion] return-visit capture failed:`, err)
  }
}

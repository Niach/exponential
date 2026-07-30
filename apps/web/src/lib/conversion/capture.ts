import { db } from "@/db/connection"
import { dailyAnonymousId } from "@/lib/conversion/anonymous"
import {
  extractAttributionParams,
  externalReferrer,
  shouldCaptureLanding,
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

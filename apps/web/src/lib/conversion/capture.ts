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

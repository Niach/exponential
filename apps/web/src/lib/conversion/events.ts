// Type-only connection import: this module is pulled in by hot tRPC routers,
// and a runtime @/db/connection import would force DATABASE_URL onto every
// unit test that touches them. Callers always pass their own db/tx handle.
import type { db } from "@/db/connection"
import { conversionEvents } from "@/db/schema"

type Tx = Parameters<Parameters<typeof db.transaction>[0]>[0]
type Dbx = typeof db | Tx

// The full first-party funnel vocabulary (EXP-362). Stored as a documented
// varchar (see conversion_events in @exp/db-schema), typed here so writers
// can't invent names.
export type ConversionEventName =
  | `landing`
  | `signup`
  | `onboarding_completed`
  | `team_created`
  | `invite_sent`
  | `invite_accepted`
  | `first_issue_created`
  | `checkout_started`
  | `subscription_first_active`
  | `seats_updated`
  | `plan_changed`
  | `cancel_scheduled`
  | `subscription_resumed`
  | `subscription_canceled`

// Conversion tracking is a CLOUD-ONLY concern — self-hosted instances must
// not collect visitor/funnel analytics about their own users. Direct env
// read (same semantics as isCloudInstance in lib/bootstrap-cloud) so this
// module keeps zero runtime imports — it is pulled in by hot tRPC routers.
export function conversionTrackingEnabled(): boolean {
  return (process.env.CLOUD_INSTANCE ?? ``).toLowerCase() === `true`
}

// Append one funnel event. Idempotency lives in the DATABASE: the partial
// unique indexes on conversion_events define what "once" means (once per user
// for signup/first_issue_created, once per subscription for the paid
// lifecycle, once per visitor-day for landing) and the unconditional
// onConflictDoNothing turns every re-fire into a free no-op — no
// read-before-write on hot paths, race-proof against webhook redelivery.
//
// NEVER throws: analytics must not be able to break signup, checkout, or
// issue creation. Callers outside a transaction pass the global `db`.
export async function recordConversionEvent(
  dbx: Dbx,
  args: {
    name: ConversionEventName
    userId?: string | null
    anonymousId?: string | null
    properties?: Record<string, unknown> | null
  }
): Promise<void> {
  if (!conversionTrackingEnabled()) return
  try {
    await dbx
      .insert(conversionEvents)
      .values({
        name: args.name,
        userId: args.userId ?? null,
        anonymousId: args.anonymousId ?? null,
        properties: args.properties ?? null,
      })
      .onConflictDoNothing()
  } catch (err) {
    console.error(`[conversion] failed to record ${args.name}:`, err)
  }
}

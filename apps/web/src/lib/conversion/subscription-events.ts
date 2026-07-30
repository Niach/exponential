import { eq } from "drizzle-orm"
import { db } from "@/db/connection"
import { creem_subscriptions } from "@/db/schema"
import {
  type ConversionEventName,
  recordConversionEvent,
} from "@/lib/conversion/events"

// Creem lifecycle → conversion_events (EXP-362). Called from the Creem
// plugin's webhook callbacks AFTER bindSubscriptionToTeam, so the persisted
// row (referenceId → purchasing user, teamId, seats) exists and is bound by
// the time we resolve it. Webhooks re-deliver and grant events re-fire on
// every renewal — the once-per-sub partial unique index (keyed on
// properties.creemSubscriptionId) turns all but the first insert per name
// into no-ops, so this needs no read-your-writes bookkeeping.

export type SubscriptionLifecycleInput = {
  creemSubscriptionId?: string | null
  status?: string | null
  /** Checkout metadata echoed on the event — referenceId fallback only. */
  metadata?: Record<string, unknown> | null
  /** True only for the terminal events (canceled/expired). */
  terminal?: boolean
}

export type SubscriptionRowLookup = (creemSubscriptionId: string) => Promise<{
  userId: string | null
  teamId: string | null
  seats: number | null
  productId: string | null
} | null>

async function loadSubscriptionRow(
  creemSubscriptionId: string
): ReturnType<SubscriptionRowLookup> {
  const [row] = await db
    .select({
      userId: creem_subscriptions.referenceId,
      teamId: creem_subscriptions.teamId,
      seats: creem_subscriptions.seats,
      productId: creem_subscriptions.productId,
    })
    .from(creem_subscriptions)
    .where(eq(creem_subscriptions.creemSubscriptionId, creemSubscriptionId))
    .limit(1)
  return row ?? null
}

export function lifecycleEventName(
  input: SubscriptionLifecycleInput
): ConversionEventName | null {
  if (input.terminal) return `subscription_canceled`
  // No trial products exist (free + Team only) — `trialing` is deliberately
  // unmapped; if a trial tier ever ships, add a `trial_started` event.
  if (input.status === `active` || input.status === `paid`) {
    return `subscription_first_active`
  }
  return null
}

export async function recordSubscriptionLifecycleEvent(
  input: SubscriptionLifecycleInput,
  deps: {
    record?: typeof recordConversionEvent
    loadSubscription?: SubscriptionRowLookup
  } = {}
): Promise<void> {
  try {
    const creemSubscriptionId = input.creemSubscriptionId?.trim()
    if (!creemSubscriptionId) return
    const name = lifecycleEventName(input)
    if (!name) return

    const row = await (deps.loadSubscription ?? loadSubscriptionRow)(
      creemSubscriptionId
    )
    const metadataReference = input.metadata?.referenceId
    const userId =
      row?.userId ??
      (typeof metadataReference === `string` ? metadataReference : null)

    await (deps.record ?? recordConversionEvent)(db, {
      name,
      userId,
      properties: {
        creemSubscriptionId,
        ...(row?.teamId ? { teamId: row.teamId } : {}),
        ...(row?.seats ? { seats: row.seats } : {}),
        ...(row?.productId ? { productId: row.productId } : {}),
        ...(input.status ? { status: input.status } : {}),
      },
    })
  } catch (err) {
    console.error(`[conversion] subscription lifecycle event failed:`, err)
  }
}

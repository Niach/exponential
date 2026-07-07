import { and, desc, eq, inArray } from "drizzle-orm"
import { TRPCError } from "@trpc/server"
import { createCreemClient } from "@creem_io/better-auth/server"
import { db } from "@/db/connection"
import { creem_subscriptions } from "@/db/schema"

// Self-service subscription changes (seat count, plan switch) mutate the
// EXISTING Creem subscription instead of running a new checkout — a second
// checkout creates a second subscription, so the customer pays the full new
// price on top of what they already paid (the original double-billing bug).
//
// Update behavior: Creem's proration is measurably broken as of 2026-07-07
// (test mode, verified by direct API calls): on a unit INCREASE or a product
// upgrade, `proration-charge-immediately` AND `proration-charge` both charge
// `new config + old config` immediately instead of the `new − old` delta —
// e.g. adding a second $60/seat/yr seat on day one charged $180, and a
// $120/yr → $240/yr product upgrade charged $359.99. (Decreases refund the
// correct delta, so the bug is increase-only.) Until Creem fixes that math we
// use `proration-none` everywhere: the change applies immediately, and the
// next renewal invoice bills the new configuration. That knowingly gives away
// the remainder of the current period on upgrades — acceptable versus
// overcharging customers 3×. When Creem confirms a fix, flip this constant to
// `proration-charge-immediately`.
export const SUBSCRIPTION_UPDATE_BEHAVIOR = `proration-none` as const

// Statuses that count as "this workspace is subscribed" — mirrors the plan
// resolution in lib/billing.ts.
export const ACTIVE_SUBSCRIPTION_STATUSES = [`active`, `trialing`, `paid`]

export type WorkspaceSubscriptionRow = typeof creem_subscriptions.$inferSelect

/**
 * The workspace's single active workspace-bound subscription row, or `null`.
 * Same most-seats-wins tiebreak as getWorkspacePlan, so seat adjustments
 * always target the subscription that currently determines the plan.
 */
export async function getActiveWorkspaceSubscription(
  workspaceId: string
): Promise<WorkspaceSubscriptionRow | null> {
  const [row] = await db
    .select()
    .from(creem_subscriptions)
    .where(
      and(
        eq(creem_subscriptions.workspaceId, workspaceId),
        inArray(creem_subscriptions.status, ACTIVE_SUBSCRIPTION_STATUSES)
      )
    )
    .orderBy(desc(creem_subscriptions.seats))
    .limit(1)
  return row ?? null
}

/**
 * Assert a subscription row is mutable self-service. Throws tRPC errors with
 * user-facing messages; pure over the row so it is unit-testable.
 */
export function assertSubscriptionMutable(
  row: Pick<
    WorkspaceSubscriptionRow,
    `creemSubscriptionId` | `cancelAtPeriodEnd`
  > | null
): asserts row is WorkspaceSubscriptionRow {
  if (!row) {
    throw new TRPCError({
      code: `PRECONDITION_FAILED`,
      message: `This workspace has no active subscription`,
    })
  }
  if (!row.creemSubscriptionId) {
    throw new TRPCError({
      code: `PRECONDITION_FAILED`,
      message: `This subscription can't be changed automatically — contact support`,
    })
  }
  if (row.cancelAtPeriodEnd) {
    throw new TRPCError({
      code: `PRECONDITION_FAILED`,
      message: `This subscription is scheduled to cancel — resume it before changing it`,
    })
  }
}

export type CreemSubscriptionItem = {
  id?: string | null | undefined
  productId?: string | null | undefined
  priceId?: string | null | undefined
  units?: number | null | undefined
}

/**
 * Build the items payload for a seat-count update. Creem requires `id`,
 * `productId` AND `priceId` on the item — sending only the item id fails with
 * "Could not find product or price" (measured), and omitting the id would
 * CREATE a new line item instead of updating. Pure and unit-tested.
 */
export function buildSeatUpdateItems(
  items: CreemSubscriptionItem[] | null | undefined,
  seats: number
): Array<{ id: string; productId: string; priceId: string; units: number }> {
  if (!items || items.length !== 1) {
    throw new TRPCError({
      code: `PRECONDITION_FAILED`,
      message: `This subscription can't be changed automatically — contact support`,
    })
  }
  const item = items[0]
  if (!item.id || !item.productId || !item.priceId) {
    throw new TRPCError({
      code: `PRECONDITION_FAILED`,
      message: `This subscription can't be changed automatically — contact support`,
    })
  }
  return [
    { id: item.id, productId: item.productId, priceId: item.priceId, units: seats },
  ]
}

function creemClient() {
  const apiKey = process.env.CREEM_API_KEY
  if (!apiKey) {
    throw new TRPCError({
      code: `PRECONDITION_FAILED`,
      message: `Billing is not configured`,
    })
  }
  return createCreemClient({
    apiKey,
    testMode: apiKey.startsWith(`creem_test_`),
  })
}

/**
 * Set the seat count on an existing Creem subscription. Returns the units
 * Creem reports back. The `subscription.update` webhook re-binds our `seats`
 * column, but callers should also write it optimistically.
 */
export async function updateCreemSubscriptionSeats(
  creemSubscriptionId: string,
  seats: number
): Promise<number> {
  const creem = creemClient()
  const current = await creem.subscriptions.get(creemSubscriptionId)
  const items = buildSeatUpdateItems(current.items, seats)
  const updated = await creem.subscriptions.update(creemSubscriptionId, {
    items,
    updateBehavior: SUBSCRIPTION_UPDATE_BEHAVIOR,
  })
  return updated.items?.[0]?.units ?? seats
}

/**
 * Switch an existing subscription to a different product (tier or billing
 * cadence) via Creem's upgrade endpoint — never via a second checkout.
 */
export async function upgradeCreemSubscriptionProduct(
  creemSubscriptionId: string,
  productId: string
): Promise<void> {
  const creem = creemClient()
  await creem.subscriptions.upgrade(creemSubscriptionId, {
    productId,
    updateBehavior: SUBSCRIPTION_UPDATE_BEHAVIOR,
  })
}

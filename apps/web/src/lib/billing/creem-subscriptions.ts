import { and, desc, eq, inArray } from "drizzle-orm"
import { TRPCError } from "@trpc/server"
import { createCreemClient } from "@creem_io/better-auth/server"
import { db } from "@/db/connection"
import { creem_subscriptions } from "@/db/schema"
import { isCloudInstance } from "@/lib/bootstrap-cloud"

// Self-service subscription changes (seat count, plan switch) mutate the
// EXISTING Creem subscription instead of running a new checkout — a second
// checkout creates a second subscription, so the customer pays the full new
// price on top of what they already paid (the original double-billing bug).
//
// Update behavior: Creem's proration was measurably broken 2026-07-07..07-09
// (test mode, direct API calls): a unit INCREASE or product upgrade charged
// `new config + old config` (later ~7× the delta) instead of `new − old`, and
// by 07-09 the DECREASE path had regressed to charging instead of refunding.
// We pinned `proration-none` while that was true. Re-verified 2026-07-21 on the
// reference sub (`sub_6SozDXpVcqEKSm7BkibgOR`, Pro yearly $60/seat/yr, ~14.6d
// into the period): both paths are now correct — an increase 2→3 charged
// exactly the prorated delta ($57.60) and a decrease 3→2 refunded it in full.
// Both flip gates met, so we run `proration-charge-immediately`: seat/plan
// changes charge (or refund) the prorated delta at the moment of change. If
// Creem regresses again, revert to `proration-none` (never overcharges).
export const SUBSCRIPTION_UPDATE_BEHAVIOR = `proration-charge-immediately` as const

// Statuses that count as "this team is subscribed" — mirrors the plan
// resolution in lib/billing.ts.
export const ACTIVE_SUBSCRIPTION_STATUSES = [`active`, `trialing`, `paid`]

export type TeamSubscriptionRow = typeof creem_subscriptions.$inferSelect

/**
 * The team's single active team-bound subscription row, or `null`.
 * Same most-seats-wins tiebreak as getTeamPlan, so seat adjustments
 * always target the subscription that currently determines the plan.
 */
export async function getActiveTeamSubscription(
  teamId: string
): Promise<TeamSubscriptionRow | null> {
  const [row] = await db
    .select()
    .from(creem_subscriptions)
    .where(
      and(
        eq(creem_subscriptions.teamId, teamId),
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
    TeamSubscriptionRow,
    `creemSubscriptionId` | `cancelAtPeriodEnd`
  > | null
): asserts row is TeamSubscriptionRow {
  if (!row) {
    throw new TRPCError({
      code: `PRECONDITION_FAILED`,
      message: `This team has no active subscription`,
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

// ── Cancel-on-delete (go-live audit) ─────────────────────────────────────────
// Deleting a team (or an account) must not leave a paying ghost
// subscription behind in Creem. The local FKs make the rows unfindable after
// the delete — `team_id` goes `set null` on team delete and the
// buyer FK (`reference_id`) CASCADES on user delete — so callers capture the
// affected rows with the find* helpers BEFORE deleting, run the local delete,
// and then cancel remotely with cancelCreemSubscriptionsBestEffort AFTER the
// transaction commits. That ordering guarantees a Creem API failure can never
// leave the team half-deleted (an orphaned REMOTE subscription is
// recoverable from the Creem dashboard; it is logged loudly), and a failed
// local delete never cancels a subscription the customer still uses.

export type CancellableSubscription = Pick<
  TeamSubscriptionRow,
  `id` | `creemSubscriptionId`
>

/**
 * Active subscription rows bound to any of the given teams. Capture
 * BEFORE deleting the teams (see the cancel-on-delete note above).
 * Accepts an optional transaction so the user-deletion cascade can capture
 * subscriptions for the solo teams it deletes in the same tx.
 */
export async function findActiveSubscriptionsForTeams(
  teamIds: string[],
  executor: Pick<typeof db, `select`> = db
): Promise<CancellableSubscription[]> {
  if (!isCloudInstance() || teamIds.length === 0) return []
  return await executor
    .select({
      id: creem_subscriptions.id,
      creemSubscriptionId: creem_subscriptions.creemSubscriptionId,
    })
    .from(creem_subscriptions)
    .where(
      and(
        inArray(creem_subscriptions.teamId, teamIds),
        inArray(creem_subscriptions.status, ACTIVE_SUBSCRIPTION_STATUSES)
      )
    )
}

/**
 * Active subscription rows PURCHASED by the user (`referenceId` — the Creem
 * customer is this user's email/card). Capture BEFORE deleting the user (see
 * the cancel-on-delete note above). This covers the user's solo teams
 * AND surviving teams they paid for: once the account is gone nobody can
 * manage the subscription and the deleted user's card would keep being
 * charged, so those cancel too (a remaining owner re-subscribes with their
 * own payment method).
 */
export async function findActiveSubscriptionsForUser(
  userId: string
): Promise<CancellableSubscription[]> {
  if (!isCloudInstance()) return []
  return await db
    .select({
      id: creem_subscriptions.id,
      creemSubscriptionId: creem_subscriptions.creemSubscriptionId,
    })
    .from(creem_subscriptions)
    .where(
      and(
        eq(creem_subscriptions.referenceId, userId),
        inArray(creem_subscriptions.status, ACTIVE_SUBSCRIPTION_STATUSES)
      )
    )
}

/**
 * Best-effort remote cancellation for the delete paths. NEVER throws — every
 * failure is logged with the Creem subscription id so it can be cancelled
 * manually from the dashboard. Cancellation is immediate (not scheduled): the
 * backing team/account no longer exists, so there is nothing to keep
 * serving until period end.
 */
export async function cancelCreemSubscriptionsBestEffort(
  subscriptions: CancellableSubscription[]
): Promise<void> {
  if (!isCloudInstance() || subscriptions.length === 0) return
  if (!process.env.CREEM_API_KEY) {
    console.error(
      `[billing] cannot cancel ${subscriptions.length} Creem subscription(s) — CREEM_API_KEY is not configured. Cancel manually in the Creem dashboard: ${subscriptions
        .map((s) => s.creemSubscriptionId ?? s.id)
        .join(`, `)}`
    )
    return
  }
  let creem: ReturnType<typeof creemClient>
  try {
    creem = creemClient()
  } catch (err) {
    console.error(`[billing] could not create the Creem client:`, err)
    return
  }
  for (const sub of subscriptions) {
    if (!sub.creemSubscriptionId) {
      // Legacy row with no remote id — nothing to cancel remotely.
      console.error(
        `[billing] subscription row ${sub.id} has no Creem subscription id — verify manually in the Creem dashboard`
      )
      continue
    }
    try {
      await creem.subscriptions.cancel(sub.creemSubscriptionId, {
        mode: `immediate`,
      })
    } catch (err) {
      console.error(
        `[billing] failed to cancel Creem subscription ${sub.creemSubscriptionId} — cancel manually in the Creem dashboard:`,
        err
      )
      continue
    }
    try {
      // Optimistic local write; the `subscription.canceled` webhook confirms
      // it. On the account-deletion path the row is already gone (buyer FK
      // cascade), so this is a harmless zero-row update.
      await db
        .update(creem_subscriptions)
        .set({ status: `canceled`, updatedAt: new Date() })
        .where(eq(creem_subscriptions.id, sub.id))
    } catch (err) {
      console.warn(
        `[billing] could not mark subscription ${sub.id} canceled locally (webhook will confirm):`,
        err
      )
    }
  }
}

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
// resolution in lib/billing.ts (ACTIVE_STATUSES; the two lists are asserted
// equal by creem-subscriptions.test.ts).
//
// `scheduled_cancel` (Creem's status between "cancel at period end" and the
// period actually ending — the plugin's webhook persistence writes
// `event.object.status` verbatim) MUST be in here: the customer paid through
// `periodEnd` and keeps every seat, byte and feature until then. Treating it
// as inactive would drop the team to Free the instant they clicked Cancel and
// — because getActiveTeamSubscription would return null — also hide the
// pending-cancel banner and break Resume. "Pending cancel" is a separate axis
// from "active": derive it with isSubscriptionPendingCancel
// (lib/billing/billing-handover.ts), which owns SCHEDULED_CANCEL_STATUS.
export const ACTIVE_SUBSCRIPTION_STATUSES = [
  `active`,
  `trialing`,
  `paid`,
  `scheduled_cancel`,
]

export type TeamSubscriptionRow = typeof creem_subscriptions.$inferSelect

/**
 * EVERY active team-bound subscription row, most seats first. Normally there
 * is at most one (the invariant creem-binding.ts enforces where the money
 * lands), but the REV2-30 duplicate window can transiently leave two — so any
 * check that must not MISS a live subscription (the team-delete gate) reads
 * this, while checks that need the one row the plan derives from take the head
 * via getActiveTeamSubscription.
 */
export async function listActiveTeamSubscriptions(
  teamId: string
): Promise<TeamSubscriptionRow[]> {
  return await db
    .select()
    .from(creem_subscriptions)
    .where(
      and(
        eq(creem_subscriptions.teamId, teamId),
        inArray(creem_subscriptions.status, ACTIVE_SUBSCRIPTION_STATUSES)
      )
    )
    .orderBy(desc(creem_subscriptions.seats))
}

/**
 * The team's single active team-bound subscription row, or `null`.
 * Same most-seats-wins tiebreak as getTeamPlan, so seat adjustments
 * always target the subscription that currently determines the plan.
 */
export async function getActiveTeamSubscription(
  teamId: string
): Promise<TeamSubscriptionRow | null> {
  const [row] = await listActiveTeamSubscriptions(teamId)
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

/**
 * Schedule cancellation at the end of the paid period (the self-service
 * "Cancel subscription" path). The customer keeps everything they paid for
 * until `periodEnd`; Creem's `subscription.canceled` webhook then flips the
 * local status. Never `immediate` — that would forfeit paid time.
 */
export async function scheduleCreemSubscriptionCancellation(
  creemSubscriptionId: string
): Promise<void> {
  const creem = creemClient()
  await creem.subscriptions.cancel(creemSubscriptionId, {
    mode: `scheduled`,
    onExecute: `cancel`,
  })
}

/**
 * Undo a scheduled cancellation (Creem status `scheduled_cancel` → active).
 * Pairs with scheduleCreemSubscriptionCancellation so a mis-click is
 * self-service recoverable — assertSubscriptionMutable refuses seat/plan
 * changes while a cancellation is pending.
 */
export async function resumeCreemSubscription(
  creemSubscriptionId: string
): Promise<void> {
  const creem = creemClient()
  await creem.subscriptions.resume(creemSubscriptionId)
}

// ── Cancel-on-delete (go-live audit) ─────────────────────────────────────────
// Destroying a TEAM must not leave a paying ghost subscription behind in
// Creem: `team_id` goes `set null` on team delete, after which the row is
// unfindable by team. Deleting a team through teams.delete/admin.deleteTeam is
// therefore GATED on the subscription being cancelled first
// (lib/billing/billing-handover.ts); the one path that still destroys a
// PAYING team is account deletion killing a solo team, which captures the
// affected rows with findActiveSubscriptionsForTeams BEFORE the delete and
// cancels them with cancelCreemSubscriptionsBestEffort AFTER the transaction
// commits. That ordering guarantees a Creem API failure can never leave the
// account half-deleted (an orphaned REMOTE subscription is recoverable from
// the Creem dashboard; it is logged loudly), and a failed local delete never
// cancels a subscription the customer still uses.
//
// Deleting an ACCOUNT never cancels a SURVIVING team's subscription — the
// subscription belongs to the team, and `reference_id` is `set null` so the
// row (and the team's plan) outlives its purchaser. See billing-handover.ts.

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

// NOTE (REV2-55): there is deliberately NO findActiveSubscriptionsForUser
// buyer-scoped capture anymore. Deleting an account used to cancel every
// subscription the person had ever purchased — including the ones funding
// teams that survive them — which dropped a live shared team to Free
// mid-period with no warning. A subscription belongs to its team; only the
// team-scoped capture above may feed a cancellation.

/**
 * Best-effort remote cancellation of subscriptions that must stop being
 * charged NOW. NEVER throws — every failure is logged with the Creem
 * subscription id so it can be cancelled manually from the dashboard.
 *
 * TWO callers, both cancelling `immediate` rather than `scheduled`:
 *
 * 1. Account deletion killing a SOLO team (the one surviving
 *    destroy-a-paying-team path — see the note above). The backing team no
 *    longer exists, so there is nothing left to serve until period end.
 * 2. The one-subscription-per-team enforcement in creem-binding.ts: a
 *    duplicate subscription bound to a team that still EXISTS and is still
 *    fully served — by the incumbent the duplicate competes with. The
 *    duplicate funds nothing on top of that, so every day it stays open is
 *    pure double-charge; `scheduled` would leave it live (and the invariant
 *    broken) until period end, which on the yearly-only Pro plan is up to a
 *    year away, long past any refund window for the charge that just landed.
 *
 * Self-service cancellation is the opposite case and uses
 * scheduleCreemSubscriptionCancellation instead: the customer is still being
 * served by that exact subscription, so it preserves the paid period.
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
      // it. The row itself survives every delete path (`team_id` and
      // `reference_id` are both `set null`), so this keeps the billing-history
      // row honest about what happened to it.
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

import { TRPCError } from "@trpc/server"
import {
  getActiveTeamSubscription,
  type TeamSubscriptionRow,
} from "@/lib/billing/creem-subscriptions"
import { isCloudInstance } from "@/lib/bootstrap-cloud"

// ── Who owns a subscription, and what may destroy it (REV2-55) ───────────────
//
// A subscription belongs to the TEAM, not to the person who paid for it.
// Everything below follows from that single rule:
//
// 1. Deleting an ACCOUNT never cancels a surviving team's subscription. The
//    buyer FK (`creem_subscriptions.reference_id`) is `set null`, so the row
//    outlives its purchaser with its team binding, seats and billing history
//    intact, and the team's remaining owners keep managing it through the
//    team-scoped billing router (nothing there reads `reference_id`). Account
//    deletion must NEVER be blocked or complicated by billing — App Store
//    5.1.1(v) / Play data-deletion require it to always go through, and
//    billing is web-only so a mobile user could not resolve a gate anyway.
//    The ONE subscription an account deletion still cancels is one bound to a
//    SOLO team the deletion itself destroys: the team is gone, so nothing is
//    left to serve and the card must stop being charged.
//
// 2. Deleting a TEAM is gated the other way round: the team is the paying
//    entity, so its subscription must be cancelled FIRST. Without the gate a
//    delete would leave a paying ghost in Creem (the local row's `team_id`
//    goes `set null`, after which nothing can find it) — the owner cancels in
//    team settings → Billing, then deletes. A subscription already scheduled
//    to cancel at period end does NOT block: the customer keeps the time they
//    paid for, no renewal follows, and the team may go now.

/** The exact copy every client (web + native) surfaces for the delete gate. */
export const TEAM_DELETE_ACTIVE_SUBSCRIPTION_MESSAGE = `This team has an active subscription — cancel the subscription in team settings → Billing before deleting the team`

/**
 * Creem's status for "cancellation scheduled, still paid through periodEnd"
 * (vocabulary: active | canceled | unpaid | paused | trialing |
 * scheduled_cancel). It lands in our `status` column verbatim via the plugin's
 * webhook persistence. Defined here — not in creem-subscriptions.ts — so the
 * pure predicate below stays free of that module's DB/Creem-client imports.
 */
export const SCHEDULED_CANCEL_STATUS = `scheduled_cancel`

export type PendingCancelSubscription = Pick<
  TeamSubscriptionRow,
  `cancelAtPeriodEnd` | `status`
>

export type TeamDeleteBillingGate = PendingCancelSubscription | null

/**
 * Is a cancellation already scheduled for this subscription? TWO signals,
 * either of which is enough:
 *
 * - `cancelAtPeriodEnd` — our own optimistic column, written by
 *   billing.cancelSubscription (the Creem plugin never persists it).
 * - `status === 'scheduled_cancel'` — Creem's own state, written verbatim by
 *   the plugin's webhook persistence. It is the ONLY signal when the
 *   cancellation was scheduled outside our UI (Creem dashboard, support), and
 *   the authoritative one after a lost/racing optimistic write.
 *
 * Everything user-visible about a pending cancellation derives from this: the
 * team-delete gate, the billing banner, and the Resume affordance.
 */
export function isSubscriptionPendingCancel(
  subscription: PendingCancelSubscription | null | undefined
): boolean {
  if (!subscription) return false
  return (
    subscription.cancelAtPeriodEnd ||
    subscription.status === SCHEDULED_CANCEL_STATUS
  )
}

/**
 * Pure gate: does this team's subscription row block deleting the team?
 * Only a LIVE subscription does — `null` (no subscription, or none in an
 * active status) and an already-scheduled cancellation both pass.
 */
export function subscriptionBlocksTeamDelete(
  subscription: TeamDeleteBillingGate
): boolean {
  return Boolean(subscription && !isSubscriptionPendingCancel(subscription))
}

/** Pure throwing form of subscriptionBlocksTeamDelete. */
export function assertTeamSubscriptionCancelled(
  subscription: TeamDeleteBillingGate
): void {
  if (subscriptionBlocksTeamDelete(subscription)) {
    throw new TRPCError({
      code: `PRECONDITION_FAILED`,
      message: TEAM_DELETE_ACTIVE_SUBSCRIPTION_MESSAGE,
    })
  }
}

/**
 * The DB-backed gate both team-deletion paths run (teams.delete and
 * admin.deleteTeam) BEFORE touching anything. Self-hosted instances have no
 * subscriptions at all, so the check is skipped there.
 */
export async function assertTeamDeletableBilling(
  teamId: string
): Promise<void> {
  if (!isCloudInstance()) return
  assertTeamSubscriptionCancelled(await getActiveTeamSubscription(teamId))
}

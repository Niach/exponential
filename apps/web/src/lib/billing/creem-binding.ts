import { and, eq, inArray, or } from "drizzle-orm"
import { db } from "@/db/connection"
import { creem_subscriptions } from "@/db/schema"
import { isSubscriptionPendingCancel } from "@/lib/billing/billing-handover"
import {
  ACTIVE_SUBSCRIPTION_STATUSES,
  cancelCreemSubscriptionsBestEffort,
} from "@/lib/billing/creem-subscriptions"

// Per-seat subscription→team binding (masterplan v5 §3.3, L19/L20).
//
// The `@creem_io/better-auth` plugin natively supports both a seat quantity
// (`units`) and arbitrary checkout `metadata`, and echoes that metadata back on
// every webhook event (`checkout.completed`, `subscription.*`). We pass
// `metadata.teamId` + `metadata.seats` (and `units: seats`) at checkout
// time; the plugin persists the base `creem_subscriptions` row (referenceId →
// user, productId, status, period, creemSubscriptionId) but NEVER writes our
// `teamId`/`seats` columns. This module reads the metadata off the webhook
// event and binds those two columns onto the already-persisted row, matched by
// `creemSubscriptionId`. Because the plugin's own updates only ever set the
// enumerated columns, a later webhook update cannot clobber the binding.

export type SubscriptionBindingInput = {
  /** Checkout/subscription metadata echoed back by Creem. */
  metadata?: Record<string, unknown> | null
  /** The Creem subscription id — the row key we bind onto. */
  creemSubscriptionId?: string | null
  /**
   * Seat quantity Creem reports on the entity itself (checkout `units` or a
   * subscription item's `units`). This is what was actually purchased, so it
   * is AUTHORITATIVE — `metadata.seats` (client-suppliable at checkout time)
   * is only a fallback for events that omit units.
   */
  units?: number | null
}

export type TeamBinding = {
  creemSubscriptionId: string
  teamId: string
  seats: number
}

/** Commit sink — abstracted so the binding logic is testable without drizzle. */
export type BindCommit = (binding: TeamBinding) => Promise<void>

/** The columns the one-subscription-per-team guard reasons over. */
export type TeamBoundSubscription = {
  id: string
  creemSubscriptionId: string | null
  status: string
  seats: number
  cancelAtPeriodEnd: boolean
  createdAt: Date
}

/** Loads the team's active subscriptions plus the incoming row itself. */
export type LoadTeamSubscriptions = (
  teamId: string,
  creemSubscriptionId: string
) => Promise<TeamBoundSubscription[]>

/** Remote-cancel sink (the real one is cancelCreemSubscriptionsBestEffort). */
export type CancelSubscriptions = (
  subscriptions: Array<{ id: string; creemSubscriptionId: string | null }>
) => Promise<void>

export type BindOptions = {
  commit?: BindCommit
  loadTeamSubscriptions?: LoadTeamSubscriptions
  cancelSubscriptions?: CancelSubscriptions
}

function toPositiveInt(value: unknown): number | null {
  const n =
    typeof value === `number`
      ? value
      : typeof value === `string`
        ? Number.parseInt(value, 10)
        : NaN
  return Number.isInteger(n) && n > 0 ? n : null
}

/**
 * Resolve the team binding from a checkout/subscription payload, or
 * `null` when it isn't bindable (no subscription id, or no `teamId` in
 * metadata — e.g. a legacy checkout created before this path existed). Seats
 * resolve entity `units` → metadata `seats` → 1, so the row is never left at
 * 0. `units` MUST win when both are present: it is the quantity Creem
 * actually charged for, while `metadata` is client-suppliable at checkout
 * time (a forged `metadata.seats` must never out-vote the paid quantity).
 */
export function extractTeamBinding(
  input: SubscriptionBindingInput
): TeamBinding | null {
  const creemSubscriptionId = input.creemSubscriptionId?.trim()
  if (!creemSubscriptionId) return null

  const rawTeamId = input.metadata?.teamId
  const teamId =
    typeof rawTeamId === `string` ? rawTeamId.trim() : ``
  if (!teamId) return null

  const seats =
    toPositiveInt(input.units) ?? toPositiveInt(input.metadata?.seats) ?? 1

  return { creemSubscriptionId, teamId, seats }
}

// ── One subscription per team, enforced where the money lands (REV2-30) ──────
// billing.createSeatCheckout refuses to MINT a second checkout while the team
// already has an active subscription, but that guard reads a row that only
// exists after the webhook — so anything minted before the first webhook lands
// (delayed webhook → owner retries and pays again; an abandoned checkout link
// paid days later) sails through and binds a SECOND paying subscription onto
// the team. Nothing surfaced that state: getTeamPlan silently takes the row
// with the most seats and the card keeps being charged for the loser. So the
// invariant is re-checked HERE, on the path a paid subscription actually
// arrives on, and the duplicate is cancelled remotely.

function isActiveSubscription(row: TeamBoundSubscription): boolean {
  return ACTIVE_SUBSCRIPTION_STATUSES.includes(row.status)
}

/**
 * Which of a team's competing active subscriptions must be cancelled: keep the
 * OLDEST, cancel everything newer. Pure, so the tiebreaks are unit-testable.
 *
 * The oldest wins because it is the one the customer has actually been living
 * on; a newer duplicate is the accidental second purchase and is the one whose
 * charge is freshest (so most recoverable). Ordering by `createdAt` also makes
 * the verdict independent of WHICH subscription's webhook triggered the check.
 *
 * Two deliberate abstentions — both return "cancel nothing" so the caller only
 * logs:
 * - the incoming subscription isn't live (not an active status, or already
 *   scheduled to cancel): nothing is being double-charged going forward.
 * - every incumbent is already scheduled to cancel: the new subscription is a
 *   legitimate replacement for a dying one, and cancelling it would leave the
 *   team with only the subscription that is about to end.
 */
export function pickDuplicateSubscriptionsToCancel(
  incoming: TeamBoundSubscription | null,
  others: TeamBoundSubscription[]
): TeamBoundSubscription[] {
  if (!incoming) return []
  if (!isActiveSubscription(incoming) || isSubscriptionPendingCancel(incoming)) {
    return []
  }
  const live = others.filter(
    (row) => isActiveSubscription(row) && !isSubscriptionPendingCancel(row)
  )
  if (live.length === 0) return []
  // `incoming` goes last so a createdAt tie resolves against it (sort is
  // stable), i.e. the incumbent is kept.
  const oldestFirst = [...live, incoming].sort(
    (a, b) => a.createdAt.getTime() - b.createdAt.getTime()
  )
  return oldestFirst.slice(1)
}

function describeSubscription(row: TeamBoundSubscription): string {
  return `${row.creemSubscriptionId ?? `(no remote id, row ${row.id})`} [${row.status}, ${row.seats} seats, created ${row.createdAt.toISOString()}]`
}

async function enforceSingleTeamSubscription(
  binding: TeamBinding,
  load: LoadTeamSubscriptions,
  cancel: CancelSubscriptions
): Promise<void> {
  try {
    const rows = await load(binding.teamId, binding.creemSubscriptionId)
    const incoming =
      rows.find(
        (row) => row.creemSubscriptionId === binding.creemSubscriptionId
      ) ?? null
    const others = rows.filter(
      (row) =>
        row.creemSubscriptionId !== binding.creemSubscriptionId &&
        isActiveSubscription(row)
    )
    // The anomaly is TWO live subscriptions on one team. An incoming row that
    // is no longer active is not one of them — which is also what keeps the
    // webhook our own cancellation triggers from re-reporting a state we just
    // resolved.
    const live =
      incoming && isActiveSubscription(incoming) ? [...others, incoming] : others
    if (live.length < 2) return

    const doomed = pickDuplicateSubscriptionsToCancel(incoming, others)
    console.error(
      `[billing] team ${binding.teamId} has ${live.length} active subscriptions bound to it: ${live.map(describeSubscription).join(`, `)}. ${
        doomed.length > 0
          ? `Cancelling the newer duplicate(s): ${doomed.map((row) => row.creemSubscriptionId ?? row.id).join(`, `)}`
          : `Cancelling nothing automatically — resolve it in the Creem dashboard`
      }`
    )
    if (doomed.length > 0) await cancel(doomed)
  } catch (err) {
    // Never let the duplicate check break the binding itself: an unbound row is
    // a team with no plan, which is worse than an undetected duplicate.
    console.error(
      `[billing] could not check team ${binding.teamId} for duplicate subscriptions:`,
      err
    )
  }
}

/** The team's active subscriptions plus the incoming row (whatever its status). */
async function loadTeamSubscriptionsFromDb(
  teamId: string,
  creemSubscriptionId: string
): Promise<TeamBoundSubscription[]> {
  return await db
    .select({
      id: creem_subscriptions.id,
      creemSubscriptionId: creem_subscriptions.creemSubscriptionId,
      status: creem_subscriptions.status,
      seats: creem_subscriptions.seats,
      cancelAtPeriodEnd: creem_subscriptions.cancelAtPeriodEnd,
      createdAt: creem_subscriptions.createdAt,
    })
    .from(creem_subscriptions)
    .where(
      or(
        and(
          eq(creem_subscriptions.teamId, teamId),
          inArray(creem_subscriptions.status, ACTIVE_SUBSCRIPTION_STATUSES)
        ),
        eq(creem_subscriptions.creemSubscriptionId, creemSubscriptionId)
      )
    )
}

/**
 * Bind a persisted `creem_subscriptions` row to its team + seat count.
 * Returns the applied binding, or `null` when the payload wasn't bindable.
 * Before committing, the one-subscription-per-team invariant is re-checked
 * against the DB (see above). All three sinks default to the real
 * DB/Creem ones; tests inject fakes.
 */
export async function bindSubscriptionToTeam(
  input: SubscriptionBindingInput,
  options: BindOptions = {}
): Promise<TeamBinding | null> {
  const binding = extractTeamBinding(input)
  if (!binding) return null
  await enforceSingleTeamSubscription(
    binding,
    options.loadTeamSubscriptions ?? loadTeamSubscriptionsFromDb,
    options.cancelSubscriptions ?? cancelCreemSubscriptionsBestEffort
  )
  await (options.commit ?? commitBindingToDb)(binding)
  return binding
}

async function commitBindingToDb(binding: TeamBinding): Promise<void> {
  await db
    .update(creem_subscriptions)
    .set({ teamId: binding.teamId, seats: binding.seats })
    .where(
      eq(creem_subscriptions.creemSubscriptionId, binding.creemSubscriptionId)
    )
}

/**
 * Map a flattened `checkout.completed` webhook payload to a binding input.
 * The nested subscription carries the id; `units` sits on the checkout entity.
 */
export function bindingInputFromCheckout(event: {
  units?: number | null
  metadata?: Record<string, unknown> | null
  subscription?: { id?: string | null } | string | null
}): SubscriptionBindingInput {
  const creemSubscriptionId =
    typeof event.subscription === `string`
      ? event.subscription
      : (event.subscription?.id ?? null)
  return {
    creemSubscriptionId,
    metadata: event.metadata ?? null,
    units: event.units ?? null,
  }
}

/**
 * Map a flattened `subscription.*` webhook payload (the `onGrantAccess` /
 * `onSubscription*` context) to a binding input. Seat quantity, when present,
 * lives on the first subscription item's `units`.
 */
export function bindingInputFromSubscription(event: {
  id?: string | null
  metadata?: Record<string, unknown> | null
  items?: Array<{ units?: number | null }> | null
}): SubscriptionBindingInput {
  return {
    creemSubscriptionId: event.id ?? null,
    metadata: event.metadata ?? null,
    units: event.items?.[0]?.units ?? null,
  }
}

import { and, eq, inArray, or } from "drizzle-orm"
import { db } from "@/db/connection"
import { creem_subscriptions } from "@/db/schema"
import {
  isSubscriptionPendingCancel,
  SCHEDULED_CANCEL_STATUS,
} from "@/lib/billing/billing-handover"
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
// event and binds those two columns onto a row keyed STRICTLY by
// `creemSubscriptionId` — upserted, not merely updated, because the plugin's
// row cannot be trusted to exist or to stay keyed (REV-12):
//
// - The plugin's subscription-event persistence, when its id lookup misses
//   (a subscription.* event landing before its checkout.completed), falls
//   back to matching by creemCustomerId and writes the NEW id onto whatever
//   row it finds — re-keying the customer's existing subscription row and
//   silently merging two paying subscriptions into one. The
//   reject_creem_subscription_rekey trigger (db/out/custom/0001_triggers.sql)
//   aborts that misdirected update, and the unique index
//   uniq_creem_subscriptions_creem_subscription_id pins one row per
//   subscription.
// - With the theft blocked, the out-of-order event leaves NO row for the new
//   subscription — so the commit below INSERTs one (seeded from the event)
//   when none exists, keeping the plan, the REV2-30 duplicate guard, the
//   team-delete gate and the admin console aware of every paying
//   subscription even before checkout.completed lands and heals the full row.
//
// The plugin's own updates only ever set its enumerated columns, so with the
// key immutable a later webhook update cannot clobber the binding.

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
  /**
   * The subscription's own status as this event reports it (`subscription.*`
   * events only — a checkout's status describes the CHECKOUT, not the
   * subscription, so bindingInputFromCheckout never fills this in). Used
   * solely to heal a stale pending-cancel flag and to seed a missing row;
   * on an EXISTING row the plugin owns the `status` column itself.
   */
  status?: string | null
  /**
   * Creem customer + product ids off the event — seed values for the
   * insert half of the commit upsert (REV-12), so a row this module creates
   * still reaches the customer portal (keyed by creemCustomerId) and plan
   * resolution. Never written onto an existing row.
   */
  creemCustomerId?: string | null
  productId?: string | null
}

export type TeamBinding = {
  creemSubscriptionId: string
  teamId: string
  seats: number
}

/**
 * What the commit must write alongside the binding. Kept off [`TeamBinding`]
 * (which stays a pure description of subscription→team) because it is healing,
 * not binding.
 */
export type BindPatch = {
  /** Reset our optimistic `cancelAtPeriodEnd` mirror to false. */
  clearPendingCancel: boolean
  /**
   * Values for the INSERT half of the commit upsert — used only when no row
   * exists yet for this creemSubscriptionId (the plugin's persistence missed
   * or was blocked from handling the event, REV-12). Enough for the row to be
   * plan-resolvable and portal-reachable; the subscription's later webhooks
   * (checkout.completed above all) heal the rest through the plugin.
   */
  seed: SubscriptionRowSeed
}

export type SubscriptionRowSeed = {
  /** The buyer (Better Auth user id), off `metadata.referenceId`. */
  referenceId: string | null
  creemCustomerId: string | null
  productId: string | null
  status: string | null
}

/** Commit sink — abstracted so the binding logic is testable without drizzle. */
export type BindCommit = (
  binding: TeamBinding,
  patch: BindPatch
) => Promise<void>

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
    // Never let the duplicate check fail the webhook: the binding is already
    // committed, and a delivery retry would re-run the same failing check.
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

// ── Healing a stale pending-cancel flag ──────────────────────────────────────
// `cancelAtPeriodEnd` is OUR optimistic mirror: billing.cancelSubscription
// writes true, billing.resumeSubscription writes false, and the Creem plugin
// never touches the column. So a cancellation scheduled AND then resumed
// outside our UI (Creem dashboard, support) leaves the flag stuck true forever
// while Creem's own status goes back to `active` — and everything that reads
// isSubscriptionPendingCancel stays wrong with it: the billing banner keeps
// claiming the subscription is ending, and assertSubscriptionMutable keeps
// refusing seat and plan changes with "resume it before changing it", for a
// subscription that IS resumed. Nothing the owner can click fixes it (Resume
// is a no-op on an already-active Creem subscription).
//
// The webhook is the one place that learns about an out-of-band resume, so it
// heals the flag there. Safe against racing our own optimistic write: the
// authoritative pending-cancel signal is the `status` column (Creem flips it
// to scheduled_cancel and webhooks that back), and isSubscriptionPendingCancel
// reads BOTH signals — so a stale-status webhook that clears the flag a moment
// early is re-covered by the scheduled_cancel status that follows.

/**
 * Does an incoming subscription status mean "live, and NOT scheduled to
 * cancel" — i.e. may a pending-cancel flag be cleared? Only an ACTIVE status
 * qualifies: `canceled`/`expired`/`paused` say nothing about a scheduled
 * cancellation, and `scheduled_cancel` is the pending state itself.
 */
export function statusClearsPendingCancel(
  status: string | null | undefined
): boolean {
  if (!status) return false
  return (
    status !== SCHEDULED_CANCEL_STATUS &&
    ACTIVE_SUBSCRIPTION_STATUSES.includes(status)
  )
}

function trimmedOrNull(value: string | null | undefined): string | null {
  const trimmed = value?.trim()
  return trimmed ? trimmed : null
}

/**
 * Bind a `creem_subscriptions` row to its team + seat count. Returns the
 * applied binding, or `null` when the payload wasn't bindable. The commit is
 * an upsert keyed strictly by `creemSubscriptionId` (see the module note —
 * the plugin's row may not exist for an out-of-order event, REV-12) and runs
 * FIRST, so a row it creates participates in the one-subscription-per-team
 * re-check (see above) within the same webhook delivery. The commit
 * additionally heals a stale pending-cancel flag when the event reports a
 * live status. All three sinks default to the real DB/Creem ones; tests
 * inject fakes.
 */
export async function bindSubscriptionToTeam(
  input: SubscriptionBindingInput,
  options: BindOptions = {}
): Promise<TeamBinding | null> {
  const binding = extractTeamBinding(input)
  if (!binding) return null
  const rawReferenceId = input.metadata?.referenceId
  await (options.commit ?? commitBindingToDb)(binding, {
    clearPendingCancel: statusClearsPendingCancel(input.status),
    seed: {
      referenceId:
        typeof rawReferenceId === `string` ? trimmedOrNull(rawReferenceId) : null,
      creemCustomerId: trimmedOrNull(input.creemCustomerId),
      productId: trimmedOrNull(input.productId),
      status: trimmedOrNull(input.status),
    },
  })
  await enforceSingleTeamSubscription(
    binding,
    options.loadTeamSubscriptions ?? loadTeamSubscriptionsFromDb,
    options.cancelSubscriptions ?? cancelCreemSubscriptionsBestEffort
  )
  return binding
}

async function commitBindingToDb(
  binding: TeamBinding,
  patch: BindPatch
): Promise<void> {
  const bindColumns = {
    teamId: binding.teamId,
    seats: binding.seats,
    ...(patch.clearPendingCancel ? { cancelAtPeriodEnd: false } : {}),
  }
  const updated = await db
    .update(creem_subscriptions)
    .set(bindColumns)
    .where(
      eq(creem_subscriptions.creemSubscriptionId, binding.creemSubscriptionId)
    )
    .returning({ id: creem_subscriptions.id })
  if (updated.length > 0) return
  // No row for this subscription yet: its subscription.* event outran its
  // checkout.completed (the plugin's misdirected customer-id fallback is
  // blocked by the reject_creem_subscription_rekey trigger), or the plugin's
  // persistence failed. Create the row ourselves so the plan, the REV2-30
  // duplicate guard and the delete gates see the paying subscription NOW;
  // checkout.completed heals the full row data when it lands. The status
  // seed defaults to the schema's `pending` for status-less (checkout)
  // payloads. `onConflictDoUpdate` (arbiter: the unique
  // creem_subscription_id index) settles the race against a concurrent
  // webhook delivery creating the row between the update above and this.
  console.error(
    `[billing] creating creem_subscriptions row for ${binding.creemSubscriptionId} from the bind path — the plugin has no row for it yet (out-of-order webhook delivery?)`
  )
  await db
    .insert(creem_subscriptions)
    .values({
      id: crypto.randomUUID(),
      creemSubscriptionId: binding.creemSubscriptionId,
      teamId: binding.teamId,
      seats: binding.seats,
      referenceId: patch.seed.referenceId,
      creemCustomerId: patch.seed.creemCustomerId,
      // NOT NULL; the plugin itself falls back to `` when a payload omits it.
      productId: patch.seed.productId ?? ``,
      ...(patch.seed.status ? { status: patch.seed.status } : {}),
    })
    .onConflictDoUpdate({
      target: creem_subscriptions.creemSubscriptionId,
      set: bindColumns,
    })
}

/** Creem entities arrive expanded (object with id) or as a bare id string. */
function entityId(
  value: { id?: string | null } | string | null | undefined
): string | null {
  if (!value) return null
  return typeof value === `string` ? value : (value.id ?? null)
}

/**
 * Map a flattened `checkout.completed` webhook payload to a binding input.
 * The nested subscription carries the id; `units` sits on the checkout
 * entity. Customer/product seed ids prefer the nested subscription's own
 * (bare-id) references over the checkout's expanded entities — same values
 * in practice, but the subscription's are authoritative for its row.
 */
export function bindingInputFromCheckout(event: {
  units?: number | null
  metadata?: Record<string, unknown> | null
  subscription?:
    | {
        id?: string | null
        customer?: { id?: string | null } | string | null
        product?: { id?: string | null } | string | null
      }
    | string
    | null
  customer?: { id?: string | null } | string | null
  product?: { id?: string | null } | string | null
}): SubscriptionBindingInput {
  const subscription =
    typeof event.subscription === `string` ? null : event.subscription
  return {
    creemSubscriptionId:
      typeof event.subscription === `string`
        ? event.subscription
        : (subscription?.id ?? null),
    metadata: event.metadata ?? null,
    units: event.units ?? null,
    creemCustomerId: entityId(subscription?.customer) ?? entityId(event.customer),
    productId: entityId(subscription?.product) ?? entityId(event.product),
  }
}

/**
 * Map a flattened `subscription.*` webhook payload (the `onGrantAccess` /
 * `onSubscription*` context) to a binding input. Seat quantity, when present,
 * lives on the first subscription item's `units`. `status` rides along so the
 * commit can heal a stale pending-cancel flag (see statusClearsPendingCancel);
 * customer/product ids seed the row when the plugin has none to bind onto.
 */
export function bindingInputFromSubscription(event: {
  id?: string | null
  metadata?: Record<string, unknown> | null
  items?: Array<{ units?: number | null }> | null
  status?: string | null
  customer?: { id?: string | null } | string | null
  product?: { id?: string | null } | string | null
}): SubscriptionBindingInput {
  return {
    creemSubscriptionId: event.id ?? null,
    metadata: event.metadata ?? null,
    units: event.items?.[0]?.units ?? null,
    status: event.status ?? null,
    creemCustomerId: entityId(event.customer),
    productId: entityId(event.product),
  }
}

import { eq } from "drizzle-orm"
import { db } from "@/db/connection"
import { creem_subscriptions } from "@/db/schema"

// Per-seat subscription→workspace binding (masterplan v5 §3.3, L19/L20).
//
// The `@creem_io/better-auth` plugin natively supports both a seat quantity
// (`units`) and arbitrary checkout `metadata`, and echoes that metadata back on
// every webhook event (`checkout.completed`, `subscription.*`). We pass
// `metadata.workspaceId` + `metadata.seats` (and `units: seats`) at checkout
// time; the plugin persists the base `creem_subscriptions` row (referenceId →
// user, productId, status, period, creemSubscriptionId) but NEVER writes our
// `workspaceId`/`seats` columns. This module reads the metadata off the webhook
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

export type WorkspaceBinding = {
  creemSubscriptionId: string
  workspaceId: string
  seats: number
}

/** Commit sink — abstracted so the binding logic is testable without drizzle. */
export type BindCommit = (binding: WorkspaceBinding) => Promise<void>

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
 * Resolve the workspace binding from a checkout/subscription payload, or
 * `null` when it isn't bindable (no subscription id, or no `workspaceId` in
 * metadata — e.g. a legacy checkout created before this path existed). Seats
 * resolve entity `units` → metadata `seats` → 1, so the row is never left at
 * 0. `units` MUST win when both are present: it is the quantity Creem
 * actually charged for, while `metadata` is client-suppliable at checkout
 * time (a forged `metadata.seats` must never out-vote the paid quantity).
 */
export function extractWorkspaceBinding(
  input: SubscriptionBindingInput
): WorkspaceBinding | null {
  const creemSubscriptionId = input.creemSubscriptionId?.trim()
  if (!creemSubscriptionId) return null

  const rawWorkspaceId = input.metadata?.workspaceId
  const workspaceId =
    typeof rawWorkspaceId === `string` ? rawWorkspaceId.trim() : ``
  if (!workspaceId) return null

  const seats =
    toPositiveInt(input.units) ?? toPositiveInt(input.metadata?.seats) ?? 1

  return { creemSubscriptionId, workspaceId, seats }
}

/**
 * Bind a persisted `creem_subscriptions` row to its workspace + seat count.
 * Returns the applied binding, or `null` when the payload wasn't bindable.
 * `commit` defaults to the real DB update; tests inject a fake.
 */
export async function bindSubscriptionToWorkspace(
  input: SubscriptionBindingInput,
  commit: BindCommit = commitBindingToDb
): Promise<WorkspaceBinding | null> {
  const binding = extractWorkspaceBinding(input)
  if (!binding) return null
  await commit(binding)
  return binding
}

async function commitBindingToDb(binding: WorkspaceBinding): Promise<void> {
  await db
    .update(creem_subscriptions)
    .set({ workspaceId: binding.workspaceId, seats: binding.seats })
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

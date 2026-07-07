import { z } from "zod"
import { TRPCError } from "@trpc/server"
import { eq } from "drizzle-orm"
import { createCheckout } from "@creem_io/better-auth/server"
import { router, authedProcedure } from "@/lib/trpc"
import { db } from "@/db/connection"
import { creem_subscriptions } from "@/db/schema"
import {
  countOwnedWorkspaces,
  getUserPlan,
  getWorkspacePlan,
  getWorkspaceUsage,
  FREE_OWNED_WORKSPACES_CAP,
  type PlanTier,
} from "@/lib/billing"
import {
  assertSubscriptionMutable,
  getActiveWorkspaceSubscription,
  updateCreemSubscriptionSeats,
  upgradeCreemSubscriptionProduct,
} from "@/lib/billing/creem-subscriptions"
import { isCloudInstance } from "@/lib/bootstrap-cloud"
import { resolveWorkspaceAccess } from "@/lib/workspace-membership"

// The Creem product ids we allow a seat checkout to target. Gating here stops a
// caller from binding an arbitrary Creem product to a workspace they own.
function allowedProductIds(): Set<string> {
  return new Set(
    [
      process.env.CREEM_PRO_PRODUCT_ID,
      process.env.CREEM_BUSINESS_PRODUCT_ID,
      process.env.CREEM_BUSINESS_YEARLY_PRODUCT_ID,
    ].filter((id): id is string => Boolean(id))
  )
}

function assertBillingConfigured(): void {
  if (!isCloudInstance()) {
    throw new TRPCError({
      code: `PRECONDITION_FAILED`,
      message: `Billing is disabled on this instance`,
    })
  }
  if (!process.env.CREEM_API_KEY) {
    throw new TRPCError({
      code: `PRECONDITION_FAILED`,
      message: `Billing is not configured`,
    })
  }
}

export const billingRouter = router({
  workspacePlan: authedProcedure
    .input(z.object({ workspaceId: z.string().uuid() }))
    .query(async ({ ctx, input }) => {
      if (!isCloudInstance()) {
        return {
          plan: `unlimited` as PlanTier,
          limits: {
            seats: Infinity,
            storageMb: Infinity,
            widgetConfigs: Infinity,
          },
          usage: { members: 0, storageMb: 0, widgetConfigs: 0 },
          subscription: null,
        }
      }

      // Only someone who can read the workspace may see its plan/usage.
      await resolveWorkspaceAccess(ctx.session.user.id, input.workspaceId)

      const [planData, usage, subscription] = await Promise.all([
        getWorkspacePlan(input.workspaceId),
        getWorkspaceUsage(input.workspaceId),
        getActiveWorkspaceSubscription(input.workspaceId),
      ])

      return {
        ...planData,
        usage,
        // The active subscription drives the settings UI: with one present,
        // seat/plan changes go through updateSeats/changePlan (mutating the
        // existing Creem subscription), NEVER through a second checkout.
        subscription: subscription
          ? {
              productId: subscription.productId,
              seats: subscription.seats,
              periodEnd: subscription.periodEnd?.toISOString() ?? null,
              cancelAtPeriodEnd: subscription.cancelAtPeriodEnd,
            }
          : null,
      }
    }),

  // Create a per-seat Creem checkout bound to a workspace. Only the workspace
  // owner may buy seats for it. We pass `units: seats` + metadata (workspaceId,
  // seats, referenceId) so the Creem plugin's webhook persistence binds the row
  // to the user (referenceId) while our onCheckoutCompleted/onGrantAccess hooks
  // bind it to the workspace + seat count (lib/billing/creem-binding.ts).
  createSeatCheckout: authedProcedure
    .input(
      z.object({
        workspaceId: z.string().uuid(),
        productId: z.string().min(1),
        seats: z.number().int().positive().max(1000),
        // Absolute URL Creem redirects to after payment. Defaults to the
        // billing settings page on this instance.
        successUrl: z.string().url().optional(),
      })
    )
    .mutation(async ({ ctx, input }) => {
      assertBillingConfigured()
      const apiKey = process.env.CREEM_API_KEY!
      if (!allowedProductIds().has(input.productId)) {
        throw new TRPCError({
          code: `BAD_REQUEST`,
          message: `Unknown product`,
        })
      }

      // Only the workspace owner may purchase seats for it.
      await resolveWorkspaceAccess(
        ctx.session.user.id,
        input.workspaceId,
        `mutate_resources`,
        { roles: [`owner`] }
      )

      // A workspace holds exactly ONE subscription. A second checkout would
      // stack a second full-price subscription on top of the existing one
      // (pay-twice bug) — seat and plan changes mutate the existing
      // subscription via updateSeats/changePlan instead.
      const existing = await getActiveWorkspaceSubscription(input.workspaceId)
      if (existing) {
        throw new TRPCError({
          code: `PRECONDITION_FAILED`,
          message: `This workspace already has an active subscription — adjust seats or switch plans instead`,
        })
      }

      const successUrl =
        input.successUrl ??
        `${process.env.BETTER_AUTH_URL ?? ``}/settings/billing`

      const { url } = await createCheckout(
        {
          apiKey,
          testMode: apiKey.startsWith(`creem_test_`),
        },
        {
          productId: input.productId,
          units: input.seats,
          customer: { email: ctx.session.user.email ?? undefined },
          successUrl,
          // referenceId → the plugin's webhook persistence keys the row to this
          // user; workspaceId + seats → our binding hooks key it to the
          // workspace. Both survive Creem's metadata round-trip.
          metadata: {
            referenceId: ctx.session.user.id,
            workspaceId: input.workspaceId,
            seats: input.seats,
          },
        }
      )

      return { url }
    }),

  // Change the seat count on the workspace's EXISTING subscription — the fix
  // for the pay-twice bug: mutating the subscription (Creem `units`) never
  // creates a second one. With `proration-none` (see creem-subscriptions.ts
  // for why) the new seats are usable immediately and the next renewal
  // invoice bills the new count.
  updateSeats: authedProcedure
    .input(
      z.object({
        workspaceId: z.string().uuid(),
        seats: z.number().int().positive().max(1000),
      })
    )
    .mutation(async ({ ctx, input }) => {
      assertBillingConfigured()

      // Same gate as buying seats: workspace owner only.
      await resolveWorkspaceAccess(
        ctx.session.user.id,
        input.workspaceId,
        `mutate_resources`,
        { roles: [`owner`] }
      )

      const subscription = await getActiveWorkspaceSubscription(
        input.workspaceId
      )
      assertSubscriptionMutable(subscription)
      if (subscription.seats === input.seats) {
        return { seats: subscription.seats }
      }

      const seats = await updateCreemSubscriptionSeats(
        subscription.creemSubscriptionId!,
        input.seats
      )

      // Optimistic write — the `subscription.update` webhook re-binds the same
      // value, so a lost webhook can't leave the seat count stale forever.
      await db
        .update(creem_subscriptions)
        .set({ seats })
        .where(eq(creem_subscriptions.id, subscription.id))

      return { seats }
    }),

  // Switch the workspace's existing subscription to a different product
  // (Pro ↔ Business, monthly ↔ yearly) via Creem's upgrade endpoint — same
  // one-subscription-per-workspace rule as updateSeats.
  changePlan: authedProcedure
    .input(
      z.object({
        workspaceId: z.string().uuid(),
        productId: z.string().min(1),
      })
    )
    .mutation(async ({ ctx, input }) => {
      assertBillingConfigured()
      if (!allowedProductIds().has(input.productId)) {
        throw new TRPCError({
          code: `BAD_REQUEST`,
          message: `Unknown product`,
        })
      }

      await resolveWorkspaceAccess(
        ctx.session.user.id,
        input.workspaceId,
        `mutate_resources`,
        { roles: [`owner`] }
      )

      const subscription = await getActiveWorkspaceSubscription(
        input.workspaceId
      )
      assertSubscriptionMutable(subscription)
      if (subscription.productId === input.productId) {
        throw new TRPCError({
          code: `BAD_REQUEST`,
          message: `The workspace is already on this plan`,
        })
      }

      await upgradeCreemSubscriptionProduct(
        subscription.creemSubscriptionId!,
        input.productId
      )

      // Optimistic write; the subscription webhooks confirm it.
      await db
        .update(creem_subscriptions)
        .set({ productId: input.productId })
        .where(eq(creem_subscriptions.id, subscription.id))

      return { productId: input.productId }
    }),

  // User-scoped plan + owned-workspace usage, for pre-gating workspace
  // creation. `limit` is the invisible free-tier abuse cap (10 owned
  // workspaces); paid users are uncapped → null (Infinity→null convention).
  userPlan: authedProcedure.query(async ({ ctx }) => {
    const userId = ctx.session.user.id
    if (!isCloudInstance()) {
      return { plan: `unlimited` as PlanTier, ownedWorkspaces: 0, limit: null }
    }
    const [{ plan }, ownedWorkspaces] = await Promise.all([
      getUserPlan(userId),
      countOwnedWorkspaces(userId),
    ])
    return {
      plan,
      ownedWorkspaces,
      limit: plan === `free` ? FREE_OWNED_WORKSPACES_CAP : null,
    }
  }),
})

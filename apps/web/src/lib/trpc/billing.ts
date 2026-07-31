import { z } from "zod"
import { TRPCError } from "@trpc/server"
import { eq } from "drizzle-orm"
import { createCheckout } from "@creem_io/better-auth/server"
import { router, authedProcedure } from "@/lib/trpc"
import { db } from "@/db/connection"
import { creem_subscriptions, users } from "@/db/schema"
import { withCreemRef } from "@/lib/billing/affiliate"
import { recordConversionEvent } from "@/lib/conversion/events"
import {
  countOwnedTeams,
  getUserPlan,
  getTeamPlan,
  getTeamUsage,
  FREE_OWNED_TEAMS_CAP,
  type PlanTier,
} from "@/lib/billing"
import {
  assertSubscriptionMutable,
  createCustomerPortalLink,
  getActiveTeamSubscription,
  resumeCreemSubscription,
  scheduleCreemSubscriptionCancellation,
  updateCreemSubscriptionSeats,
  upgradeCreemSubscriptionProduct,
} from "@/lib/billing/creem-subscriptions"
import {
  isSubscriptionPendingCancel,
  SCHEDULED_CANCEL_STATUS,
} from "@/lib/billing/billing-handover"
import { isCloudInstance } from "@/lib/bootstrap-cloud"
import { resolveTeamAccess } from "@/lib/team-membership"

// The Creem product ids we allow a seat checkout to target. Gating here stops a
// caller from binding an arbitrary Creem product to a team they own.
function allowedProductIds(): Set<string> {
  return new Set(
    [
      process.env.CREEM_TEAM_PRODUCT_ID,
      process.env.CREEM_TEAM_YEARLY_PRODUCT_ID,
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
  teamPlan: authedProcedure
    .input(z.object({ teamId: z.string().uuid() }))
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

      // Only someone who can read the team may see its plan/usage.
      await resolveTeamAccess(ctx.session.user.id, input.teamId)

      const [planData, usage, subscription] = await Promise.all([
        getTeamPlan(input.teamId),
        getTeamUsage(input.teamId),
        getActiveTeamSubscription(input.teamId),
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
              // Derived, not the raw column: a cancellation scheduled outside
              // our UI only shows up as Creem's `scheduled_cancel` status, and
              // the whole billing UI (pending-cancel banner, Resume button,
              // seat/plan controls) keys off this flag.
              cancelAtPeriodEnd: isSubscriptionPendingCancel(subscription),
            }
          : null,
      }
    }),

  // Create a per-seat Creem checkout bound to a team. Only the team
  // owner may buy seats for it. We pass `units: seats` + metadata (teamId,
  // seats, referenceId) so the Creem plugin's webhook persistence binds the row
  // to the user (referenceId) while our onCheckoutCompleted/onGrantAccess hooks
  // bind it to the team + seat count (lib/billing/creem-binding.ts).
  createSeatCheckout: authedProcedure
    .input(
      z.object({
        teamId: z.string().uuid(),
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

      // Only the team owner may purchase seats for it.
      await resolveTeamAccess(
        ctx.session.user.id,
        input.teamId,
        `mutate_resources`,
        { roles: [`owner`] }
      )

      // A team holds exactly ONE subscription. A second checkout would
      // stack a second full-price subscription on top of the existing one
      // (pay-twice bug) — seat and plan changes mutate the existing
      // subscription via updateSeats/changePlan instead.
      const existing = await getActiveTeamSubscription(input.teamId)
      if (existing) {
        throw new TRPCError({
          code: `PRECONDITION_FAILED`,
          message: `This team already has an active subscription — adjust seats or switch plans instead`,
        })
      }

      const successUrl =
        input.successUrl ??
        `${process.env.BETTER_AUTH_URL ?? ``}/settings/billing`

      // Affiliate attribution (EXP-384) — read off the USER row, never from
      // client input (metadata is client-suppliable; the buyer must not get to
      // name their own affiliate). `signupRef` rides checkout metadata so Creem
      // transactions reconcile against the admin funnel; the signed `creem_ref`
      // click token is re-appended to the hosted checkout URL below.
      const [attribution] = await ctx.db
        .select({
          signupRef: users.signupRef,
          signupCreemRef: users.signupCreemRef,
        })
        .from(users)
        .where(eq(users.id, ctx.session.user.id))

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
          // user; teamId + seats → our binding hooks key it to the
          // team. Both survive Creem's metadata round-trip.
          metadata: {
            referenceId: ctx.session.user.id,
            teamId: input.teamId,
            seats: input.seats,
            ...(attribution?.signupRef
              ? { signupRef: attribution.signupRef }
              : {}),
          },
        }
      )

      await recordConversionEvent(ctx.db, {
        name: `checkout_started`,
        userId: ctx.session.user.id,
        properties: {
          teamId: input.teamId,
          productId: input.productId,
          seats: input.seats,
          ...(attribution?.signupRef
            ? { signupRef: attribution.signupRef }
            : {}),
        },
      })

      return { url: withCreemRef(url, attribution?.signupCreemRef) }
    }),

  // Mint a Creem customer-portal link (EXP-315) so the owner can fetch
  // invoices, update the payment method, and self-service cancel. Keyed off
  // the SUBSCRIPTION row's `creemCustomerId` — never the session user's own
  // (REV2-55: the subscription belongs to the team, so every current owner
  // gets the portal, including after the purchaser left). The Creem plugin's
  // own `/api/auth/creem/create-portal` endpoint is blocked at the auth
  // route for exactly that reason (plus its arbitrary-customerId body).
  createPortalSession: authedProcedure
    .input(z.object({ teamId: z.string().uuid() }))
    .mutation(async ({ ctx, input }) => {
      assertBillingConfigured()

      // The portal exposes invoices and the payment method: owner-only,
      // same gate as every other billing mutation.
      await resolveTeamAccess(
        ctx.session.user.id,
        input.teamId,
        `mutate_resources`,
        { roles: [`owner`] }
      )

      const subscription = await getActiveTeamSubscription(input.teamId)
      if (!subscription) {
        throw new TRPCError({
          code: `PRECONDITION_FAILED`,
          message: `This team has no active subscription`,
        })
      }
      if (!subscription.creemCustomerId) {
        throw new TRPCError({
          code: `PRECONDITION_FAILED`,
          message: `This subscription has no billing portal — contact support`,
        })
      }

      const url = await createCustomerPortalLink(subscription.creemCustomerId)
      return { url }
    }),

  // Change the seat count on the team's EXISTING subscription — the fix
  // for the pay-twice bug: mutating the subscription (Creem `units`) never
  // creates a second one. With `proration-charge-immediately` (see
  // creem-subscriptions.ts for why) the new seats are usable immediately and
  // the prorated delta is charged (or refunded) at the moment of change.
  updateSeats: authedProcedure
    .input(
      z.object({
        teamId: z.string().uuid(),
        seats: z.number().int().positive().max(1000),
      })
    )
    .mutation(async ({ ctx, input }) => {
      assertBillingConfigured()

      // Same gate as buying seats: team owner only.
      await resolveTeamAccess(
        ctx.session.user.id,
        input.teamId,
        `mutate_resources`,
        { roles: [`owner`] }
      )

      const subscription = await getActiveTeamSubscription(input.teamId)
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

      await recordConversionEvent(ctx.db, {
        name: `seats_updated`,
        userId: ctx.session.user.id,
        properties: {
          teamId: input.teamId,
          creemSubscriptionId: subscription.creemSubscriptionId,
          from: subscription.seats,
          to: seats,
        },
      })

      return { seats }
    }),

  // Switch the team's existing subscription to a different product
  // (monthly ↔ yearly) via Creem's upgrade endpoint — same
  // one-subscription-per-team rule as updateSeats.
  changePlan: authedProcedure
    .input(
      z.object({
        teamId: z.string().uuid(),
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

      await resolveTeamAccess(
        ctx.session.user.id,
        input.teamId,
        `mutate_resources`,
        { roles: [`owner`] }
      )

      const subscription = await getActiveTeamSubscription(input.teamId)
      assertSubscriptionMutable(subscription)
      if (subscription.productId === input.productId) {
        throw new TRPCError({
          code: `BAD_REQUEST`,
          message: `The team is already on this plan`,
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

      await recordConversionEvent(ctx.db, {
        name: `plan_changed`,
        userId: ctx.session.user.id,
        properties: {
          teamId: input.teamId,
          creemSubscriptionId: subscription.creemSubscriptionId,
          from: subscription.productId,
          to: input.productId,
        },
      })

      return { productId: input.productId }
    }),

  // Cancel the team's subscription at the end of the paid period. The
  // subscription belongs to the TEAM (REV2-55), so this is the ONE
  // user-facing cancel path — and the prerequisite for deleting a paying team
  // (lib/billing/billing-handover.ts). Never immediate: the team keeps every
  // paid seat, byte and feature until `periodEnd`, then drops to Free when
  // Creem's `subscription.canceled` webhook lands.
  cancelSubscription: authedProcedure
    .input(z.object({ teamId: z.string().uuid() }))
    .mutation(async ({ ctx, input }) => {
      assertBillingConfigured()

      // Same gate as buying seats: team owner only.
      await resolveTeamAccess(
        ctx.session.user.id,
        input.teamId,
        `mutate_resources`,
        { roles: [`owner`] }
      )

      const subscription = await getActiveTeamSubscription(input.teamId)
      if (!subscription) {
        throw new TRPCError({
          code: `PRECONDITION_FAILED`,
          message: `This team has no active subscription`,
        })
      }
      // Idempotent: a second confirm (or a race with the webhook) is a no-op
      // rather than an error the owner has to interpret. Creem's own
      // `scheduled_cancel` status counts too — it is the only signal when the
      // cancellation was scheduled outside this router.
      if (isSubscriptionPendingCancel(subscription)) {
        return {
          cancelAtPeriodEnd: true,
          periodEnd: subscription.periodEnd?.toISOString() ?? null,
        }
      }
      if (!subscription.creemSubscriptionId) {
        throw new TRPCError({
          code: `PRECONDITION_FAILED`,
          message: `This subscription can't be changed automatically — contact support`,
        })
      }

      await scheduleCreemSubscriptionCancellation(
        subscription.creemSubscriptionId
      )

      // Optimistic write — the plugin never persists this column, so our own
      // write is what makes the pending cancellation visible (to the billing
      // UI and to the team-delete gate).
      await db
        .update(creem_subscriptions)
        .set({ cancelAtPeriodEnd: true, updatedAt: new Date() })
        .where(eq(creem_subscriptions.id, subscription.id))

      await recordConversionEvent(ctx.db, {
        name: `cancel_scheduled`,
        userId: ctx.session.user.id,
        properties: {
          teamId: input.teamId,
          creemSubscriptionId: subscription.creemSubscriptionId,
        },
      })

      return {
        cancelAtPeriodEnd: true,
        periodEnd: subscription.periodEnd?.toISOString() ?? null,
      }
    }),

  // Undo a pending cancellation before the period ends — without it a
  // mis-click would strand the team (seat/plan changes refuse to run while a
  // cancellation is scheduled) until the plan lapsed.
  resumeSubscription: authedProcedure
    .input(z.object({ teamId: z.string().uuid() }))
    .mutation(async ({ ctx, input }) => {
      assertBillingConfigured()

      await resolveTeamAccess(
        ctx.session.user.id,
        input.teamId,
        `mutate_resources`,
        { roles: [`owner`] }
      )

      const subscription = await getActiveTeamSubscription(input.teamId)
      // Same derived signal the banner uses — a cancellation scheduled from
      // the Creem dashboard must still be resumable from here.
      if (!subscription || !isSubscriptionPendingCancel(subscription)) {
        throw new TRPCError({
          code: `PRECONDITION_FAILED`,
          message: `This subscription is not scheduled to cancel`,
        })
      }
      if (!subscription.creemSubscriptionId) {
        throw new TRPCError({
          code: `PRECONDITION_FAILED`,
          message: `This subscription can't be changed automatically — contact support`,
        })
      }

      await resumeCreemSubscription(subscription.creemSubscriptionId)

      // Optimistic write. `status` is cleared alongside the flag because the
      // pending-cancel signal is derived from BOTH — leaving a stale
      // `scheduled_cancel` here would keep the banner up (and re-cancel a
      // no-op) until Creem's confirming webhook landed. The webhook overwrites
      // it with the authoritative value moments later.
      await db
        .update(creem_subscriptions)
        .set({
          cancelAtPeriodEnd: false,
          status:
            subscription.status === SCHEDULED_CANCEL_STATUS
              ? `active`
              : subscription.status,
          updatedAt: new Date(),
        })
        .where(eq(creem_subscriptions.id, subscription.id))

      await recordConversionEvent(ctx.db, {
        name: `subscription_resumed`,
        userId: ctx.session.user.id,
        properties: {
          teamId: input.teamId,
          creemSubscriptionId: subscription.creemSubscriptionId,
        },
      })

      return { cancelAtPeriodEnd: false }
    }),

  // User-scoped plan + owned-team usage, for pre-gating team
  // creation. `limit` is the invisible free-tier abuse cap (10 owned
  // teams); paid users are uncapped → null (Infinity→null convention).
  userPlan: authedProcedure.query(async ({ ctx }) => {
    const userId = ctx.session.user.id
    if (!isCloudInstance()) {
      return { plan: `unlimited` as PlanTier, ownedTeams: 0, limit: null }
    }
    const [{ plan }, ownedTeams] = await Promise.all([
      getUserPlan(userId),
      countOwnedTeams(userId),
    ])
    return {
      plan,
      ownedTeams,
      limit: plan === `free` ? FREE_OWNED_TEAMS_CAP : null,
    }
  }),
})

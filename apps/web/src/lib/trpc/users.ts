import { z } from "zod"
import { TRPCError } from "@trpc/server"
import { router, authedProcedure } from "@/lib/trpc"
import { apikeys, users } from "@/db/auth-schema"
import { auth } from "@/lib/auth"
import { getReadableUserIdsInTeams } from "@/lib/team-membership"
import { guardAndCleanupTeamsForUserDeletion } from "@/lib/account-deletion"
import {
  captureAppleTokens,
  revokeAppleTokensBestEffort,
} from "@/lib/auth/apple-revocation"
import { deleteStorageObjects } from "@/lib/storage/issue-attachment-cleanup"
import {
  cancelCreemSubscriptionsBestEffort,
  findActiveSubscriptionsForUser,
} from "@/lib/billing/creem-subscriptions"
import { and, desc, eq, inArray, sql } from "drizzle-orm"

export const usersRouter = router({
  listByTeamIds: authedProcedure.query(async ({ ctx }) => {
    // Same email-safe scoping as the users shape: only co-members of
    // teams the caller actually joined (not all public teams).
    const userIds = await getReadableUserIdsInTeams(ctx.session.user.id)

    if (userIds.length === 0) {
      return { users: [] }
    }

    const userRows = await ctx.db
      .select({ id: users.id, name: users.name, email: users.email })
      .from(users)
      .where(inArray(users.id, userIds))

    return { users: userRows }
  }),

  // ── Personal API keys (expu_) ─────────────────────────────────────────────
  // The user's own long-lived credential for desktop coding sessions + MCP
  // clients: the launcher writes it into the worktree's .mcp.json, and the
  // Better Auth apiKey plugin resolves `Authorization: Bearer expu_…` back to
  // this user. The raw key is returned exactly once at mint time (only a hash
  // is stored); revoke by deleting the row.

  mintPersonalApiKey: authedProcedure
    .input(z.object({ name: z.string().min(1).max(180).optional() }).optional())
    .mutation(async ({ ctx, input }) => {
      const created = await auth.api.createApiKey({
        body: {
          name: (input?.name ?? `Personal key`).slice(0, 180),
          userId: ctx.session.user.id,
          expiresIn: null,
          rateLimitEnabled: false,
          metadata: { kind: `personal` },
        },
      })
      // `key` is the RAW credential — returned exactly once (only a hash is
      // stored). The rest is display metadata so the client can render the new
      // row without a follow-up list call.
      return {
        key: created.key,
        id: created.id,
        name: created.name ?? null,
        start: created.start ?? null,
        prefix: created.prefix ?? null,
        createdAt: created.createdAt,
      }
    }),

  listPersonalApiKeys: authedProcedure.query(async ({ ctx }) => {
    const rows = await ctx.db
      .select({
        id: apikeys.id,
        name: apikeys.name,
        start: apikeys.start,
        prefix: apikeys.prefix,
        createdAt: apikeys.createdAt,
        lastRequest: apikeys.lastRequest,
      })
      .from(apikeys)
      .where(eq(apikeys.referenceId, ctx.session.user.id))
      .orderBy(desc(apikeys.createdAt))
    return { keys: rows }
  }),

  // Dismiss the "Get the desktop app" card (Agents view). Sets a per-user
  // timestamp flag (like onboardingCompletedAt) surfaced read-only on the
  // session so the card stays hidden across reloads. The client also hides it
  // immediately via local state — the session field is fetched once.
  dismissDesktopAppCard: authedProcedure.mutation(async ({ ctx }) => {
    await ctx.db
      .update(users)
      .set({ desktopAppCardDismissedAt: new Date(), updatedAt: new Date() })
      .where(eq(users.id, ctx.session.user.id))
    return { ok: true }
  }),

  // Dismiss the "Getting started" cards on the empty board (EXP-88).
  // Same contract as dismissDesktopAppCard: one-way per-user timestamp flag
  // surfaced read-only on the session; the client hides the cards immediately
  // via local state.
  dismissGettingStarted: authedProcedure.mutation(async ({ ctx }) => {
    await ctx.db
      .update(users)
      .set({ gettingStartedDismissedAt: new Date(), updatedAt: new Date() })
      .where(eq(users.id, ctx.session.user.id))
    return { ok: true }
  }),

  revokePersonalApiKey: authedProcedure
    .input(z.object({ id: z.string().min(1) }))
    .mutation(async ({ ctx, input }) => {
      await ctx.db
        .delete(apikeys)
        .where(
          and(
            eq(apikeys.id, input.id),
            eq(apikeys.referenceId, ctx.session.user.id)
          )
        )
      return { ok: true }
    }),

  // ── Self-service account deletion ─────────────────────────────────────────
  // App Store guideline 5.1.1(v) requires in-app account deletion when the app
  // supports account creation (email-only deletion is explicitly insufficient).
  // Mirrors admin.deleteUser: the users row delete cascades sessions, accounts,
  // apikeys, memberships, issues/comments authored, fcm tokens, notifications.
  // Additionally removes teams where the caller is the ONLY member (their
  // personal team + solo teams) so no orphaned data survives — the
  // privacy policy promises deletion of "all associated data".
  deleteAccount: authedProcedure
    .input(z.object({ confirm: z.literal(true) }))
    .mutation(async ({ ctx }) => {
      const userId = ctx.session.user.id

      const [me] = await ctx.db
        .select({ isAdmin: users.isAdmin, isAgent: users.isAgent })
        .from(users)
        .where(eq(users.id, userId))
      if (!me) throw new TRPCError({ code: `NOT_FOUND` })
      if (me.isAgent) {
        // Widget-helpdesk bot users own widget-created issues; deleting one
        // cascades those issues away. They also never have interactive
        // sessions — refuse defensively.
        throw new TRPCError({ code: `FORBIDDEN` })
      }
      if (me.isAdmin) {
        const [{ adminCount }] = await ctx.db
          .select({ adminCount: sql<number>`count(*)::int` })
          .from(users)
          .where(eq(users.isAdmin, true))
        if (adminCount <= 1) {
          throw new TRPCError({
            code: `BAD_REQUEST`,
            message: `You are the last admin of this instance — promote another admin before deleting your account`,
          })
        }
      }

      // Subscriptions the caller purchased, captured BEFORE the delete — the
      // buyer FK cascades with the users row, after which the remote Creem
      // subscription would keep charging with nothing left to find it by.
      const doomedSubscriptions = await findActiveSubscriptionsForUser(userId)

      // Sign in with Apple pairing to revoke after the delete (guideline
      // 5.1.1(v)) — captured now because the accounts row cascades away with
      // the users row. Web-flow accounts carry tokens; native-idToken pairings
      // store none (nothing to revoke). See lib/auth/apple-revocation.ts.
      const appleTokens = await captureAppleTokens(ctx.db, userId)

      let storageKeys: string[] = []
      await ctx.db.transaction(async (tx) => {
        // Fail closed when the caller is the sole owner of a team that
        // still has other members, then delete their solo teams (shared
        // with admin.deleteUser — see lib/account-deletion.ts).
        const cleanup = await guardAndCleanupTeamsForUserDeletion(
          tx,
          userId,
          `self`
        )
        storageKeys = cleanup.storageKeys
        // Subscriptions bound to the deleted solo teams but purchased by
        // SOMEONE ELSE (e.g. after an ownership hand-off) — invisible to the
        // buyer-scoped capture above, yet their team just vanished.
        for (const sub of cleanup.doomedTeamSubscriptions) {
          if (!doomedSubscriptions.some((s) => s.id === sub.id)) {
            doomedSubscriptions.push(sub)
          }
        }
        await tx.delete(users).where(eq(users.id, userId))
      })

      // Best-effort AFTER commit: a Creem API failure logs loudly but never
      // leaves the account half-deleted.
      await cancelCreemSubscriptionsBestEffort(doomedSubscriptions)
      // The users-row cascade dropped attachment rows but not their S3 blobs.
      await deleteStorageObjects(storageKeys)
      // Revoke the Apple pairing so a re-signup delivers the name again.
      await revokeAppleTokensBestEffort(appleTokens)

      return { ok: true }
    }),
})

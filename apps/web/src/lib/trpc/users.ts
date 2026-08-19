import { z } from "zod"
import { TRPCError } from "@trpc/server"
import { router, authedProcedure } from "@/lib/trpc"
import { apikeys, users } from "@/db/auth-schema"
import { auth } from "@/lib/auth"
import { getReadableUserIdsInTeams } from "@/lib/team-membership"
import { invalidateMembershipCaches } from "@/lib/auth/membership-cache"
import { invalidateSessionCache } from "@/lib/auth/resolve-bearer"
import { guardAndCleanupTeamsForUserDeletion } from "@/lib/account-deletion"
import { relayKillSessionsBestEffort } from "@/lib/coding-session-kill"
import {
  captureOAuthTokens,
  revokeOAuthTokensBestEffort,
} from "@/lib/auth/oauth-revocation"
import { deleteStorageObjects } from "@/lib/storage/issue-attachment-cleanup"
import {
  cancelCreemSubscriptionsBestEffort,
  type CancellableSubscription,
} from "@/lib/billing/creem-subscriptions"
import { and, desc, eq, gt, inArray, isNull, sql } from "drizzle-orm"
import { truncateAttributionInput } from "@/lib/conversion/attribution"
import { isCloudInstance } from "@/lib/bootstrap-cloud"

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

  // Stamp signup attribution (EXP-362) onto the caller's OWN fresh account.
  // Cookieless: ref/utm params ride URLs from the marketing site through the
  // auth flow (incl. the OAuth callbackURL), and the client claims them right
  // after its first authenticated load. Self-reported, but the guards keep it
  // honest: write-once (every column still null) and only within 24h of
  // account creation — an established account can never rewrite its origin.
  claimSignupAttribution: authedProcedure
    .input(
      z.object({
        ref: z.string().max(512).optional(),
        utmSource: z.string().max(512).optional(),
        utmMedium: z.string().max(512).optional(),
        utmCampaign: z.string().max(512).optional(),
        // Creem's signed affiliate click token (EXP-384) — persisted so the
        // checkout can re-forward it (lib/billing/affiliate.ts).
        creemRef: z.string().max(512).optional(),
        referrer: z.string().max(2048).optional(),
        landingPath: z.string().max(2048).optional(),
      })
    )
    .mutation(async ({ ctx, input }) => {
      // Cloud-only (EXP-362): self-hosted instances collect no attribution.
      if (!isCloudInstance()) return { claimed: false }
      const clean = truncateAttributionInput(input)
      const hasAny = Object.values(clean).some((value) => value !== null)
      if (!hasAny) return { claimed: false }

      const dayAgo = new Date(Date.now() - 24 * 60 * 60 * 1000)
      const claimed = await ctx.db
        .update(users)
        .set({
          signupRef: clean.ref,
          signupUtmSource: clean.utmSource,
          signupUtmMedium: clean.utmMedium,
          signupUtmCampaign: clean.utmCampaign,
          signupCreemRef: clean.creemRef,
          signupReferrer: clean.referrer,
          signupLandingPath: clean.landingPath,
          updatedAt: new Date(),
        })
        .where(
          and(
            eq(users.id, ctx.session.user.id),
            gt(users.createdAt, dayAgo),
            isNull(users.signupRef),
            isNull(users.signupUtmSource),
            isNull(users.signupUtmMedium),
            isNull(users.signupUtmCampaign),
            isNull(users.signupCreemRef),
            isNull(users.signupReferrer),
            isNull(users.signupLandingPath)
          )
        )
        .returning({ id: users.id })
      return { claimed: claimed.length > 0 }
    }),

  // The caller's stored IANA timezone (EXP-369) — the clock their daily
  // digest send hour is read in. SERVER-ONLY column (never synced), so the
  // account panel reads it here. `null` = never captured → the sweep uses UTC.
  timezone: authedProcedure.query(async ({ ctx }) => {
    const [row] = await ctx.db
      .select({ timezone: users.timezone })
      .from(users)
      .where(eq(users.id, ctx.session.user.id))
    return { timezone: row?.timezone ?? null }
  }),

  // Claim/replace the caller's timezone. Clients fire this with
  // `onlyIfUnset: true` right after login (best-effort, from the device's own
  // Intl/OS zone); the account panel sends an explicit pick without the flag.
  setTimezone: authedProcedure
    .input(
      z.object({
        timezone: z.string().min(1).max(64),
        onlyIfUnset: z.boolean().optional(),
      })
    )
    .mutation(async ({ ctx, input }) => {
      // Intl is the tz database the digest math runs on — anything it rejects
      // would silently degrade every send point to UTC.
      try {
        new Intl.DateTimeFormat(`en-US`, { timeZone: input.timezone })
      } catch {
        throw new TRPCError({
          code: `BAD_REQUEST`,
          message: `Unknown timezone: ${input.timezone}`,
        })
      }
      const updated = await ctx.db
        .update(users)
        .set({ timezone: input.timezone, updatedAt: new Date() })
        .where(
          input.onlyIfUnset
            ? and(eq(users.id, ctx.session.user.id), isNull(users.timezone))
            : eq(users.id, ctx.session.user.id)
        )
        .returning({ id: users.id })
      return { saved: updated.length > 0 }
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

  // LEGACY (EXP-548): the "Getting started" checklist no longer dismisses on
  // any client — it hides itself once every entry is done. Kept only so
  // desktop builds before EXP-548 (which fire it best-effort from their
  // Dismiss button) keep getting a 200; nothing reads the flag any more.
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
      // The revoked key must stop resolving in-process immediately (REV2-7 —
      // resolveSession caches token-credential sessions for 30s).
      invalidateSessionCache()
      return { ok: true }
    }),

  // ── Self-service account deletion ─────────────────────────────────────────
  // App Store guideline 5.1.1(v) requires in-app account deletion when the app
  // supports account creation (email-only deletion is explicitly insufficient).
  // Mirrors admin.deleteUser: the users row delete cascades sessions, accounts,
  // apikeys (via reference_id — the plugin's name for the user FK), memberships,
  // comments authored, fcm tokens and notifications. Issues the user CREATED
  // are NOT deleted — issues.creator_id is ON DELETE SET NULL, so they survive
  // with a null creator (they may be shared team data) — and neither are the
  // attachments they uploaded (uploader_id is `set null` too, REV2-36: those
  // blobs are embedded in surviving descriptions/comments). Additionally
  // removes teams where the caller is the ONLY member (their personal team +
  // solo teams) so no orphaned data survives — the privacy policy promises
  // deletion of "all associated data".
  //
  // Deletion is NEVER blocked or delayed by billing (App Store 5.1.1(v) /
  // Play data-deletion, and billing is web-only so a mobile caller could not
  // resolve a gate): the only subscription it cancels is one funding a solo
  // team it destroys. See lib/billing/billing-handover.ts.
  deleteAccount: authedProcedure
    .input(z.object({ confirm: z.literal(true) }))
    .mutation(async ({ ctx }) => {
      const userId = ctx.session.user.id

      const [me] = await ctx.db
        .select({ isAdmin: users.isAdmin })
        .from(users)
        .where(eq(users.id, userId))
      if (!me) throw new TRPCError({ code: `NOT_FOUND` })
      if (me.isAdmin) {
        const [{ adminCount }] = await ctx.db
          .select({ adminCount: sql<number>`count(*)::int` })
          .from(users)
          .where(eq(users.isAdmin, true))
        if (adminCount <= 1) {
          throw new TRPCError({
            code: `BAD_REQUEST`,
            message: `You are the last admin of this instance. Promote another admin before deleting your account.`,
          })
        }
      }

      // OAuth grants to revoke after the delete (Apple: guideline 5.1.1(v);
      // Google/OIDC: the offline refresh token would otherwise stay live at the
      // provider) — captured now because the accounts rows cascade away with
      // the users row. Rows without tokens (password logins, native-idToken
      // Apple pairings) are skipped. See lib/auth/oauth-revocation.ts.
      const oauthTokens = await captureOAuthTokens(ctx.db, userId)

      let storageKeys: string[] = []
      // Subscriptions bound to the SOLO teams this deletion destroys — the
      // only ones it may cancel (REV2-55: a subscription belongs to its team,
      // so the plans funding teams that survive the caller stay live and stay
      // manageable by their remaining owners).
      let doomedSubscriptions: CancellableSubscription[] = []
      // Cross-account coding sessions the cleanup ended in-tx (EXP-445).
      let endedSessionIds: string[] = []
      await ctx.db.transaction(async (tx) => {
        // Fail closed when the caller is the sole owner of a team that
        // still has other members, then delete their solo teams, scrub their
        // mentions and email residue (shared with admin.deleteUser — see
        // lib/account-deletion.ts).
        const cleanup = await guardAndCleanupTeamsForUserDeletion(
          tx,
          userId,
          `self`
        )
        storageKeys = cleanup.storageKeys
        doomedSubscriptions = cleanup.doomedTeamSubscriptions
        endedSessionIds = cleanup.endedSessionIds
        await tx.delete(users).where(eq(users.id, userId))
      })
      // Post-commit: the users-row cascade dropped memberships (and possibly
      // whole solo teams), and the dead user's tokens must stop resolving.
      invalidateMembershipCaches()
      invalidateSessionCache()

      // Best-effort AFTER commit: a Creem API failure logs loudly but never
      // leaves the account half-deleted.
      await cancelCreemSubscriptionsBestEffort(doomedSubscriptions)
      // Blobs stranded by the SOLO-TEAM deletes above (their attachment rows
      // cascaded away, the S3 objects did not). Attachments this user merely
      // uploaded into a SURVIVING team are not here: `uploader_id` is `set
      // null`, so those rows (and their blobs) outlive the account.
      await deleteStorageObjects(storageKeys)
      // Revoke the provider grants: the Apple pairing (so a re-signup delivers
      // the name again) plus every other provider's stored token.
      await revokeOAuthTokensBestEffort(oauthTokens)
      // Tear the shared-device runs down immediately (EXP-445); the committed
      // `ended` rows above are the durable signal either way.
      await relayKillSessionsBestEffort(endedSessionIds)

      return { ok: true }
    }),
})

import { z } from "zod"
import { TRPCError } from "@trpc/server"
import { router, authedProcedure, type Context } from "@/lib/trpc"
import { accounts, apikeys, passkeys, users } from "@/db/auth-schema"
import { boards, teams } from "@/db/schema"
import {
  accountRowFilter,
  assertNotLastWayIn,
  configuredProviders,
  loadSignInMethods,
  passkeyRowFilter,
} from "@/lib/auth/sign-in-methods"
import {
  SIGN_IN_LINK_TICKET_TTL_MS,
  mintSignInLinkTicket,
} from "@/lib/auth/sign-in-link-ticket"
import { buildAuthConfig } from "@/lib/auth/config"
import { auth } from "@/lib/auth"
import { getReadableUserIdsInTeams } from "@/lib/team-membership"
import { invalidateMembershipCaches } from "@/lib/auth/membership-cache"
import { invalidateSessionCache } from "@/lib/auth/resolve-bearer"
import {
  AGENT_KEY_MANAGES_KEYS_MESSAGE,
  AGENT_KEY_SCOPE_MESSAGE,
  API_KEY_KINDS,
  assertNotAgentApiKeySession,
  assertNotApiKeySession,
  parseApiKeyMetadata,
  type ApiKeyScope,
} from "@/lib/auth/api-key-kind"
import { getUserTeamIds } from "@/lib/auth/membership"
import { boardVisible } from "@/lib/board-visibility"
import {
  clampScopeSelection,
  scopeSelectionInput,
} from "@/lib/mcp/scope-selection"
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

/** A scoped key's selection as the list shows it: the granted teams and
 * boards by name (FEED-76). Unknown teams and trashed/archived boards drop
 * out, so a key whose every target vanished shows an empty scope, never a
 * widened one. `null` entries (unscoped keys) stay `null`. */
export interface ResolvedApiKeyScope {
  teams: Array<{ id: string; name: string }>
  boards: Array<{ id: string; name: string; prefix: string }>
}

async function resolveApiKeyScopes(
  db: Context[`db`],
  scopes: Array<ApiKeyScope | null>
): Promise<Array<ResolvedApiKeyScope | null>> {
  const teamIds = [...new Set(scopes.flatMap((s) => s?.teamIds ?? []))]
  const boardIds = [...new Set(scopes.flatMap((s) => s?.boardIds ?? []))]
  const teamRows =
    teamIds.length === 0
      ? []
      : await db
          .select({ id: teams.id, name: teams.name })
          .from(teams)
          .where(inArray(teams.id, teamIds))
  const boardRows =
    boardIds.length === 0
      ? []
      : await db
          .select({ id: boards.id, name: boards.name, prefix: boards.prefix })
          .from(boards)
          .where(and(inArray(boards.id, boardIds), boardVisible()))
  const teamById = new Map(teamRows.map((row) => [row.id, row]))
  const boardById = new Map(boardRows.map((row) => [row.id, row]))
  return scopes.map((scope) => {
    if (!scope) return null
    return {
      teams: scope.teamIds.flatMap((id) => {
        const row = teamById.get(id)
        return row ? [{ id: row.id, name: row.name }] : []
      }),
      boards: scope.boardIds.flatMap((id) => {
        const row = boardById.get(id)
        return row ? [{ id: row.id, name: row.name, prefix: row.prefix }] : []
      }),
    }
  })
}

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

  // EXP-1140: `purpose: 'agent'` tags the hidden key the launcher mints for
  // the AGENT it spawns (lib/auth/api-key-kind.ts). Such a key is refused
  // where a person's key is not (`mcpServers.resolveForLaunch`). A key the
  // person mints in Settings stays `personal`. The agent key itself manages
  // NO keys: minting a `personal` one would be its way around every
  // kind-gated procedure.
  //
  // FEED-76: `scope` = the consent screen's team/board selection. Clamped to
  // membership and stored in the key's metadata; the key then works on
  // /api/mcp only, confined like an OAuth grant. "Everything" (or no scope)
  // stores NO scope: an ordinary full key. The agent's key is never scoped
  // (the launcher needs its full tRPC/shape access).
  mintPersonalApiKey: authedProcedure
    .input(
      z
        .object({
          name: z.string().min(1).max(180).optional(),
          purpose: z.enum(API_KEY_KINDS).optional(),
          scope: scopeSelectionInput.optional(),
        })
        .optional()
    )
    .mutation(async ({ ctx, input }) => {
      await assertNotAgentApiKeySession(
        ctx.db,
        ctx.session,
        AGENT_KEY_MANAGES_KEYS_MESSAGE
      )
      const kind = input?.purpose ?? `personal`
      let scope: ApiKeyScope | null = null
      if (input?.scope) {
        if (kind === `agent`) {
          throw new TRPCError({
            code: `BAD_REQUEST`,
            message: AGENT_KEY_SCOPE_MESSAGE,
          })
        }
        const memberTeamIds = new Set(
          await getUserTeamIds(ctx.session.user.id)
        )
        const clamped = await clampScopeSelection(
          ctx.db,
          memberTeamIds,
          input.scope
        )
        if (!clamped.allTeams) {
          scope = {
            allTeams: false,
            teamIds: clamped.teamIds,
            boardIds: clamped.boardIds,
          }
        }
      }
      const created = await auth.api.createApiKey({
        body: {
          name: (input?.name ?? `Personal key`).slice(0, 180),
          userId: ctx.session.user.id,
          expiresIn: null,
          rateLimitEnabled: false,
          metadata: scope ? { kind, scope } : { kind },
        },
      })
      // `key` is the RAW credential — returned exactly once (only a hash is
      // stored). The rest is display metadata so the client can render the new
      // row without a follow-up list call.
      const [resolvedScope] = await resolveApiKeyScopes(ctx.db, [scope])
      return {
        key: created.key,
        id: created.id,
        name: created.name ?? null,
        start: created.start ?? null,
        prefix: created.prefix ?? null,
        createdAt: created.createdAt,
        scope: resolvedScope ?? null,
      }
    }),

  listPersonalApiKeys: authedProcedure.query(async ({ ctx }) => {
    await assertNotAgentApiKeySession(
      ctx.db,
      ctx.session,
      AGENT_KEY_MANAGES_KEYS_MESSAGE
    )
    const rows = await ctx.db
      .select({
        id: apikeys.id,
        name: apikeys.name,
        start: apikeys.start,
        prefix: apikeys.prefix,
        createdAt: apikeys.createdAt,
        lastRequest: apikeys.lastRequest,
        metadata: apikeys.metadata,
      })
      .from(apikeys)
      .where(eq(apikeys.referenceId, ctx.session.user.id))
      .orderBy(desc(apikeys.createdAt))
    // FEED-76: the stored scope, resolved to names for the row caption; the
    // raw metadata never leaves the server.
    const scopes = await resolveApiKeyScopes(
      ctx.db,
      rows.map((row) => parseApiKeyMetadata(row.metadata).scope)
    )
    return {
      keys: rows.map(({ metadata: _metadata, ...row }, index) => ({
        ...row,
        scope: scopes[index] ?? null,
      })),
    }
  }),

  // ── Sign-in methods (EXP-1126) ─────────────────────────────────────────────
  // ONE payload for every client's Settings › Account › Sign-in methods: the
  // primary email, the configured providers with their link state, the
  // passkeys, and how many ways in the account has. Linking a provider is a
  // browser round-trip (web: authClient.linkSocial / oauth2.link; natives:
  // mintSignInLinkTicket + the handoff's link mode); the email changes
  // through the Better Auth email-otp endpoints; removals come here so the
  // last-way-in rule is enforced in one place and never behind Better Auth's
  // 1-day fresh-session gate.

  signInMethods: authedProcedure.query(async ({ ctx }) => {
    const methods = await loadSignInMethods(ctx.db, ctx.session.user.id)
    if (!methods) throw new TRPCError({ code: `NOT_FOUND` })
    return methods
  }),

  // Identity changes need a real browser/native session, never an `expu_`
  // key (a person's or the agent's): see lib/auth/api-key-kind.ts.
  unlinkSignInMethod: authedProcedure
    .input(z.object({ providerId: z.string().min(1).max(120) }))
    .mutation(async ({ ctx, input }) => {
      await assertNotApiKeySession(ctx.db, ctx.session)
      const userId = ctx.session.user.id
      const [row] = await ctx.db
        .select({ id: accounts.id })
        .from(accounts)
        .where(accountRowFilter(userId, input.providerId))
        .limit(1)
      if (!row) throw new TRPCError({ code: `NOT_FOUND` })
      await assertNotLastWayIn(ctx.db, userId, { providerId: input.providerId })
      // The provider grant outlives the row unless revoked (deleteAccount
      // precedent): capture the tokens first, revoke best-effort after.
      const tokens = (await captureOAuthTokens(ctx.db, userId)).filter(
        (t) => t.providerId === input.providerId
      )
      await ctx.db.delete(accounts).where(eq(accounts.id, row.id))
      await revokeOAuthTokensBestEffort(tokens)
      return { ok: true }
    }),

  deletePasskey: authedProcedure
    .input(z.object({ id: z.string().min(1).max(200) }))
    .mutation(async ({ ctx, input }) => {
      await assertNotApiKeySession(ctx.db, ctx.session)
      const userId = ctx.session.user.id
      const [row] = await ctx.db
        .select({ id: passkeys.id })
        .from(passkeys)
        .where(passkeyRowFilter(userId, input.id))
        .limit(1)
      if (!row) throw new TRPCError({ code: `NOT_FOUND` })
      await assertNotLastWayIn(ctx.db, userId, { passkeyId: input.id })
      await ctx.db.delete(passkeys).where(eq(passkeys.id, row.id))
      return { ok: true }
    }),

  // A native app's entry into the browser handoff's LINK mode: the ticket
  // names the caller's session and one configured provider, and
  // /api/mobile-oauth-start?link=<ticket> runs Better Auth's link-social on
  // that session. Two minutes, single-use (lib/auth/sign-in-link-ticket.ts).
  mintSignInLinkTicket: authedProcedure
    .input(z.object({ provider: z.string().min(1).max(120) }))
    .mutation(async ({ ctx, input }) => {
      await assertNotApiKeySession(ctx.db, ctx.session)
      const config = buildAuthConfig()
      // SLOP-7: the natives connect GitHub for repositories through this
      // same link hop (SLOP-25: desktop `github_connect::connect_github`),
      // so `github` is mintable whenever the App's OAuth client is configured
      // — GitHub LOGIN being off only keeps it out of the sign-in methods.
      const known =
        configuredProviders(config).some((p) => p.id === input.provider) ||
        (input.provider === `github` && config.githubConnectEnabled)
      if (!known) {
        throw new TRPCError({
          code: `BAD_REQUEST`,
          message: `That sign-in provider is not offered on this instance.`,
        })
      }
      const sessionId = ctx.session.session.id
      return {
        ticket: mintSignInLinkTicket({ sessionId, provider: input.provider }),
        expiresInSeconds: SIGN_IN_LINK_TICKET_TTL_MS / 1000,
      }
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

  revokePersonalApiKey: authedProcedure
    .input(z.object({ id: z.string().min(1) }))
    .mutation(async ({ ctx, input }) => {
      await assertNotAgentApiKeySession(
        ctx.db,
        ctx.session,
        AGENT_KEY_MANAGES_KEYS_MESSAGE
      )
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
  // blobs are embedded in surviving descriptions/comments). Their issue
  // DRAFTS do cascade (EXP-878), and the draft attachments' blobs are
  // reclaimed with them (lib/account-deletion.ts). Additionally
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
      // Blobs stranded by the SOLO-TEAM deletes above plus the user's DRAFT
      // attachments in every team (EXP-878; their rows cascaded away, the S3
      // objects did not). Attachments this user merely uploaded into a
      // SURVIVING issue are not here: `uploader_id` is `set null`, so those
      // rows (and their blobs) outlive the account.
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

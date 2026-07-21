import { z } from "zod"
import { TRPCError } from "@trpc/server"
import { and, desc, eq, inArray, isNotNull, or, sql } from "drizzle-orm"
import { router, adminProcedure, generateTxId } from "@/lib/trpc"
import { users, accounts, sessions } from "@/db/auth-schema"
import {
  attachments,
  teams,
  teamMembers,
  boards,
  issues,
  issueEvents,
  emailBounces,
  emailDeliveries,
  creem_subscriptions,
} from "@/db/schema"
import { suppressSesDestination } from "@/lib/email"
import {
  getTeamPlan,
  getTeamUsage,
  planFromSubscription,
  parseCompTier,
  resolveEffectiveTier,
  type PlanTier,
} from "@/lib/billing"
import { deleteStorageObjects } from "@/lib/storage/issue-attachment-cleanup"
import { getFeedbackTeamId, isCloudInstance } from "@/lib/bootstrap-cloud"
import { guardAndCleanupTeamsForUserDeletion } from "@/lib/account-deletion"
import {
  captureAppleTokens,
  revokeAppleTokensBestEffort,
} from "@/lib/auth/apple-revocation"
import {
  ACTIVE_SUBSCRIPTION_STATUSES,
  cancelCreemSubscriptionsBestEffort,
  findActiveSubscriptionsForUser,
  findActiveSubscriptionsForTeams,
  getActiveTeamSubscription,
} from "@/lib/billing/creem-subscriptions"
import type { db as Database } from "@/db/connection"

function bytesToMb(bytes: number): number {
  return Math.round((bytes / (1024 * 1024)) * 10) / 10
}

// The team's effective plan for admin read surfaces: comp floor over the
// Creem-derived tier, mirroring getTeamPlan (self-hosted → unlimited).
function effectivePlanForAdmin(
  cloud: boolean,
  sub: { productId: string; seats: number } | null,
  rawCompTier: string | null
): { plan: PlanTier; creemTier: PlanTier; compTier: PlanTier | null; compApplied: boolean } {
  const compTier = parseCompTier(rawCompTier)
  if (!cloud) {
    return { plan: `unlimited`, creemTier: `unlimited`, compTier, compApplied: false }
  }
  const creemTier = planFromSubscription(sub).plan
  const plan = resolveEffectiveTier(creemTier, compTier)
  return { plan, creemTier, compTier, compApplied: plan !== creemTier }
}

export const adminRouter = router({
  listUsers: adminProcedure.query(async ({ ctx }) => {
    const rows = await ctx.db
      .select({
        id: users.id,
        name: users.name,
        email: users.email,
        image: users.image,
        isAdmin: users.isAdmin,
        createdAt: users.createdAt,
        teamCount: sql<number>`count(distinct ${teamMembers.teamId})::int`,
        providers: sql<string[]>`coalesce(array_agg(distinct ${accounts.providerId}) filter (where ${accounts.providerId} is not null), '{}')`,
        // max() is duplicate-insensitive, so the join fan-out that forces the
        // count(distinct …) above is harmless here.
        lastLoginAt: sql<Date | null>`max(${sessions.createdAt})`,
        lastActiveAt: sql<Date | null>`max(${sessions.updatedAt})`,
      })
      .from(users)
      .where(eq(users.isAgent, false))
      .leftJoin(teamMembers, eq(teamMembers.userId, users.id))
      .leftJoin(accounts, eq(accounts.userId, users.id))
      .leftJoin(sessions, eq(sessions.userId, users.id))
      .groupBy(users.id)
      .orderBy(desc(users.createdAt))

    return rows
  }),

  setUserAdmin: adminProcedure
    .input(
      z.object({
        userId: z.string(),
        isAdmin: z.boolean(),
      })
    )
    .mutation(async ({ ctx, input }) => {
      // Block demoting the last admin (including self).
      if (!input.isAdmin) {
        const [{ adminCount }] = await ctx.db
          .select({ adminCount: sql<number>`count(*)::int` })
          .from(users)
          .where(eq(users.isAdmin, true))
        if (
          adminCount <= 1 &&
          (await isTargetAdmin(ctx.db, input.userId))
        ) {
          throw new TRPCError({
            code: `BAD_REQUEST`,
            message: `Cannot demote the last admin`,
          })
        }
      }

      await ctx.db
        .update(users)
        .set({ isAdmin: input.isAdmin, updatedAt: new Date() })
        .where(eq(users.id, input.userId))

      return { ok: true }
    }),

  deleteUser: adminProcedure
    .input(z.object({ userId: z.string() }))
    .mutation(async ({ ctx, input }) => {
      if (input.userId === ctx.session.user.id) {
        throw new TRPCError({
          code: `BAD_REQUEST`,
          message: `Cannot delete yourself`,
        })
      }

      const [target] = await ctx.db
        .select({ isAdmin: users.isAdmin, isAgent: users.isAgent })
        .from(users)
        .where(eq(users.id, input.userId))
        .limit(1)
      if (!target) {
        throw new TRPCError({ code: `NOT_FOUND`, message: `User not found` })
      }
      if (target.isAgent) {
        // Same guard as users.deleteAccount: synthetic widget bot users own
        // every issue their widget created, and issues.creator_id cascades on
        // user delete — deleting the bot would irreversibly erase all its
        // widget-submitted feedback.
        throw new TRPCError({
          code: `BAD_REQUEST`,
          message: `This is a synthetic widget user — deleting it would cascade-delete every issue its widget created`,
        })
      }

      // Block deleting the last admin.
      if (target.isAdmin) {
        const [{ adminCount }] = await ctx.db
          .select({ adminCount: sql<number>`count(*)::int` })
          .from(users)
          .where(eq(users.isAdmin, true))
        if (adminCount <= 1) {
          throw new TRPCError({
            code: `BAD_REQUEST`,
            message: `Cannot delete the last admin`,
          })
        }
      }

      // Subscriptions the user purchased, captured BEFORE the delete — the
      // buyer FK cascades with the users row, after which the remote Creem
      // subscription would keep charging with nothing left to find it by.
      const doomedSubscriptions = await findActiveSubscriptionsForUser(
        input.userId
      )

      // Apple pairing to revoke after the delete (guideline 5.1.1(v)) —
      // captured now because the accounts row cascades with the users row.
      // Native-idToken pairings store no tokens (nothing to revoke).
      const appleTokens = await captureAppleTokens(ctx.db, input.userId)

      let storageKeys: string[] = []
      await ctx.db.transaction(async (tx) => {
        // Same orphan safety as users.deleteAccount (lib/account-deletion.ts):
        // fail closed when the user is the sole owner of a team that
        // still has other members — an admin delete must not silently strand
        // a team — and delete teams where they are the only member.
        const cleanup = await guardAndCleanupTeamsForUserDeletion(
          tx,
          input.userId,
          `admin`
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
        await tx.delete(users).where(eq(users.id, input.userId))
      })

      // Best-effort AFTER commit: a Creem API failure logs loudly but never
      // leaves the user half-deleted.
      await cancelCreemSubscriptionsBestEffort(doomedSubscriptions)
      // The users-row cascade dropped attachment rows but not their S3 blobs.
      await deleteStorageObjects(storageKeys)
      // Revoke the deleted user's Apple pairing (best-effort).
      await revokeAppleTokensBestEffort(appleTokens)

      return { ok: true }
    }),

  listTeams: adminProcedure.query(async ({ ctx }) => {
    const wsRows = await ctx.db
      .select({
        id: teams.id,
        name: teams.name,
        slug: teams.slug,
        compTier: teams.compTier,
        createdAt: teams.createdAt,
      })
      .from(teams)
      .orderBy(desc(teams.createdAt))

    if (wsRows.length === 0) return []

    const ids = wsRows.map((w) => w.id)

    // Grouped queries only — one per aggregate, never per team (the old
    // implementation ran getTeamPlan once per row).
    const [memberRows, boardRows, issueRows, storageRows, ownerRows, subRows] =
      await Promise.all([
        ctx.db
          .select({
            teamId: teamMembers.teamId,
            count: sql<number>`count(*)::int`,
          })
          .from(teamMembers)
          .where(inArray(teamMembers.teamId, ids))
          .groupBy(teamMembers.teamId),
        ctx.db
          .select({
            teamId: boards.teamId,
            count: sql<number>`count(*)::int`,
          })
          .from(boards)
          .where(inArray(boards.teamId, ids))
          .groupBy(boards.teamId),
        ctx.db
          .select({
            teamId: boards.teamId,
            count: sql<number>`count(${issues.id})::int`,
          })
          .from(issues)
          .innerJoin(boards, eq(boards.id, issues.boardId))
          .where(inArray(boards.teamId, ids))
          .groupBy(boards.teamId),
        ctx.db
          .select({
            teamId: attachments.teamId,
            totalBytes: sql<string>`coalesce(sum(${attachments.sizeBytes}), 0)::bigint`,
          })
          .from(attachments)
          .where(inArray(attachments.teamId, ids))
          .groupBy(attachments.teamId),
        ctx.db
          .select({
            teamId: teamMembers.teamId,
            userId: users.id,
            name: users.name,
            email: users.email,
          })
          .from(teamMembers)
          .innerJoin(users, eq(users.id, teamMembers.userId))
          .where(
            and(
              inArray(teamMembers.teamId, ids),
              eq(teamMembers.role, `owner`)
            )
          ),
        ctx.db
          .select({
            teamId: creem_subscriptions.teamId,
            productId: creem_subscriptions.productId,
            seats: creem_subscriptions.seats,
            status: creem_subscriptions.status,
            periodEnd: creem_subscriptions.periodEnd,
          })
          .from(creem_subscriptions)
          .where(
            and(
              inArray(creem_subscriptions.teamId, ids),
              inArray(creem_subscriptions.status, ACTIVE_SUBSCRIPTION_STATUSES)
            )
          )
          .orderBy(desc(creem_subscriptions.seats)),
      ])

    const memberCounts = new Map(memberRows.map((r) => [r.teamId, r.count]))
    const boardCounts = new Map(boardRows.map((r) => [r.teamId, r.count]))
    const issueCounts = new Map(issueRows.map((r) => [r.teamId, r.count]))
    const storageMbByWs = new Map(
      storageRows.map((r) => [r.teamId, bytesToMb(Number(r.totalBytes))])
    )
    const ownersByWs = new Map<
      string,
      { id: string; name: string; email: string }[]
    >()
    for (const o of ownerRows) {
      const list = ownersByWs.get(o.teamId) ?? []
      list.push({ id: o.userId, name: o.name, email: o.email })
      ownersByWs.set(o.teamId, list)
    }
    // Rows arrive seats-desc, so the first row per team is the same
    // most-seats-wins subscription getTeamPlan resolves.
    const subByWs = new Map<string, (typeof subRows)[number]>()
    for (const s of subRows) {
      if (s.teamId && !subByWs.has(s.teamId)) {
        subByWs.set(s.teamId, s)
      }
    }

    const cloud = isCloudInstance()

    return wsRows.map((w) => {
      const sub = subByWs.get(w.id) ?? null
      const { plan, compTier, compApplied } = effectivePlanForAdmin(
        cloud,
        sub,
        w.compTier
      )
      return {
        id: w.id,
        name: w.name,
        slug: w.slug,
        createdAt: w.createdAt,
        plan,
        compTier,
        compApplied,
        subscription: sub
          ? {
              tier: planFromSubscription(sub).plan,
              seats: sub.seats,
              status: sub.status,
              periodEnd: sub.periodEnd,
            }
          : null,
        memberCount: memberCounts.get(w.id) ?? 0,
        boardCount: boardCounts.get(w.id) ?? 0,
        issueCount: issueCounts.get(w.id) ?? 0,
        storageMb: storageMbByWs.get(w.id) ?? 0,
        owners: ownersByWs.get(w.id) ?? [],
      }
    })
  }),

  getTeamDetail: adminProcedure
    .input(z.object({ teamId: z.string().uuid() }))
    .query(async ({ ctx, input }) => {
      const [ws] = await ctx.db
        .select({
          id: teams.id,
          name: teams.name,
          slug: teams.slug,
          iconUrl: teams.iconUrl,
          compTier: teams.compTier,
          createdAt: teams.createdAt,
        })
        .from(teams)
        .where(eq(teams.id, input.teamId))
        .limit(1)
      if (!ws) {
        throw new TRPCError({ code: `NOT_FOUND`, message: `Team not found` })
      }

      const [
        planData,
        usage,
        subscription,
        [issueCountRow],
        memberRows,
        boardRows,
        eventRows,
      ] = await Promise.all([
        // Already comp-aware (the comp floor lives in getTeamPlan).
        getTeamPlan(input.teamId),
        getTeamUsage(input.teamId),
        getActiveTeamSubscription(input.teamId),
        ctx.db
          .select({ count: sql<number>`count(${issues.id})::int` })
          .from(issues)
          .innerJoin(boards, eq(boards.id, issues.boardId))
          .where(eq(boards.teamId, input.teamId)),
        ctx.db
          .select({
            userId: users.id,
            name: users.name,
            email: users.email,
            image: users.image,
            isAgent: users.isAgent,
            role: teamMembers.role,
            memberSince: teamMembers.createdAt,
            lastActiveAt: sql<Date | null>`max(${sessions.updatedAt})`,
          })
          .from(teamMembers)
          .innerJoin(users, eq(users.id, teamMembers.userId))
          .leftJoin(sessions, eq(sessions.userId, users.id))
          .where(eq(teamMembers.teamId, input.teamId))
          .groupBy(users.id, teamMembers.id)
          .orderBy(desc(teamMembers.createdAt)),
        ctx.db
          .select({
            id: boards.id,
            name: boards.name,
            slug: boards.slug,
            deletedAt: boards.deletedAt,
            createdAt: boards.createdAt,
            issueCount: sql<number>`count(${issues.id})::int`,
          })
          .from(boards)
          .leftJoin(issues, eq(issues.boardId, boards.id))
          .where(eq(boards.teamId, input.teamId))
          .groupBy(boards.id)
          .orderBy(desc(boards.createdAt)),
        ctx.db
          .select({
            id: issueEvents.id,
            type: issueEvents.type,
            payload: issueEvents.payload,
            createdAt: issueEvents.createdAt,
            issueIdentifier: issues.identifier,
            issueTitle: issues.title,
            actorName: users.name,
            actorEmail: users.email,
          })
          .from(issueEvents)
          .innerJoin(issues, eq(issues.id, issueEvents.issueId))
          .leftJoin(users, eq(users.id, issueEvents.actorUserId))
          .where(eq(issueEvents.teamId, input.teamId))
          .orderBy(desc(issueEvents.createdAt))
          .limit(50),
      ])

      // email_deliveries has no team column — scope via BOTH linkages it
      // does have: the recipient being a member (digest/notification mail) and
      // the issue living in this team (covers widget_resolution rows,
      // whose user_id is null because widget reporters have no users row).
      const memberIds = memberRows.map((m) => m.userId)
      const emailRows = await ctx.db
        .select({
          id: emailDeliveries.id,
          toEmail: emailDeliveries.toEmail,
          kind: emailDeliveries.kind,
          status: emailDeliveries.status,
          provider: emailDeliveries.provider,
          error: emailDeliveries.error,
          sentAt: emailDeliveries.sentAt,
          createdAt: emailDeliveries.createdAt,
          issueIdentifier: issues.identifier,
        })
        .from(emailDeliveries)
        .leftJoin(issues, eq(issues.id, emailDeliveries.issueId))
        .leftJoin(boards, eq(boards.id, issues.boardId))
        .where(
          or(
            memberIds.length
              ? inArray(emailDeliveries.userId, memberIds)
              : sql`false`,
            eq(boards.teamId, input.teamId)
          )
        )
        .orderBy(desc(emailDeliveries.createdAt))
        .limit(50)

      const compTier = parseCompTier(ws.compTier)
      const creemTier = planFromSubscription(
        subscription
          ? { productId: subscription.productId, seats: subscription.seats }
          : null
      ).plan

      return {
        team: {
          id: ws.id,
          name: ws.name,
          slug: ws.slug,
          iconUrl: ws.iconUrl,
          createdAt: ws.createdAt,
        },
        plan: planData.plan,
        limits: {
          seats: planData.limits.seats,
          storageMb: planData.limits.storageMb,
          widgetConfigs: planData.limits.widgetConfigs,
        },
        compTier,
        // True when the comp floor (not the subscription) determines the plan.
        compApplied:
          isCloudInstance() &&
          compTier !== null &&
          resolveEffectiveTier(creemTier, compTier) !== creemTier,
        subscription: subscription
          ? {
              id: subscription.id,
              tier: creemTier,
              productId: subscription.productId,
              seats: subscription.seats,
              status: subscription.status,
              periodStart: subscription.periodStart,
              periodEnd: subscription.periodEnd,
              cancelAtPeriodEnd: subscription.cancelAtPeriodEnd,
            }
          : null,
        usage,
        issueCount: issueCountRow?.count ?? 0,
        members: memberRows,
        boards: boardRows,
        events: eventRows,
        emailDeliveries: emailRows,
      }
    }),

  getUserDetail: adminProcedure
    .input(z.object({ userId: z.string() }))
    .query(async ({ ctx, input }) => {
      const [user] = await ctx.db
        .select({
          id: users.id,
          name: users.name,
          email: users.email,
          image: users.image,
          isAdmin: users.isAdmin,
          isAgent: users.isAgent,
          createdAt: users.createdAt,
        })
        .from(users)
        .where(eq(users.id, input.userId))
        .limit(1)
      if (!user) {
        throw new TRPCError({ code: `NOT_FOUND`, message: `User not found` })
      }

      const [providerRows, membershipRows, sessionRows, emailRows, [issueCountRow]] =
        await Promise.all([
          ctx.db
            .select({ providerId: accounts.providerId })
            .from(accounts)
            .where(eq(accounts.userId, input.userId)),
          ctx.db
            .select({
              teamId: teamMembers.teamId,
              role: teamMembers.role,
              memberSince: teamMembers.createdAt,
              name: teams.name,
              slug: teams.slug,
              compTier: teams.compTier,
            })
            .from(teamMembers)
            .innerJoin(teams, eq(teams.id, teamMembers.teamId))
            .where(eq(teamMembers.userId, input.userId))
            .orderBy(desc(teamMembers.createdAt)),
          ctx.db
            .select({
              id: sessions.id,
              createdAt: sessions.createdAt,
              updatedAt: sessions.updatedAt,
              expiresAt: sessions.expiresAt,
              ipAddress: sessions.ipAddress,
              userAgent: sessions.userAgent,
            })
            .from(sessions)
            .where(eq(sessions.userId, input.userId))
            .orderBy(desc(sessions.updatedAt))
            .limit(20),
          ctx.db
            .select({
              id: emailDeliveries.id,
              toEmail: emailDeliveries.toEmail,
              kind: emailDeliveries.kind,
              status: emailDeliveries.status,
              provider: emailDeliveries.provider,
              error: emailDeliveries.error,
              sentAt: emailDeliveries.sentAt,
              createdAt: emailDeliveries.createdAt,
              issueIdentifier: issues.identifier,
            })
            .from(emailDeliveries)
            .leftJoin(issues, eq(issues.id, emailDeliveries.issueId))
            .where(eq(emailDeliveries.userId, input.userId))
            .orderBy(desc(emailDeliveries.createdAt))
            .limit(50),
          ctx.db
            .select({ count: sql<number>`count(*)::int` })
            .from(issues)
            .where(eq(issues.creatorId, input.userId)),
        ])

      // One grouped subscription query for all of the user's teams —
      // same effective-plan resolution as listTeams, no per-row lookups.
      const wsIds = membershipRows.map((m) => m.teamId)
      const subRows = wsIds.length
        ? await ctx.db
            .select({
              teamId: creem_subscriptions.teamId,
              productId: creem_subscriptions.productId,
              seats: creem_subscriptions.seats,
            })
            .from(creem_subscriptions)
            .where(
              and(
                inArray(creem_subscriptions.teamId, wsIds),
                inArray(creem_subscriptions.status, ACTIVE_SUBSCRIPTION_STATUSES)
              )
            )
            .orderBy(desc(creem_subscriptions.seats))
        : []
      const subByWs = new Map<string, (typeof subRows)[number]>()
      for (const s of subRows) {
        if (s.teamId && !subByWs.has(s.teamId)) {
          subByWs.set(s.teamId, s)
        }
      }
      const cloud = isCloudInstance()

      return {
        user: {
          ...user,
          providers: providerRows.map((p) => p.providerId),
        },
        teams: membershipRows.map((m) => {
          const { plan, compApplied } = effectivePlanForAdmin(
            cloud,
            subByWs.get(m.teamId) ?? null,
            m.compTier
          )
          return {
            id: m.teamId,
            name: m.name,
            slug: m.slug,
            role: m.role,
            memberSince: m.memberSince,
            plan,
            compApplied,
          }
        }),
        sessions: sessionRows,
        emailDeliveries: emailRows,
        createdIssuesCount: issueCountRow?.count ?? 0,
      }
    }),

  setTeamCompTier: adminProcedure
    .input(
      z.object({
        teamId: z.string().uuid(),
        // null clears the comp back to the pure Creem-derived plan. `free` is
        // deliberately not grantable — a floor of free is a no-op.
        compTier: z.enum([`pro`, `business`, `unlimited`]).nullable(),
      })
    )
    .mutation(async ({ ctx, input }) => {
      const updated = await ctx.db
        .update(teams)
        .set({ compTier: input.compTier, updatedAt: new Date() })
        .where(eq(teams.id, input.teamId))
        .returning({ id: teams.id })
      if (updated.length === 0) {
        throw new TRPCError({ code: `NOT_FOUND`, message: `Team not found` })
      }
      return { ok: true }
    }),

  overview: adminProcedure.query(async ({ ctx }) => {
    const signupDay = sql<string>`to_char(date_trunc('day', ${users.createdAt}), 'YYYY-MM-DD')`
    const wsDay = sql<string>`to_char(date_trunc('day', ${teams.createdAt}), 'YYYY-MM-DD')`

    const [
      [userCount],
      [teamCount],
      [boardCount],
      [issueCount],
      [storageSum],
      subRows,
      signupRows,
      wsCreatedRows,
    ] = await Promise.all([
      ctx.db
        .select({ count: sql<number>`count(*)::int` })
        .from(users)
        .where(eq(users.isAgent, false)),
      ctx.db.select({ count: sql<number>`count(*)::int` }).from(teams),
      ctx.db.select({ count: sql<number>`count(*)::int` }).from(boards),
      ctx.db.select({ count: sql<number>`count(*)::int` }).from(issues),
      ctx.db
        .select({
          totalBytes: sql<string>`coalesce(sum(${attachments.sizeBytes}), 0)::bigint`,
        })
        .from(attachments),
      ctx.db
        .select({
          productId: creem_subscriptions.productId,
          seats: creem_subscriptions.seats,
        })
        .from(creem_subscriptions)
        .where(
          and(
            inArray(creem_subscriptions.status, ACTIVE_SUBSCRIPTION_STATUSES),
            isNotNull(creem_subscriptions.teamId)
          )
        ),
      ctx.db
        .select({ day: signupDay, count: sql<number>`count(*)::int` })
        .from(users)
        .where(
          and(
            eq(users.isAgent, false),
            sql`${users.createdAt} >= now() - interval '30 days'`
          )
        )
        .groupBy(signupDay)
        .orderBy(signupDay),
      ctx.db
        .select({ day: wsDay, count: sql<number>`count(*)::int` })
        .from(teams)
        .where(sql`${teams.createdAt} >= now() - interval '30 days'`)
        .groupBy(wsDay)
        .orderBy(wsDay),
    ])

    // Naive monthly revenue: pro $5/seat/mo, business $10/seat/mo. Yearly
    // subscriptions (pro is yearly-ONLY, business optionally yearly) are
    // normalized to their monthly-equivalent rate here — this is an MRR-style
    // estimate, not this month's actual cash collection.
    let estimatedMrr = 0
    let seatTotal = 0
    for (const sub of subRows) {
      const tier = planFromSubscription(sub).plan
      const seats = Number.isInteger(sub.seats) && sub.seats > 0 ? sub.seats : 1
      seatTotal += seats
      if (tier === `pro`) estimatedMrr += 5 * seats
      else if (tier === `business`) estimatedMrr += 10 * seats
    }

    return {
      totals: {
        users: userCount.count,
        teams: teamCount.count,
        boards: boardCount.count,
        issues: issueCount.count,
        storageMb: bytesToMb(Number(storageSum.totalBytes)),
        activeSubscriptions: subRows.length,
        seats: seatTotal,
        estimatedMrr,
      },
      signupsByDay: signupRows,
      teamsByDay: wsCreatedRows,
    }
  }),

  // Bounce/complaint feedback per address (fed by /api/webhooks/ses) — the
  // worklist for suppressing bad addresses before they damage sender
  // reputation (EXP-227).
  listEmailBounces: adminProcedure.query(async ({ ctx }) => {
    return await ctx.db
      .select({
        id: emailBounces.id,
        email: emailBounces.email,
        kind: emailBounces.kind,
        bounceType: emailBounces.bounceType,
        bounceSubType: emailBounces.bounceSubType,
        diagnostic: emailBounces.diagnostic,
        eventCount: emailBounces.eventCount,
        lastEventAt: emailBounces.lastEventAt,
        suppressedAt: emailBounces.suppressedAt,
      })
      .from(emailBounces)
      .orderBy(desc(emailBounces.lastEventAt))
      .limit(200)
  }),

  // Put a bounced/complaining address on the SES account-level suppression
  // list. SES then blocks sends to it at the source — the reputation-safe
  // terminal state for a hard-bouncing address.
  suppressEmailBounce: adminProcedure
    .input(z.object({ bounceId: z.string().uuid() }))
    .mutation(async ({ ctx, input }) => {
      const [row] = await ctx.db
        .select({
          id: emailBounces.id,
          email: emailBounces.email,
          kind: emailBounces.kind,
          suppressedAt: emailBounces.suppressedAt,
        })
        .from(emailBounces)
        .where(eq(emailBounces.id, input.bounceId))
        .limit(1)
      if (!row) {
        throw new TRPCError({ code: `NOT_FOUND`, message: `Bounce not found` })
      }
      if (row.suppressedAt) return { ok: true }

      try {
        await suppressSesDestination(
          row.email,
          row.kind === `complaint` ? `COMPLAINT` : `BOUNCE`
        )
      } catch (err) {
        throw new TRPCError({
          code: `PRECONDITION_FAILED`,
          message: err instanceof Error ? err.message : `SES suppression failed`,
        })
      }

      await ctx.db
        .update(emailBounces)
        .set({ suppressedAt: new Date(), updatedAt: new Date() })
        .where(eq(emailBounces.id, row.id))
      return { ok: true }
    }),

  deleteTeam: adminProcedure
    .input(z.object({ teamId: z.string().uuid() }))
    .mutation(async ({ ctx, input }) => {
      // The cloud boot would recreate the feedback team EMPTY — block.
      if (input.teamId === (await getFeedbackTeamId())) {
        throw new TRPCError({
          code: `BAD_REQUEST`,
          message: `The feedback team cannot be deleted`,
        })
      }

      // Capture BEFORE the delete: creem_subscriptions.team_id goes
      // `set null` when the team row is deleted.
      const doomedSubscriptions = await findActiveSubscriptionsForTeams([
        input.teamId,
      ])

      // Collected inside the tx BEFORE the cascade drops the attachment rows;
      // the cascade never touches S3, so without this the blobs orphan.
      let storageKeys: string[] = []
      const result = await ctx.db.transaction(async (tx) => {
        const txId = await generateTxId(tx)
        storageKeys = (
          await tx
            .select({ storageKey: attachments.storageKey })
            .from(attachments)
            .where(eq(attachments.teamId, input.teamId))
        ).map((row) => row.storageKey)
        await tx.delete(teams).where(eq(teams.id, input.teamId))
        return { ok: true, txId }
      })

      // Best-effort AFTER commit: a Creem API failure logs loudly but never
      // leaves the team half-deleted.
      await cancelCreemSubscriptionsBestEffort(doomedSubscriptions)
      await deleteStorageObjects(storageKeys)

      return result
    }),
})

async function isTargetAdmin(
  db: typeof Database,
  userId: string
): Promise<boolean> {
  const [u] = await db
    .select({ isAdmin: users.isAdmin })
    .from(users)
    .where(eq(users.id, userId))
    .limit(1)
  return Boolean(u?.isAdmin)
}

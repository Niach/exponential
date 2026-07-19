import { z } from "zod"
import { TRPCError } from "@trpc/server"
import { and, desc, eq, inArray, isNotNull, or, sql } from "drizzle-orm"
import { router, adminProcedure, generateTxId } from "@/lib/trpc"
import { users, accounts, sessions } from "@/db/auth-schema"
import {
  attachments,
  workspaces,
  workspaceMembers,
  projects,
  issues,
  issueEvents,
  emailDeliveries,
  creem_subscriptions,
} from "@/db/schema"
import {
  getWorkspacePlan,
  getWorkspaceUsage,
  planFromSubscription,
  parseCompTier,
  resolveEffectiveTier,
  type PlanTier,
} from "@/lib/billing"
import { deleteStorageObjects } from "@/lib/storage/issue-attachment-cleanup"
import { getFeedbackWorkspaceId, isCloudInstance } from "@/lib/bootstrap-cloud"
import { guardAndCleanupWorkspacesForUserDeletion } from "@/lib/account-deletion"
import {
  ACTIVE_SUBSCRIPTION_STATUSES,
  cancelCreemSubscriptionsBestEffort,
  findActiveSubscriptionsForUser,
  findActiveSubscriptionsForWorkspaces,
  getActiveWorkspaceSubscription,
} from "@/lib/billing/creem-subscriptions"
import type { db as Database } from "@/db/connection"

function bytesToMb(bytes: number): number {
  return Math.round((bytes / (1024 * 1024)) * 10) / 10
}

// The workspace's effective plan for admin read surfaces: comp floor over the
// Creem-derived tier, mirroring getWorkspacePlan (self-hosted → unlimited).
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
        workspaceCount: sql<number>`count(distinct ${workspaceMembers.workspaceId})::int`,
        providers: sql<string[]>`coalesce(array_agg(distinct ${accounts.providerId}) filter (where ${accounts.providerId} is not null), '{}')`,
        // max() is duplicate-insensitive, so the join fan-out that forces the
        // count(distinct …) above is harmless here.
        lastLoginAt: sql<Date | null>`max(${sessions.createdAt})`,
        lastActiveAt: sql<Date | null>`max(${sessions.updatedAt})`,
      })
      .from(users)
      .where(eq(users.isAgent, false))
      .leftJoin(workspaceMembers, eq(workspaceMembers.userId, users.id))
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

      let storageKeys: string[] = []
      await ctx.db.transaction(async (tx) => {
        // Same orphan safety as users.deleteAccount (lib/account-deletion.ts):
        // fail closed when the user is the sole owner of a workspace that
        // still has other members — an admin delete must not silently strand
        // a team — and delete workspaces where they are the only member.
        const cleanup = await guardAndCleanupWorkspacesForUserDeletion(
          tx,
          input.userId,
          `admin`
        )
        storageKeys = cleanup.storageKeys
        // Subscriptions bound to the deleted solo workspaces but purchased by
        // SOMEONE ELSE (e.g. after an ownership hand-off) — invisible to the
        // buyer-scoped capture above, yet their workspace just vanished.
        for (const sub of cleanup.doomedWorkspaceSubscriptions) {
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

      return { ok: true }
    }),

  listWorkspaces: adminProcedure.query(async ({ ctx }) => {
    const wsRows = await ctx.db
      .select({
        id: workspaces.id,
        name: workspaces.name,
        slug: workspaces.slug,
        compTier: workspaces.compTier,
        createdAt: workspaces.createdAt,
      })
      .from(workspaces)
      .orderBy(desc(workspaces.createdAt))

    if (wsRows.length === 0) return []

    const ids = wsRows.map((w) => w.id)

    // Grouped queries only — one per aggregate, never per workspace (the old
    // implementation ran getWorkspacePlan once per row).
    const [memberRows, projectRows, issueRows, storageRows, ownerRows, subRows] =
      await Promise.all([
        ctx.db
          .select({
            workspaceId: workspaceMembers.workspaceId,
            count: sql<number>`count(*)::int`,
          })
          .from(workspaceMembers)
          .where(inArray(workspaceMembers.workspaceId, ids))
          .groupBy(workspaceMembers.workspaceId),
        ctx.db
          .select({
            workspaceId: projects.workspaceId,
            count: sql<number>`count(*)::int`,
          })
          .from(projects)
          .where(inArray(projects.workspaceId, ids))
          .groupBy(projects.workspaceId),
        ctx.db
          .select({
            workspaceId: projects.workspaceId,
            count: sql<number>`count(${issues.id})::int`,
          })
          .from(issues)
          .innerJoin(projects, eq(projects.id, issues.projectId))
          .where(inArray(projects.workspaceId, ids))
          .groupBy(projects.workspaceId),
        ctx.db
          .select({
            workspaceId: attachments.workspaceId,
            totalBytes: sql<string>`coalesce(sum(${attachments.sizeBytes}), 0)::bigint`,
          })
          .from(attachments)
          .where(inArray(attachments.workspaceId, ids))
          .groupBy(attachments.workspaceId),
        ctx.db
          .select({
            workspaceId: workspaceMembers.workspaceId,
            userId: users.id,
            name: users.name,
            email: users.email,
          })
          .from(workspaceMembers)
          .innerJoin(users, eq(users.id, workspaceMembers.userId))
          .where(
            and(
              inArray(workspaceMembers.workspaceId, ids),
              eq(workspaceMembers.role, `owner`)
            )
          ),
        ctx.db
          .select({
            workspaceId: creem_subscriptions.workspaceId,
            productId: creem_subscriptions.productId,
            seats: creem_subscriptions.seats,
            status: creem_subscriptions.status,
            periodEnd: creem_subscriptions.periodEnd,
          })
          .from(creem_subscriptions)
          .where(
            and(
              inArray(creem_subscriptions.workspaceId, ids),
              inArray(creem_subscriptions.status, ACTIVE_SUBSCRIPTION_STATUSES)
            )
          )
          .orderBy(desc(creem_subscriptions.seats)),
      ])

    const memberCounts = new Map(memberRows.map((r) => [r.workspaceId, r.count]))
    const projectCounts = new Map(projectRows.map((r) => [r.workspaceId, r.count]))
    const issueCounts = new Map(issueRows.map((r) => [r.workspaceId, r.count]))
    const storageMbByWs = new Map(
      storageRows.map((r) => [r.workspaceId, bytesToMb(Number(r.totalBytes))])
    )
    const ownersByWs = new Map<
      string,
      { id: string; name: string; email: string }[]
    >()
    for (const o of ownerRows) {
      const list = ownersByWs.get(o.workspaceId) ?? []
      list.push({ id: o.userId, name: o.name, email: o.email })
      ownersByWs.set(o.workspaceId, list)
    }
    // Rows arrive seats-desc, so the first row per workspace is the same
    // most-seats-wins subscription getWorkspacePlan resolves.
    const subByWs = new Map<string, (typeof subRows)[number]>()
    for (const s of subRows) {
      if (s.workspaceId && !subByWs.has(s.workspaceId)) {
        subByWs.set(s.workspaceId, s)
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
        projectCount: projectCounts.get(w.id) ?? 0,
        issueCount: issueCounts.get(w.id) ?? 0,
        storageMb: storageMbByWs.get(w.id) ?? 0,
        owners: ownersByWs.get(w.id) ?? [],
      }
    })
  }),

  getWorkspaceDetail: adminProcedure
    .input(z.object({ workspaceId: z.string().uuid() }))
    .query(async ({ ctx, input }) => {
      const [ws] = await ctx.db
        .select({
          id: workspaces.id,
          name: workspaces.name,
          slug: workspaces.slug,
          iconUrl: workspaces.iconUrl,
          compTier: workspaces.compTier,
          createdAt: workspaces.createdAt,
        })
        .from(workspaces)
        .where(eq(workspaces.id, input.workspaceId))
        .limit(1)
      if (!ws) {
        throw new TRPCError({ code: `NOT_FOUND`, message: `Workspace not found` })
      }

      const [
        planData,
        usage,
        subscription,
        [issueCountRow],
        memberRows,
        projectRows,
        eventRows,
      ] = await Promise.all([
        // Already comp-aware (the comp floor lives in getWorkspacePlan).
        getWorkspacePlan(input.workspaceId),
        getWorkspaceUsage(input.workspaceId),
        getActiveWorkspaceSubscription(input.workspaceId),
        ctx.db
          .select({ count: sql<number>`count(${issues.id})::int` })
          .from(issues)
          .innerJoin(projects, eq(projects.id, issues.projectId))
          .where(eq(projects.workspaceId, input.workspaceId)),
        ctx.db
          .select({
            userId: users.id,
            name: users.name,
            email: users.email,
            image: users.image,
            isAgent: users.isAgent,
            role: workspaceMembers.role,
            memberSince: workspaceMembers.createdAt,
            lastActiveAt: sql<Date | null>`max(${sessions.updatedAt})`,
          })
          .from(workspaceMembers)
          .innerJoin(users, eq(users.id, workspaceMembers.userId))
          .leftJoin(sessions, eq(sessions.userId, users.id))
          .where(eq(workspaceMembers.workspaceId, input.workspaceId))
          .groupBy(users.id, workspaceMembers.id)
          .orderBy(desc(workspaceMembers.createdAt)),
        ctx.db
          .select({
            id: projects.id,
            name: projects.name,
            slug: projects.slug,
            deletedAt: projects.deletedAt,
            createdAt: projects.createdAt,
            issueCount: sql<number>`count(${issues.id})::int`,
          })
          .from(projects)
          .leftJoin(issues, eq(issues.projectId, projects.id))
          .where(eq(projects.workspaceId, input.workspaceId))
          .groupBy(projects.id)
          .orderBy(desc(projects.createdAt)),
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
          .where(eq(issueEvents.workspaceId, input.workspaceId))
          .orderBy(desc(issueEvents.createdAt))
          .limit(50),
      ])

      // email_deliveries has no workspace column — scope via BOTH linkages it
      // does have: the recipient being a member (digest/notification mail) and
      // the issue living in this workspace (covers widget_resolution rows,
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
        .leftJoin(projects, eq(projects.id, issues.projectId))
        .where(
          or(
            memberIds.length
              ? inArray(emailDeliveries.userId, memberIds)
              : sql`false`,
            eq(projects.workspaceId, input.workspaceId)
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
        workspace: {
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
        projects: projectRows,
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
              workspaceId: workspaceMembers.workspaceId,
              role: workspaceMembers.role,
              memberSince: workspaceMembers.createdAt,
              name: workspaces.name,
              slug: workspaces.slug,
              compTier: workspaces.compTier,
            })
            .from(workspaceMembers)
            .innerJoin(workspaces, eq(workspaces.id, workspaceMembers.workspaceId))
            .where(eq(workspaceMembers.userId, input.userId))
            .orderBy(desc(workspaceMembers.createdAt)),
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

      // One grouped subscription query for all of the user's workspaces —
      // same effective-plan resolution as listWorkspaces, no per-row lookups.
      const wsIds = membershipRows.map((m) => m.workspaceId)
      const subRows = wsIds.length
        ? await ctx.db
            .select({
              workspaceId: creem_subscriptions.workspaceId,
              productId: creem_subscriptions.productId,
              seats: creem_subscriptions.seats,
            })
            .from(creem_subscriptions)
            .where(
              and(
                inArray(creem_subscriptions.workspaceId, wsIds),
                inArray(creem_subscriptions.status, ACTIVE_SUBSCRIPTION_STATUSES)
              )
            )
            .orderBy(desc(creem_subscriptions.seats))
        : []
      const subByWs = new Map<string, (typeof subRows)[number]>()
      for (const s of subRows) {
        if (s.workspaceId && !subByWs.has(s.workspaceId)) {
          subByWs.set(s.workspaceId, s)
        }
      }
      const cloud = isCloudInstance()

      return {
        user: {
          ...user,
          providers: providerRows.map((p) => p.providerId),
        },
        workspaces: membershipRows.map((m) => {
          const { plan, compApplied } = effectivePlanForAdmin(
            cloud,
            subByWs.get(m.workspaceId) ?? null,
            m.compTier
          )
          return {
            id: m.workspaceId,
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

  setWorkspaceCompTier: adminProcedure
    .input(
      z.object({
        workspaceId: z.string().uuid(),
        // null clears the comp back to the pure Creem-derived plan. `free` is
        // deliberately not grantable — a floor of free is a no-op.
        compTier: z.enum([`pro`, `business`, `unlimited`]).nullable(),
      })
    )
    .mutation(async ({ ctx, input }) => {
      const updated = await ctx.db
        .update(workspaces)
        .set({ compTier: input.compTier, updatedAt: new Date() })
        .where(eq(workspaces.id, input.workspaceId))
        .returning({ id: workspaces.id })
      if (updated.length === 0) {
        throw new TRPCError({ code: `NOT_FOUND`, message: `Workspace not found` })
      }
      return { ok: true }
    }),

  overview: adminProcedure.query(async ({ ctx }) => {
    const signupDay = sql<string>`to_char(date_trunc('day', ${users.createdAt}), 'YYYY-MM-DD')`
    const wsDay = sql<string>`to_char(date_trunc('day', ${workspaces.createdAt}), 'YYYY-MM-DD')`

    const [
      [userCount],
      [workspaceCount],
      [projectCount],
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
      ctx.db.select({ count: sql<number>`count(*)::int` }).from(workspaces),
      ctx.db.select({ count: sql<number>`count(*)::int` }).from(projects),
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
            isNotNull(creem_subscriptions.workspaceId)
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
        .from(workspaces)
        .where(sql`${workspaces.createdAt} >= now() - interval '30 days'`)
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
        workspaces: workspaceCount.count,
        projects: projectCount.count,
        issues: issueCount.count,
        storageMb: bytesToMb(Number(storageSum.totalBytes)),
        activeSubscriptions: subRows.length,
        seats: seatTotal,
        estimatedMrr,
      },
      signupsByDay: signupRows,
      workspacesByDay: wsCreatedRows,
    }
  }),

  deleteWorkspace: adminProcedure
    .input(z.object({ workspaceId: z.string().uuid() }))
    .mutation(async ({ ctx, input }) => {
      // The cloud boot would recreate the feedback workspace EMPTY — block.
      if (input.workspaceId === (await getFeedbackWorkspaceId())) {
        throw new TRPCError({
          code: `BAD_REQUEST`,
          message: `The feedback workspace cannot be deleted`,
        })
      }

      // Capture BEFORE the delete: creem_subscriptions.workspace_id goes
      // `set null` when the workspace row is deleted.
      const doomedSubscriptions = await findActiveSubscriptionsForWorkspaces([
        input.workspaceId,
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
            .where(eq(attachments.workspaceId, input.workspaceId))
        ).map((row) => row.storageKey)
        await tx.delete(workspaces).where(eq(workspaces.id, input.workspaceId))
        return { ok: true, txId }
      })

      // Best-effort AFTER commit: a Creem API failure logs loudly but never
      // leaves the workspace half-deleted.
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

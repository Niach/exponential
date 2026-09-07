import { z } from "zod"
import { TRPCError } from "@trpc/server"
import { and, desc, eq, inArray, isNotNull, sql, type SQL } from "drizzle-orm"
import { router, adminProcedure, type Context } from "@/lib/trpc"
import {
  boards,
  conversionEvents,
  creem_subscriptions,
  devices,
  issues,
  sessions,
  teamInvites,
  teamMembers,
  teams,
  users,
} from "@/db/schema"
import { isCloudInstance } from "@/lib/bootstrap-cloud"
import { platformsByUserSubquery } from "@/lib/client-platforms"
import { ACTIVE_SUBSCRIPTION_STATUSES } from "@/lib/billing/creem-subscriptions"

// Admin console → Conversions (EXP-362). One aggregate procedure feeding the
// whole /admin/conversions page in a single loader call (the admin.overview
// convention). All numbers are PERIOD counts — each funnel stage counted
// within the window independently, not a cohort followed through it — and
// visitor counts are unique visitor-DAYS (the cookieless anonymous id
// rotates daily) with best-effort bot filtering: directional, not exact.

// Signups in the window grouped by claimed attribution, with the paid count
// per group. Extracted so admin-conversions.test.ts can assert the COMPILED
// SQL without a database (EXP-373).
//
// The paid count is a JOIN and not a correlated `exists (...)` inside the
// select list on purpose: drizzle's buildSelection strips the table qualifier
// off every column ref in a select-list sql`` template when the query has a
// single FROM table, so an outer `${users.id}` compiled to a bare `"id"` that
// silently rebound to conversion_events.id inside the subquery — `operator
// does not exist: text = uuid`, a 500 that took the whole admin page down.
// Grouping the subquery keeps the join 1:0..1 so `count(*)` (signups) stays
// exact, and the join makes every column ref qualified again.
export function buildSignupSourcesQuery(db: Context[`db`], windowStart: SQL) {
  const paidUsers = db
    .select({ userId: conversionEvents.userId })
    .from(conversionEvents)
    .where(
      sql`${conversionEvents.name} = 'subscription_first_active' and ${conversionEvents.userId} is not null`
    )
    .groupBy(conversionEvents.userId)
    .as(`paid_users`)

  return db
    .select({
      ref: users.signupRef,
      utmSource: users.signupUtmSource,
      utmMedium: users.signupUtmMedium,
      signups: sql<number>`count(*)::int`,
      paid: sql<number>`count(${paidUsers.userId})::int`,
    })
    .from(users)
    .leftJoin(paidUsers, eq(paidUsers.userId, users.id))
    .where(sql`${users.createdAt} >= ${windowStart}`)
    .groupBy(users.signupRef, users.signupUtmSource, users.signupUtmMedium)
    .orderBy(sql`count(*) desc`)
    .limit(50)
}

// EXP-759: the signup COHORT — every user who signed up in the window,
// followed through the onboarding chain by STATE tables rather than events,
// so it stays correct whatever the event vocabulary covers (board and device
// steps emit no conversion event; invited users complete onboarding silently
// in teamInvites.accept). Same shape rule as buildSignupSourcesQuery: one
// grouped 1:0..1 subquery per stage, LEFT JOINed to users, never a correlated
// subquery in the select list.
export function signupCohortSources(db: Context[`db`]) {
  const n = (alias: string) => sql<number>`count(*)::int`.as(alias)
  return {
    membership: db
      .select({ userId: teamMembers.userId, teams: n(`teams`) })
      .from(teamMembers)
      .groupBy(teamMembers.userId)
      .as(`m`),
    // Boards in ANY of the user's teams (trashed/archived included — "made
    // one" is the fact), plus the newest so the cohort can tell "created a
    // board after signing up" from "joined a team that already had boards".
    boardsByUser: db
      .select({
        userId: teamMembers.userId,
        boards: sql<number>`count(${boards.id})::int`.as(`boards`),
        lastBoardAt: sql<Date>`max(${boards.createdAt})`.as(`last_board_at`),
      })
      .from(teamMembers)
      .innerJoin(boards, eq(boards.teamId, teamMembers.teamId))
      .groupBy(teamMembers.userId)
      .as(`b`),
    issuesByUser: db
      .select({ userId: issues.creatorId, issues: n(`issues`) })
      .from(issues)
      .where(isNotNull(issues.creatorId))
      .groupBy(issues.creatorId)
      .as(`i`),
    invitesByUser: db
      .select({ userId: teamInvites.invitedById, invites: n(`invites`) })
      .from(teamInvites)
      .groupBy(teamInvites.invitedById)
      .as(`inv`),
    devicesByUser: db
      .select({ userId: devices.userId, devices: n(`devices`) })
      .from(devices)
      .groupBy(devices.userId)
      .as(`d`),
    // return_visit is recorded on WEB document loads only (attribution.ts);
    // natives never "return" by this metric — the platform ledger's last
    // touch is the cross-client signal.
    returnsByUser: db
      .select({
        userId: conversionEvents.userId,
        lastDay: sql<string>`max(${conversionEvents.properties}->>'day')`.as(
          `last_day`
        ),
        returnDays: n(`return_days`),
      })
      .from(conversionEvents)
      .where(
        sql`${conversionEvents.name} = 'return_visit' and ${conversionEvents.userId} is not null`
      )
      .groupBy(conversionEvents.userId)
      .as(`rv`),
    // State, not event: member of a team with a live subscription.
    paidByUser: db
      .select({ userId: teamMembers.userId, paidTeams: n(`paid_teams`) })
      .from(teamMembers)
      .innerJoin(
        creem_subscriptions,
        and(
          eq(creem_subscriptions.teamId, teamMembers.teamId),
          inArray(creem_subscriptions.status, ACTIVE_SUBSCRIPTION_STATUSES)
        )
      )
      .groupBy(teamMembers.userId)
      .as(`paid`),
    platformsByUser: platformsByUserSubquery(db),
    lastSessionByUser: db
      .select({
        userId: sessions.userId,
        lastActiveAt: sql<Date>`max(${sessions.updatedAt})`.as(`last_active_at`),
      })
      .from(sessions)
      .groupBy(sessions.userId)
      .as(`s`),
  }
}

export function buildSignupCohortQuery(db: Context[`db`], windowStart: SQL) {
  const s = signupCohortSources(db)
  return db
    .select({
      signups: sql<number>`count(*)::int`,
      onboarded: sql<number>`count(${users.onboardingCompletedAt})::int`,
      withTeam: sql<number>`count(${s.membership.userId})::int`,
      withBoard: sql<number>`count(${s.boardsByUser.userId})::int`,
      boardAfterSignup: sql<number>`count(*) filter (where ${s.boardsByUser.lastBoardAt} > ${users.createdAt}::timestamptz)::int`,
      withIssue: sql<number>`count(${s.issuesByUser.userId})::int`,
      withInvite: sql<number>`count(${s.invitesByUser.userId})::int`,
      withDevice: sql<number>`count(${s.devicesByUser.userId})::int`,
      returned: sql<number>`count(*) filter (where ${s.returnsByUser.lastDay} > to_char(${users.createdAt}, 'YYYY-MM-DD'))::int`,
      // Any client seen on a later day than the signup — the cross-platform
      // "came back" (return_visit above is web-only).
      returnedAnyClient: sql<number>`count(*) filter (where ${s.platformsByUser.lastSeenAt}::date > ${users.createdAt}::date)::int`,
      paid: sql<number>`count(${s.paidByUser.userId})::int`,
    })
    .from(users)
    .leftJoin(s.membership, eq(s.membership.userId, users.id))
    .leftJoin(s.boardsByUser, eq(s.boardsByUser.userId, users.id))
    .leftJoin(s.issuesByUser, eq(s.issuesByUser.userId, users.id))
    .leftJoin(s.invitesByUser, eq(s.invitesByUser.userId, users.id))
    .leftJoin(s.devicesByUser, eq(s.devicesByUser.userId, users.id))
    .leftJoin(s.returnsByUser, eq(s.returnsByUser.userId, users.id))
    .leftJoin(s.platformsByUser, eq(s.platformsByUser.userId, users.id))
    .leftJoin(s.paidByUser, eq(s.paidByUser.userId, users.id))
    .where(sql`${users.createdAt} >= ${windowStart}`)
}

// The per-user journey behind the cohort: newest 50 signups in the window
// with every stage as a count, so the admin can see WHERE each one stopped.
export function buildSignupJourneyQuery(db: Context[`db`], windowStart: SQL) {
  const s = signupCohortSources(db)
  return db
    .select({
      id: users.id,
      email: users.email,
      name: users.name,
      createdAt: users.createdAt,
      onboardingCompletedAt: users.onboardingCompletedAt,
      signupRef: users.signupRef,
      signupUtmSource: users.signupUtmSource,
      signupReferrer: users.signupReferrer,
      teams: sql<number>`coalesce(${s.membership.teams}, 0)`,
      boards: sql<number>`coalesce(${s.boardsByUser.boards}, 0)`,
      boardAfterSignup: sql<boolean>`coalesce(${s.boardsByUser.lastBoardAt} > ${users.createdAt}::timestamptz, false)`,
      issues: sql<number>`coalesce(${s.issuesByUser.issues}, 0)`,
      invites: sql<number>`coalesce(${s.invitesByUser.invites}, 0)`,
      devices: sql<number>`coalesce(${s.devicesByUser.devices}, 0)`,
      returnDays: sql<number>`coalesce(${s.returnsByUser.returnDays}, 0)`,
      paidTeams: sql<number>`coalesce(${s.paidByUser.paidTeams}, 0)`,
      platforms: sql<string[]>`coalesce(${s.platformsByUser.platforms}, '{}')`,
      lastActiveAt: sql<
        Date | null
      >`greatest(${s.lastSessionByUser.lastActiveAt}::timestamptz, ${s.platformsByUser.lastSeenAt})`,
    })
    .from(users)
    .leftJoin(s.membership, eq(s.membership.userId, users.id))
    .leftJoin(s.boardsByUser, eq(s.boardsByUser.userId, users.id))
    .leftJoin(s.issuesByUser, eq(s.issuesByUser.userId, users.id))
    .leftJoin(s.invitesByUser, eq(s.invitesByUser.userId, users.id))
    .leftJoin(s.devicesByUser, eq(s.devicesByUser.userId, users.id))
    .leftJoin(s.returnsByUser, eq(s.returnsByUser.userId, users.id))
    .leftJoin(s.paidByUser, eq(s.paidByUser.userId, users.id))
    .leftJoin(s.platformsByUser, eq(s.platformsByUser.userId, users.id))
    .leftJoin(s.lastSessionByUser, eq(s.lastSessionByUser.userId, users.id))
    .where(sql`${users.createdAt} >= ${windowStart}`)
    .orderBy(desc(users.createdAt))
    .limit(50)
}

export const adminConversionsRouter = router({
  overview: adminProcedure
    .input(
      z
        .object({
          days: z
            .union([z.literal(7), z.literal(30), z.literal(90)])
            .default(30),
        })
        .default({ days: 30 })
    )
    .query(async ({ ctx, input }) => {
      // Cloud-only (EXP-362): self-hosted instances record no events, and
      // the admin console hides the page — refuse loudly if called anyway.
      if (!isCloudInstance()) {
        throw new TRPCError({
          code: `PRECONDITION_FAILED`,
          message: `Conversion tracking is a cloud-only feature`,
        })
      }
      const windowStart = sql`now() - make_interval(days => ${input.days})`
      const eventDay = sql<string>`to_char(date_trunc('day', ${conversionEvents.createdAt}), 'YYYY-MM-DD')`

      const [
        [funnelCounts],
        eventsByDay,
        sources,
        paidConversions,
        recentEvents,
        [cohortCounts],
        recentSignups,
      ] = await Promise.all([
        ctx.db
          .select({
            // The entry-path filter mirrors the capture-side allowlist
            // (EXP-522) so pre-allowlist rows on deep app paths don't pollute
            // the denominator until they age out of the window.
            visitors: sql<number>`count(distinct ${conversionEvents.anonymousId}) filter (where ${conversionEvents.name} = 'landing' and (${conversionEvents.properties}->>'path' = '/' or ${conversionEvents.properties}->>'path' like '/auth/%'))::int`,
            signups: sql<number>`count(*) filter (where ${conversionEvents.name} = 'signup')::int`,
            activated: sql<number>`count(distinct ${conversionEvents.userId}) filter (where ${conversionEvents.name} in ('first_issue_created', 'invite_sent'))::int`,
            paid: sql<number>`count(*) filter (where ${conversionEvents.name} = 'subscription_first_active')::int`,
            canceled: sql<number>`count(*) filter (where ${conversionEvents.name} = 'subscription_canceled')::int`,
          })
          .from(conversionEvents)
          .where(sql`${conversionEvents.createdAt} >= ${windowStart}`),
        ctx.db
          .select({
            day: eventDay,
            name: conversionEvents.name,
            count: sql<number>`count(*)::int`,
          })
          .from(conversionEvents)
          .where(sql`${conversionEvents.createdAt} >= ${windowStart}`)
          .groupBy(eventDay, conversionEvents.name)
          .orderBy(eventDay),
        buildSignupSourcesQuery(ctx.db, windowStart),
        ctx.db
          .select({
            id: conversionEvents.id,
            createdAt: conversionEvents.createdAt,
            userEmail: users.email,
            userCreatedAt: users.createdAt,
            signupRef: users.signupRef,
            signupUtmSource: users.signupUtmSource,
            teamName: sql<
              string | null
            >`(select t.name from ${teams} t where t.id::text = ${conversionEvents.properties}->>'teamId')`,
            seats: sql<
              number | null
            >`(${conversionEvents.properties}->>'seats')::int`,
            creemSubscriptionId: sql<
              string | null
            >`${conversionEvents.properties}->>'creemSubscriptionId'`,
          })
          .from(conversionEvents)
          .leftJoin(users, eq(users.id, conversionEvents.userId))
          .where(
            sql`${conversionEvents.name} = 'subscription_first_active' and ${conversionEvents.createdAt} >= ${windowStart}`
          )
          .orderBy(desc(conversionEvents.createdAt))
          .limit(50),
        ctx.db
          .select({
            id: conversionEvents.id,
            name: conversionEvents.name,
            createdAt: conversionEvents.createdAt,
            userEmail: users.email,
            anonymousId: conversionEvents.anonymousId,
            properties: conversionEvents.properties,
          })
          .from(conversionEvents)
          .leftJoin(users, eq(users.id, conversionEvents.userId))
          .where(sql`${conversionEvents.createdAt} >= ${windowStart}`)
          .orderBy(desc(conversionEvents.createdAt))
          .limit(50),
        buildSignupCohortQuery(ctx.db, windowStart),
        buildSignupJourneyQuery(ctx.db, windowStart),
      ])

      return {
        days: input.days,
        funnel: {
          visitors: funnelCounts?.visitors ?? 0,
          signups: funnelCounts?.signups ?? 0,
          activated: funnelCounts?.activated ?? 0,
          paid: funnelCounts?.paid ?? 0,
          canceled: funnelCounts?.canceled ?? 0,
        },
        eventsByDay,
        sources,
        paidConversions: paidConversions.map((row) => ({
          ...row,
          daysFromSignup:
            row.userCreatedAt && row.createdAt
              ? Math.max(
                  0,
                  Math.round(
                    (new Date(row.createdAt).getTime() -
                      new Date(row.userCreatedAt).getTime()) /
                      86_400_000
                  )
                )
              : null,
        })),
        recentEvents,
        cohort: {
          signups: cohortCounts?.signups ?? 0,
          onboarded: cohortCounts?.onboarded ?? 0,
          withTeam: cohortCounts?.withTeam ?? 0,
          withBoard: cohortCounts?.withBoard ?? 0,
          boardAfterSignup: cohortCounts?.boardAfterSignup ?? 0,
          withIssue: cohortCounts?.withIssue ?? 0,
          withInvite: cohortCounts?.withInvite ?? 0,
          withDevice: cohortCounts?.withDevice ?? 0,
          returned: cohortCounts?.returned ?? 0,
          returnedAnyClient: cohortCounts?.returnedAnyClient ?? 0,
          paid: cohortCounts?.paid ?? 0,
        },
        recentSignups,
      }
    }),
})

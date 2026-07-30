import { z } from "zod"
import { TRPCError } from "@trpc/server"
import { desc, eq, sql } from "drizzle-orm"
import { router, adminProcedure } from "@/lib/trpc"
import { conversionEvents, teams, users } from "@/db/schema"
import { isCloudInstance } from "@/lib/bootstrap-cloud"

// Admin console → Conversions (EXP-362). One aggregate procedure feeding the
// whole /admin/conversions page in a single loader call (the admin.overview
// convention). All numbers are PERIOD counts — each funnel stage counted
// within the window independently, not a cohort followed through it — and
// visitor counts are unique visitor-DAYS (the cookieless anonymous id
// rotates daily) with best-effort bot filtering: directional, not exact.

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
      ] = await Promise.all([
        ctx.db
          .select({
            visitors: sql<number>`count(distinct ${conversionEvents.anonymousId}) filter (where ${conversionEvents.name} = 'landing')::int`,
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
        ctx.db
          .select({
            ref: users.signupRef,
            utmSource: users.signupUtmSource,
            utmMedium: users.signupUtmMedium,
            signups: sql<number>`count(*)::int`,
            paid: sql<number>`count(*) filter (where exists (select 1 from ${conversionEvents} ce where ce.user_id = ${users.id} and ce.name = 'subscription_first_active'))::int`,
          })
          .from(users)
          .where(sql`${users.createdAt} >= ${windowStart}`)
          .groupBy(
            users.signupRef,
            users.signupUtmSource,
            users.signupUtmMedium
          )
          .orderBy(sql`count(*) desc`)
          .limit(50),
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
          .orderBy(desc(conversionEvents.createdAt))
          .limit(50),
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
      }
    }),
})

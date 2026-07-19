import { z } from "zod"
import { TRPCError } from "@trpc/server"
import {
  router,
  authedProcedure,
  publicProcedure,
  generateTxId,
} from "@/lib/trpc"
import { attachments, teams, teamMembers } from "@/db/schema"
import { and, asc, eq, ne } from "drizzle-orm"
import { deleteStorageObjects } from "@/lib/storage/issue-attachment-cleanup"
import { randomBytes } from "crypto"
import { getFeedbackTeamId } from "@/lib/bootstrap-cloud"
import {
  assertTeamOwner,
  getTeamMember,
} from "@/lib/team-membership"
import {
  assertCanCreateTeam,
  assertCanUseHelpdesk,
} from "@/lib/billing"
import {
  cancelCreemSubscriptionsBestEffort,
  findActiveSubscriptionsForTeams,
} from "@/lib/billing/creem-subscriptions"

function slugify(input: string): string {
  return input
    .toLowerCase()
    .normalize(`NFKD`)
    .replace(/[̀-ͯ]/g, ``)
    .replace(/[^a-z0-9]+/g, `-`)
    .replace(/^-+|-+$/g, ``)
    .slice(0, 48)
}

type DbOrTx = {
  // eslint-disable-next-line quotes -- esbuild rejects template literals inside typeof import()
  select: typeof import("@/db/connection").db.select
}

// Oldest non-feedback membership — the user's "default" team. The
// bootstrap feedback team never counts: INITIAL_ADMIN accounts get owner
// membership there on promotion, which must not read as "has a team".
async function findNonFeedbackMembership(db: DbOrTx, userId: string) {
  const feedbackTeamId = await getFeedbackTeamId()
  const [membership] = await db
    .select({ teamId: teamMembers.teamId })
    .from(teamMembers)
    .where(
      and(
        eq(teamMembers.userId, userId),
        feedbackTeamId
          ? ne(teamMembers.teamId, feedbackTeamId)
          : undefined
      )
    )
    .orderBy(asc(teamMembers.createdAt))
    .limit(1)
  return membership
}

async function uniqueSlug(tx: DbOrTx, base: string): Promise<string> {
  const root = slugify(base) || `team`
  let candidate = root
  let suffix = 0
  while (suffix < 5) {
    const [existing] = await tx
      .select({ id: teams.id })
      .from(teams)
      .where(eq(teams.slug, candidate))
      .limit(1)
    if (!existing) return candidate
    suffix += 1
    candidate = `${root}-${suffix}`
  }
  return `${root}-${randomBytes(3).toString(`hex`)}`
}

export const teamsRouter = router({
  // The user's default landing team (EXP-188): oldest non-feedback
  // membership, or null when the user has none — signup no longer
  // auto-creates a team, so clients route null to the onboarding
  // create-or-join choice. Never creates anything.
  getDefault: authedProcedure.query(async ({ ctx }) => {
    const membership = await findNonFeedbackMembership(
      ctx.db,
      ctx.session.user.id
    )
    if (!membership) return { team: null }

    const [team] = await ctx.db
      .select()
      .from(teams)
      .where(eq(teams.id, membership.teamId))
      .limit(1)
    return { team: team ?? null }
  }),

  // Open to every user (EXP-188) — the creator becomes owner. The only gate
  // is the invisible free-tier owned-team abuse cap (lib/billing.ts).
  create: authedProcedure
    .input(
      z.object({
        name: z.string().min(1).max(255),
        iconUrl: z.string().url().max(2048).optional(),
      })
    )
    .mutation(async ({ ctx, input }) => {
      const userId = ctx.session.user.id
      await assertCanCreateTeam(userId)

      return await ctx.db.transaction(async (tx) => {
        const slug = await uniqueSlug(tx, input.name)

        const txId = await generateTxId(tx)
        const [team] = await tx
          .insert(teams)
          .values({
            name: input.name,
            slug,
            iconUrl: input.iconUrl,
          })
          .returning()

        await tx.insert(teamMembers).values({
          teamId: team.id,
          userId,
          role: `owner`,
        })

        return { team, txId }
      })
    }),

  // Teams are always private — there are no visibility flags here.
  // `helpdeskEnabled` is the team-level helpdesk switch (owner-only like
  // every field on this procedure; ENABLING is plan-gated, disabling is
  // always allowed).
  update: authedProcedure
    .input(
      z.object({
        id: z.string().uuid(),
        name: z.string().min(1).max(255).optional(),
        iconUrl: z.string().url().max(2048).nullable().optional(),
        helpdeskEnabled: z.boolean().optional(),
      })
    )
    .mutation(async ({ ctx, input }) => {
      const { id, ...updates } = input
      await assertTeamOwner(ctx.session.user.id, id)

      if (updates.helpdeskEnabled === true) {
        await assertCanUseHelpdesk(id)
      }

      return await ctx.db.transaction(async (tx) => {
        const txId = await generateTxId(tx)
        const [team] = await tx
          .update(teams)
          .set({ ...updates, updatedAt: new Date() })
          .where(eq(teams.id, id))
          .returning()
        return { team, txId }
      })
    }),

  delete: authedProcedure
    .input(z.object({ teamId: z.string().uuid() }))
    .mutation(async ({ ctx, input }) => {
      await assertTeamOwner(ctx.session.user.id, input.teamId)

      // Block deletion of the bootstrap feedback team — the cloud boot
      // would recreate it EMPTY, silently losing every feedback issue.
      if (input.teamId === (await getFeedbackTeamId())) {
        throw new TRPCError({
          code: `BAD_REQUEST`,
          message: `Cannot delete the feedback team`,
        })
      }

      // Capture BEFORE the delete: creem_subscriptions.team_id goes
      // `set null` when the team row is deleted, after which the remote
      // Creem subscription would keep charging with nothing left to find it
      // by (the paying-ghost bug).
      const doomedSubscriptions = await findActiveSubscriptionsForTeams([
        input.teamId,
      ])

      // Collected inside the tx BEFORE the cascade drops the attachment rows;
      // the cascade never touches S3, so without this the blobs orphan.
      let storageKeys: string[] = []
      // No last-team guard (EXP-188): deleting your only team is allowed —
      // nothing self-heals a replacement anymore, clients route the
      // team-less state back into onboarding.
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

  // Member-only read of minimal team metadata by slug (used by the web
  // route guard). Returns NOT_FOUND for non-members and anonymous callers
  // alike, to avoid leaking existence.
  getBySlug: publicProcedure
    .input(z.object({ slug: z.string().min(1).max(255) }))
    .query(async ({ ctx, input }) => {
      const [team] = await ctx.db
        .select({
          id: teams.id,
          name: teams.name,
          slug: teams.slug,
          iconUrl: teams.iconUrl,
        })
        .from(teams)
        .where(eq(teams.slug, input.slug))
        .limit(1)

      if (!team) {
        throw new TRPCError({ code: `NOT_FOUND` })
      }

      const userId = ctx.session?.user?.id
      const member = userId
        ? await getTeamMember(userId, team.id)
        : undefined

      if (!member) {
        throw new TRPCError({ code: `NOT_FOUND` })
      }
      return { ...team, membership: member.role }
    }),
})

import { z } from "zod"
import { and, eq } from "drizzle-orm"
import { router, authedProcedure, generateTxId } from "@/lib/trpc"
import { labels } from "@/db/schema"
import { resolveTeamAccess } from "@/lib/team-membership"

const labelNameSchema = z.string().min(1).max(255)
const labelColorSchema = z.string().regex(/^#[0-9a-fA-F]{6}$/)

export const labelsRouter = router({
  create: authedProcedure
    .input(
      z.object({
        teamId: z.string().uuid(),
        name: labelNameSchema,
        color: labelColorSchema.default(`#6366f1`),
      })
    )
    .mutation(async ({ ctx, input }) => {
      await resolveTeamAccess(
        ctx.session.user.id,
        input.teamId,
        `mutate_resources`
      )
      return await ctx.db.transaction(async (tx) => {
        const txId = await generateTxId(tx)
        const [label] = await tx
          .insert(labels)
          .values({
            teamId: input.teamId,
            name: input.name,
            color: input.color,
          })
          .returning()

        return { txId, label }
      })
    }),

  update: authedProcedure
    .input(
      z.object({
        teamId: z.string().uuid(),
        labelId: z.string().uuid(),
        name: labelNameSchema.optional(),
        color: labelColorSchema.optional(),
      })
    )
    .mutation(async ({ ctx, input }) => {
      await resolveTeamAccess(
        ctx.session.user.id,
        input.teamId,
        `mutate_resources`
      )
      return await ctx.db.transaction(async (tx) => {
        const txId = await generateTxId(tx)
        const updates: { name?: string; color?: string } = {}
        if (input.name !== undefined) updates.name = input.name
        if (input.color !== undefined) updates.color = input.color

        if (Object.keys(updates).length > 0) {
          await tx
            .update(labels)
            .set(updates)
            .where(
              and(
                eq(labels.id, input.labelId),
                eq(labels.teamId, input.teamId)
              )
            )
        }

        return { txId }
      })
    }),

  delete: authedProcedure
    .input(
      z.object({
        teamId: z.string().uuid(),
        labelId: z.string().uuid(),
      })
    )
    .mutation(async ({ ctx, input }) => {
      await resolveTeamAccess(
        ctx.session.user.id,
        input.teamId,
        `mutate_resources`
      )
      return await ctx.db.transaction(async (tx) => {
        const txId = await generateTxId(tx)
        await tx
          .delete(labels)
          .where(
            and(
              eq(labels.id, input.labelId),
              eq(labels.teamId, input.teamId)
            )
          )

        return { txId }
      })
    }),
})

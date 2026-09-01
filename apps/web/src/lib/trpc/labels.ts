import { z } from "zod"
import { and, eq, ne, sql } from "drizzle-orm"
import { TRPCError } from "@trpc/server"
import { DEFAULT_ACCENT_COLOR, hexColorSchema } from "@exp/db-schema/domain"
import { router, authedProcedure, generateTxId } from "@/lib/trpc"
import { labels } from "@/db/schema"
import { resolveTeamAccess } from "@/lib/team-membership"
import { isUniqueViolation } from "@/lib/trpc/db-errors"

const labelNameSchema = z.string().min(1).max(255)
const labelColorSchema = hexColorSchema

function duplicateNameError(name: string): TRPCError {
  return new TRPCError({
    code: `CONFLICT`,
    message: `A label named "${name}" already exists in this team`,
  })
}

export const labelsRouter = router({
  create: authedProcedure
    .input(
      z.object({
        teamId: z.string().uuid(),
        name: labelNameSchema,
        color: labelColorSchema.default(DEFAULT_ACCENT_COLOR),
      })
    )
    .mutation(async ({ ctx, input }) => {
      await resolveTeamAccess(
        ctx.session.user.id,
        input.teamId,
        `mutate_resources`
      )
      try {
        return await ctx.db.transaction(async (tx) => {
          const txId = await generateTxId(tx)
          // Pre-check against the (team_id, lower(name)) unique so the caller
          // gets a readable CONFLICT instead of a raw 23505 (EXP-254).
          const [clash] = await tx
            .select({ id: labels.id })
            .from(labels)
            .where(
              and(
                eq(labels.teamId, input.teamId),
                sql`lower(${labels.name}) = lower(${input.name})`
              )
            )
            .limit(1)
          if (clash) throw duplicateNameError(input.name)

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
      } catch (err) {
        // The pre-check races concurrent creators — translate a late unique
        // violation into the same CONFLICT.
        if (isUniqueViolation(err)) throw duplicateNameError(input.name)
        throw err
      }
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
      try {
        return await ctx.db.transaction(async (tx) => {
          const txId = await generateTxId(tx)
          const updates: { name?: string; color?: string } = {}
          if (input.name !== undefined) updates.name = input.name
          if (input.color !== undefined) updates.color = input.color

          if (updates.name !== undefined) {
            const [clash] = await tx
              .select({ id: labels.id })
              .from(labels)
              .where(
                and(
                  eq(labels.teamId, input.teamId),
                  ne(labels.id, input.labelId),
                  sql`lower(${labels.name}) = lower(${updates.name})`
                )
              )
              .limit(1)
            if (clash) throw duplicateNameError(updates.name)
          }

          // EXP-707: echo the row (envelope rule: mutations return
          // { row, txId }) so callers can read back what they wrote.
          let label: typeof labels.$inferSelect | undefined
          if (Object.keys(updates).length > 0) {
            ;[label] = await tx
              .update(labels)
              .set(updates)
              .where(
                and(
                  eq(labels.id, input.labelId),
                  eq(labels.teamId, input.teamId)
                )
              )
              .returning()
          } else {
            ;[label] = await tx
              .select()
              .from(labels)
              .where(
                and(
                  eq(labels.id, input.labelId),
                  eq(labels.teamId, input.teamId)
                )
              )
              .limit(1)
          }
          if (!label) {
            throw new TRPCError({
              code: `NOT_FOUND`,
              message: `Label not found`,
            })
          }

          return { txId, label }
        })
      } catch (err) {
        if (isUniqueViolation(err) && input.name !== undefined) {
          throw duplicateNameError(input.name)
        }
        throw err
      }
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
            and(eq(labels.id, input.labelId), eq(labels.teamId, input.teamId))
          )

        return { txId }
      })
    }),
})

import { z } from "zod"
import { and, eq, ne, sql } from "drizzle-orm"
import { TRPCError } from "@trpc/server"
import { router, authedProcedure, generateTxId } from "@/lib/trpc"
import { labels } from "@/db/schema"
import { resolveTeamAccess } from "@/lib/team-membership"

const labelNameSchema = z.string().min(1).max(255)
const labelColorSchema = z.string().regex(/^#[0-9a-fA-F]{6}$/)

function duplicateNameError(name: string): TRPCError {
  return new TRPCError({
    code: `CONFLICT`,
    message: `A label named "${name}" already exists in this team`,
  })
}

// Postgres unique_violation (23505), as surfaced by postgres-js directly or
// wrapped in an error cause by drizzle.
function isUniqueViolation(err: unknown): boolean {
  if (!err || typeof err !== `object`) return false
  const candidate = err as { code?: unknown; cause?: unknown }
  if (candidate.code === `23505`) return true
  return isUniqueViolation(candidate.cause)
}

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

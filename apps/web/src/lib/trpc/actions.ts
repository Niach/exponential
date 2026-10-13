import { z } from "zod"
import { TRPCError } from "@trpc/server"
import { and, asc, desc, eq, ne } from "drizzle-orm"
import {
  actionIconSchema,
  actionInputsSchema,
  actionPromptPlaceholderSchema,
} from "@exp/db-schema/domain"
import { router, authedProcedure, generateTxId } from "@/lib/trpc"
import { actions } from "@/db/schema"
import { assertTeamMember, assertTeamOwner } from "@/lib/team-membership"
import { isUniqueViolation } from "@/lib/trpc/db-errors"

// Team actions = SAVED PROMPTS (EXP-253 → EXP-1291): a name, an icon, a
// markdown body and typed pick inputs. Running one is `sessions.send({
// actionId, inputs})`; no triggers (they live on team agents), no repository
// and no MCP list (the composer picks both per start). Listing is
// Electric-synced (the `actions` shape — body EXCLUDED by the server-pinned
// allowlist); `get` is the ONLY body path. Reads are member-gated, writes
// team-owner-only.

export const MAX_ACTION_NAME = 255
// The body is a markdown prompt; 64KB is far above any sane prompt and far
// below anything that could hurt Postgres (api/contact.ts precedent).
export const MAX_ACTION_BODY = 64 * 1024

// The pinned wire shape (the shape's allowlist, camelCased).
const wireColumns = {
  id: actions.id,
  teamId: actions.teamId,
  name: actions.name,
  icon: actions.icon,
  inputs: actions.inputs,
  promptPlaceholder: actions.promptPlaceholder,
  sortOrder: actions.sortOrder,
  createdAt: actions.createdAt,
  updatedAt: actions.updatedAt,
}

// `get` and the write echoes carry the server-only body too.
const detailColumns = { ...wireColumns, body: actions.body }

// NUL bytes are valid JSON/zod strings but unstorable in Postgres text
// (22P05) — reject them up front so a crafted input 400s instead of 500ing.
const noNul = { message: `Must not contain NUL bytes` }
const hasNoNul = (value: string) => !value.includes(`\u0000`)

const nameSchema = z
  .string()
  .trim()
  .min(1)
  .max(MAX_ACTION_NAME)
  .refine(hasNoNul, noNul)

// Deliberately NOT trimmed — markdown leading/trailing whitespace is content;
// only fully blank bodies are rejected.
const bodySchema = z
  .string()
  .max(MAX_ACTION_BODY)
  .refine(hasNoNul, noNul)
  .refine((value) => value.trim().length > 0, {
    message: `Body must not be empty`,
  })

export const actionCreateInputSchema = z.object({
  teamId: z.string().uuid(),
  name: nameSchema,
  icon: actionIconSchema.nullable().optional(),
  body: bodySchema,
  inputs: actionInputsSchema.optional(),
  promptPlaceholder: actionPromptPlaceholderSchema.nullable().optional(),
})

export const actionUpdateInputSchema = z.object({
  id: z.string().uuid(),
  name: nameSchema.optional(),
  icon: actionIconSchema.nullable().optional(),
  body: bodySchema.optional(),
  inputs: actionInputsSchema.optional(),
  promptPlaceholder: actionPromptPlaceholderSchema.nullable().optional(),
  sortOrder: z.number().finite().optional(),
})

async function loadAction(id: string) {
  const { db } = await import(`@/db/connection`)
  const [action] = await db
    .select(detailColumns)
    .from(actions)
    .where(eq(actions.id, id))
    .limit(1)
  if (!action) {
    throw new TRPCError({ code: `NOT_FOUND`, message: `Action not found` })
  }
  return action
}

function duplicateNameError(name: string): TRPCError {
  return new TRPCError({
    code: `CONFLICT`,
    message: `An action named "${name}" already exists in this team`,
  })
}

export const actionsRouter = router({
  // Any team member — clients list these to build the Actions surface.
  list: authedProcedure
    .input(z.object({ teamId: z.string().uuid() }))
    .query(async ({ ctx, input }) => {
      await assertTeamMember(ctx.session.user.id, input.teamId)
      const rows = await ctx.db
        .select(wireColumns)
        .from(actions)
        .where(eq(actions.teamId, input.teamId))
        .orderBy(asc(actions.sortOrder), asc(actions.name))
      return { actions: rows }
    }),

  // Member-gated single fetch — the ONLY body path: synced rows exclude it.
  get: authedProcedure
    .input(z.object({ id: z.string().uuid() }))
    .query(async ({ ctx, input }) => {
      const action = await loadAction(input.id)
      await assertTeamMember(ctx.session.user.id, action.teamId)
      return { action }
    }),

  create: authedProcedure
    .input(actionCreateInputSchema)
    .mutation(async ({ ctx, input }) => {
      await assertTeamOwner(ctx.session.user.id, input.teamId)
      // EXP-707: actions are Electric-synced, so every write returns a txId
      // sync barrier like the other synced-table routers.
      return await ctx.db.transaction(async (tx) => {
        const txId = await generateTxId(tx)
        // Append to the end of the list by default.
        const [last] = await tx
          .select({ sortOrder: actions.sortOrder })
          .from(actions)
          .where(eq(actions.teamId, input.teamId))
          .orderBy(desc(actions.sortOrder))
          .limit(1)
        const [action] = await tx
          .insert(actions)
          .values({
            teamId: input.teamId,
            name: input.name,
            icon: input.icon ?? null,
            body: input.body,
            inputs: input.inputs ?? [],
            promptPlaceholder: input.promptPlaceholder || null,
            sortOrder: (last?.sortOrder ?? 0) + 1,
          })
          .onConflictDoNothing({ target: [actions.teamId, actions.name] })
          .returning(detailColumns)
        if (!action) throw duplicateNameError(input.name)
        return { action, txId }
      })
    }),

  update: authedProcedure
    .input(actionUpdateInputSchema)
    .mutation(async ({ ctx, input }) => {
      const existing = await loadAction(input.id)
      await assertTeamOwner(ctx.session.user.id, existing.teamId)

      // Pre-check renames against the (teamId, name) unique so the caller
      // gets a readable CONFLICT instead of a raw 23505.
      if (input.name !== undefined && input.name !== existing.name) {
        const [clash] = await ctx.db
          .select({ id: actions.id })
          .from(actions)
          .where(
            and(
              eq(actions.teamId, existing.teamId),
              eq(actions.name, input.name),
              ne(actions.id, input.id)
            )
          )
          .limit(1)
        if (clash) throw duplicateNameError(input.name)
      }

      const updates: Partial<typeof actions.$inferInsert> = {}
      if (input.name !== undefined) updates.name = input.name
      if (input.icon !== undefined) updates.icon = input.icon
      if (input.body !== undefined) updates.body = input.body
      // Whole-array replace — inputs are small and orderful, no patching.
      if (input.inputs !== undefined) updates.inputs = input.inputs
      if (input.promptPlaceholder !== undefined) {
        updates.promptPlaceholder = input.promptPlaceholder || null
      }
      if (input.sortOrder !== undefined) updates.sortOrder = input.sortOrder

      // Nothing to change — the current row (drizzle rejects an empty set()).
      if (Object.keys(updates).length === 0) return { action: existing }

      try {
        return await ctx.db.transaction(async (tx) => {
          const txId = await generateTxId(tx)
          const [action] = await tx
            .update(actions)
            .set(updates)
            .where(eq(actions.id, input.id))
            .returning(detailColumns)
          if (!action) {
            throw new TRPCError({
              code: `NOT_FOUND`,
              message: `Action not found`,
            })
          }
          return { action, txId }
        })
      } catch (err) {
        // The rename pre-check above races concurrent writers — translate a
        // late (teamId, name) unique violation into the same CONFLICT.
        if (isUniqueViolation(err)) {
          throw duplicateNameError(input.name ?? existing.name)
        }
        throw err
      }
    }),

  // Threads that ran it keep their `action_name` snapshot (FK SET NULL).
  delete: authedProcedure
    .input(z.object({ id: z.string().uuid() }))
    .mutation(async ({ ctx, input }) => {
      const existing = await loadAction(input.id)
      await assertTeamOwner(ctx.session.user.id, existing.teamId)
      return await ctx.db.transaction(async (tx) => {
        const txId = await generateTxId(tx)
        await tx.delete(actions).where(eq(actions.id, input.id))
        return { ok: true as const, id: input.id, txId }
      })
    }),
})

import { z } from "zod"
import { TRPCError } from "@trpc/server"
import { and, asc, desc, eq, ne } from "drizzle-orm"
import { boardIconSchema } from "@exp/db-schema/domain"
import { router, authedProcedure, generateTxId } from "@/lib/trpc"
import { prompts } from "@/db/schema"
import { assertTeamMember, assertTeamOwner } from "@/lib/team-membership"
import { isUniqueViolation } from "@/lib/trpc/db-errors"

// NEW-2: team PROMPTS (actions became these): a name, a curated icon and a
// markdown body, nothing else. Running one is `sessions.send({promptId,
// text})`: a root whose first message is the body plus what was typed.
// Listing is Electric-synced (the `prompts` shape, body EXCLUDED by the
// server-pinned allowlist); `get` is the ONLY body path. Reads are
// member-gated, writes team-owner-only.

export const MAX_PROMPT_NAME = 255
// The body is a markdown prompt; 64KB is far above any sane prompt and far
// below anything that could hurt Postgres (api/contact.ts precedent).
export const MAX_PROMPT_BODY = 64 * 1024

// The pinned wire shape (the shape's allowlist, camelCased).
const wireColumns = {
  id: prompts.id,
  teamId: prompts.teamId,
  name: prompts.name,
  icon: prompts.icon,
  sortOrder: prompts.sortOrder,
  createdAt: prompts.createdAt,
  updatedAt: prompts.updatedAt,
}

// `get` and the write echoes carry the server-only body too.
const detailColumns = { ...wireColumns, body: prompts.body }

// NUL bytes are valid JSON/zod strings but unstorable in Postgres text
// (22P05) — reject them up front so a crafted input 400s instead of 500ing.
const noNul = { message: `Must not contain NUL bytes` }
const hasNoNul = (value: string) => !value.includes(`\u0000`)

const nameSchema = z.string().trim().min(1).max(MAX_PROMPT_NAME).refine(hasNoNul, noNul)

// Deliberately NOT trimmed — markdown leading/trailing whitespace is content;
// only fully blank bodies are rejected.
const bodySchema = z
  .string()
  .max(MAX_PROMPT_BODY)
  .refine(hasNoNul, noNul)
  .refine((value) => value.trim().length > 0, { message: `Body must not be empty` })

// Strict: the retired action fields (inputs, promptPlaceholder) are refused,
// never silently dropped.
export const promptCreateInputSchema = z.strictObject({
  teamId: z.string().uuid(),
  name: nameSchema,
  icon: boardIconSchema.nullable().optional(),
  body: bodySchema,
})

export const promptUpdateInputSchema = z.strictObject({
  id: z.string().uuid(),
  name: nameSchema.optional(),
  icon: boardIconSchema.nullable().optional(),
  body: bodySchema.optional(),
  sortOrder: z.number().finite().optional(),
})

async function loadPrompt(id: string) {
  const { db } = await import(`@/db/connection`)
  const [prompt] = await db.select(detailColumns).from(prompts).where(eq(prompts.id, id)).limit(1)
  if (!prompt) throw new TRPCError({ code: `NOT_FOUND`, message: `Prompt not found` })
  return prompt
}

function duplicateNameError(name: string): TRPCError {
  return new TRPCError({
    code: `CONFLICT`,
    message: `A prompt named "${name}" already exists in this team`,
  })
}

export const promptsRouter = router({
  // Any team member (MCP `prompts_list`; clients read the synced shape).
  list: authedProcedure
    .input(z.object({ teamId: z.string().uuid() }))
    .query(async ({ ctx, input }) => {
      await assertTeamMember(ctx.session.user.id, input.teamId)
      const rows = await ctx.db
        .select(wireColumns)
        .from(prompts)
        .where(eq(prompts.teamId, input.teamId))
        .orderBy(asc(prompts.sortOrder), asc(prompts.name))
      return { prompts: rows }
    }),

  // Member-gated single fetch — the ONLY body path: synced rows exclude it.
  get: authedProcedure
    .input(z.object({ id: z.string().uuid() }))
    .query(async ({ ctx, input }) => {
      const prompt = await loadPrompt(input.id)
      await assertTeamMember(ctx.session.user.id, prompt.teamId)
      return { prompt }
    }),

  create: authedProcedure
    .input(promptCreateInputSchema)
    .mutation(async ({ ctx, input }) => {
      await assertTeamOwner(ctx.session.user.id, input.teamId)
      // Electric-synced: every write returns a txId sync barrier.
      return await ctx.db.transaction(async (tx) => {
        const txId = await generateTxId(tx)
        const [last] = await tx
          .select({ sortOrder: prompts.sortOrder })
          .from(prompts)
          .where(eq(prompts.teamId, input.teamId))
          .orderBy(desc(prompts.sortOrder))
          .limit(1)
        const [prompt] = await tx
          .insert(prompts)
          .values({
            teamId: input.teamId,
            name: input.name,
            icon: input.icon ?? null,
            body: input.body,
            sortOrder: (last?.sortOrder ?? 0) + 1,
          })
          .onConflictDoNothing({ target: [prompts.teamId, prompts.name] })
          .returning(detailColumns)
        if (!prompt) throw duplicateNameError(input.name)
        return { prompt, txId }
      })
    }),

  update: authedProcedure
    .input(promptUpdateInputSchema)
    .mutation(async ({ ctx, input }) => {
      const existing = await loadPrompt(input.id)
      await assertTeamOwner(ctx.session.user.id, existing.teamId)
      if (input.name !== undefined && input.name !== existing.name) {
        const [clash] = await ctx.db
          .select({ id: prompts.id })
          .from(prompts)
          .where(and(eq(prompts.teamId, existing.teamId), eq(prompts.name, input.name), ne(prompts.id, input.id)))
          .limit(1)
        if (clash) throw duplicateNameError(input.name)
      }
      const updates: Partial<typeof prompts.$inferInsert> = {}
      if (input.name !== undefined) updates.name = input.name
      if (input.icon !== undefined) updates.icon = input.icon
      if (input.body !== undefined) updates.body = input.body
      if (input.sortOrder !== undefined) updates.sortOrder = input.sortOrder
      if (Object.keys(updates).length === 0) return { prompt: existing }
      try {
        return await ctx.db.transaction(async (tx) => {
          const txId = await generateTxId(tx)
          const [prompt] = await tx
            .update(prompts)
            .set(updates)
            .where(eq(prompts.id, input.id))
            .returning(detailColumns)
          if (!prompt) throw new TRPCError({ code: `NOT_FOUND`, message: `Prompt not found` })
          return { prompt, txId }
        })
      } catch (err) {
        if (isUniqueViolation(err)) throw duplicateNameError(input.name ?? existing.name)
        throw err
      }
    }),

  delete: authedProcedure
    .input(z.object({ id: z.string().uuid() }))
    .mutation(async ({ ctx, input }) => {
      const existing = await loadPrompt(input.id)
      await assertTeamOwner(ctx.session.user.id, existing.teamId)
      return await ctx.db.transaction(async (tx) => {
        const txId = await generateTxId(tx)
        await tx.delete(prompts).where(eq(prompts.id, input.id))
        return { ok: true as const, id: input.id, txId }
      })
    }),
})

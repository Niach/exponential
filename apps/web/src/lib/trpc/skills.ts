import { z } from "zod"
import { TRPCError } from "@trpc/server"
import { and, asc, eq, inArray, ne } from "drizzle-orm"
import {
  SKILL_BODY_MAX,
  SKILL_DESCRIPTION_MAX,
  SKILL_NAME_MAX,
  SKILL_NAME_PATTERN,
} from "@exp/db-schema/domain"
import { router, authedProcedure, generateTxId } from "@/lib/trpc"
import { sessions, skills } from "@/db/schema"
import { assertTeamMember, assertTeamOwner } from "@/lib/team-membership"
import { isUniqueViolation } from "@/lib/trpc/db-errors"
import { parseMcpSessionHeader } from "@/lib/emcp/session-header"
import { isSessionPrincipal } from "@/lib/sessions/host"
import { builtinSkill, isBuiltinName, sessionSkillIds } from "@/lib/skills/builtin"

// #EXP-1656: team SKILLS (fixtures/skills.json): a name, the description
// that triggers it and a SKILL.md body; no files, no history. Listing is
// Electric-synced (the `skills` shape, body EXCLUDED by the server-pinned
// allowlist); `get` is the member body read and `resolveForLaunch` the
// running device's. Reads are member-gated, writes team-owner-only (like
// prompts).

// The pinned wire shape (the shape's allowlist, camelCased).
const wireColumns = {
  id: skills.id,
  teamId: skills.teamId,
  name: skills.name,
  description: skills.description,
  enabledByDefault: skills.enabledByDefault,
  builtin: skills.builtin,
  createdById: skills.createdById,
  createdAt: skills.createdAt,
  updatedAt: skills.updatedAt,
}

// `get` and the write echoes carry the server-only body too.
const detailColumns = { ...wireColumns, body: skills.body }

// NUL bytes are valid JSON/zod strings but unstorable in Postgres (22P05).
const noNul = { message: `Must not contain NUL bytes` }
const hasNoNul = (value: string) => !value.includes(`\u0000`)

const nameSchema = z
  .string()
  .trim()
  .min(1)
  .max(SKILL_NAME_MAX)
  .regex(SKILL_NAME_PATTERN, `Use lowercase letters, numbers and hyphens`)

const descriptionSchema = z.string().trim().min(1).max(SKILL_DESCRIPTION_MAX).refine(hasNoNul, noNul)

// NOT trimmed: markdown whitespace is content; the cap is UTF-8 BYTES.
const bodySchema = z
  .string()
  .max(SKILL_BODY_MAX)
  .refine(hasNoNul, noNul)
  .refine((value) => value.trim().length > 0, { message: `Body must not be empty` })
  .refine((value) => Buffer.byteLength(value, `utf8`) <= SKILL_BODY_MAX, {
    message: `Body is too long`,
  })

export const skillCreateInputSchema = z.strictObject({
  teamId: z.string().uuid(),
  name: nameSchema,
  description: descriptionSchema,
  body: bodySchema,
  enabledByDefault: z.boolean().optional(),
})

export const skillUpdateInputSchema = z.strictObject({
  id: z.string().uuid(),
  name: nameSchema.optional(),
  description: descriptionSchema.optional(),
  body: bodySchema.optional(),
  enabledByDefault: z.boolean().optional(),
})

async function loadSkill(id: string) {
  const { db } = await import(`@/db/connection`)
  const [skill] = await db.select(detailColumns).from(skills).where(eq(skills.id, id)).limit(1)
  if (!skill) throw new TRPCError({ code: `NOT_FOUND`, message: `Skill not found` })
  return skill
}

// #VAPP-213: a built-in's body and files come from the shipped set, never
// the row (the row follows it at the next boot's reconcile).
function withShipped<T extends { name: string; builtin: boolean; body: string }>(skill: T): T & { files: string[] } {
  const shipped = skill.builtin ? builtinSkill(skill.name) : null
  if (!shipped) return { ...skill, files: [] }
  return { ...skill, body: shipped.body, files: shipped.files.map((file) => file.path) }
}

function readOnlyError(): TRPCError {
  return new TRPCError({ code: `FORBIDDEN`, message: `Built-in skills are read-only` })
}

function duplicateNameError(name: string): TRPCError {
  return new TRPCError({
    code: `CONFLICT`,
    message: `A skill named "${name}" already exists in this team`,
  })
}

/** One skill as the running device installs it: a SKILL.md and, for a
 *  built-in, the files beside it (`<name>/<path>`). */
export interface ResolvedSkill {
  id: string
  name: string
  description: string
  body: string
  files: { path: string; content: string }[]
}

export const skillsRouter = router({
  // Any team member (clients read the synced shape).
  list: authedProcedure
    .input(z.object({ teamId: z.string().uuid() }))
    .query(async ({ ctx, input }) => {
      await assertTeamMember(ctx.session.user.id, input.teamId)
      const rows = await ctx.db
        .select(wireColumns)
        .from(skills)
        .where(eq(skills.teamId, input.teamId))
        .orderBy(asc(skills.name))
      return { skills: rows }
    }),

  // Member-gated single fetch: the member body path (synced rows exclude it).
  get: authedProcedure
    .input(z.object({ id: z.string().uuid() }))
    .query(async ({ ctx, input }) => {
      const skill = await loadSkill(input.id)
      await assertTeamMember(ctx.session.user.id, skill.teamId)
      return { skill: withShipped(skill) }
    }),

  // #VAPP-213: one of a built-in's files (`get` lists their paths).
  file: authedProcedure
    .input(z.object({ id: z.string().uuid(), path: z.string().min(1).max(200) }))
    .query(async ({ ctx, input }) => {
      const skill = await loadSkill(input.id)
      await assertTeamMember(ctx.session.user.id, skill.teamId)
      const file = (skill.builtin ? builtinSkill(skill.name)?.files : undefined)?.find((f) => f.path === input.path)
      if (!file) throw new TRPCError({ code: `NOT_FOUND`, message: `No file "${input.path}" in skill ${skill.name}` })
      return { path: file.path, content: file.content }
    }),

  create: authedProcedure
    .input(skillCreateInputSchema)
    .mutation(async ({ ctx, input }) => {
      await assertTeamOwner(ctx.session.user.id, input.teamId)
      if (isBuiltinName(input.name)) throw duplicateNameError(input.name)
      return await ctx.db.transaction(async (tx) => {
        const txId = await generateTxId(tx)
        const [skill] = await tx
          .insert(skills)
          .values({
            teamId: input.teamId,
            name: input.name,
            description: input.description,
            body: input.body,
            enabledByDefault: input.enabledByDefault ?? false,
            createdById: ctx.session.user.id,
          })
          .onConflictDoNothing({ target: [skills.teamId, skills.name] })
          .returning(detailColumns)
        if (!skill) throw duplicateNameError(input.name)
        return { skill, txId }
      })
    }),

  update: authedProcedure
    .input(skillUpdateInputSchema)
    .mutation(async ({ ctx, input }) => {
      const existing = await loadSkill(input.id)
      await assertTeamOwner(ctx.session.user.id, existing.teamId)
      if (existing.builtin) throw readOnlyError()
      if (input.name !== undefined && input.name !== existing.name && isBuiltinName(input.name)) {
        throw duplicateNameError(input.name)
      }
      if (input.name !== undefined && input.name !== existing.name) {
        const [clash] = await ctx.db
          .select({ id: skills.id })
          .from(skills)
          .where(and(eq(skills.teamId, existing.teamId), eq(skills.name, input.name), ne(skills.id, input.id)))
          .limit(1)
        if (clash) throw duplicateNameError(input.name)
      }
      const updates: Partial<typeof skills.$inferInsert> = {}
      if (input.name !== undefined) updates.name = input.name
      if (input.description !== undefined) updates.description = input.description
      if (input.body !== undefined) updates.body = input.body
      if (input.enabledByDefault !== undefined) updates.enabledByDefault = input.enabledByDefault
      if (Object.keys(updates).length === 0) return { skill: existing }
      try {
        return await ctx.db.transaction(async (tx) => {
          const txId = await generateTxId(tx)
          const [skill] = await tx
            .update(skills)
            .set({ ...updates, updatedAt: new Date() })
            .where(eq(skills.id, input.id))
            .returning(detailColumns)
          if (!skill) throw new TRPCError({ code: `NOT_FOUND`, message: `Skill not found` })
          return { skill, txId }
        })
      } catch (err) {
        if (isUniqueViolation(err)) throw duplicateNameError(input.name ?? existing.name)
        throw err
      }
    }),

  // A deleted skill simply stops resolving: picks naming it are skipped at
  // the next launch (no cascade over the jsonb picks).
  delete: authedProcedure
    .input(z.object({ id: z.string().uuid() }))
    .mutation(async ({ ctx, input }) => {
      const existing = await loadSkill(input.id)
      await assertTeamOwner(ctx.session.user.id, existing.teamId)
      if (existing.builtin) throw readOnlyError()
      return await ctx.db.transaction(async (tx) => {
        const txId = await generateTxId(tx)
        await tx.delete(skills).where(eq(skills.id, input.id))
        return { ok: true as const, id: input.id, txId }
      })
    }),

  /** The running device's read: the skills its live session picked (name,
   *  description, body; pick order), named by `X-Exp-Session-Id` like
   *  `mcpServers.resolveForLaunch`. A picked skill that was deleted since is
   *  listed in `skipped`, never a launch blocker. */
  resolveForLaunch: authedProcedure
    .input(z.object({}))
    .mutation(async ({ ctx }): Promise<{ skills: ResolvedSkill[]; skipped: string[] }> => {
      const userId = ctx.session.user.id
      const sessionId = parseMcpSessionHeader(ctx.request)
      if (!sessionId) {
        throw new TRPCError({
          code: `FORBIDDEN`,
          message: `Skills resolve only for a session the Exponential launcher started (missing X-Exp-Session-Id).`,
        })
      }
      const [run] = await ctx.db
        .select({
          id: sessions.id,
          userId: sessions.userId,
          deviceId: sessions.deviceId,
          teamId: sessions.teamId,
          status: sessions.status,
          skillIds: sessions.skillIds,
          appId: sessions.appId,
        })
        .from(sessions)
        .where(eq(sessions.id, sessionId))
        .limit(1)
      if (!run || !(await isSessionPrincipal(userId, run)) || run.status === `ended`) {
        throw new TRPCError({
          code: `FORBIDDEN`,
          message: `Skills resolve only for a live session of your own.`,
        })
      }
      await assertTeamMember(userId, run.teamId)
      // #VAPP-213: an app EDITOR session also runs with the built-ins
      // (`builtin.editorDelivery`), after its own picks, deduped; the same
      // pick `sessions.skillPicks` reads back, so the device's between-turn
      // check never sees it move.
      const ids = await sessionSkillIds(ctx.db, run)
      if (ids.length === 0) return { skills: [], skipped: [] }
      const rows = await ctx.db
        .select({ id: skills.id, name: skills.name, description: skills.description, body: skills.body, builtin: skills.builtin })
        .from(skills)
        .where(and(eq(skills.teamId, run.teamId), inArray(skills.id, ids)))
      const byId = new Map(rows.map((row) => [row.id, row]))
      return {
        skills: ids.flatMap((id): ResolvedSkill[] => {
          const row = byId.get(id)
          if (!row) return []
          const shipped = row.builtin ? builtinSkill(row.name) : null
          return [
            {
              id: row.id,
              name: row.name,
              description: shipped?.description ?? row.description,
              body: shipped?.body ?? row.body,
              files: shipped?.files.map((file) => ({ path: file.path, content: file.content })) ?? [],
            },
          ]
        }),
        skipped: ids.filter((id) => !byId.has(id)),
      }
    }),
})

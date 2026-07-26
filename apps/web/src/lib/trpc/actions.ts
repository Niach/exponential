import { z } from "zod"
import { TRPCError } from "@trpc/server"
import { and, asc, desc, eq, ne } from "drizzle-orm"
import { actionIconSchema, actionInputsSchema } from "@exp/db-schema/domain"
import { router, authedProcedure } from "@/lib/trpc"
import { actions, repositories } from "@/db/schema"
import { assertTeamMember, assertTeamOwner } from "@/lib/team-membership"
import {
  BUILTIN_CREATE_ACTION_ID,
  BUILTIN_CREATE_ACTION_NAME,
  BUILTIN_FIX_CONFLICTS_ID,
  BUILTIN_FIX_CONFLICTS_NAME,
  builtinCreateAction,
  builtinFixConflictsAction,
  isBuiltinActionId,
} from "@/lib/builtin-actions"

// Team action prompts (EXP-253). Listing is Electric-synced since EXP-268
// (the `actions` shape — body EXCLUDED by the server-pinned allowlist), but
// this router stays load-bearing: `list` serves pre-shape native builds + MCP,
// `get` is the ONLY body path (editors on open, the desktop right before a
// run), and all writes are tRPC. Reads are member-gated (running is a member
// affordance); writes are team-owner-only.

export const MAX_ACTION_NAME = 255
export const MAX_ACTION_DESCRIPTION = 2048
// The body is a markdown prompt; 64KB is far above any sane prompt and far
// below anything that could hurt Postgres (api/contact.ts precedent).
export const MAX_ACTION_BODY = 64 * 1024

// The pinned wire shape — teamId included: it is the primary scope key the
// clients group and query by (unlike run-configs' denormalized copy).
const wireColumns = {
  id: actions.id,
  teamId: actions.teamId,
  repositoryId: actions.repositoryId,
  name: actions.name,
  description: actions.description,
  icon: actions.icon,
  body: actions.body,
  inputs: actions.inputs,
  sortOrder: actions.sortOrder,
  createdAt: actions.createdAt,
  updatedAt: actions.updatedAt,
}

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

const descriptionSchema = z
  .string()
  .trim()
  .max(MAX_ACTION_DESCRIPTION)
  .refine(hasNoNul, noNul)
  .nullable()

// Deliberately NOT trimmed — markdown leading/trailing whitespace is content;
// only fully blank bodies are rejected.
const bodySchema = z
  .string()
  .max(MAX_ACTION_BODY)
  .refine(hasNoNul, noNul)
  .refine((value) => value.trim().length > 0, {
    message: `Body must not be empty`,
  })

async function loadAction(id: string) {
  const { db } = await import(`@/db/connection`)
  const [action] = await db
    .select()
    .from(actions)
    .where(eq(actions.id, id))
    .limit(1)
  if (!action) {
    throw new TRPCError({ code: `NOT_FOUND`, message: `Action not found` })
  }
  return action
}

// An action's optional repo must come from the SAME team's registry — a
// cross-team id would make members clone a repo their team never connected.
async function assertRepoInTeam(repositoryId: string, teamId: string) {
  const { db } = await import(`@/db/connection`)
  const [repo] = await db
    .select({ teamId: repositories.teamId })
    .from(repositories)
    .where(eq(repositories.id, repositoryId))
    .limit(1)
  if (!repo) {
    throw new TRPCError({ code: `BAD_REQUEST`, message: `Repository not found` })
  }
  if (repo.teamId !== teamId) {
    throw new TRPCError({
      code: `BAD_REQUEST`,
      message: `Repository must belong to the team`,
    })
  }
}

function duplicateNameError(name: string): TRPCError {
  return new TRPCError({
    code: `CONFLICT`,
    message: `An action named "${name}" already exists in this team`,
  })
}

// The builtins' ids are accepted by every id field (so clients get a readable
// error instead of a zod 400) but rejected before any DB work.
const actionIdSchema = z
  .string()
  .uuid()
  .or(z.literal(BUILTIN_CREATE_ACTION_ID))
  .or(z.literal(BUILTIN_FIX_CONFLICTS_ID))

function rejectBuiltin(id: string, verb: string): void {
  if (isBuiltinActionId(id)) {
    throw new TRPCError({
      code: `BAD_REQUEST`,
      message: `Built-in actions can't be ${verb}`,
    })
  }
}

// Keep the list free of a second "Create action" / "Fix merge conflicts"
// entry — the builtins own those names on every team.
function assertNotReservedName(name: string): void {
  const normalized = name.trim().toLowerCase()
  for (const reserved of [
    BUILTIN_CREATE_ACTION_NAME,
    BUILTIN_FIX_CONFLICTS_NAME,
  ]) {
    if (normalized === reserved.toLowerCase()) {
      throw new TRPCError({
        code: `CONFLICT`,
        message: `"${reserved}" is a built-in action name`,
      })
    }
  }
}

// Postgres unique_violation (23505), as surfaced by postgres-js directly or
// wrapped in an error cause by drizzle.
function isUniqueViolation(err: unknown): boolean {
  if (!err || typeof err !== `object`) return false
  const candidate = err as { code?: unknown; cause?: unknown }
  if (candidate.code === `23505`) return true
  return isUniqueViolation(candidate.cause)
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
      // EXP-257/EXP-259: the virtual builtins ride every list — clients pin
      // them first by the `builtin` flag (explicit false on real rows so the
      // union stays uniformly narrowable).
      return {
        actions: [
          ...rows.map((row) => ({ ...row, builtin: false as const })),
          builtinCreateAction(input.teamId),
          builtinFixConflictsAction(input.teamId),
        ],
      }
    }),

  // Member-gated single fetch — the ONLY body path: synced rows exclude the
  // body, so editors fetch it on open and the desktop right before a run.
  get: authedProcedure
    .input(z.object({ id: actionIdSchema }))
    .query(async ({ ctx, input }) => {
      // The builtin has no server body — desktops compose its prompt from
      // their own shipped constants.
      rejectBuiltin(input.id, `fetched`)
      const action = await loadAction(input.id)
      await assertTeamMember(ctx.session.user.id, action.teamId)
      return { action }
    }),

  create: authedProcedure
    .input(
      z.object({
        teamId: z.string().uuid(),
        name: nameSchema,
        description: descriptionSchema.optional(),
        icon: actionIconSchema.nullable().optional(),
        repositoryId: z.string().uuid().nullable().optional(),
        body: bodySchema,
        inputs: actionInputsSchema.optional(),
      })
    )
    .mutation(async ({ ctx, input }) => {
      await assertTeamOwner(ctx.session.user.id, input.teamId)
      assertNotReservedName(input.name)
      if (input.repositoryId) {
        await assertRepoInTeam(input.repositoryId, input.teamId)
      }

      // Append to the end of the list by default.
      const [last] = await ctx.db
        .select({ sortOrder: actions.sortOrder })
        .from(actions)
        .where(eq(actions.teamId, input.teamId))
        .orderBy(desc(actions.sortOrder))
        .limit(1)
      const nextSortOrder = (last?.sortOrder ?? 0) + 1

      const [action] = await ctx.db
        .insert(actions)
        .values({
          teamId: input.teamId,
          repositoryId: input.repositoryId ?? null,
          name: input.name,
          description: input.description ?? null,
          icon: input.icon ?? null,
          body: input.body,
          inputs: input.inputs ?? [],
          sortOrder: nextSortOrder,
        })
        .onConflictDoNothing({
          target: [actions.teamId, actions.name],
        })
        .returning(wireColumns)

      if (!action) throw duplicateNameError(input.name)
      return { action }
    }),

  update: authedProcedure
    .input(
      z.object({
        id: actionIdSchema,
        name: nameSchema.optional(),
        description: descriptionSchema.optional(),
        icon: actionIconSchema.nullable().optional(),
        repositoryId: z.string().uuid().nullable().optional(),
        body: bodySchema.optional(),
        inputs: actionInputsSchema.optional(),
        sortOrder: z.number().finite().optional(),
      })
    )
    .mutation(async ({ ctx, input }) => {
      rejectBuiltin(input.id, `edited`)
      const existing = await loadAction(input.id)
      await assertTeamOwner(ctx.session.user.id, existing.teamId)
      if (input.name !== undefined) assertNotReservedName(input.name)
      if (input.repositoryId) {
        await assertRepoInTeam(input.repositoryId, existing.teamId)
      }

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
      if (input.description !== undefined) {
        updates.description = input.description
      }
      if (input.icon !== undefined) updates.icon = input.icon
      if (input.repositoryId !== undefined) {
        updates.repositoryId = input.repositoryId
      }
      if (input.body !== undefined) updates.body = input.body
      // Whole-array replace — inputs are small and orderful, no patching.
      if (input.inputs !== undefined) updates.inputs = input.inputs
      if (input.sortOrder !== undefined) updates.sortOrder = input.sortOrder

      // Nothing to change — return the current row (drizzle rejects an empty
      // .set()).
      if (Object.keys(updates).length === 0) {
        const [action] = await ctx.db
          .select(wireColumns)
          .from(actions)
          .where(eq(actions.id, input.id))
          .limit(1)
        if (!action) {
          throw new TRPCError({
            code: `NOT_FOUND`,
            message: `Action not found`,
          })
        }
        return { action }
      }

      let action
      try {
        ;[action] = await ctx.db
          .update(actions)
          .set(updates)
          .where(eq(actions.id, input.id))
          .returning(wireColumns)
      } catch (err) {
        // The rename pre-check above races concurrent writers — translate a
        // late (teamId, name) unique violation into the same CONFLICT.
        if (isUniqueViolation(err)) {
          throw duplicateNameError(input.name ?? existing.name)
        }
        throw err
      }
      if (!action) {
        throw new TRPCError({
          code: `NOT_FOUND`,
          message: `Action not found`,
        })
      }
      return { action }
    }),

  // Live coding_sessions rows survive a delete batch-shaped: action_id nulls
  // (FK SET NULL) while the action_name snapshot keeps labeling the run.
  delete: authedProcedure
    .input(z.object({ id: actionIdSchema }))
    .mutation(async ({ ctx, input }) => {
      rejectBuiltin(input.id, `deleted`)
      const existing = await loadAction(input.id)
      await assertTeamOwner(ctx.session.user.id, existing.teamId)
      await ctx.db.delete(actions).where(eq(actions.id, input.id))
      return { ok: true as const }
    }),
})

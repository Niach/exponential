import { z } from "zod"
import { TRPCError } from "@trpc/server"
import { and, asc, desc, eq, ne } from "drizzle-orm"
import { router, authedProcedure } from "@/lib/trpc"
import { runConfigs } from "@/db/schema"
import {
  assertWorkspaceMember,
  assertWorkspaceOwner,
  getProjectWorkspaceId,
} from "@/lib/workspace-membership"
import {
  MAX_ARGV_ITEMS,
  MAX_ARG_LENGTH,
  MAX_CWD_LENGTH,
  MAX_ENV_ENTRIES,
  MAX_ENV_KEY_LENGTH,
  MAX_ENV_VALUE_LENGTH,
  MAX_RUN_CONFIG_NAME,
  runConfigCwdError,
  sanitizeRunConfigEnv,
} from "@/lib/run-configs"

// Per-project terminal run commands (EXP-2). tRPC-only — NOT an Electric
// shape: the desktops fetch on demand and gate execution behind the
// per-device Trust & Run commandSetHash prompt (DB-stored argv run locally
// reverses the never-execute-synced-values invariant, so the trust prompt is
// non-negotiable and re-fires whenever the fetched config set changes).
// Reads are member-gated; writes are workspace-owner-only.

// The pinned wire shape: {id, projectId, name, argv, cwd, env, sortOrder,
// createdAt, updatedAt} — workspaceId is a server-side denormalization detail
// and stays off the wire.
const wireColumns = {
  id: runConfigs.id,
  projectId: runConfigs.projectId,
  name: runConfigs.name,
  argv: runConfigs.argv,
  cwd: runConfigs.cwd,
  env: runConfigs.env,
  sortOrder: runConfigs.sortOrder,
  createdAt: runConfigs.createdAt,
  updatedAt: runConfigs.updatedAt,
}

// NUL bytes are valid JSON/zod strings but unstorable in Postgres text/jsonb
// (22P05) — reject them up front so a crafted input 400s instead of 500ing.
const noNul = { message: `Must not contain NUL bytes` }
const hasNoNul = (value: string) => !value.includes(`\u0000`)

const nameSchema = z
  .string()
  .trim()
  .min(1)
  .max(MAX_RUN_CONFIG_NAME)
  .refine(hasNoNul, noNul)

const argvSchema = z
  .array(z.string().max(MAX_ARG_LENGTH).refine(hasNoNul, noNul))
  .min(1)
  .max(MAX_ARGV_ITEMS)
  .refine((argv) => argv[0]!.trim().length > 0, {
    message: `argv[0] must be a program name`,
  })

// Env keys follow the conventional NAME grammar; blocked keys (PATH,
// LD_PRELOAD, DYLD_*) are stripped after parsing, not rejected.
const envSchema = z
  .record(
    z
      .string()
      .min(1)
      .max(MAX_ENV_KEY_LENGTH)
      .regex(/^[A-Za-z_][A-Za-z0-9_]*$/, `Invalid environment variable name`),
    z.string().max(MAX_ENV_VALUE_LENGTH).refine(hasNoNul, noNul)
  )
  .refine((env) => Object.keys(env).length <= MAX_ENV_ENTRIES, {
    message: `Too many environment variables`,
  })

const cwdSchema = z
  .string()
  .trim()
  .max(MAX_CWD_LENGTH)
  .refine(hasNoNul, noNul)
  .nullable()

// Normalize + validate cwd: empty means repo root (stored as null); anything
// else must be a relative path with no `..` segments.
function normalizeCwd(cwd: string | null | undefined): string | null {
  if (!cwd) return null
  const error = runConfigCwdError(cwd)
  if (error) throw new TRPCError({ code: `BAD_REQUEST`, message: error })
  return cwd
}

async function loadRunConfig(id: string) {
  const { db } = await import(`@/db/connection`)
  const [config] = await db
    .select()
    .from(runConfigs)
    .where(eq(runConfigs.id, id))
    .limit(1)
  if (!config) {
    throw new TRPCError({ code: `NOT_FOUND`, message: `Run config not found` })
  }
  return config
}

function duplicateNameError(name: string): TRPCError {
  return new TRPCError({
    code: `CONFLICT`,
    message: `A run config named "${name}" already exists in this project`,
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

export const runConfigsRouter = router({
  // Any member of the project's workspace — the desktops list these to build
  // the play menu (and hash the set for the trust prompt).
  list: authedProcedure
    .input(z.object({ projectId: z.string().uuid() }))
    .query(async ({ ctx, input }) => {
      const project = await getProjectWorkspaceId(input.projectId)
      await assertWorkspaceMember(ctx.session.user.id, project.workspaceId)

      const configs = await ctx.db
        .select(wireColumns)
        .from(runConfigs)
        .where(eq(runConfigs.projectId, input.projectId))
        .orderBy(asc(runConfigs.sortOrder), asc(runConfigs.name))
      return { configs }
    }),

  create: authedProcedure
    .input(
      z.object({
        projectId: z.string().uuid(),
        name: nameSchema,
        argv: argvSchema,
        cwd: cwdSchema.optional(),
        env: envSchema.optional(),
      })
    )
    .mutation(async ({ ctx, input }) => {
      const project = await getProjectWorkspaceId(input.projectId)
      await assertWorkspaceOwner(ctx.session.user.id, project.workspaceId)

      // Append to the end of the list by default.
      const [last] = await ctx.db
        .select({ sortOrder: runConfigs.sortOrder })
        .from(runConfigs)
        .where(eq(runConfigs.projectId, input.projectId))
        .orderBy(desc(runConfigs.sortOrder))
        .limit(1)
      const nextSortOrder = (last?.sortOrder ?? 0) + 1

      const [config] = await ctx.db
        .insert(runConfigs)
        .values({
          projectId: input.projectId,
          workspaceId: project.workspaceId,
          name: input.name,
          argv: input.argv,
          cwd: normalizeCwd(input.cwd),
          env: sanitizeRunConfigEnv(input.env ?? {}),
          sortOrder: nextSortOrder,
        })
        .onConflictDoNothing({
          target: [runConfigs.projectId, runConfigs.name],
        })
        .returning(wireColumns)

      if (!config) throw duplicateNameError(input.name)
      return { config }
    }),

  update: authedProcedure
    .input(
      z.object({
        id: z.string().uuid(),
        name: nameSchema.optional(),
        argv: argvSchema.optional(),
        cwd: cwdSchema.optional(),
        env: envSchema.optional(),
        sortOrder: z.number().finite().optional(),
      })
    )
    .mutation(async ({ ctx, input }) => {
      const existing = await loadRunConfig(input.id)
      await assertWorkspaceOwner(ctx.session.user.id, existing.workspaceId)

      // Pre-check renames against the (projectId, name) unique so the caller
      // gets a readable CONFLICT instead of a raw 23505.
      if (input.name !== undefined && input.name !== existing.name) {
        const [clash] = await ctx.db
          .select({ id: runConfigs.id })
          .from(runConfigs)
          .where(
            and(
              eq(runConfigs.projectId, existing.projectId),
              eq(runConfigs.name, input.name),
              ne(runConfigs.id, input.id)
            )
          )
          .limit(1)
        if (clash) throw duplicateNameError(input.name)
      }

      const updates: Partial<typeof runConfigs.$inferInsert> = {}
      if (input.name !== undefined) updates.name = input.name
      if (input.argv !== undefined) updates.argv = input.argv
      if (input.cwd !== undefined) updates.cwd = normalizeCwd(input.cwd)
      if (input.env !== undefined) updates.env = sanitizeRunConfigEnv(input.env)
      if (input.sortOrder !== undefined) updates.sortOrder = input.sortOrder

      // Nothing to change — return the current row (drizzle rejects an empty
      // .set()).
      if (Object.keys(updates).length === 0) {
        const [config] = await ctx.db
          .select(wireColumns)
          .from(runConfigs)
          .where(eq(runConfigs.id, input.id))
          .limit(1)
        if (!config) {
          throw new TRPCError({
            code: `NOT_FOUND`,
            message: `Run config not found`,
          })
        }
        return { config }
      }

      let config
      try {
        ;[config] = await ctx.db
          .update(runConfigs)
          .set(updates)
          .where(eq(runConfigs.id, input.id))
          .returning(wireColumns)
      } catch (err) {
        // The rename pre-check above races concurrent writers — translate a
        // late (projectId, name) unique violation into the same CONFLICT.
        if (isUniqueViolation(err)) {
          throw duplicateNameError(input.name ?? existing.name)
        }
        throw err
      }
      if (!config) {
        throw new TRPCError({
          code: `NOT_FOUND`,
          message: `Run config not found`,
        })
      }
      return { config }
    }),

  delete: authedProcedure
    .input(z.object({ id: z.string().uuid() }))
    .mutation(async ({ ctx, input }) => {
      const existing = await loadRunConfig(input.id)
      await assertWorkspaceOwner(ctx.session.user.id, existing.workspaceId)
      await ctx.db.delete(runConfigs).where(eq(runConfigs.id, input.id))
      return { ok: true as const }
    }),
})

import { z } from "zod"
import { TRPCError } from "@trpc/server"
import { router, authedProcedure, generateTxId } from "@/lib/trpc"
import { boards, repositories } from "@/db/schema"
import { and, eq, isNotNull, isNull } from "drizzle-orm"
import {
  boardIconSchema,
  BOARD_TRASH_RETENTION_MS,
} from "@exp/db-schema/domain"
import {
  assertBoardMember,
  resolveTeamAccess,
  assertTeamOwner,
} from "@/lib/team-membership"
import type { db } from "@/db/connection"
import {
  assertCanManageRepos,
  connectRepositoryInTx,
} from "@/lib/trpc/repositories"

type Tx = Parameters<Parameters<(typeof db)[`transaction`]>[0]>[0]

// A repository is OPTIONAL on every board (the type collapse): coding
// features gate purely on repo presence. Create either points at an existing
// registry repo (`repositoryId`) or connects one inline (`fullName`, same
// validation as repositories.add).
const fullNameSchema = z
  .string()
  .min(1)
  .max(255)
  .regex(/^[^/\s]+\/[^/\s]+$/, `Expected "owner/name"`)

const repositoryInputSchema = z.union([
  z.object({ repositoryId: z.string().uuid() }),
  z.object({
    fullName: fullNameSchema,
    defaultBranch: z.string().min(1).max(255).optional(),
    private: z.boolean().optional(),
  }),
])

// Validate that a repo id belongs to the team and isn't archived.
async function assertRepositoryInTeam(
  tx: Tx,
  repositoryId: string,
  teamId: string
): Promise<string> {
  const [repo] = await tx
    .select({ id: repositories.id })
    .from(repositories)
    .where(
      and(
        eq(repositories.id, repositoryId),
        eq(repositories.teamId, teamId),
        isNull(repositories.archivedAt)
      )
    )
    .limit(1)
  if (!repo) {
    throw new TRPCError({
      code: `BAD_REQUEST`,
      message: `Repository not found in this team`,
    })
  }
  return repo.id
}

// Postgres unique_violation (23505), as surfaced by postgres-js directly or
// wrapped in an error cause by drizzle.
function isUniqueViolation(err: unknown): boolean {
  if (!err || typeof err !== `object`) return false
  const candidate = err as { code?: unknown; cause?: unknown }
  if (candidate.code === `23505`) return true
  return isUniqueViolation(candidate.cause)
}

function slugify(name: string): string {
  return name
    .toLowerCase()
    .trim()
    .replace(/[^\w\s-]/g, ``)
    .replace(/[\s_]+/g, `-`)
    .replace(/-+/g, `-`)
    .replace(/^-|-$/g, ``)
}

export const boardsRouter = router({
  create: authedProcedure
    .input(
      z.object({
        teamId: z.string().uuid(),
        name: z.string().min(1).max(255),
        // Identifiers are minted as `{PREFIX}-{number}` and the cross-client
        // issue-ref token contract (lib/issue-refs.ts) only matches
        // letter-led alphanumeric prefixes — reject whitespace/symbol
        // prefixes at the door so a board can never mint unreferenceable
        // identifiers (EXP-46 hardening; stored uppercased below).
        prefix: z
          .string()
          .trim()
          .regex(
            /^[A-Za-z][A-Za-z0-9]{0,9}$/,
            `Prefix must be 1-10 letters or digits, starting with a letter`
          ),
        color: z
          .string()
          .regex(/^#[0-9a-fA-F]{6}$/)
          .optional(),
        icon: boardIconSchema.nullish(),
        // Always optional (coding features gate on repo presence). Either
        // target an existing registry repo or connect one inline in the same
        // transaction (onboarding/create dialogs stay a single call).
        repository: repositoryInputSchema.optional(),
      })
    )
    .mutation(async ({ ctx, input }) => {
      await resolveTeamAccess(
        ctx.session.user.id,
        input.teamId,
        `mutate_resources`
      )
      const repositoryInput = input.repository
      const inlineConnect =
        repositoryInput != null && `fullName` in repositoryInput
      if (inlineConnect) {
        // The inline connect path needs owner/admin (repo management), beyond
        // the member-level board create. Boards & repos are unlimited on
        // every tier now (v5 per-seat model), so there is no plan cap here.
        await assertCanManageRepos(ctx.session.user.id, input.teamId)
      }
      // Symbol/emoji-only names slugify to `` — fall back to the (alphanumeric)
      // prefix, then a generic root, mirroring teams' uniqueSlug fallback,
      // so a board can never insert the unroutable slug '' (EXP-46).
      const slug =
        slugify(input.name) || slugify(input.prefix) || `board`
      let result
      try {
        result = await ctx.db.transaction(async (tx) => {
          const txId = await generateTxId(tx)

          const repositoryId = !repositoryInput
            ? null
            : `fullName` in repositoryInput
              ? await connectRepositoryInTx(tx, {
                  userId: ctx.session.user.id,
                  teamId: input.teamId,
                  fullName: repositoryInput.fullName,
                  defaultBranch: repositoryInput.defaultBranch,
                  private: repositoryInput.private,
                })
              : await assertRepositoryInTeam(
                  tx,
                  repositoryInput.repositoryId,
                  input.teamId
                )

          const [board] = await tx
            .insert(boards)
            .values({
              teamId: input.teamId,
              name: input.name,
              slug,
              prefix: input.prefix.toUpperCase(),
              color: input.color ?? `#6366f1`,
              icon: input.icon ?? null,
              repositoryId,
            })
            .returning()

          return { board, txId }
        })
      } catch (error) {
        // A trashed board keeps its (team_id, slug) reservation for the
        // whole retention window; distinguish that from a live-name clash.
        if (isUniqueViolation(error)) {
          const [trashed] = await ctx.db
            .select({ id: boards.id })
            .from(boards)
            .where(
              and(
                eq(boards.teamId, input.teamId),
                eq(boards.slug, slug),
                isNotNull(boards.deletedAt)
              )
            )
            .limit(1)
          throw new TRPCError({
            code: `BAD_REQUEST`,
            message: trashed
              ? `A board with this name is in the trash — restore it or wait for the purge`
              : `A board with this name already exists`,
          })
        }
        throw error
      }

      return result
    }),

  // Owner/admin: retarget a board's backing repo — or detach it entirely
  // (repositoryId: null) from a non-dev board. Existing worktrees for
  // old-repo issues keep working locally (they're just git); new launches use
  // the new repo.
  setRepository: authedProcedure
    .input(
      z.object({
        boardId: z.string().uuid(),
        repositoryId: z.string().uuid().nullable(),
      })
    )
    .mutation(async ({ ctx, input }) => {
      const boardRecord = await assertBoardMember(
        ctx.session.user.id,
        input.boardId
      )
      await assertCanManageRepos(
        ctx.session.user.id,
        boardRecord.teamId
      )

      // Protected boards (the dogfood board) keep their repo — mirrors the
      // delete/archive guards.
      const [current] = await ctx.db
        .select({ isProtected: boards.isProtected })
        .from(boards)
        .where(eq(boards.id, input.boardId))
        .limit(1)
      if (current?.isProtected) {
        throw new TRPCError({
          code: `BAD_REQUEST`,
          message: `This board is protected — its repository cannot be changed`,
        })
      }

      return await ctx.db.transaction(async (tx) => {
        const txId = await generateTxId(tx)
        const repositoryId =
          input.repositoryId === null
            ? null
            : await assertRepositoryInTeam(
                tx,
                input.repositoryId,
                boardRecord.teamId
              )
        const [board] = await tx
          .update(boards)
          .set({ repositoryId })
          .where(eq(boards.id, input.boardId))
          .returning()
        return { board, txId }
      })
    }),

  update: authedProcedure
    .input(
      z.object({
        id: z.string().uuid(),
        name: z.string().min(1).max(255).optional(),
        color: z
          .string()
          .regex(/^#[0-9a-fA-F]{6}$/)
          .optional(),
        icon: boardIconSchema.nullable().optional(),
        archivedAt: z
          .string()
          .datetime()
          .transform((value) => new Date(value))
          .nullable()
          .optional(),
      })
    )
    .mutation(async ({ ctx, input }) => {
      const { id, ...updates } = input

      const boardRecord = await assertBoardMember(ctx.session.user.id, id)

      // Archiving is structure-significant — team-owner-only.
      // Name/color/icon stay member-editable.
      const ownerGated = Object.hasOwn(updates, `archivedAt`)
      if (ownerGated) {
        await assertTeamOwner(
          ctx.session.user.id,
          boardRecord.teamId
        )
      }

      const [current] = await ctx.db
        .select({ isProtected: boards.isProtected })
        .from(boards)
        .where(eq(boards.id, id))
        .limit(1)

      // Protected boards (the dogfood board) can't be archived;
      // name/color/icon stay editable.
      const attemptsArchive =
        Object.hasOwn(updates, `archivedAt`) && updates.archivedAt != null
      if (attemptsArchive && current?.isProtected) {
        throw new TRPCError({
          code: `BAD_REQUEST`,
          message: `This board is protected and cannot be archived`,
        })
      }

      const [board] = await ctx.db
        .update(boards)
        .set(updates)
        .where(eq(boards.id, id))
        .returning()

      return { board }
    }),

  // Soft delete: move the board to the trash. The purge sweep hard-deletes it
  // (cascade) after BOARD_TRASH_RETENTION_HOURS; owners can restore it before
  // then. Direct select (not the trash-filtered helper) so an already-trashed
  // board stays resolvable for the idempotent no-op.
  delete: authedProcedure
    .input(z.object({ boardId: z.string().uuid() }))
    .mutation(async ({ ctx, input }) => {
      const [board] = await ctx.db
        .select({
          teamId: boards.teamId,
          deletedAt: boards.deletedAt,
          isProtected: boards.isProtected,
        })
        .from(boards)
        .where(eq(boards.id, input.boardId))
        .limit(1)

      if (!board) {
        throw new TRPCError({ code: `NOT_FOUND`, message: `Board not found` })
      }

      await assertTeamOwner(ctx.session.user.id, board.teamId)

      if (board.isProtected) {
        throw new TRPCError({
          code: `BAD_REQUEST`,
          message: `This board is protected and cannot be deleted`,
        })
      }

      // Already trashed → nothing changed, so no sync barrier needed.
      if (board.deletedAt) {
        return { ok: true as const }
      }

      const result = await ctx.db.transaction(async (tx) => {
        const txId = await generateTxId(tx)
        await tx
          .update(boards)
          .set({ deletedAt: new Date() })
          .where(
            and(eq(boards.id, input.boardId), isNull(boards.deletedAt))
          )
        return { ok: true as const, txId }
      })
      return result
    }),

  // Owner-only: pull a board back out of the trash. Direct select bypasses the
  // trash-filtered helper (which would 404 a trashed board). The slug was
  // reserved the whole time, so there is no restore-time conflict.
  restore: authedProcedure
    .input(z.object({ boardId: z.string().uuid() }))
    .mutation(async ({ ctx, input }) => {
      const [board] = await ctx.db
        .select({ teamId: boards.teamId })
        .from(boards)
        .where(eq(boards.id, input.boardId))
        .limit(1)

      if (!board) {
        throw new TRPCError({ code: `NOT_FOUND`, message: `Board not found` })
      }

      await assertTeamOwner(ctx.session.user.id, board.teamId)

      const result = await ctx.db.transaction(async (tx) => {
        const txId = await generateTxId(tx)
        const restored = await tx
          .update(boards)
          .set({ deletedAt: null })
          .where(
            and(eq(boards.id, input.boardId), isNotNull(boards.deletedAt))
          )
          .returning({ id: boards.id })
        if (restored.length === 0) {
          throw new TRPCError({
            code: `NOT_FOUND`,
            message: `Board is not in the trash`,
          })
        }
        return { ok: true as const, txId }
      })
      return result
    }),

  // Owner-only: the trashed boards for the web trash UI. Restore is owner-only
  // and the trash card lives in the owner-gated Boards section, so member
  // visibility buys nothing.
  listDeleted: authedProcedure
    .input(z.object({ teamId: z.string().uuid() }))
    .query(async ({ ctx, input }) => {
      await assertTeamOwner(ctx.session.user.id, input.teamId)
      const rows = await ctx.db
        .select({
          id: boards.id,
          name: boards.name,
          slug: boards.slug,
          prefix: boards.prefix,
          color: boards.color,
          icon: boards.icon,
          repositoryId: boards.repositoryId,
          deletedAt: boards.deletedAt,
        })
        .from(boards)
        .where(
          and(
            eq(boards.teamId, input.teamId),
            isNotNull(boards.deletedAt)
          )
        )
      return rows.map((row) => ({
        ...row,
        purgeAt: row.deletedAt
          ? new Date(row.deletedAt.getTime() + BOARD_TRASH_RETENTION_MS)
          : null,
      }))
    }),
})

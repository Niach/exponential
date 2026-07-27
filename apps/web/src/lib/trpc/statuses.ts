// EXP-314 — per-team custom issue statuses (Linear-style). Member-managed
// like labels (`mutate_resources`, no owner gate — the user decision for this
// feature); the 7 builtin rows (builtin_key != NULL) are locked: never
// renamed, recolored, or deleted, only moved within their category. Category
// writes serialize on a FOR UPDATE lock over the category's rows so the
// started cap (ISSUE_STATUS_STARTED_MAX) can't be raced past and concurrent
// moves keep a consistent order.
import { z } from "zod"
import { and, asc, eq, ne, sql } from "drizzle-orm"
import { TRPCError } from "@trpc/server"
import { router, authedProcedure, generateTxId } from "@/lib/trpc"
import { issues, issueStatuses } from "@/db/schema"
import { resolveTeamAccess } from "@/lib/team-membership"
import {
  CATEGORY_ANCHOR,
  ISSUE_STATUS_STARTED_MAX,
  type IssueStatusCategory,
  issueStatusCategorySchema,
} from "@/lib/domain"
import { applyStatusDerivations } from "@/lib/trpc/issues"
import { recordIssueEvent } from "@/lib/integrations/activity"

const statusNameSchema = z.string().min(1).max(255)
const statusColorSchema = z.string().regex(/^#[0-9a-fA-F]{6}$/)

function duplicateNameError(name: string): TRPCError {
  return new TRPCError({
    code: `CONFLICT`,
    message: `A status named "${name}" already exists in this team`,
  })
}

// Postgres unique_violation (23505), as surfaced by postgres-js directly or
// wrapped in an error cause by drizzle (labels.ts convention).
function isUniqueViolation(err: unknown): boolean {
  if (!err || typeof err !== `object`) return false
  const candidate = err as { code?: unknown; cause?: unknown }
  if (candidate.code === `23505`) return true
  return isUniqueViolation(candidate.cause)
}

type Tx = Parameters<
  // eslint-disable-next-line quotes
  Parameters<typeof import("@/db/connection").db.transaction>[0]
>[0]

// Serialize ALL structural status writes for a team on one advisory lock,
// taken as the FIRST statement of every create/move/delete transaction.
// Row locks alone are not enough: FOR UPDATE cannot block phantom INSERTs
// (two concurrent creates each counting 3 started rows would both pass the
// cap), and move/delete's row-then-category lock order could deadlock
// against create's category-first order. xact-scoped — released at commit.
async function lockTeamStatuses(tx: Tx, teamId: string): Promise<void> {
  await tx.execute(
    sql`SELECT pg_advisory_xact_lock(hashtext(${`issue_statuses:${teamId}`}))`
  )
}

// Lock the category's rows in display order — the per-row serialization for
// the started cap, appends, and moves (lockTeamStatuses guards the phantoms).
async function lockCategory(
  tx: Tx,
  teamId: string,
  category: IssueStatusCategory
) {
  return await tx
    .select({
      id: issueStatuses.id,
      sortOrder: issueStatuses.sortOrder,
    })
    .from(issueStatuses)
    .where(
      and(
        eq(issueStatuses.teamId, teamId),
        eq(issueStatuses.category, category)
      )
    )
    .orderBy(
      asc(issueStatuses.sortOrder),
      asc(issueStatuses.createdAt),
      asc(issueStatuses.id)
    )
    .for(`update`)
}

async function loadStatusForWrite(tx: Tx, teamId: string, statusId: string) {
  const [row] = await tx
    .select()
    .from(issueStatuses)
    .where(
      and(eq(issueStatuses.id, statusId), eq(issueStatuses.teamId, teamId))
    )
    .limit(1)
    .for(`update`)
  if (!row) {
    throw new TRPCError({ code: `NOT_FOUND`, message: `Status not found` })
  }
  return row
}

function assertNotBuiltin(row: { builtinKey: string | null }): void {
  if (row.builtinKey !== null) {
    throw new TRPCError({
      code: `BAD_REQUEST`,
      message: `Built-in statuses cannot be edited or deleted`,
    })
  }
}

export const statusesRouter = router({
  // Server-authoritative count of issues still referencing a status. The
  // FK doesn't care about board trash, so this includes issues on trashed
  // boards — which never sync, so the client's synced count can undershoot.
  // Powers the delete dialog's "N issues will move to …" copy (EXP-320).
  referencingCount: authedProcedure
    .input(
      z.object({
        teamId: z.string().uuid(),
        statusId: z.string().uuid(),
      })
    )
    .query(async ({ ctx, input }) => {
      await resolveTeamAccess(ctx.session.user.id, input.teamId, `read`)
      const [status] = await ctx.db
        .select({ id: issueStatuses.id })
        .from(issueStatuses)
        .where(
          and(
            eq(issueStatuses.id, input.statusId),
            eq(issueStatuses.teamId, input.teamId)
          )
        )
        .limit(1)
      if (!status) {
        throw new TRPCError({ code: `NOT_FOUND`, message: `Status not found` })
      }
      const [row] = await ctx.db
        .select({ count: sql<number>`count(*)::int` })
        .from(issues)
        .where(eq(issues.statusId, input.statusId))
      return { count: row?.count ?? 0 }
    }),

  create: authedProcedure
    .input(
      z.object({
        teamId: z.string().uuid(),
        category: issueStatusCategorySchema,
        name: statusNameSchema,
        color: statusColorSchema,
      })
    )
    .mutation(async ({ ctx, input }) => {
      await resolveTeamAccess(
        ctx.session.user.id,
        input.teamId,
        `mutate_resources`
      )
      if (input.category === `duplicate`) {
        // Duplicate is a fixed single-status category (no + button anywhere).
        throw new TRPCError({
          code: `BAD_REQUEST`,
          message: `The duplicate category cannot have custom statuses`,
        })
      }
      try {
        return await ctx.db.transaction(async (tx) => {
          const txId = await generateTxId(tx)
          await lockTeamStatuses(tx, input.teamId)
          const siblings = await lockCategory(tx, input.teamId, input.category)
          if (
            input.category === `started` &&
            siblings.length >= ISSUE_STATUS_STARTED_MAX
          ) {
            // The pie-clock fill tables are defined only up to 4 started
            // statuses.
            throw new TRPCError({
              code: `PRECONDITION_FAILED`,
              message: `A team can have at most ${ISSUE_STATUS_STARTED_MAX} started statuses`,
            })
          }

          // Pre-check against (team_id, lower(name)) so the caller gets a
          // readable CONFLICT instead of a raw 23505 (labels EXP-254
          // convention). Also blocks shadowing a builtin's name — intended.
          const [clash] = await tx
            .select({ id: issueStatuses.id })
            .from(issueStatuses)
            .where(
              and(
                eq(issueStatuses.teamId, input.teamId),
                sql`lower(${issueStatuses.name}) = lower(${input.name})`
              )
            )
            .limit(1)
          if (clash) throw duplicateNameError(input.name)

          const maxSort = siblings.reduce(
            (max, row) => Math.max(max, row.sortOrder),
            0
          )
          const [status] = await tx
            .insert(issueStatuses)
            .values({
              teamId: input.teamId,
              category: input.category,
              name: input.name,
              color: input.color,
              sortOrder: maxSort + 1,
              builtinKey: null,
            })
            .returning()

          return { txId, status }
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
        statusId: z.string().uuid(),
        name: statusNameSchema.optional(),
        color: statusColorSchema.optional(),
        // Deliberately NO category input — a status's category is immutable
        // (moving an issue population between lifecycle semantics silently
        // would corrupt completedAt/anchor invariants).
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
          const row = await loadStatusForWrite(tx, input.teamId, input.statusId)
          assertNotBuiltin(row)

          const updates: { name?: string; color?: string } = {}
          if (input.name !== undefined) updates.name = input.name
          if (input.color !== undefined) updates.color = input.color

          if (updates.name !== undefined) {
            const [clash] = await tx
              .select({ id: issueStatuses.id })
              .from(issueStatuses)
              .where(
                and(
                  eq(issueStatuses.teamId, input.teamId),
                  ne(issueStatuses.id, input.statusId),
                  sql`lower(${issueStatuses.name}) = lower(${updates.name})`
                )
              )
              .limit(1)
            if (clash) throw duplicateNameError(updates.name)
          }

          if (Object.keys(updates).length > 0) {
            await tx
              .update(issueStatuses)
              .set(updates)
              .where(eq(issueStatuses.id, input.statusId))
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

  // Swap a status with its neighbor within its category. Builtins ARE movable
  // (only name/color/delete are locked): reordering started statuses is how a
  // team shapes the clock fills. The whole category is re-indexed 1..n first,
  // so historic sort_order ties can never make a move a silent no-op.
  move: authedProcedure
    .input(
      z.object({
        teamId: z.string().uuid(),
        statusId: z.string().uuid(),
        direction: z.enum([`up`, `down`]),
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
        await lockTeamStatuses(tx, input.teamId)
        const row = await loadStatusForWrite(tx, input.teamId, input.statusId)
        const siblings = await lockCategory(tx, input.teamId, row.category)

        const order = siblings.map((s) => s.id)
        const index = order.indexOf(input.statusId)
        const targetIndex = input.direction === `up` ? index - 1 : index + 1
        if (targetIndex >= 0 && targetIndex < order.length) {
          ;[order[index], order[targetIndex]] = [
            order[targetIndex],
            order[index],
          ]
        }
        // Persist positions 1..n, touching only rows whose position changed
        // (an edge move is an idempotent no-op — safe for double-clicks).
        for (let position = 0; position < order.length; position++) {
          const sibling = siblings.find((s) => s.id === order[position])!
          if (sibling.sortOrder !== position + 1) {
            await tx
              .update(issueStatuses)
              .set({ sortOrder: position + 1 })
              .where(eq(issueStatuses.id, sibling.id))
          }
        }

        return { txId }
      })
    }),

  delete: authedProcedure
    .input(
      z.object({
        teamId: z.string().uuid(),
        statusId: z.string().uuid(),
        // Where this status's issues go. Required whenever any issue still
        // references the status (the count includes trashed-board issues —
        // the FK doesn't care about trash, so the client's synced count can
        // undershoot; always honor the PRECONDITION_FAILED).
        reassignToId: z.string().uuid().optional(),
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
        await lockTeamStatuses(tx, input.teamId)
        const row = await loadStatusForWrite(tx, input.teamId, input.statusId)
        assertNotBuiltin(row)

        // Lock every referencing issue (ordered like bulkUpdate for deadlock
        // hygiene) — the reassignment below derives per-issue values.
        const referencing = await tx
          .select({
            id: issues.id,
            status: issues.status,
            duplicateOfId: issues.duplicateOfId,
          })
          .from(issues)
          .where(eq(issues.statusId, input.statusId))
          .orderBy(issues.id)
          .for(`update`)

        let target: typeof issueStatuses.$inferSelect | null = null
        if (referencing.length > 0) {
          if (!input.reassignToId) {
            throw new TRPCError({
              code: `PRECONDITION_FAILED`,
              message: `${referencing.length} issues use this status — pick a replacement first`,
            })
          }
          if (input.reassignToId === input.statusId) {
            throw new TRPCError({
              code: `BAD_REQUEST`,
              message: `Cannot reassign issues to the status being deleted`,
            })
          }
          const [targetRow] = await tx
            .select()
            .from(issueStatuses)
            .where(
              and(
                eq(issueStatuses.id, input.reassignToId),
                eq(issueStatuses.teamId, input.teamId)
              )
            )
            .limit(1)
          if (!targetRow) {
            throw new TRPCError({
              code: `BAD_REQUEST`,
              message: `Replacement status not found`,
            })
          }
          if (targetRow.category === `duplicate`) {
            throw new TRPCError({
              code: `BAD_REQUEST`,
              message: `Issues cannot be reassigned to the duplicate status`,
            })
          }
          target = targetRow

          const targetAnchor =
            targetRow.builtinKey ?? CATEGORY_ANCHOR[targetRow.category]
          for (const issue of referencing) {
            const setValues: Record<string, unknown> = {
              status: targetAnchor,
              statusId: targetRow.id,
            }
            // Same completedAt/duplicate rules as any status write.
            applyStatusDerivations(setValues, issue)
            await tx
              .update(issues)
              .set(setValues)
              .where(eq(issues.id, issue.id))
            await recordIssueEvent(tx, {
              issueId: issue.id,
              teamId: input.teamId,
              actorUserId: ctx.session.user.id,
              type: `status_changed`,
              payload: {
                from: issue.status,
                to: targetAnchor,
                fromStatusId: row.id,
                toStatusId: targetRow.id,
                fromName: row.name,
                toName: targetRow.name,
              },
            })
          }
          // Deliberately NO notification fan-out: deleting a status is an
          // admin bulk action — pinging every subscriber of every reassigned
          // issue would be pure noise (mirrors bulkUpdate's fan-out cap
          // rationale, taken to zero).
        }

        await tx
          .delete(issueStatuses)
          .where(eq(issueStatuses.id, input.statusId))

        return {
          txId,
          reassigned: referencing.length,
          reassignedToId: target?.id ?? null,
        }
      })
    }),
})

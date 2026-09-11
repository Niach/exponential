import { z } from "zod"
import { and, eq, sql } from "drizzle-orm"
import { TRPCError } from "@trpc/server"
import { router, authedProcedure, generateTxId } from "@/lib/trpc"
import { actions, boards, codingSessions, issues, pins } from "@/db/schema"
import { pinKindSchema, type PinKind } from "@exp/db-schema/domain"
import { assertTeamMember } from "@/lib/team-membership"
import { boardVisible } from "@/lib/board-visibility"

// EXP-778: personal pins — the sidebar's "Pinned" group. One `toggle`
// endpoint: a pin exists or it doesn't, and every client renders the row from
// the synced `pins` shape, so the mutation only needs to flip the row and
// hand back the txId. The target must live in `teamId` (an issue on a visible
// board of the team, a coding session of the team, an action of the team) and
// the caller must be a member; ownership of the pin row itself is the
// caller's user id, never the input.

// eslint-disable-next-line quotes -- `typeof import()` requires a string literal; the backtick autofix breaks it
type Db = typeof import("@/db/connection").db
type Tx = Parameters<Parameters<Db[`transaction`]>[0]>[0]

function targetColumn(kind: PinKind) {
  switch (kind) {
    case `issue`:
      return pins.issueId
    case `session`:
      return pins.sessionId
    case `action`:
      return pins.actionId
  }
}

async function assertTargetInTeam(
  db: Db | Tx,
  teamId: string,
  kind: PinKind,
  targetId: string
) {
  let found = false
  if (kind === `issue`) {
    const [row] = await db
      .select({ id: issues.id })
      .from(issues)
      .innerJoin(boards, eq(boards.id, issues.boardId))
      .where(
        and(eq(issues.id, targetId), eq(boards.teamId, teamId), boardVisible())
      )
      .limit(1)
    found = !!row
  } else if (kind === `session`) {
    const [row] = await db
      .select({ id: codingSessions.id })
      .from(codingSessions)
      .where(
        and(eq(codingSessions.id, targetId), eq(codingSessions.teamId, teamId))
      )
      .limit(1)
    found = !!row
  } else {
    const [row] = await db
      .select({ id: actions.id })
      .from(actions)
      .where(and(eq(actions.id, targetId), eq(actions.teamId, teamId)))
      .limit(1)
    found = !!row
  }
  if (!found) {
    throw new TRPCError({
      code: `NOT_FOUND`,
      message: `Nothing to pin: the ${kind} is not in this team`,
    })
  }
}

export const pinsRouter = router({
  // Pin the target if it isn't pinned, unpin it if it is. Returns the
  // resulting state so a client can flip its toggle before the shape lands.
  toggle: authedProcedure
    .input(
      z.object({
        teamId: z.string().uuid(),
        kind: pinKindSchema,
        targetId: z.string().uuid(),
      })
    )
    .mutation(async ({ ctx, input }) => {
      const userId = ctx.session.user.id
      await assertTeamMember(userId, input.teamId)
      const column = targetColumn(input.kind)
      return await ctx.db.transaction(async (tx) => {
        const txId = await generateTxId(tx)
        const deleted = await tx
          .delete(pins)
          .where(and(eq(pins.userId, userId), eq(column, input.targetId)))
          .returning({ id: pins.id })
        if (deleted.length > 0) return { txId, pinned: false }
        await assertTargetInTeam(tx, input.teamId, input.kind, input.targetId)
        // Append: one past the caller's current tail, so the sidebar keeps
        // pin order (fractional room stays for a later drag reorder).
        const [tail] = await tx
          .select({ max: sql<number>`coalesce(max(${pins.sortOrder}), 0)` })
          .from(pins)
          .where(eq(pins.userId, userId))
        await tx.insert(pins).values({
          userId,
          teamId: input.teamId,
          kind: input.kind,
          issueId: input.kind === `issue` ? input.targetId : null,
          sessionId: input.kind === `session` ? input.targetId : null,
          actionId: input.kind === `action` ? input.targetId : null,
          sortOrder: Number(tail?.max ?? 0) + 1,
        })
        return { txId, pinned: true }
      })
    }),
})

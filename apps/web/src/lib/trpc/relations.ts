import { TRPCError } from "@trpc/server"
import { z } from "zod"
import { and, eq } from "drizzle-orm"
import { router, authedProcedure, generateTxId } from "@/lib/trpc"
import { boards, issueRelations, issues } from "@/db/schema"
import { assertIssueAccess } from "@/lib/team-membership"
import { boardVisible } from "@/lib/board-visibility"
import { issueRelationTypeSchema } from "@/lib/domain"
import {
  canonicalizeRelation,
  deleteRelationInTx,
  insertRelationInTx,
} from "@/lib/issue-relations"
import { issuesRouter } from "@/lib/trpc/issues"

// EXP-736 — explicit ("user") issue relations. The auto-derived `#IDENT`
// rows are written by lib/issue-relations.ts from the text writers instead;
// this router is only the picker's two mutations.
//
// `duplicate` is NOT written here: it is the dual-write of
// issues.duplicate_of_id, so both arms delegate to issues.update and let
// finalizeIssueUpdateInTx's syncDuplicateMirror produce the row. That keeps
// the status/duplicateOfId lockstep, the canonical-issue validation and the
// completedAt derivation in ONE place.

export const relationsRouter = router({
  create: authedProcedure
    .input(
      z.object({
        issueId: z.string().uuid(),
        relatedIssueId: z.string().uuid(),
        type: issueRelationTypeSchema,
        // The picker's inverse half ("sub-issue of", "blocked by",
        // "duplicated by"): swap the pair before storing so both halves land
        // on the same canonical row.
        inverse: z.boolean().optional(),
      })
    )
    .mutation(async ({ ctx, input }) => {
      if (input.issueId === input.relatedIssueId) {
        throw new TRPCError({
          code: `BAD_REQUEST`,
          message: `An issue cannot relate to itself`,
        })
      }

      const issueContext = await assertIssueAccess(
        ctx.session.user.id,
        input.issueId,
        `write`
      )

      // The other issue must live in the same team, on a board that is
      // neither trashed nor archived — the same gate the duplicate picker
      // applies (a hidden issue would render as an unresolvable row).
      const [other] = await ctx.db
        .select({ teamId: boards.teamId })
        .from(issues)
        .innerJoin(boards, eq(boards.id, issues.boardId))
        .where(and(eq(issues.id, input.relatedIssueId), boardVisible()))
        .limit(1)
      if (!other || other.teamId !== issueContext.teamId) {
        throw new TRPCError({
          code: `BAD_REQUEST`,
          message: `The related issue must be in the same team`,
        })
      }

      const canonical = canonicalizeRelation(
        input.issueId,
        input.relatedIssueId,
        input.type,
        input.inverse === true
      )

      if (canonical.type === `duplicate`) {
        // canonical.issueId is the DUPLICATE, canonical.relatedIssueId the
        // canonical issue (the enum's own direction).
        const result = await issuesRouter
          .createCaller(ctx)
          .update({
            id: canonical.issueId,
            duplicateOfId: canonical.relatedIssueId,
          })
        return { txId: result.txId, relation: null }
      }

      return await ctx.db.transaction(async (tx) => {
        // Cycle guard for the two directed types: A blocks B and B blocks A
        // (or two-way parenthood) is never a state a user meant to reach, and
        // the UNIQUE index can't catch it — the pairs differ.
        const [reverse] = await tx
          .select({ id: issueRelations.id })
          .from(issueRelations)
          .where(
            and(
              eq(issueRelations.issueId, canonical.relatedIssueId),
              eq(issueRelations.relatedIssueId, canonical.issueId),
              eq(issueRelations.type, canonical.type)
            )
          )
          .limit(1)
        if (reverse) {
          throw new TRPCError({
            code: `BAD_REQUEST`,
            message: `The opposite relation already exists`,
          })
        }

        const txId = await generateTxId(tx)
        const relation = await insertRelationInTx(tx, {
          ...canonical,
          source: `user`,
          teamId: issueContext.teamId,
          actorUserId: ctx.session.user.id,
        })
        return { txId, relation }
      })
    }),

  delete: authedProcedure
    .input(z.object({ id: z.string().uuid() }))
    .mutation(async ({ ctx, input }) => {
      const [row] = await ctx.db
        .select({
          id: issueRelations.id,
          issueId: issueRelations.issueId,
          relatedIssueId: issueRelations.relatedIssueId,
          type: issueRelations.type,
        })
        .from(issueRelations)
        .where(eq(issueRelations.id, input.id))
        .limit(1)
      if (!row) {
        throw new TRPCError({
          code: `NOT_FOUND`,
          message: `Relation not found`,
        })
      }

      // Gate on the SOURCE issue: it owns the row's team/board scoping, and
      // both issues are in the same team by construction.
      await assertIssueAccess(ctx.session.user.id, row.issueId, `write`)

      if (row.type === `duplicate`) {
        const result = await issuesRouter
          .createCaller(ctx)
          .update({ id: row.issueId, duplicateOfId: null })
        return { txId: result.txId, id: row.id }
      }

      return await ctx.db.transaction(async (tx) => {
        const txId = await generateTxId(tx)
        await deleteRelationInTx(tx, { id: row.id }, ctx.session.user.id)
        return { txId, id: row.id }
      })
    }),
})

import { z } from "zod"
import { and, eq } from "drizzle-orm"
import { TRPCError } from "@trpc/server"
import {
  COMPOSER_DRAFT_KEY_MAX,
  parseComposerDraftKey,
} from "@exp/domain-contract/composer-draft"
import { router, authedProcedure, generateTxId } from "@/lib/trpc"
import { composerDrafts } from "@/db/schema"
import { resolveTeamAccess } from "@/lib/team-membership"

// #FEED-219 (fixture `composer-draft.json`): every composer's synced DRAFT,
// one row per (caller, target key). Owner-scoped like `issueDrafts`: the
// shape is `user_id = me`, `delete` needs no team lookup (the `user_id`
// predicate IS the authorization), and `upsert` only checks the caller can
// read the team the row files under. Clients debounce their writes (the
// fixture's `debounceMs`); a text that is empty or only whitespace is a
// delete (`composerDraftWrite`), never an empty row.

/** Long enough for any message a composer sends. */
export const COMPOSER_DRAFT_TEXT_MAX = 100_000

const keySchema = z
  .string()
  .min(1)
  .max(COMPOSER_DRAFT_KEY_MAX)
  .refine((key) => parseComposerDraftKey(key) !== null, {
    message: `Unknown composer draft key`,
  })

export const composerDraftsRouter = router({
  /** Create or replace the caller's draft for one target. */
  upsert: authedProcedure
    .input(
      z.object({
        teamId: z.string().uuid(),
        key: keySchema,
        text: z.string().max(COMPOSER_DRAFT_TEXT_MAX),
      })
    )
    .mutation(async ({ ctx, input }) => {
      const userId = ctx.session.user.id
      if (input.text.trim().length === 0) {
        throw new TRPCError({
          code: `BAD_REQUEST`,
          message: `An empty draft is a delete`,
        })
      }
      await resolveTeamAccess(userId, input.teamId, `read`)
      return await ctx.db.transaction(async (tx) => {
        const txId = await generateTxId(tx)
        const [draft] = await tx
          .insert(composerDrafts)
          .values({ userId, teamId: input.teamId, key: input.key, text: input.text })
          .onConflictDoUpdate({
            target: [composerDrafts.userId, composerDrafts.key],
            set: { teamId: input.teamId, text: input.text, updatedAt: new Date() },
          })
          .returning()
        return { draft, txId }
      })
    }),

  /** Drop the caller's draft for one target (a send, an emptied field, a
   *  discard). Deleting nothing is fine. */
  delete: authedProcedure
    .input(z.object({ key: keySchema }))
    .mutation(async ({ ctx, input }) => {
      const userId = ctx.session.user.id
      return await ctx.db.transaction(async (tx) => {
        const txId = await generateTxId(tx)
        const deleted = await tx
          .delete(composerDrafts)
          .where(and(eq(composerDrafts.userId, userId), eq(composerDrafts.key, input.key)))
          .returning({ id: composerDrafts.id })
        return { txId, deleted: deleted.length > 0 }
      })
    }),
})

import { router, authedProcedure, generateTxId } from "@/lib/trpc"
import { db } from "@/db/connection"
import { workspaces } from "@/db/schema"
import { eq } from "drizzle-orm"

export const workspacesRouter = router({
  ensureDefault: authedProcedure.mutation(async ({ ctx }) => {
    const userName = ctx.session.user.name || `My`

    return await db.transaction(async (tx) => {
      const existing = await tx
        .select()
        .from(workspaces)
        .where(eq(workspaces.slug, `default`))
        .limit(1)

      if (existing.length > 0) {
        return { workspace: existing[0], txId: 0 }
      }

      const txId = await generateTxId(tx)
      const [workspace] = await tx
        .insert(workspaces)
        .values({
          name: `${userName}'s Workspace`,
          slug: `default`,
        })
        .returning()

      return { workspace, txId }
    })
  }),
})

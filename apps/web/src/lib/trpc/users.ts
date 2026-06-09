import { router, authedProcedure } from "@/lib/trpc"
import { users } from "@/db/auth-schema"
import { getReadableUserIdsInWorkspaces } from "@/lib/workspace-membership"
import { inArray } from "drizzle-orm"

export const usersRouter = router({
  listByWorkspaceIds: authedProcedure.query(async ({ ctx }) => {
    // Same email-safe scoping as the users shape: only co-members of
    // workspaces the caller actually joined (not all public workspaces).
    const userIds = await getReadableUserIdsInWorkspaces(ctx.session.user.id)

    if (userIds.length === 0) {
      return { users: [] }
    }

    const userRows = await ctx.db
      .select({ id: users.id, name: users.name, email: users.email })
      .from(users)
      .where(inArray(users.id, userIds))

    return { users: userRows }
  }),
})

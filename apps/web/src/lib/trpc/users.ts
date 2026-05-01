import { router, authedProcedure } from "@/lib/trpc"
import { db } from "@/db/connection"
import { users } from "@/db/auth-schema"
import { getUserIdsInWorkspaces } from "@/lib/workspace-membership"
import { inArray } from "drizzle-orm"

export const usersRouter = router({
  listByWorkspaceIds: authedProcedure.query(async ({ ctx }) => {
    const userIds = await getUserIdsInWorkspaces(ctx.session.user.id)

    if (userIds.length === 0) {
      return { users: [] }
    }

    const userRows = await db
      .select({ id: users.id, name: users.name, email: users.email })
      .from(users)
      .where(inArray(users.id, userIds))

    return { users: userRows }
  }),
})

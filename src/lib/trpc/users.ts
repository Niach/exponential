import { router, authedProcedure } from "@/lib/trpc"
import { db } from "@/db/connection"
import { users } from "@/db/auth-schema"

export const usersRouter = router({
  listByWorkspaceIds: authedProcedure.query(async () => {
    const userRows = await db
      .select({ id: users.id, name: users.name, email: users.email })
      .from(users)

    return { users: userRows }
  }),
})

import { TRPCError } from "@trpc/server"
import { eq } from "drizzle-orm"
import { db } from "@/db/connection"
import { users } from "@/db/auth-schema"

export async function assertAdmin(userId: string) {
  const [u] = await db
    .select({ isAdmin: users.isAdmin })
    .from(users)
    .where(eq(users.id, userId))
    .limit(1)
  if (!u?.isAdmin) {
    throw new TRPCError({ code: `FORBIDDEN`, message: `Admin access required` })
  }
}

export async function isUserAdmin(userId: string): Promise<boolean> {
  const [u] = await db
    .select({ isAdmin: users.isAdmin })
    .from(users)
    .where(eq(users.id, userId))
    .limit(1)
  return Boolean(u?.isAdmin)
}

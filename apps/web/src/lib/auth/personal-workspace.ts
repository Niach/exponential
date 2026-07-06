import { randomBytes } from "crypto"
import { and, eq } from "drizzle-orm"
import { workspaces, workspaceMembers } from "@/db/schema"
import type { db as dbType } from "@/db/connection"

type Tx = Parameters<Parameters<typeof dbType.transaction>[0]>[0]

// A user's "personal" workspace is any non-public workspace they belong to.
// The public feedback board never counts as a landing spot.
export async function findNonPublicMembership(tx: Tx, userId: string) {
  const [membership] = await tx
    .select({ workspaceId: workspaceMembers.workspaceId })
    .from(workspaceMembers)
    .innerJoin(workspaces, eq(workspaces.id, workspaceMembers.workspaceId))
    .where(
      and(eq(workspaceMembers.userId, userId), eq(workspaces.isPublic, false))
    )
    .limit(1)
  return membership
}

export async function createPersonalWorkspace(
  tx: Tx,
  args: { userId: string; userName: string | null }
) {
  const slug = `ws-${randomBytes(4).toString(`hex`)}`
  const [workspace] = await tx
    .insert(workspaces)
    .values({
      name: `${args.userName || `My`}'s Workspace`,
      slug,
    })
    .returning()

  await tx.insert(workspaceMembers).values({
    workspaceId: workspace.id,
    userId: args.userId,
    role: `owner`,
  })

  return workspace
}

// Signup-time entry point (Better Auth user.create.after hook): every real
// account gets its personal workspace immediately, so mobile/desktop-first
// signups never land workspace-less. Idempotent — an existing non-public
// membership short-circuits, and `workspaces.ensureDefault` remains the
// self-heal for legacy accounts. No txId capture here: nothing is waiting on
// Electric to confirm this write.
export async function ensurePersonalWorkspace(args: {
  userId: string
  userName: string | null
}) {
  const { db } = await import(`@/db/connection`)
  await db.transaction(async (tx) => {
    const existing = await findNonPublicMembership(tx, args.userId)
    if (existing) return
    await createPersonalWorkspace(tx, args)
  })
}

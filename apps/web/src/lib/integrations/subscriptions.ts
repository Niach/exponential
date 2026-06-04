import { eq } from "drizzle-orm"
import { db } from "@/db/connection"
import { issueSubscribers } from "@/db/schema"
import { users } from "@/db/auth-schema"
import type { SubscriberSource } from "@/lib/domain"

type Tx = Parameters<Parameters<typeof db.transaction>[0]>[0]

// Auto-subscribe a user to an issue (D7). Called inside the create/assign/
// comment/mention transactions. Skips agent users (they have no inbox) and
// inserts only when no (issue,user) row exists yet — so a prior MANUAL
// unsubscribe (source='manual', unsubscribed=true) is preserved and auto-events
// do NOT resurrect the subscription. workspace_id is also trigger-denormalized
// from issue→project; we pass the known value to satisfy the NOT NULL insert.
export async function ensureSubscribed(
  tx: Tx,
  args: {
    issueId: string
    userId: string
    workspaceId: string
    source: SubscriberSource
  }
): Promise<void> {
  const [u] = await tx
    .select({ isAgent: users.isAgent })
    .from(users)
    .where(eq(users.id, args.userId))
    .limit(1)
  if (!u || u.isAgent) return

  await tx
    .insert(issueSubscribers)
    .values({
      issueId: args.issueId,
      userId: args.userId,
      workspaceId: args.workspaceId,
      source: args.source,
    })
    .onConflictDoNothing({
      target: [issueSubscribers.issueId, issueSubscribers.userId],
    })
}

import { randomUUID } from "node:crypto"
import { db } from "@/db/connection"
import { users, teamMembers } from "@/db/schema"

type Tx = Parameters<Parameters<typeof db.transaction>[0]>[0]

export function widgetUserName(configName: string): string {
  return `Widget: ${configName}`
}

// Every widget config owns one synthetic bot user (the `creatorId` of issues
// it submits). `isAgent=true` keeps it out of subscriptions/notifications and
// @-mentions, while a plain `member` membership lets clients resolve its
// display name (the users shape only syncs co-members). NEVER delete this user
// — issues.creator_id cascades on user delete, so removing it would delete the
// widget's issues; widget_configs.widget_user_id is `restrict` to keep that
// mistake loud.
export async function createWidgetUser(
  tx: Tx,
  args: { teamId: string; configName: string }
): Promise<string> {
  const widgetUserId = randomUUID()
  const now = new Date()

  await tx.insert(users).values({
    id: widgetUserId,
    name: widgetUserName(args.configName),
    email: `widget-${widgetUserId}@exponential.local`,
    emailVerified: true,
    image: null,
    isAdmin: false,
    isAgent: true,
    createdAt: now,
    updatedAt: now,
  })

  await tx
    .insert(teamMembers)
    .values({
      teamId: args.teamId,
      userId: widgetUserId,
      role: `member`,
    })
    .onConflictDoNothing()

  return widgetUserId
}

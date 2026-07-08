import { and, eq, isNull, ne } from "drizzle-orm"
import { db } from "@/db/connection"
import { projects, users, workspaceMembers, workspaces } from "@/db/schema"
import { getFeedbackWorkspaceId } from "@/lib/bootstrap-cloud"

// The single definition of "needs onboarding". Web, iOS and Android all gate
// the first-run wizard purely on `onboardingCompletedAt` from the session, so
// this is the one place the rule lives: a user who already has a project in a
// workspace they're an explicit member of doesn't need the wizard — the flag
// is backfilled on session read (covers accounts that predate the wizard).
// Membership in the bootstrap feedback workspace deliberately does NOT count:
// its projects are shared infra, not evidence the user set up their own space.
export async function resolveOnboardingCompletedAt(user: {
  id: string
  onboardingCompletedAt?: Date | string | null
}): Promise<Date | string | null> {
  if (user.onboardingCompletedAt != null) return user.onboardingCompletedAt

  const feedbackWorkspaceId = await getFeedbackWorkspaceId()
  const [evidence] = await db
    .select({ projectId: projects.id })
    .from(workspaceMembers)
    .innerJoin(
      workspaces,
      and(
        eq(workspaces.id, workspaceMembers.workspaceId),
        feedbackWorkspaceId
          ? ne(workspaces.id, feedbackWorkspaceId)
          : undefined
      )
    )
    .innerJoin(projects, eq(projects.workspaceId, workspaces.id))
    .where(eq(workspaceMembers.userId, user.id))
    .limit(1)

  if (!evidence) return null

  const completedAt = new Date()
  await db
    .update(users)
    .set({ onboardingCompletedAt: completedAt, updatedAt: completedAt })
    .where(and(eq(users.id, user.id), isNull(users.onboardingCompletedAt)))
  return completedAt
}

import { and, eq, isNull } from "drizzle-orm"
import { db } from "@/db/connection"
import { boards, users, teamMembers, teams } from "@/db/schema"

// The single definition of "needs onboarding". Web, iOS and Android all gate
// the first-run wizard purely on `onboardingCompletedAt` from the session, so
// this is the one place the rule lives: a user who already has a board in a
// team they're an explicit member of doesn't need the wizard — the flag
// is backfilled on session read (covers accounts that predate the wizard).
export async function resolveOnboardingCompletedAt(user: {
  id: string
  onboardingCompletedAt?: Date | string | null
}): Promise<Date | string | null> {
  if (user.onboardingCompletedAt != null) return user.onboardingCompletedAt

  const [evidence] = await db
    .select({ boardId: boards.id })
    .from(teamMembers)
    .innerJoin(teams, eq(teams.id, teamMembers.teamId))
    .innerJoin(boards, eq(boards.teamId, teams.id))
    .where(eq(teamMembers.userId, user.id))
    .limit(1)

  if (!evidence) return null

  const completedAt = new Date()
  await db
    .update(users)
    .set({ onboardingCompletedAt: completedAt, updatedAt: completedAt })
    .where(and(eq(users.id, user.id), isNull(users.onboardingCompletedAt)))
  return completedAt
}

// The "Get the desktop app" card dismissal (EXP-51) must survive the session
// cookie cache (5-min TTL): right after dismissing, the cached user snapshot
// still carries null, so a reload inside the TTL would resurrect the card.
// Dismissal is one-way — a cached non-null value is trusted as-is; a cached
// null gets one cheap PK re-read on session resolution. (The old
// gettingStartedDismissedAt sibling retired with EXP-548/EXP-560 — the
// checklist hides itself once complete.)
export async function resolveDesktopCardDismissal(user: {
  id: string
  desktopAppCardDismissedAt?: Date | string | null
}): Promise<{
  desktopAppCardDismissedAt: Date | string | null
}> {
  if (user.desktopAppCardDismissedAt != null) {
    return { desktopAppCardDismissedAt: user.desktopAppCardDismissedAt }
  }
  const [row] = await db
    .select({
      desktopAppCardDismissedAt: users.desktopAppCardDismissedAt,
    })
    .from(users)
    .where(eq(users.id, user.id))
    .limit(1)
  return {
    desktopAppCardDismissedAt: row?.desktopAppCardDismissedAt ?? null,
  }
}

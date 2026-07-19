import { randomBytes } from "crypto"
import { and, eq, ne } from "drizzle-orm"
import { teams, teamMembers } from "@/db/schema"
import type { db as dbType } from "@/db/connection"
import { getFeedbackTeamId } from "@/lib/bootstrap-cloud"

type Tx = Parameters<Parameters<typeof dbType.transaction>[0]>[0]

// A user's "personal" team is any team they belong to EXCEPT the
// bootstrap feedback team — INITIAL_ADMIN accounts get owner membership
// there on promotion, which must never count as "already has a personal
// team".
export async function findPersonalMembership(tx: Tx, userId: string) {
  const feedbackTeamId = await getFeedbackTeamId()
  const [membership] = await tx
    .select({ teamId: teamMembers.teamId })
    .from(teamMembers)
    .where(
      and(
        eq(teamMembers.userId, userId),
        feedbackTeamId
          ? ne(teamMembers.teamId, feedbackTeamId)
          : undefined
      )
    )
    .limit(1)
  return membership
}

// Would the user still have a personal team if `excludeTeamId` went
// away? Used by `teams.delete` to refuse deleting the LAST personal
// team (EXP-82) — the EXP-43 ensureDefault self-heal would otherwise
// silently recreate it on some clients (Android home bootstrap, desktop,
// web /t/default) and not others (iOS), which reads as data corruption.
export async function findOtherPersonalMembership(
  tx: Tx,
  userId: string,
  excludeTeamId: string
) {
  const feedbackTeamId = await getFeedbackTeamId()
  const [membership] = await tx
    .select({ teamId: teamMembers.teamId })
    .from(teamMembers)
    .where(
      and(
        eq(teamMembers.userId, userId),
        ne(teamMembers.teamId, excludeTeamId),
        feedbackTeamId
          ? ne(teamMembers.teamId, feedbackTeamId)
          : undefined
      )
    )
    .limit(1)
  return membership
}

export async function createPersonalTeam(
  tx: Tx,
  args: { userId: string; userName: string | null }
) {
  const slug = `ws-${randomBytes(4).toString(`hex`)}`
  const [team] = await tx
    .insert(teams)
    .values({
      // "Team" is the user-facing word for teams since the teams rename;
      // existing team names are user data and stay untouched.
      name: `${args.userName || `My`}'s Team`,
      slug,
    })
    .returning()

  await tx.insert(teamMembers).values({
    teamId: team.id,
    userId: args.userId,
    role: `owner`,
  })

  return team
}

// Signup-time entry point (Better Auth user.create.after hook): every real
// account gets its personal team immediately, so mobile/desktop-first
// signups never land team-less. Idempotent — an existing personal
// membership short-circuits, and `teams.ensureDefault` remains the
// self-heal for legacy accounts. No txId capture here: nothing is waiting on
// Electric to confirm this write.
export async function ensurePersonalTeam(args: {
  userId: string
  userName: string | null
}) {
  const { db } = await import(`@/db/connection`)
  await db.transaction(async (tx) => {
    const existing = await findPersonalMembership(tx, args.userId)
    if (existing) return
    await createPersonalTeam(tx, args)
  })
}

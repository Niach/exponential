import { and, eq, or, sql } from "drizzle-orm"
import { TRPCError } from "@trpc/server"
import { db } from "@/db/connection"
import { teamMembers, users } from "@/db/schema"

// #FEED-218: an MCP caller names a member three ways: the USER id
// (exponential_people `id`, what records, grants and packages store), the
// team MEMBER id (exponential_people `memberId`) or the email. Everything
// past the MCP boundary takes the user id; this is the one place that maps.

const UUID = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i

/** How `ref` may match: a user id always, a member id only when it is a
 *  UUID (`team_members.id`), an email only when it holds an @. Pure. */
export function teamUserRefKinds(ref: string): { userId: true; memberId: boolean; email: boolean } {
  return { userId: true, memberId: UUID.test(ref), email: ref.includes(`@`) }
}

/** The user id of the team member `ref` names (user id, member id or
 *  email, case-insensitive), or BAD_REQUEST when it names no member. */
export async function resolveTeamUserRef(teamId: string, ref: string): Promise<string> {
  const value = ref.trim()
  const kinds = teamUserRefKinds(value)
  const matches = [eq(teamMembers.userId, value)]
  if (kinds.memberId) matches.push(eq(teamMembers.id, value))
  if (kinds.email) matches.push(sql`lower(${users.email}) = ${value.toLowerCase()}`)
  const rows = await db
    .select({ userId: teamMembers.userId })
    .from(teamMembers)
    .innerJoin(users, eq(users.id, teamMembers.userId))
    .where(and(eq(teamMembers.teamId, teamId), or(...matches)))
    .limit(2)
  if (rows.length !== 1) throw new TRPCError({ code: `BAD_REQUEST`, message: `${value} is no member of this team: pass a user id, member id or email from exponential_people list` })
  return rows[0]!.userId
}

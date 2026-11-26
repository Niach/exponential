import { eq, inArray, and } from "drizzle-orm"
import { heldAnswer, type HeldAnswer } from "@exp/domain-contract/held-gone"
import { db } from "@/db/connection"
import { devices, sessions, teamMembers } from "@/db/schema"
import type { Db } from "@/lib/sessions/tx"
import { spaceOf } from "@/lib/spaces/context"
import { spaceRootIds } from "@/lib/spaces/grant"

const NO_ROOT = `00000000-0000-0000-0000-000000000000`

/** #EXP-2138 (held-gone.json SERVER): of `ids`, which do not exist any more
 *  (`gone`) and which exist but are not the caller's to have (`unreadable`:
 *  neither started, nor readable, nor on a machine of its own). Membership is
 *  read fresh, never from the cache: a wrong answer deletes. */
export async function heldGone(
  userId: string,
  ids: readonly string[],
  database: Db = db
): Promise<{ gone: string[]; unreadable: string[] }> {
  const asked = [...new Set(ids)]
  if (asked.length === 0) return { gone: [], unreadable: [] }
  const rows = await database
    .select({
      id: sessions.id,
      userId: sessions.userId,
      teamId: sessions.teamId,
      shared: sessions.shared,
      deviceId: sessions.deviceId,
    })
    .from(sessions)
    .where(inArray(sessions.id, asked))
  const teamIds = rows.length
    ? (
        await database
          .select({ teamId: teamMembers.teamId })
          .from(teamMembers)
          .where(and(eq(teamMembers.userId, userId), inArray(teamMembers.teamId, [...new Set(rows.map((row) => row.teamId))])))
      ).map((row) => row.teamId)
    : []
  const deviceIds = rows.length
    ? (
        await database
          .select({ deviceId: devices.deviceId })
          .from(devices)
          .where(and(eq(devices.userId, userId), inArray(devices.deviceId, [...new Set(rows.map((row) => row.deviceId))])))
      ).map((row) => row.deviceId)
    : []
  const byId = new Map(rows.map((row) => [row.id, row]))
  const answers = asked.map((id) => [id, heldAnswer(byId.get(id) ?? null, { userId, teamIds, deviceIds })] as const)
  // #EXP-2161: through a member space, a session that is not of its trees is
  // none of its business (`unreadable`), like another member's.
  const space = spaceOf(userId)
  if (space) {
    const own = await spaceRootIds(database, space)
    const mine = new Set(
      rows.length ? (await database.select({ id: sessions.id }).from(sessions).where(and(inArray(sessions.id, rows.map((row) => row.id)), inArray(sessions.rootId, own.length ? own : [NO_ROOT])))).map((row) => row.id) : []
    )
    const outside = answers.flatMap(([id, answer]) => (answer !== `gone` && !mine.has(id) ? [id] : []))
    return {
      gone: answers.flatMap(([id, answer]) => (answer === `gone` ? [id] : [])),
      unreadable: [...new Set([...answers.flatMap(([id, answer]) => (answer === `unreadable` ? [id] : [])), ...outside])],
    }
  }
  const of = (kind: HeldAnswer) => answers.filter(([, answer]) => answer === kind).map(([id]) => id)
  return { gone: of(`gone`), unreadable: of(`unreadable`) }
}

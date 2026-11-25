// #EXP-2161 (member-space.json, THE END): an Assistant without a device (its
// space ended, none picked since) takes nothing: every send to it or into
// its conversation's tree is refused and stores nothing. What was said before
// stays readable there.
import { TRPCError } from "@trpc/server"
import { and, eq } from "drizzle-orm"
import { MEMBER_SPACE_COPY } from "@exp/domain-contract/member-space"
import { agents, sessions, type Agent } from "@/db/schema"
import { isSpaceDeviceId, type SpaceReader } from "@/lib/spaces/grant"

export const assistantWithoutDevice = (agent: Pick<Agent, `role` | `deviceId`> | null | undefined): boolean =>
  agent?.role === `assistant` && agent.deviceId === ``

export function assertAssistantHasDevice(agent: Pick<Agent, `role` | `deviceId`> | null | undefined): void {
  if (assistantWithoutDevice(agent)) throw new TRPCError({ code: `PRECONDITION_FAILED`, message: MEMBER_SPACE_COPY.noDevice })
}

/** A conversation whose space ended, and any session that left its space. */
export const SPACE_CHAT_OVER = `This chat ran on a team device and stays there: it cannot continue`

/** A session of a space never goes on anywhere else: no agent on another
 *  machine resumes a history its host could have written. */
export function assertStaysOnSpace(session: { deviceId: string | null }, deviceId: string): void {
  if (isSpaceDeviceId(session.deviceId) && session.deviceId !== deviceId) {
    throw new TRPCError({ code: `PRECONDITION_FAILED`, message: SPACE_CHAT_OVER })
  }
}

/** The tree of `sessionId` is no conversation of an Assistant without a
 *  device, nor one whose space ended. */
export async function assertTreeHasDevice(exec: SpaceReader, sessionId: string): Promise<void> {
  const [row] = await exec.select({ rootId: sessions.rootId }).from(sessions).where(eq(sessions.id, sessionId)).limit(1)
  if (!row) return
  const [agent] = await exec
    .select({ role: agents.role, deviceId: agents.deviceId })
    .from(sessions)
    .innerJoin(agents, eq(agents.id, sessions.agentId))
    .where(eq(sessions.id, row.rootId ?? sessionId))
    .limit(1)
  assertAssistantHasDevice(agent)
  if (agent) return
  const [root] = await exec
    .select({ deviceId: sessions.deviceId, userId: sessions.userId })
    .from(sessions)
    .where(eq(sessions.id, row.rootId ?? sessionId))
    .limit(1)
  if (!root || !isSpaceDeviceId(root.deviceId)) return
  const [own] = await exec
    .select({ role: agents.role, deviceId: agents.deviceId })
    .from(agents)
    .where(and(eq(agents.ownerId, root.userId), eq(agents.role, `assistant`)))
    .limit(1)
  assertAssistantHasDevice(own)
  throw new TRPCError({ code: `PRECONDITION_FAILED`, message: SPACE_CHAT_OVER })
}

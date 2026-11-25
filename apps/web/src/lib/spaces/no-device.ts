// #EXP-2161 (member-space.json, THE END): an Assistant without a device (its
// space ended, none picked since) takes nothing: every send to it or into
// its conversation's tree is refused and stores nothing. What was said before
// stays readable there.
import { TRPCError } from "@trpc/server"
import { eq } from "drizzle-orm"
import { MEMBER_SPACE_COPY } from "@exp/domain-contract/member-space"
import { agents, sessions, type Agent } from "@/db/schema"
import type { SpaceReader } from "@/lib/spaces/grant"

export const assistantWithoutDevice = (agent: Pick<Agent, `role` | `deviceId`> | null | undefined): boolean =>
  agent?.role === `assistant` && agent.deviceId === ``

export function assertAssistantHasDevice(agent: Pick<Agent, `role` | `deviceId`> | null | undefined): void {
  if (assistantWithoutDevice(agent)) throw new TRPCError({ code: `PRECONDITION_FAILED`, message: MEMBER_SPACE_COPY.noDevice })
}

/** The tree of `sessionId` is no conversation of an Assistant without a device. */
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
}

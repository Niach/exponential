// #EXP-2161 (member-space.json, THE CREDENTIAL): what a member space's OWN
// key may call over tRPC. Exactly `procedures`; every session it names runs
// on the space, every machine it names is the space, every tree it names is
// one of the space's. Fails closed: an input this file does not read is
// still checked by the procedure inside `runInSpace`.
import { TRPCError } from "@trpc/server"
import { eq } from "drizzle-orm"
import { MEMBER_SPACE } from "@exp/domain-contract/member-space"
import { deviceCommands, messages, sessions } from "@/db/schema"
import { messageWithSession } from "@/lib/sessions/messages"
import { parseMcpSessionHeader } from "@/lib/emcp/session-header"
import { SPACE_KEY_MESSAGE, type SpaceGrant } from "@/lib/spaces/context"
import { rootInSpace, runsOnSpace, type SpaceReader } from "@/lib/spaces/grant"

const forbidden = () => new TRPCError({ code: `FORBIDDEN`, message: SPACE_KEY_MESSAGE })
const notFound = () => new TRPCError({ code: `NOT_FOUND`, message: `Session not found` })
const UUID_RE = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i

/** Resolve their session from `X-Exp-Session-Id`: required. */
const HEADER_PROCEDURES: ReadonlySet<string> = new Set([`mcpServers.resolveForLaunch`, `skills.resolveForLaunch`])

/** The session `id` runs on the space, in one of its trees. */
export async function assertSessionOnSpace(exec: SpaceReader, space: SpaceGrant, id: string): Promise<void> {
  if (!UUID_RE.test(id)) throw notFound()
  const [row] = await exec
    .select({ id: sessions.id, rootId: sessions.rootId, deviceId: sessions.deviceId, userId: sessions.userId })
    .from(sessions)
    .where(eq(sessions.id, id))
    .limit(1)
  if (!row || !runsOnSpace(space, row) || !(await rootInSpace(exec, space, row.rootId ?? row.id, `own`))) throw notFound()
}

/** Refuses a call a space's key may not make. */
export async function assertSpaceCall(
  exec: SpaceReader,
  space: SpaceGrant,
  path: string,
  input: unknown,
  request: Request
): Promise<void> {
  if (!(MEMBER_SPACE.procedures as readonly string[]).includes(path)) throw forbidden()
  const headerSession = parseMcpSessionHeader(request)
  if (headerSession) await assertSessionOnSpace(exec, space, headerSession)
  else if (HEADER_PROCEDURES.has(path)) throw forbidden()
  const args = input && typeof input === `object` ? (input as Record<string, unknown>) : {}
  if (args.deviceId !== undefined && args.deviceId !== null && args.deviceId !== space.deviceId) throw forbidden()
  if (args.sessionId !== undefined && args.sessionId !== null) {
    if (typeof args.sessionId !== `string`) throw notFound()
    await assertSessionOnSpace(exec, space, args.sessionId)
  }
  if (args.rootId !== undefined && args.rootId !== null) {
    if (typeof args.rootId !== `string` || !UUID_RE.test(args.rootId)) throw notFound()
    if (!(await rootInSpace(exec, space, args.rootId, `own`))) throw notFound()
  }
  if (args.messageId !== undefined && args.messageId !== null) {
    if (typeof args.messageId !== `string` || !UUID_RE.test(args.messageId)) throw notFound()
    const [message] = await exec.select(messageWithSession()).from(messages).where(eq(messages.id, args.messageId)).limit(1)
    if (!message) throw notFound()
    await assertSessionOnSpace(exec, space, message.sessionId)
  }
  if (args.commandId !== undefined) {
    if (typeof args.commandId !== `string` || !UUID_RE.test(args.commandId)) throw forbidden()
    const [command] = await exec
      .select({ deviceRowId: deviceCommands.deviceRowId })
      .from(deviceCommands)
      .where(eq(deviceCommands.id, args.commandId))
      .limit(1)
    if (command?.deviceRowId !== space.spaceId) throw forbidden()
  }
}

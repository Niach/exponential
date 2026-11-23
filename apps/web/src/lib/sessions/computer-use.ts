import { TRPCError } from "@trpc/server"
import { and, eq, ne } from "drizzle-orm"
import { db } from "@/db/connection"
import { devices, sessions } from "@/db/schema"
import composerMenu from "@exp/domain-contract/fixtures/composer-menu.json"

// #EXP-1899: the device's Computer use switch is the gate. A session's
// `computer_use` only says whether it FOLLOWS that switch (true) or opted
// out (false); the launcher grants while the switch is on AND the session
// follows (spec: composer-menu.json `computerUse`). #FEED-302: over MCP a
// session may start another with it on its owner's own switched-on device
// (`computerUse.agentStart`); every such start is logged.

type Exec = Pick<typeof db, `select`>
type Writer = Pick<typeof db, `update`>

/** A build advertising this cap applies the gate itself; for any other the
 *  server stores the EFFECTIVE value ([`gatedComputerUse`]). */
export const COMPUTER_USE_SWITCH_CAP: string = composerMenu.computerUse.switchCap

const agentStart = composerMenu.computerUse.agentStart

/** #FEED-302 (composer-menu.json `computerUse.agentStart.refusals`): why an
 *  explicit `computerUse: true` over MCP is refused. */
export const COMPUTER_USE_REFUSALS = agentStart.refusals
export type ComputerUseRefusal = keyof typeof COMPUTER_USE_REFUSALS

const refuse = (why: ComputerUseRefusal): never => {
  throw new TRPCError({ code: `FORBIDDEN`, message: COMPUTER_USE_REFUSALS[why] })
}

/** #FEED-302: whether a session may START another with computer use over
 *  MCP (the fixture's `agentStart.cases`): a session makes the call, the
 *  person it works for owns its tree AND the target device, and that
 *  device's switch is on. The first failing rule answers. */
export function agentStartVerdict(facts: {
  caller: boolean
  ownTree: boolean
  ownDevice: boolean
  switchOn: boolean
}): `allowed` | Exclude<ComputerUseRefusal, `running`> {
  if (!facts.caller) return `caller`
  if (!facts.ownTree || !facts.ownDevice) return `owner`
  return facts.switchOn ? `allowed` : `switch`
}

/** The server line a start under [`agentStartVerdict`] leaves in the new
 *  session (`agentStart.log`). */
export const computerUseStartLog = (callerSessionId: string): string =>
  agentStart.log.replace(`{session}`, callerSessionId)

/** Whether an MCP start asks for MORE than it inherits: an explicit true
 *  where the caller or `from` (a child's parent; null = a new root) is off
 *  or missing. Only then [`assertAgentStart`] decides. */
export function raisesComputerUse(args: {
  pick: boolean | undefined
  viaMcp: boolean
  caller: boolean | null
  from: boolean | null
}): boolean {
  return args.viaMcp && args.pick === true && !((args.caller ?? false) && (args.from ?? true))
}

/** What a NEW session stores. `from` = the row it inherits from (a child's
 *  parent, a continuation's ended row; null = a new root), `caller` = the
 *  calling session's own value (null = none). A person's start follows the
 *  switch unless it opts out; over MCP an omitted key is the inherited value
 *  (a root: the caller's, none = off) and an explicit true that raises it
 *  ([`raisesComputerUse`]) needs `ownerStart`: [`assertAgentStart`] passed
 *  (#FEED-302). */
export function computerUseAtStart(args: {
  pick: boolean | undefined
  viaMcp: boolean
  caller: boolean | null
  from: boolean | null
  ownerStart?: boolean
}): boolean {
  const { pick, viaMcp, from } = args
  if (pick === false) return false
  if (!viaMcp) return pick ?? from ?? true
  if (raisesComputerUse(args)) {
    if (!args.ownerStart) refuse(args.caller === null ? `caller` : `owner`)
    return true
  }
  return from ?? args.caller ?? false
}

/** #FEED-302: the facts of [`agentStartVerdict`] for a start by `caller` as
 *  `starterId` on `deviceId` (host row of `ownerId`), read in the start's
 *  transaction; refuses unless allowed. */
export async function assertAgentStart(
  exec: Exec,
  args: {
    caller: Pick<typeof sessions.$inferSelect, `id` | `rootId` | `userId`> | null
    starterId: string
    deviceId: string
    ownerId: string
  }
): Promise<void> {
  const { caller, starterId } = args
  let ownTree = false
  if (caller && caller.userId === starterId) {
    const [root] =
      caller.rootId === caller.id
        ? [caller]
        : await exec.select({ userId: sessions.userId }).from(sessions).where(eq(sessions.id, caller.rootId)).limit(1)
    ownTree = root?.userId === starterId
  }
  const ownDevice = args.ownerId === starterId
  let switchOn = false
  if (caller && ownTree && ownDevice) {
    const [device] = await exec
      .select({ launchDefaults: devices.launchDefaults })
      .from(devices)
      .where(and(eq(devices.deviceId, args.deviceId), eq(devices.userId, args.ownerId)))
      .limit(1)
    switchOn = device?.launchDefaults?.computerUse === true
  }
  const verdict = agentStartVerdict({ caller: caller !== null, ownTree, ownDevice, switchOn })
  if (verdict !== `allowed`) refuse(verdict)
}

/** What a new session on `deviceId` stores, in its start's transaction:
 *  [`computerUseAtStart`], with [`assertAgentStart`] deciding an MCP raise.
 *  `raised` = that rule allowed it: the caller logs the start. */
export async function resolveComputerUseAtStart(
  exec: Exec,
  args: {
    pick: boolean | undefined
    viaMcp: boolean
    caller: Pick<typeof sessions.$inferSelect, `id` | `rootId` | `userId` | `computerUse`> | null
    from: boolean | null
    starterId: string
    deviceId: string
    ownerId: string
  }
): Promise<{ computerUse: boolean; raised: boolean }> {
  const rule = { pick: args.pick, viaMcp: args.viaMcp, caller: args.caller?.computerUse ?? null, from: args.from }
  const raised = raisesComputerUse(rule)
  if (raised) await assertAgentStart(exec, args)
  return { computerUse: computerUseAtStart({ ...rule, ownerStart: raised }), raised }
}

/** A pick for an EXISTING session (a message's, a configure's): unchanged
 *  when omitted, false always, true over MCP only where it changes nothing
 *  (a running session is never raised over MCP, #FEED-302 `running`). */
export function assertComputerUsePick(args: {
  pick: boolean | undefined
  viaMcp: boolean
  caller: boolean | null
  target: boolean
}): void {
  if (args.pick === true && args.viaMcp && !(args.target && (args.caller ?? false))) refuse(`running`)
}

/** The value to STORE for a session on this device: a build without
 *  [`COMPUTER_USE_SWITCH_CAP`] lets the row win over its switch, so it gets
 *  `wanted` AND the server's copy of the switch. */
export async function gatedComputerUse(
  exec: Exec,
  wanted: boolean,
  deviceId: string,
  ownerId: string
): Promise<boolean> {
  if (!wanted) return false
  const [row] = await exec
    .select({ caps: devices.caps, launchDefaults: devices.launchDefaults })
    .from(devices)
    .where(and(eq(devices.deviceId, deviceId), eq(devices.userId, ownerId)))
    .limit(1)
  if (!row) return false
  return (row.caps ?? []).includes(COMPUTER_USE_SWITCH_CAP) || row.launchDefaults?.computerUse === true
}

/** The switch of a build without the cap went off: its live sessions stop
 *  following, so their next relaunch gets no grant. */
export async function dropComputerUseOnDevice(tx: Writer, deviceId: string): Promise<void> {
  await tx
    .update(sessions)
    .set({ computerUse: false, updatedAt: new Date() })
    .where(and(eq(sessions.deviceId, deviceId), eq(sessions.computerUse, true), ne(sessions.status, `ended`)))
}

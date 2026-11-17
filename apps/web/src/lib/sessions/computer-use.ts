import { TRPCError } from "@trpc/server"
import { and, eq, ne } from "drizzle-orm"
import { db } from "@/db/connection"
import { devices, sessions } from "@/db/schema"
import composerMenu from "@exp/domain-contract/fixtures/composer-menu.json"

// #EXP-1899: the device's Computer use switch is the gate. A session's
// `computer_use` only says whether it FOLLOWS that switch (true) or opted
// out (false); the launcher grants while the switch is on AND the session
// follows (spec: composer-menu.json `computerUse`).

type Exec = Pick<typeof db, `select`>
type Writer = Pick<typeof db, `update`>

/** A build advertising this cap applies the gate itself; for any other the
 *  server stores the EFFECTIVE value ([`gatedComputerUse`]). */
export const COMPUTER_USE_SWITCH_CAP: string = composerMenu.computerUse.switchCap

export const COMPUTER_USE_MCP_REFUSAL = `Computer use is off for this session: only a person turns it on, in the session's menu. Over MCP it can only be turned off.`

const refuse = (): never => {
  throw new TRPCError({ code: `FORBIDDEN`, message: COMPUTER_USE_MCP_REFUSAL })
}

/** What a NEW session stores. `from` = the row it inherits from (a child's
 *  parent, a continuation's ended row; null = a new root), `caller` = the
 *  calling session's own value (null = none). A person's start follows the
 *  switch unless it opts out; over MCP nothing is raised: an omitted key is
 *  the inherited value (a root: the caller's, none = off) and an explicit
 *  true is refused when the caller or `from` is off. */
export function computerUseAtStart(args: {
  pick: boolean | undefined
  viaMcp: boolean
  caller: boolean | null
  from: boolean | null
}): boolean {
  const { pick, viaMcp, from } = args
  if (pick === false) return false
  if (!viaMcp) return pick ?? from ?? true
  const caller = args.caller ?? false
  if (pick === true && !(caller && (from ?? true))) refuse()
  return from ?? caller
}

/** A pick for an EXISTING session (a message's, a configure's): unchanged
 *  when omitted, false always, true over MCP only where it changes nothing. */
export function assertComputerUsePick(args: {
  pick: boolean | undefined
  viaMcp: boolean
  caller: boolean | null
  target: boolean
}): void {
  if (args.pick === true && args.viaMcp && !(args.target && (args.caller ?? false))) refuse()
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

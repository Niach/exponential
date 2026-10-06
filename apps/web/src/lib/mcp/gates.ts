// EXP-660: per-request tool gates. The MCP server is rebuilt on every POST
// (stateless transport), so what a client sees in tools/list can depend on
// the caller — and a tool an agent can never use is pure context noise once
// it has searched for it. Registration stays context hygiene, NOT the
// security boundary — the routers hold membership and ownership. (The first
// gate, helpdesk, went with the helpdesk in SLOP-4.)
//
// EXP-679 / EXP-1222: the second gate is sessionsEnd. It opens for EVERY run
// of the caller's (a person may tell the agent "end this run"), not just the
// unattended ones it used to (an attended run on a pre-0.14.63 daemon is the
// one TEMPORARY exception, see the shim below). `unattended` (`started_reason`
// set) rides next to it: it only picks the instructions' wording — an
// unattended run closes out with the tool LAST, an attended one calls it
// only when asked. Registration stays context hygiene: `endSessionByAgent`
// remains the authority — it ends the run for whoever reaches it.
//
// EXP-879: the fourth gate is sessionResults. Publishing a screenshot is an
// act ON this run, so it needs the same owner-or-host header check — but
// nothing beyond it: an attended run's pictures are exactly as useful as an
// automation's. A caller with no run of its own (a human's MCP client) has
// nothing to publish on, so the tool never registers for it.
//
// EXP-700 / EXP-1089: the third gate is askParent. It used to open only for
// a run another run started; since EXP-1089 EVERY run of the caller's gets
// the tool (tools are lazy-loaded, an unused one costs nothing): `to: 'user'`
// asks the person who owns the run from any run. `parent` relays to a live
// starter run when there is one, which the handler checks (the parent stamps
// `parent_session_id` only after its sessions_start poll returns, so linkage
// is never part of the gate); EXP-1216: with none it parks the question on
// the row for an MCP-client starter instead of failing.
import { and, eq, sql } from "drizzle-orm"
import { db } from "@/db/connection"
import { codingSessions, devices } from "@/db/schema"
import { parseVersionTuple } from "@/lib/client-version"
import type { McpAccess } from "./scope"

export interface McpToolGates {
  /** EXP-1222: the caller runs INSIDE a coding session of its own (owner or
   * host, any `started_reason`) — it may end it with
   * `exponential_sessions_end`. Temporarily off for an ATTENDED run whose
   * daemon is older than 0.14.63 (its playbook would end it at close-out). */
  sessionsEnd: boolean
  /** EXP-1222: that run was started by a trigger or another agent
   * (`started_reason` set) — nobody is watching, so the instructions tell it
   * to close out with `exponential_sessions_end` LAST. Wording only, never a
   * registration gate. */
  unattended: boolean
  /** EXP-700 / EXP-1089: the caller runs INSIDE a coding session of its own
   * (owner or host, any `started_reason`) — it may ask a question via
   * `exponential_sessions_ask_parent`: its starter, or the person (`to:
   * 'user'`). Who may be asked is the handler's check, not the gate's. */
  askParent: boolean
  /** EXP-879: the caller runs INSIDE a coding session of its own (owner or
   * host, any `started_reason`) — it may publish screenshots of its work with
   * `exponential_sessions_results`. A human's MCP client has no run to publish
   * on, so the tool stays out of its surface entirely. */
  sessionResults: boolean
}

/** The worst-case surface — the default `registerExponentialTools` takes, so
 * the tests and the context budget measure EVERY tool. The route passes the
 * resolved value; nothing else should. */
export const ALL_MCP_TOOL_GATES: McpToolGates = {
  sessionsEnd: true,
  unattended: true,
  askParent: true,
  sessionResults: true,
}

export async function resolveMcpToolGates(
  userId: string,
  // The OAuth grant — no gate reads it since SLOP-4 (the helpdesk gate did);
  // kept so the route's call shape and a future grant-scoped gate stay put.
  _access: McpAccess,
  // EXP-679: the coding_sessions row this request runs inside (null for a
  // human's MCP client, which never gets the session tools).
  sessionId: string | null = null
): Promise<McpToolGates> {
  return await resolveSessionGates(userId, sessionId)
}

// Compat shim (release train 2026-10-06, EXP-1222 follow-up): desktop/CLI
// 0.14.61 and 0.14.62 inject the OLD playbook, whose close-out reads "if
// `exponential_sessions_end` is registered, it is your last call". With
// EXP-1222 registering the tool for every run, a PERSON-started run on such
// a daemon would end itself at close-out. So an attended run gets the tool
// only when its host device reports a version >= 0.14.63 (the first
// playbook that calls it only when asked); a missing or unparseable version
// counts as OLD. Unattended runs always get it (their close-out IS the
// tool). Delete once CLIENT_MIN_VERSION_DESKTOP and _CLI are >= 0.14.63.
const FIRST_VERSION_ATTENDED_SESSIONS_END: [number, number, number] = [
  0, 14, 63,
]

function attendedRunMayEnd(deviceVersion: string | null): boolean {
  if (!deviceVersion) return false
  const version = parseVersionTuple(deviceVersion)
  if (!version) return false
  for (let i = 0; i < 3; i++) {
    if (version[i] !== FIRST_VERSION_ATTENDED_SESSIONS_END[i]) {
      return version[i]! > FIRST_VERSION_ATTENDED_SESSIONS_END[i]!
    }
  }
  return true
}

/** One indexed lookup for all the session-header gates: the header's run
 * must exist and belong to the caller (owner or host — the same pair
 * `endSessionByAgent` accepts). `askParent` (EXP-1089) and `sessionResults`
 * (EXP-879) need nothing more — any run of the caller's may ask a question
 * or publish screenshots of its own work, attended or not. `unattended`
 * reads the row's `started_reason`; `sessionsEnd` (EXP-1222) follows it,
 * except that an ATTENDED run on an old daemon stays without the tool (the
 * shim above), which is why the host's devices row rides along (the HOST's
 * row: a shared-device run is requester-owned with `host_user_id` = the
 * device owner, EXP-432). */
async function resolveSessionGates(
  userId: string,
  sessionId: string | null
): Promise<McpToolGates> {
  const closed = {
    sessionsEnd: false,
    unattended: false,
    askParent: false,
    sessionResults: false,
  }
  if (!sessionId) return closed
  const [row] = await db
    .select({
      userId: codingSessions.userId,
      hostUserId: codingSessions.hostUserId,
      startedReason: codingSessions.startedReason,
      deviceVersion: devices.version,
    })
    .from(codingSessions)
    .leftJoin(
      devices,
      and(
        eq(
          devices.userId,
          sql`coalesce(${codingSessions.hostUserId}, ${codingSessions.userId})`
        ),
        eq(devices.deviceId, codingSessions.deviceId)
      )
    )
    .where(eq(codingSessions.id, sessionId))
    .limit(1)
  if (!row) return closed
  if (row.userId !== userId && row.hostUserId !== userId) return closed
  const unattended = row.startedReason !== null
  return {
    sessionsEnd: unattended || attendedRunMayEnd(row.deviceVersion ?? null),
    unattended,
    askParent: true,
    sessionResults: true,
  }
}

// EXP-660: per-request tool gates. The MCP server is rebuilt on every POST
// (stateless transport), so what a client sees in tools/list can depend on
// the caller — and a tool an agent can never use is pure context noise once
// it has searched for it. Registration stays context hygiene, NOT the
// security boundary — lib/sessions/** and the routers hold visibility and
// ownership.
//
// EXP-1291: ONE header gate, `inSession` — the X-Exp-Session-Id row exists
// and is the caller's own (its starter or the device owner hosting it). It
// registers the tools that act on the caller's OWN session
// (`exponential_guide_write`, `exponential_sessions_compact`). `unattended`
// and `agent` only pick the instructions' wording: an unattended session (a
// child, or a root another live session watches: its spawner) closes out with
// `exponential_sessions_end` LAST, an attended one only when asked, and a
// team agent's conversation (its root carries `agent_id`) never ends.
import { and, eq, sql } from "drizzle-orm"
import { alias } from "drizzle-orm/pg-core"
import { db } from "@/db/connection"
import { devices, sessions } from "@/db/schema"
import { parseVersionTuple } from "@/lib/client-version"
import { isSessionPrincipal, sessionDeviceOwner } from "@/lib/sessions/host"
import type { McpAccess } from "./scope"

export interface McpToolGates {
  /** The caller runs INSIDE a session of its own (starter or host). */
  inSession: boolean
  /** That session is a child or a root another session spawned: nobody is
   * watching. Wording only, never a registration gate. */
  unattended: boolean
  /** That session's root is a team agent's conversation (it never ends).
   * Wording only. */
  agent: boolean
  /** EXP-1280: that session is still planning (`sessions.plan_mode`), so the
   * instructions carry the plan-mode gate. Wording only. */
  planMode?: boolean
  /** EXP-1280: that session's host device feeds a card the session posts on
   * ITSELF back as its next message (`hostFeedsOwnCards`). False drops the
   * registration of `exponential_plan_propose` / `exponential_sessions_ask`
   * and their instructions sentence. Absent = true. */
  cards?: boolean
}

/** The first desktop/CLI that feeds a session its OWN answered cards. */
const OWN_CARDS_MIN_VERSION: readonly [number, number, number] = [0, 14, 73]

/** EXP-1280: a desktop/CLI before 0.14.73 feeds a session only its pending
 * inputs and open cards: a card the session authored itself is neither, so
 * an agent that posts one and ends its turn waits forever. Its host's
 * reported version decides; an unreported or unparseable one fails OPEN
 * (the client-version rule). */
export function hostFeedsOwnCards(version: string | null | undefined): boolean {
  if (!version) return true
  const have = parseVersionTuple(version)
  if (!have) return true
  for (let i = 0; i < 3; i++) {
    if (have[i]! !== OWN_CARDS_MIN_VERSION[i]!) return have[i]! > OWN_CARDS_MIN_VERSION[i]!
  }
  return true
}

/** The worst-case surface — the default `registerExponentialTools` takes, so
 * the tests and the context budget measure EVERY tool. The route passes the
 * resolved value; nothing else should. */
export const ALL_MCP_TOOL_GATES: McpToolGates = {
  inSession: true,
  unattended: true,
  agent: false,
  planMode: true,
  cards: true,
}

const CLOSED: McpToolGates = { inSession: false, unattended: false, agent: false }

/** One indexed lookup (the row + its root): the header's session must exist
 * and belong to the caller. */
export async function resolveMcpToolGates(
  userId: string,
  // The OAuth grant — no gate reads it since SLOP-4; kept so the route's
  // call shape and a future grant-scoped gate stay put.
  _access: McpAccess,
  sessionId: string | null = null
): Promise<McpToolGates> {
  if (!sessionId) return CLOSED
  const root = alias(sessions, `root`)
  const [row] = await db
    .select({
      userId: sessions.userId,
      teamId: sessions.teamId,
      deviceId: sessions.deviceId,
      parentId: sessions.parentId,
      rootAgentId: root.agentId,
      planMode: sessions.planMode,
      // NEW-1: a live session watches this one (its spawner): nobody is
      // watching in person.
      watched: sql<boolean>`EXISTS (SELECT 1 FROM sessions w WHERE w.status <> 'ended' AND w.wake_on @> jsonb_build_array(${sessions.id}::text))`,
    })
    .from(sessions)
    .leftJoin(root, eq(root.id, sessions.rootId))
    .where(eq(sessions.id, sessionId))
    .limit(1)
  if (!row) return CLOSED
  if (!(await isSessionPrincipal(userId, row))) return CLOSED
  // The host device's row (devices are per (user, deviceId)): its reported
  // version says whether it feeds the session's own cards (`cards`).
  const [host] = await db
    .select({ version: devices.version })
    .from(devices)
    .where(and(eq(devices.deviceId, row.deviceId), eq(devices.userId, await sessionDeviceOwner(row))))
    .limit(1)
  return {
    inSession: true,
    unattended: row.parentId !== null || row.watched === true,
    agent: row.rootAgentId !== null,
    planMode: row.planMode === true,
    cards: hostFeedsOwnCards(host?.version),
  }
}

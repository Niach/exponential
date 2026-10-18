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
// child, or a root another session spawned) closes out with
// `exponential_sessions_end` LAST, an attended one only when asked, and a
// team agent (its root is `kind = agent`) never ends.
import { eq } from "drizzle-orm"
import { alias } from "drizzle-orm/pg-core"
import { db } from "@/db/connection"
import { sessions } from "@/db/schema"
import type { McpAccess } from "./scope"

export interface McpToolGates {
  /** The caller runs INSIDE a session of its own (starter or host). */
  inSession: boolean
  /** That session is a child or a root another session spawned: nobody is
   * watching. Wording only, never a registration gate. */
  unattended: boolean
  /** That session's root is a team agent (it never ends). Wording only. */
  agent: boolean
  /** EXP-1280: that session is still planning (`sessions.plan_mode`), so the
   * instructions carry the plan-mode gate. Wording only. */
  planMode?: boolean
}

/** The worst-case surface — the default `registerExponentialTools` takes, so
 * the tests and the context budget measure EVERY tool. The route passes the
 * resolved value; nothing else should. */
export const ALL_MCP_TOOL_GATES: McpToolGates = {
  inSession: true,
  unattended: true,
  agent: false,
  planMode: true,
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
      hostUserId: sessions.hostUserId,
      parentId: sessions.parentId,
      rootKind: root.kind,
      rootSpawnedBy: root.spawnedBySessionId,
      planMode: sessions.planMode,
    })
    .from(sessions)
    .leftJoin(root, eq(root.id, sessions.rootId))
    .where(eq(sessions.id, sessionId))
    .limit(1)
  if (!row) return CLOSED
  if (row.userId !== userId && row.hostUserId !== userId) return CLOSED
  return {
    inSession: true,
    unattended: row.parentId !== null || row.rootSpawnedBy != null,
    agent: row.rootKind === `agent`,
    planMode: row.planMode === true,
  }
}

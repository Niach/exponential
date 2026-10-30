import { editorBrief } from "@exp/domain-contract/apps"
import { db } from "@/db/connection"
import { findConversation, loadAgentForUser } from "@/lib/agents/conversation"
import { findEditorSession, loadAppForUser } from "@/lib/apps/editor-session"

/** EXP-1458: where a `sessions.send {agentId|appId}` lands, so the writer
 *  can seal it: the caller's conversation root with the agent / editor
 *  root with the app (null = the send creates it) and that root's current
 *  key (null = keyless: the send carries `seal`). `brief` = the app brief
 *  the server would prefix to a plaintext text (a new or ended editor
 *  session): a sealed text is the writer's whole message, so it composes
 *  `brief + "\n\n" + text` itself (apps/editor-session.ts `briefedText`). */
export interface SendTarget {
  rootId: string | null
  epoch: number | null
  publicKey: string | null
  brief: string | null
}

export async function sendTarget(userId: string, input: { agentId?: string; appId?: string }): Promise<SendTarget> {
  if (input.agentId) {
    const agent = await loadAgentForUser(userId, input.agentId)
    const root = await findConversation(db, agent.id, userId)
    return { ...keyOf(root), brief: null }
  }
  const app = await loadAppForUser(userId, input.appId!)
  const root = await findEditorSession(db, app.id, userId)
  return { ...keyOf(root), brief: !root || root.status === `ended` ? editorBrief(app) : null }
}

function keyOf(
  root: { id: string; sealEpoch: number | null; sealPublicKey: string | null } | null
): Omit<SendTarget, `brief`> {
  if (!root) return { rootId: null, epoch: null, publicKey: null }
  const keyed = root.sealEpoch != null && root.sealPublicKey != null
  return { rootId: root.id, epoch: keyed ? root.sealEpoch : null, publicKey: keyed ? root.sealPublicKey : null }
}

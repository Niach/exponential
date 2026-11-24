import { editorBrief } from "@exp/domain-contract/apps"
import { db } from "@/db/connection"
import { findConversation, loadAgentForUser } from "@/lib/agents/conversation"
import { findEditorSession, loadAppForUser } from "@/lib/apps/editor-session"
import { vouchesFor, type Vouch } from "@/lib/sessions/wrap-signatures"

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
  /** #EDE-97: the root is an outside MCP client's tree (in clear for life):
   *  the writer sends plaintext, never a key. */
  clearTree: boolean
  /** #EXP-1941: the server flagged this root's key for rotation (the tree
   *  turned private: readers it dropped still hold the current epoch). A
   *  call does not start on such a key: the host rotates the root before
   *  its hello, and a caller whose answer still says so refuses the call.
   *  Older clients ignore the field. */
  rotationDue: boolean
  /** #EXP-1950: a signed wrap of the current epoch for some install, which a
   *  writer holding no secret of it verifies the key with (`acceptVouch`);
   *  null when none is signed. Older clients ignore the field. */
  vouch: Vouch | null
}

export async function sendTarget(userId: string, input: { agentId?: string; appId?: string }): Promise<SendTarget> {
  if (input.agentId) {
    const agent = await loadAgentForUser(userId, input.agentId)
    const root = await findConversation(db, agent, userId)
    return { ...keyOf(root), brief: null, vouch: await vouchOf(keyOf(root)) }
  }
  const app = await loadAppForUser(userId, input.appId!)
  const root = await findEditorSession(db, app.id, userId)
  return { ...keyOf(root), brief: !root || root.status === `ended` ? editorBrief(app) : null, vouch: await vouchOf(keyOf(root)) }
}

async function vouchOf(key: { rootId: string | null; epoch: number | null }): Promise<Vouch | null> {
  if (!key.rootId || key.epoch === null) return null
  return (await vouchesFor(db, [{ id: key.rootId, sealEpoch: key.epoch }])).get(key.rootId) ?? null
}

function keyOf(
  root: { id: string; sealEpoch: number | null; sealPublicKey: string | null; sealRotateEpoch: number | null; clearTree: boolean } | null
): Omit<SendTarget, `brief` | `vouch`> {
  if (!root) return { rootId: null, epoch: null, publicKey: null, clearTree: false, rotationDue: false }
  const keyed = root.sealEpoch != null && root.sealPublicKey != null
  return {
    rootId: root.id,
    epoch: keyed ? root.sealEpoch : null,
    publicKey: keyed ? root.sealPublicKey : null,
    clearTree: root.clearTree,
    rotationDue: keyed && root.sealRotateEpoch != null,
  }
}

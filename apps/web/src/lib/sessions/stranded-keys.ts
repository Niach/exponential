// #EXP-2123 (install-trust.json `keyMaker`, a STRANDED root): the repair for
// chats keyed or rotated by an install that waited for approval BEFORE the
// server refused that. Such an install wrapped the key for itself only, so
// no approved install and no device can open the chat. The server never
// sees a secret and cannot rotate: it FLAGS the root (`seal_rotate_epoch`),
// `sessionKeys.due` names it `stranded` to every approved reader, and the
// first one online makes the next epoch. Router-free (scripts + suites).
import { and, eq, inArray, isNotNull, isNull } from "drizzle-orm"
import { parseBodyEnvelope, parseSealedValue } from "@exp/domain-contract/sealed-content"
import { installs, messages, sessionKeys, sessions } from "@/db/schema"
import { waitingInstallIds } from "@/lib/install-approvals"
import type { Tx } from "@/lib/sessions/tx"
import type { Reader } from "@/lib/sessions/tree-keys"

export interface StrandedRoot {
  rootId: string
  teamId: string
  epoch: number
  /** `waiting` = its current epoch is held only by installs that wait for
   *  approval (approving one lets it wrap the key for the readers, nothing
   *  is lost); `noHolder` = by no live install at all. */
  kind: `waiting` | `noHolder`
  /** The waiting installs holding the current epoch. */
  holders: string[]
  /** Already due (`seal_rotate_epoch` set). */
  flagged: boolean
  /** Messages sealed at an epoch no approved install holds: they stay
   *  readable only where that secret is (the waiting install), if anywhere. */
  messages: number
}

const MESSAGE_CHUNK = 200

/** Every keyed root whose CURRENT epoch no live, approved install holds a wrap of. */
export async function findStrandedKeys(database: Reader): Promise<StrandedRoot[]> {
  const roots = (await database.select().from(sessions).where(and(isNull(sessions.parentId), isNotNull(sessions.sealEpoch)))).filter(
    (root) => root.parentId === null && root.sealEpoch !== null && !root.clearTree
  )
  if (roots.length === 0) return []
  const wraps = await database.select({ rootId: sessionKeys.rootId, epoch: sessionKeys.epoch, installId: sessionKeys.installId }).from(sessionKeys)
  const live = (await database.select().from(installs).where(isNull(installs.revokedAt))).filter((row) => !row.revokedAt)
  const liveIds = new Set(live.map((row) => row.id))
  const holderIds = new Set(wraps.map((row) => row.installId))
  const waiting = await waitingInstallIds(
    database,
    live.filter((row) => holderIds.has(row.id))
  )
  /** root → the epochs a live, approved install holds. */
  const approved = new Map<string, Set<number>>()
  /** root → the waiting installs holding each epoch. */
  const waitingAt = new Map<string, Map<number, string[]>>()
  for (const wrap of wraps) {
    if (!liveIds.has(wrap.installId)) continue
    if (waiting.has(wrap.installId)) {
      const epochs = waitingAt.get(wrap.rootId) ?? new Map<number, string[]>()
      epochs.set(wrap.epoch, [...(epochs.get(wrap.epoch) ?? []), wrap.installId])
      waitingAt.set(wrap.rootId, epochs)
    } else approved.set(wrap.rootId, (approved.get(wrap.rootId) ?? new Set()).add(wrap.epoch))
  }
  const stranded = roots.filter((root) => !approved.get(root.id)?.has(root.sealEpoch!))
  const lost = new Map<string, number>()
  for (let at = 0; at < stranded.length; at += MESSAGE_CHUNK) {
    const ids = stranded.slice(at, at + MESSAGE_CHUNK).map((root) => root.id)
    const rows = await database.select({ rootId: messages.rootId, body: messages.body, card: messages.card }).from(messages).where(inArray(messages.rootId, ids))
    for (const row of rows) {
      const body = parseBodyEnvelope(row.body)
      const sealedCard = (row.card as { sealed?: unknown } | null)?.sealed
      const card = typeof sealedCard === `string` ? parseSealedValue(sealedCard) : null
      const epochs = [body, card].flatMap((value) => (value?.kind === `sealed` ? [value.epoch] : []))
      if (epochs.some((epoch) => !approved.get(row.rootId)?.has(epoch))) lost.set(row.rootId, (lost.get(row.rootId) ?? 0) + 1)
    }
  }
  return stranded
    .map((root) => {
      const holders = waitingAt.get(root.id)?.get(root.sealEpoch!) ?? []
      return {
        rootId: root.id,
        teamId: root.teamId,
        epoch: root.sealEpoch!,
        kind: holders.length > 0 ? (`waiting` as const) : (`noHolder` as const),
        holders,
        flagged: root.sealRotateEpoch !== null,
        messages: lost.get(root.id) ?? 0,
      }
    })
    .sort((a, b) => (a.rootId < b.rootId ? -1 : a.rootId > b.rootId ? 1 : 0))
}

/** Flags `roots` for rotation at the epoch found (a root that moved on
 *  meanwhile is left alone); the count newly flagged. */
export async function flagStrandedKeysInTx(tx: Tx, roots: readonly Pick<StrandedRoot, `rootId` | `epoch`>[]): Promise<number> {
  let flagged = 0
  for (const root of roots) {
    const rows = await tx
      .update(sessions)
      .set({ sealRotateEpoch: root.epoch })
      .where(and(eq(sessions.id, root.rootId), eq(sessions.sealEpoch, root.epoch), isNull(sessions.sealRotateEpoch)))
      .returning({ id: sessions.id })
    flagged += rows.length
  }
  return flagged
}

// #EXP-1958 (fixtures/sealed-content.json `cardStamps`): THE web path for a
// tap on a replies / suggestions card. `cardItemWire` decides by the STORED
// card: an unsealed one keeps the server actions (the server composes the
// text); a sealed one this tab opened sends the item's text SEALED through
// the composer's writer (`sendSessionMessage` + `cardItem`) or names the
// opened ids / text to the stamp actions; a sealed one it cannot open is
// refused here, nothing leaves.

import { cardItemWire, type CardItemAction } from "@exp/domain-contract/sealed-content"
import { suggestionRequest } from "@/lib/sessions/suggestion-card"
import { issueCollection, messageCollection } from "@/lib/collections"
import { SKIP_ERROR_TOAST } from "@/lib/action-error"
import { trpc } from "@/lib/trpc-client"
import { currentKeyHolder } from "./current-key-holder"
import { NoKeyError } from "./seal-send"
import { sendSessionMessage } from "./send-sealed"
import { openCardCached } from "./sealed-open"

export type CardTap = {
  action: CardItemAction
  messageId: string
  itemId?: string
  /** Start: the runner picks. */
  deviceId?: string
  account?: string
  model?: string
}

type Row = { id: string; sessionId: string; rootId: string; teamId?: string; card: Record<string, unknown> | null }
type Item = {
  id: string
  text?: string
  title?: string
  issueIds?: unknown
  sent?: boolean
  here?: boolean
  dismissed?: boolean
  sessionId?: string
  issueId?: string
}

const UUID = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i

const handled = (item: Item) => !!(item.sent || item.here || item.dismissed || item.sessionId || item.issueId)

/** An item's issue ids as uuids: an `#IDENT` the issue store knows resolves,
 *  anything else is dropped. */
function itemIssueIds(item: Item, teamId: string | undefined): string[] | undefined {
  if (!Array.isArray(item.issueIds)) return undefined
  const out = new Set<string>()
  for (const raw of item.issueIds) {
    if (typeof raw !== `string`) continue
    if (UUID.test(raw)) {
      out.add(raw)
      continue
    }
    const ident = raw.replace(/^#/, ``).toUpperCase()
    for (const row of issueCollection.values()) {
      if ((!teamId || row.teamId === teamId) && row.identifier.toUpperCase() === ident) out.add(row.id)
    }
  }
  return out.size > 0 ? [...out] : undefined
}

function serverCall(tap: CardTap): Promise<unknown> {
  const { messageId } = tap
  const itemId = tap.itemId ?? ``
  switch (tap.action) {
    case `reply`:
      return trpc.sessions.sendReply.mutate({ messageId, itemId }, SKIP_ERROR_TOAST)
    case `here`:
      return trpc.sessions.startSuggestion.mutate({ messageId, itemId, where: `here` }, SKIP_ERROR_TOAST)
    case `start`: {
      const { deviceId, account, model } = tap
      return trpc.sessions.startSuggestion.mutate(
        { messageId, itemId, where: `child`, ...(deviceId ? { deviceId } : {}), ...(account ? { account } : {}), ...(model ? { model } : {}) },
        SKIP_ERROR_TOAST
      )
    }
    case `startAll`:
      return trpc.sessions.startAllSuggestions.mutate({ messageId }, SKIP_ERROR_TOAST)
    case `dismiss`:
      return trpc.sessions.dismissSuggestion.mutate({ messageId, itemId }, SKIP_ERROR_TOAST)
    case `dismissReplies`:
      return trpc.sessions.dismissReplies.mutate({ messageId }, SKIP_ERROR_TOAST)
    case `file`:
      return trpc.sessions.fileSuggestion.mutate({ messageId, itemId }, SKIP_ERROR_TOAST)
  }
}

/** A tap on a replies / suggestions card, on the wire its stored card takes. */
export async function tapCardItem(tap: CardTap): Promise<unknown> {
  const row = messageCollection.get(tap.messageId) as Row | undefined
  const sealed = !!row?.card && `sealed` in row.card
  const holder = currentKeyHolder()
  const opened = sealed && holder ? openCardCached(row!.card!, holder.keysFor(row!.rootId)) : null
  const { wire } = cardItemWire({ sealed, opened: opened?.status === `opened`, action: tap.action })
  if (wire === `server`) return serverCall(tap)
  if (wire === `refused`) throw new NoKeyError()

  const items = (Array.isArray(opened!.card.items) ? opened!.card.items : []) as Item[]
  const item = tap.itemId ? items.find((entry) => entry.id === tap.itemId) : undefined
  const { messageId } = tap
  const sessionId = row!.sessionId
  const start = (each: Item) =>
    sendSessionMessage(
      {
        parentId: sessionId,
        text: suggestionRequest({ title: each.title ?? ``, text: each.text ?? `` }),
        ...(each.title ? { name: each.title } : {}),
        ...(itemIssueIds(each, row!.teamId) ? { issueIds: itemIssueIds(each, row!.teamId) } : {}),
        ...(tap.deviceId ? { deviceId: tap.deviceId } : {}),
        ...(tap.account ? { account: tap.account } : {}),
        ...(tap.model ? { model: tap.model } : {}),
      },
      SKIP_ERROR_TOAST,
      { cardItem: { messageId, itemId: each.id } }
    )
  if (tap.action === `startAll`) {
    const results = []
    // One send per open item, in card order (the tree caps refuse as-is).
    for (const each of items.filter((entry) => !handled(entry))) results.push(await start(each))
    return results
  }
  if (tap.action === `dismissReplies`) {
    const itemIds = items.filter((entry) => !entry.sent).map((entry) => entry.id)
    if (itemIds.length === 0) return null
    return trpc.sessions.dismissReplies.mutate({ messageId, itemIds }, SKIP_ERROR_TOAST)
  }
  if (!item) throw new Error(`This item is no longer on the card`)
  switch (tap.action) {
    case `reply`:
      return sendSessionMessage({ sessionId, text: item.text ?? `` }, SKIP_ERROR_TOAST, { cardItem: { messageId, itemId: item.id } })
    case `here`:
      return sendSessionMessage(
        { sessionId, text: suggestionRequest({ title: item.title ?? ``, text: item.text ?? `` }) },
        SKIP_ERROR_TOAST,
        { cardItem: { messageId, itemId: item.id } }
      )
    case `start`:
      return start(item)
    case `dismiss`:
      return trpc.sessions.dismissSuggestion.mutate({ messageId, itemId: item.id }, SKIP_ERROR_TOAST)
    case `file`:
      return trpc.sessions.fileSuggestion.mutate(
        { messageId, itemId: item.id, title: item.title ?? ``, text: item.text ?? `` },
        SKIP_ERROR_TOAST
      )
  }
}

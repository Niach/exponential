import { useEffect, useState } from "react"
import {
  CHATS_COPY,
  chatDeleteTitle,
  chatSelectionActions,
  chatSelectionFailedTitle,
  type ChatListView,
  type ChatSelectionAction,
  type ChatSelectionRow,
} from "@exp/domain-contract/chats"
import {
  BulkBarAction,
  BulkBarCapsule,
  BulkBarSeparator,
  CHAT_SELECTION_ICONS,
  Prompt,
  toastActionError,
} from "@exp/ui"
import { BulkBarDock } from "@/components/bulk-action-bar"
import { actionErrorDescription, SKIP_ERROR_TOAST } from "@/lib/action-error"
import { sessionCollection, sessionReadsCollection } from "@/lib/collections"
import { trpc } from "@/lib/trpc-client"

// #EXP-1388 (chats.json SELECTION + ARCHIVE): the Chats list's selection bar
// = THE issue list's bulk bar (the same `BulkBarCapsule` in the same
// `BulkBarDock`): close, the count (`copy.selection.count`, the bare
// number like the issue bar), then `chatSelectionActions` as icon buttons
// (labelled on md+). Mark read / unread = `sessions.setRead`,
// Archive / Unarchive = `sessions.setArchived` (batched, 200 ids a call),
// Stop = `sessions.end` per chat, Delete asks first (`chatDeleteTitle`) then
// `sessions.delete` per chat, at most `IN_FLIGHT` at once (each ends a tree
// and relays to devices). Every action leaves the selection; a failure toasts
// `chatSelectionFailedTitle` ("{n} of {m} …" when only some failed). Escape
// leaves it too.

const COPY = CHATS_COPY
const BATCH = 200
const IN_FLIGHT = 4

/** A selection action that failed for `failed` of its `total` chats. */
class SelectionError extends Error {
  constructor(
    readonly failed: number,
    readonly total: number,
    readonly first: unknown
  ) {
    super(`selection failed`)
  }
}

function batches(ids: readonly string[]): string[][] {
  const out: string[][] = []
  for (let i = 0; i < ids.length; i += BATCH) out.push(ids.slice(i, i + BATCH))
  return out
}

/** Runs one bar action over its chats; resolves once Electric echoed the
 *  last write (bounded, like the chat menu's). */
async function runChatAction(action: ChatSelectionAction, sessionIds: readonly string[]): Promise<void> {
  if (action === `markRead` || action === `markUnread`) {
    let txid: number | undefined
    for (const ids of batches(sessionIds)) {
      ;({ txid } = await trpc.sessions.setRead.mutate({ sessionIds: ids, read: action === `markRead` }, SKIP_ERROR_TOAST))
    }
    if (txid !== undefined) await sessionReadsCollection.utils.awaitTxId(txid, 3000).catch(() => undefined)
    return
  }
  if (action === `archive` || action === `unarchive`) {
    let txid: number | undefined
    for (const ids of batches(sessionIds)) {
      ;({ txid } = await trpc.sessions.setArchived.mutate(
        { sessionIds: ids, archived: action === `archive` },
        SKIP_ERROR_TOAST
      ))
    }
    if (txid !== undefined) await sessionReadsCollection.utils.awaitTxId(txid, 3000).catch(() => undefined)
    return
  }
  // Stop / Delete: one call per chat, `IN_FLIGHT` workers; every one is
  // tried, then the failures are counted.
  let next = 0
  let last: number | undefined
  const failures: unknown[] = []
  const worker = async () => {
    while (next < sessionIds.length) {
      const sessionId = sessionIds[next++]
      try {
        const { txid } =
          action === `stop`
            ? await trpc.sessions.end.mutate({ sessionId }, SKIP_ERROR_TOAST)
            : await trpc.sessions.delete.mutate({ sessionId }, SKIP_ERROR_TOAST)
        last = Math.max(last ?? txid, txid)
      } catch (error) {
        failures.push(error)
      }
    }
  }
  await Promise.all(Array.from({ length: Math.min(IN_FLIGHT, sessionIds.length) }, worker))
  if (last !== undefined) await sessionCollection.utils.awaitTxId(last, 3000).catch(() => undefined)
  if (failures.length > 0) throw new SelectionError(failures.length, sessionIds.length, failures[0])
}

export function ChatSelectionBar({
  rows,
  view,
  onClear,
}: {
  /** The selected chats, in selection order. */
  rows: readonly ChatSelectionRow[]
  view: ChatListView
  onClear: () => void
}) {
  const [busy, setBusy] = useState(false)
  const [deleting, setDeleting] = useState<string[] | null>(null)
  const actions = chatSelectionActions({ rows, view })

  // Escape leaves the selection (the delete confirm owns it while open).
  useEffect(() => {
    if (deleting) return
    const onKey = (event: KeyboardEvent) => {
      if (event.key === `Escape` && !event.defaultPrevented) onClear()
    }
    window.addEventListener(`keydown`, onKey)
    return () => window.removeEventListener(`keydown`, onKey)
  }, [deleting, onClear])

  const run = async (action: ChatSelectionAction, sessionIds: readonly string[]) => {
    if (busy) return
    setBusy(true)
    try {
      await runChatAction(action, sessionIds)
    } catch (error) {
      const title =
        error instanceof SelectionError ? chatSelectionFailedTitle(error.failed, error.total) : COPY.toasts.selectionFailed
      toastActionError(title, {
        description: actionErrorDescription(error instanceof SelectionError ? error.first : error),
      })
    } finally {
      setBusy(false)
      setDeleting(null)
      onClear()
    }
  }

  return (
    <BulkBarDock>
      <BulkBarCapsule
        count={COPY.selection.count.replace(`{n}`, String(rows.length))}
        onClear={onClear}
      >
        {actions.map(({ action, sessionIds }) => {
          const Icon = CHAT_SELECTION_ICONS[action]
          const destructive = action === `delete`
          return (
            <span key={action} className="contents">
              {destructive && <BulkBarSeparator />}
              <BulkBarAction
                icon={<Icon className="size-4" />}
                label={COPY.selection[action]}
                destructive={destructive}
                disabled={busy}
                data-testid={`chat-selection-${action}`}
                onClick={() => (destructive ? setDeleting(sessionIds) : void run(action, sessionIds))}
              />
            </span>
          )
        })}
        <Prompt
          open={deleting !== null}
          onOpenChange={(open) => {
            if (!open && !busy) setDeleting(null)
          }}
          busy={busy}
          title={chatDeleteTitle(deleting?.length ?? rows.length)}
          body={COPY.confirmDelete.body}
          data-testid="chat-delete-confirm"
          actions={[
            { label: `Cancel`, role: `cancel` },
            {
              label: COPY.menu.delete,
              role: `destructive`,
              busy,
              onSelect: () => {
                if (deleting) void run(`delete`, deleting)
              },
            },
          ]}
        />
      </BulkBarCapsule>
    </BulkBarDock>
  )
}

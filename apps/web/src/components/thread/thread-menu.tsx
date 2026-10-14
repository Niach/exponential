import { useCallback, useState, type ReactNode } from "react"
import { conceptIcon, type MenuEntry } from "@exp/ui"
import { IssuePickerDialog } from "@/components/issue-picker-dialog"
import { sessionIssueCollection } from "@/lib/collections"
import { failedAction, SKIP_ERROR_TOAST } from "@/lib/action-error"
import { trpc } from "@/lib/trpc-client"

// EXP-1284 / EXP-1293: the header strip's ONE menu (THE Menu), behind the
// "+" and every issue chip's context menu:
//
//   Link an issue      THE search list → `sessions.linkIssues` (the root)
//   Unlink <IDENT>     issue chips only → `sessions.unlinkIssue`
//
// Failed writes toast `Couldn't link the issue` / `Couldn't unlink the
// issue` (feedback-states); the menu never offers what the viewer cannot do.

const LinkIcon = conceptIcon(`nav-search`)
const UnlinkIcon = conceptIcon(`ui-close`)

export const THREAD_MENU_COPY = {
  link: `Link an issue`,
  unlink: (identifier: string) => `Unlink ${identifier}`,
  pickerTitle: `Link an issue`,
} as const

export interface ThreadMenuIssue {
  id: string
  identifier: string
}

export function useThreadMenu({
  rootId,
  issueIds,
  canEdit,
}: {
  /** The thread's root session. */
  rootId: string
  /** The linked issues (the picker hides them). */
  issueIds: readonly string[]
  /** May link/unlink issues (a member). */
  canEdit: boolean
}): {
  entries: (issue?: ThreadMenuIssue) => MenuEntry[]
  openPicker: () => void
  dialogs: ReactNode
} {
  const [pickerOpen, setPickerOpen] = useState(false)

  const link = useCallback(
    async (issueId: string) => {
      try {
        const result = await trpc.sessions.linkIssues.mutate(
          { sessionId: rootId, issueIds: [issueId] },
          SKIP_ERROR_TOAST
        )
        await sessionIssueCollection.utils.awaitTxId(result.txid)
      } catch (error) {
        failedAction(`link`, `the issue`, error, { retry: () => void link(issueId) })
      }
    },
    [rootId]
  )
  const unlink = useCallback(
    async (issueId: string) => {
      try {
        const result = await trpc.sessions.unlinkIssue.mutate(
          { sessionId: rootId, issueId },
          SKIP_ERROR_TOAST
        )
        await sessionIssueCollection.utils.awaitTxId(result.txid)
      } catch (error) {
        failedAction(`unlink`, `the issue`, error, { retry: () => void unlink(issueId) })
      }
    },
    [rootId]
  )

  const entries = (issue?: ThreadMenuIssue): MenuEntry[] => {
    if (!canEdit) return []
    const rows: MenuEntry[] = [
      {
        kind: `item`,
        id: `link`,
        label: THREAD_MENU_COPY.link,
        icon: LinkIcon,
        onSelect: () => setPickerOpen(true),
        "data-testid": `thread-menu-link`,
      },
    ]
    if (issue) {
      rows.push({ kind: `separator`, id: `sep` })
      rows.push({
        kind: `item`,
        id: `unlink`,
        label: THREAD_MENU_COPY.unlink(issue.identifier),
        icon: UnlinkIcon,
        onSelect: () => void unlink(issue.id),
        "data-testid": `thread-menu-unlink`,
      })
    }
    return rows
  }

  const dialogs = (
    <IssuePickerDialog
      open={pickerOpen}
      onOpenChange={setPickerOpen}
      title={THREAD_MENU_COPY.pickerTitle}
      excludeIssueIds={[...issueIds]}
      onPick={(issue) => void link(issue.id)}
    />
  )

  return { entries, openPicker: () => setPickerOpen(true), dialogs }
}

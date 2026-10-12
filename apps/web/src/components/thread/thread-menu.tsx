import { useCallback, useState, type ReactNode } from "react"
import { useNavigate } from "@tanstack/react-router"
import { conceptIcon, type MenuEntry } from "@exp/ui"
import { IssuePickerDialog } from "@/components/issue-picker-dialog"
import { updateWorkTabs } from "@/hooks/use-work-tabs"
import { useOpenNewDraft } from "@/hooks/use-open-new-draft"
import { threadItemsCollection } from "@/lib/collections"
import { failedAction, SKIP_ERROR_TOAST } from "@/lib/action-error"
import { openNewRunTab } from "@/lib/work-tabs"
import { trpc } from "@/lib/trpc-client"

// EXP-1284: the thread's ONE menu (THE Menu), behind the strip's "+" and
// every issue tab's context menu:
//
//   Link an issue             THE search list → `threads.link`
//   New issue in this thread  the New issue page carrying `?thread=`
//                             (`issues.create({threadId})` on Create)
//   Start a session here          the New run tab (the composer) prefilled with
//                             the thread (and, from an issue tab, that issue)
//   Unlink <IDENT>            issue tabs only → `threads.unlink`
//
// Failed writes toast `Couldn't link the issue` / `Couldn't unlink the issue`
// (feedback-states); the "+" never offers what the viewer cannot do.

const LinkIcon = conceptIcon(`nav-search`)
const NewIssueIcon = conceptIcon(`ui-add`)
const StartIcon = conceptIcon(`action-run`)
const UnlinkIcon = conceptIcon(`ui-close`)

export const THREAD_MENU_COPY = {
  link: `Link an issue`,
  newIssue: `New issue in this thread`,
  start: `Start a session here`,
  unlink: (identifier: string) => `Unlink ${identifier}`,
  pickerTitle: `Link an issue`,
} as const

export interface ThreadMenuIssue {
  id: string
  identifier: string
}

export function useThreadMenu({
  teamId,
  teamSlug,
  threadId,
  issueIds,
  canEdit,
  canCreate,
  canStart,
}: {
  teamId: string
  teamSlug: string
  threadId: string
  /** The thread's issues (the picker hides them). */
  issueIds: readonly string[]
  /** May link/unlink issues (comment capability). */
  canEdit: boolean
  /** May file issues. */
  canCreate: boolean
  /** May start runs (a member with steering on). */
  canStart: boolean
}): {
  entries: (issue?: ThreadMenuIssue) => MenuEntry[]
  dialogs: ReactNode
} {
  const navigate = useNavigate()
  const openNewDraft = useOpenNewDraft(teamSlug)
  const [pickerOpen, setPickerOpen] = useState(false)

  const link = useCallback(
    async (issueId: string) => {
      try {
        const result = await trpc.threads.link.mutate({ id: threadId, issueId }, SKIP_ERROR_TOAST)
        await threadItemsCollection.utils.awaitTxId(result.txid)
      } catch (error) {
        failedAction(`link`, `the issue`, error, { retry: () => void link(issueId) })
      }
    },
    [threadId]
  )
  const unlink = useCallback(
    async (issueId: string) => {
      try {
        const result = await trpc.threads.unlink.mutate({ id: threadId, issueId }, SKIP_ERROR_TOAST)
        await threadItemsCollection.utils.awaitTxId(result.txid)
      } catch (error) {
        failedAction(`unlink`, `the issue`, error, { retry: () => void unlink(issueId) })
      }
    },
    [threadId]
  )
  const startHere = useCallback(
    (issue?: ThreadMenuIssue) => {
      // The New run tab IS the composer (EXP-1255); the thread rides along
      // until the run starts, then the tab lands INTO this thread.
      updateWorkTabs(teamId, openNewRunTab)
      void navigate({
        to: `/t/$teamSlug/agent`,
        params: { teamSlug },
        search: {
          thread: threadId,
          ...(issue ? { issues: issue.id } : {}),
        } as never,
      })
    },
    [navigate, teamId, teamSlug, threadId]
  )

  const entries = (issue?: ThreadMenuIssue): MenuEntry[] => {
    const rows: MenuEntry[] = []
    if (canEdit) {
      rows.push({
        kind: `item`,
        id: `link`,
        label: THREAD_MENU_COPY.link,
        icon: LinkIcon,
        onSelect: () => setPickerOpen(true),
        "data-testid": `thread-menu-link`,
      })
    }
    if (canCreate) {
      rows.push({
        kind: `item`,
        id: `new-issue`,
        label: THREAD_MENU_COPY.newIssue,
        icon: NewIssueIcon,
        onSelect: () => openNewDraft({ threadId }),
        "data-testid": `thread-menu-new-issue`,
      })
    }
    if (canStart) {
      rows.push({
        kind: `item`,
        id: `start`,
        label: THREAD_MENU_COPY.start,
        icon: StartIcon,
        onSelect: () => startHere(issue),
        "data-testid": `thread-menu-start`,
      })
    }
    if (issue && canEdit) {
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

  return { entries, dialogs }
}

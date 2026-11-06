import { useCallback, useMemo, useState, type ReactNode } from "react"
import { useNavigate } from "@tanstack/react-router"
import { conceptIcon, Menu, ThreadTabsAddButton, type MenuEntry } from "@exp/ui"
import { IssuePickerBody } from "@/components/issue-picker-host"
import { useIssueRefs, type ResolvedIssueRef } from "@/components/issue-ref-provider"
import { sessionIssueCollection } from "@/lib/collections"
import { failedAction, SKIP_ERROR_TOAST } from "@/lib/action-error"
import { newDraftNavigation } from "@/lib/issue-draft-page"
import { trpc } from "@/lib/trpc-client"
import { usePullRequestClose, usePullRequestTrainRows } from "@/components/session-merge-button"
import { usePullRequestGuideEntries } from "@/hooks/use-pull-request-checks"
import type { PullRequestRow, SessionRow } from "@/lib/session-tree"
import { ChatHeaderMenu, useChatMenuEntries, type useChatActions } from "@/components/thread/chat-menu"

// EXP-1284 / EXP-1293 / EXP-1334: the header strip's ONE menu (THE Menu),
// behind the "+" (LEFT of Conversation) and every issue chip's context menu
// — the desktop IDE's `thread_menu` rows ×4:
//
//   Link an issue ›    THE search list (the linked issues checked) →
//                      `sessions.linkIssues` (the root; the server posts the
//                      person's `Link #IDENT` message, which wakes the
//                      dispatcher); a pick closes the menu
//   New issue          the New issue page filing INTO this thread
//                      (`?thread=`, EXP-1284: created = linked)
//   Unlink <IDENT>     issue chips only → `sessions.unlinkIssue`
//
// Failed writes toast `Couldn't link the issue` / `Couldn't unlink the
// issue` (feedback-states); the menu never offers what the viewer cannot do.

const LinkIcon = conceptIcon(`nav-search`)
const NewIcon = conceptIcon(`ui-add`)
const UnlinkIcon = conceptIcon(`ui-close`)

export const THREAD_MENU_COPY = {
  link: `Link an issue`,
  newIssue: `New issue`,
  unlink: (identifier: string) => `Unlink ${identifier}`,
  add: `Add an issue`,
} as const

export interface ThreadMenuIssue {
  id: string
  identifier: string
}

const NO_ROWS: ResolvedIssueRef[] = []

/** THE search list for "Link an issue ›": the team's issues, the linked
 *  ones checked (a checked row stays; only a new pick links). */
function LinkIssueList({
  linkedIds,
  onLink,
}: {
  linkedIds: readonly string[]
  onLink: (issueId: string) => void
}) {
  const issueRefs = useIssueRefs()
  const [query, setQuery] = useState(``)
  const rows = issueRefs?.rows ?? NO_ROWS
  const linked = useMemo(() => new Set(linkedIds), [linkedIds])
  const checked = useMemo(() => rows.filter((row) => linked.has(row.id)), [rows, linked])
  return (
    <div
      data-testid="thread-menu-link-list"
      className="flex max-h-[min(24rem,var(--radix-dropdown-menu-content-available-height,24rem))] min-h-0 flex-col"
    >
      <IssuePickerBody
        mode="multi"
        teamId={issueRefs?.teamId}
        rows={rows}
        checked={checked}
        onToggle={(id) => {
          if (!linked.has(id)) onLink(id)
        }}
        query={query}
        onQueryChange={setQuery}
        resolveHit={(hit) => issueRefs?.fromHit(hit) ?? null}
        limit={30}
        searchPlaceholder="Search issues"
        autoFocus
        emptyText={(text) =>
          text.trim() ? `No issues match "${text}"` : `No issues to pick from`
        }
      />
    </div>
  )
}

export function useThreadMenu({
  rootId,
  teamSlug,
  issueIds,
  newIssueBoardId,
  canEdit,
}: {
  /** The thread's root session. */
  rootId: string
  teamSlug: string
  /** The linked issues (the search list checks them). */
  issueIds: readonly string[]
  /** New issue files onto this board (the thread's first issue's). */
  newIssueBoardId?: string
  /** May link/unlink issues (a member). */
  canEdit: boolean
}): {
  entries: (issue?: ThreadMenuIssue, close?: () => void) => MenuEntry[]
  /** The "+" with its menu (absent = read-only). */
  addMenu: ReactNode
} {
  const navigate = useNavigate()
  const [addOpen, setAddOpen] = useState(false)

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

  const entries = (issue?: ThreadMenuIssue, close?: () => void): MenuEntry[] => {
    if (!canEdit) return []
    const rows: MenuEntry[] = [
      {
        kind: `submenu`,
        id: `link`,
        label: THREAD_MENU_COPY.link,
        icon: LinkIcon,
        contentClassName: `w-[26rem] p-0`,
        "data-testid": `thread-menu-link`,
        body: (
          <LinkIssueList
            linkedIds={issueIds}
            onLink={(issueId) => {
              close?.()
              void link(issueId)
            }}
          />
        ),
      },
      {
        kind: `item`,
        id: `new`,
        label: THREAD_MENU_COPY.newIssue,
        icon: NewIcon,
        "data-testid": `thread-menu-new-issue`,
        onSelect: () =>
          void navigate(
            newDraftNavigation({ teamSlug, boardId: newIssueBoardId, threadId: rootId })
          ),
      },
    ]
    if (issue) {
      rows.push({ kind: `separator`, id: `sep` })
      rows.push({
        kind: `item`,
        id: `unlink`,
        label: THREAD_MENU_COPY.unlink(issue.identifier),
        icon: UnlinkIcon,
        destructive: true,
        onSelect: () => void unlink(issue.id),
        "data-testid": `thread-menu-unlink`,
      })
    }
    return rows
  }

  const addMenu = canEdit ? (
    <Menu
      open={addOpen}
      onOpenChange={setAddOpen}
      align="start"
      aria-label={THREAD_MENU_COPY.add}
      title={THREAD_MENU_COPY.add}
      data-testid="thread-add-menu"
      contentClassName="w-60"
      entries={entries(undefined, () => setAddOpen(false))}
      trigger={<ThreadTabsAddButton />}
    />
  ) : null

  return { entries, addMenu }
}

/** Thread 58cb32df (chats.json MENUS): the thread header's `…` = the chat's
 *  own menu (Delete, the rows' right-click menu)
 *  plus the Guide PR's rows (#EXP-1710 `mergeMenuEntries`): its Checks
 *  submenu (any viewer), Merge now (owners), Hold | Resume auto-merge and
 *  Close (members, a live PR). The delete
 *  confirm is the host's (`useChatActions().dialog`). */
export function ThreadHeaderMenu({
  root,
  title,
  pr,
  canClosePr,
  isOwner = false,
  actions,
  className,
}: {
  root: SessionRow
  title: string
  /** The Guide's PR (null = none). */
  pr: PullRequestRow | null
  canClosePr: boolean
  /** #EXP-1710: an owner also gets Merge now. */
  isOwner?: boolean
  actions: Pick<ReturnType<typeof useChatActions>, `requestDelete`>
  className?: string
}) {
  const chat = useChatMenuEntries(root, actions)
  const close = usePullRequestClose(pr ?? NO_PR, canClosePr && pr !== null)
  // #EXP-1710 (merge-train.json `menu`): the PR's Checks submenu (any
  // viewer), Merge now (owners), Hold | Resume auto-merge, Close.
  const train = usePullRequestTrainRows(pr)
  const prEntries = usePullRequestGuideEntries(
    pr,
    { member: canClosePr, owner: isOwner && canClosePr },
    { "close-pr": close.entry, "merge-now": train.mergeNow, hold: train.hold, resume: train.resume }
  )
  const entries: MenuEntry[] =
    chat.length > 0 && prEntries.length > 0
      ? [...chat, { kind: `separator`, id: `sep-pr` }, ...prEntries]
      : [...chat, ...prEntries]
  return (
    <>
      <ChatHeaderMenu entries={entries} title={title} className={className} />
      {close.dialog}
      {train.dialog}
    </>
  )
}

const NO_PR = { id: ``, state: `closed` } as const

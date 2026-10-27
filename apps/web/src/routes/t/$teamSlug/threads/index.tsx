import { useCallback, useEffect, useMemo, useRef, useState, type MouseEvent } from "react"
import { createFileRoute, Link, redirect, useNavigate } from "@tanstack/react-router"
import {
  CHATS_COPY,
  type ChatFilter,
  type ChatListView,
  type ChatSelectionRow,
} from "@exp/domain-contract/chats"
import {
  AgentBubbles,
  Button,
  ChatFilterChips,
  ChatList,
  ChatsHeader,
  conceptIcon,
  useIsMobile,
} from "@exp/ui"
import { useIssueMenuSelection } from "@/components/issue-context-menu/selection"
import { TAB_BAR_CLEARANCE } from "@/components/team/mobile-tab-bar"
import { ChatSelectionBar } from "@/components/thread/chat-selection-bar"
import { useSession } from "@/hooks/use-session"
import { useTeamBySlug } from "@/hooks/use-team-data"
import { useChatList } from "@/hooks/use-chat-list"
import { useEnteringRows } from "@/hooks/use-entering-rows"
import { useIssueSearch } from "@/hooks/use-issue-search"
import { useTeamSessions } from "@/hooks/use-session-tree"
import { useTeamPermissions } from "@/hooks/use-team-permissions"
import { useUnreadNotificationCount } from "@/hooks/use-unread-notifications"
import {
  chatSelectionClick,
  chatSelectionEnter,
  chatSelectionPrune,
  type ChatSelection,
} from "@/lib/chat-selection"
import { pageTitle } from "@/lib/page-title"
import { canSteerSession } from "@/lib/session-account-switch"
import { THREADS_LIST_COPY } from "@/lib/session-tree"

// Thread 58cb32df + #EXP-1380 (chats.json LIST + WIDE): the product calls
// them CHATS, and every form factor paints the SAME list of the ONE active
// team: the agent bubbles, the filter chips, the flat rows (right-click /
// long-press = the row's menu through `menuProps`). Phones put the large
// headline (search + inbox) on top; wide keeps the list in one column at
// the page's list width (the old Needs you · Running · Recent bands left
// the page; the rail keeps its own rows).
//
// #EXP-1388 (SELECTION + ARCHIVE): the list SELECTS — a touch long-press or
// the pointer menu's Select enters it (through the layout's menu host, the
// list registered like the issue list), cmd/ctrl-click toggles, shift-click
// ranges, the wide checkbox column (hover-revealed, the issue row's) toggles
// and Cmd/Ctrl+A selects every row; while selecting the rows lead with the
// selection glyph, a click toggles, and THE bulk bar (`ChatSelectionBar`) acts. `?view=archived` =
// the Archived page (phones: the pushed page header with the back control;
// wide: a back link to Chats over the same page), opened from the list's
// Archived entry.

const COPY = CHATS_COPY
const OPEN_OVERLAY = `[data-state="open"][role="menu"], [data-state="open"][role="listbox"], [data-state="open"][role="dialog"], [data-state="open"][role="alertdialog"]`
const UiBackIcon = conceptIcon(`ui-back`)

export const Route = createFileRoute(`/t/$teamSlug/threads/`)({
  head: () => ({ meta: [{ title: pageTitle(THREADS_LIST_COPY.title) }] }),
  validateSearch: (search: Record<string, unknown>): { view?: `archived` } =>
    search.view === `archived` ? { view: `archived` } : {},
  beforeLoad: async ({ context, location }) => {
    if (!context.session) {
      throw redirect({
        to: `/auth/login`,
        search: { redirect: location.href },
      })
    }
  },
  component: ThreadsPage,
})

function ThreadsPage() {
  const isMobile = useIsMobile()
  const { teamSlug } = Route.useParams()
  const view: ChatListView = Route.useSearch().view === `archived` ? `archived` : `list`
  const archivedView = view === `archived`
  const navigate = useNavigate()
  const team = useTeamBySlug(teamSlug)
  const { data: authSession } = useSession()
  const me = authSession?.user?.id ?? null
  const [filter, setFilter] = useState<ChatFilter>(`all`)
  const { list, bubbles, cliOf, isReady } = useChatList(team?.id, me, filter, view)
  const entering = useEnteringRows(list.rows.map((row) => row.sessionId))
  const unread = useUnreadNotificationCount(team?.id)
  const issueSearch = useIssueSearch()
  const { rows: sessionRows } = useTeamSessions(team?.id)
  const { isOwner } = useTeamPermissions(team)

  // ── selection ──
  const [selection, setSelection] = useState<ChatSelection | null>(null)
  const order = useMemo(() => list.rows.map((row) => row.sessionId), [list.rows])
  // The selection keeps only rows still in the list; a view switch clears it.
  useEffect(() => {
    setSelection((current) => chatSelectionPrune(current, order))
  }, [order])
  useEffect(() => {
    setSelection(null)
  }, [view])
  const clearSelection = useCallback(() => setSelection(null), [])
  const listRef = useRef<HTMLDivElement>(null)
  const selectionRef = useRef(selection)
  selectionRef.current = selection
  useIssueMenuSelection(
    useMemo(
      () => ({
        root: listRef,
        isSelected: (id: string) => selectionRef.current?.ids.includes(id) ?? false,
        toggle: (id: string) => setSelection((current) => chatSelectionEnter(current, id)),
      }),
      []
    )
  )
  const selectedIds = useMemo(() => (selection ? new Set(selection.ids) : null), [selection])

  const openChat = (threadId: string) =>
    void navigate({ to: `/t/$teamSlug/threads/$threadId`, params: { teamSlug, threadId } })
  const onRowClick = (sessionId: string, event: MouseEvent<HTMLButtonElement>) => {
    const next = chatSelectionClick(selectionRef.current, {
      id: sessionId,
      order,
      toggle: event.metaKey || event.ctrlKey,
      range: event.shiftKey,
    })
    if (next.open) openChat(sessionId)
    else setSelection(next.selection)
  }

  // The wide checkbox column: a click toggles (shift = the range), the
  // issue row's checkbox cell.
  const onSelectToggle = (sessionId: string, event: MouseEvent<HTMLElement>) => {
    const next = chatSelectionClick(selectionRef.current, {
      id: sessionId,
      order,
      toggle: true,
      range: event.shiftKey,
    })
    setSelection(next.selection)
  }

  // Cmd/Ctrl+A selects every row, the issue list's rule: never while an
  // overlay is up or a field (or anything outside the list) has focus.
  const orderRef = useRef(order)
  orderRef.current = order
  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (event.defaultPrevented || !(event.metaKey || event.ctrlKey) || event.key.toLowerCase() !== `a`) return
      if (document.querySelector(OPEN_OVERLAY) !== null) return
      const active = document.activeElement
      if (active !== null && active !== document.body && !listRef.current?.contains(active)) return
      if (orderRef.current.length === 0) return
      event.preventDefault()
      const ids = orderRef.current
      setSelection({ ids, anchor: ids[ids.length - 1] ?? null })
    }
    window.addEventListener(`keydown`, onKey)
    return () => window.removeEventListener(`keydown`, onKey)
  }, [])

  const selectionRows = useMemo((): ChatSelectionRow[] => {
    if (!selection) return []
    const byId = new Map(sessionRows.map((row) => [row.id, row]))
    const listed = new Map(list.rows.map((row) => [row.sessionId, row]))
    return selection.ids.flatMap((id) => {
      const session = byId.get(id)
      const row = listed.get(id)
      if (!session || !row) return []
      return [
        {
          sessionId: id,
          rootStatus: session.status,
          unreadCount: row.unreadCount,
          canSteer: canSteerSession(session, me),
          canDelete: Boolean(me) && (session.userId === me || isOwner),
        },
      ]
    })
  }, [selection, sessionRows, list.rows, me, isOwner])

  const toChats = () => void navigate({ to: `/t/$teamSlug/threads`, params: { teamSlug }, search: {} })

  return (
    // EXP-771: the SCROLLER is full width, the reading column lives inside it.
    <div className="relative h-full">
      <div className="h-full overflow-y-auto">
        <div
          className={`mx-auto flex w-full max-w-3xl flex-col gap-4 px-4 ${
            isMobile ? `pt-[max(1rem,env(safe-area-inset-top))]` : `pt-4 pb-4`
          } ${TAB_BAR_CLEARANCE} ${selection ? `md:pb-20` : ``}`}
        >
          {isMobile ? (
            <ChatsHeader
              title={archivedView ? COPY.archived.title : COPY.title}
              leading={
                archivedView ? (
                  <Button
                    variant="ghost"
                    size="icon"
                    aria-label={COPY.title}
                    className="-ml-2 size-9 shrink-0 rounded-full text-muted-foreground hover:text-foreground"
                    onClick={toChats}
                  >
                    <UiBackIcon className="size-5" />
                  </Button>
                ) : undefined
              }
              inboxUnread={unread > 0}
              onSearch={issueSearch.open}
              onInbox={() => void navigate({ to: `/t/$teamSlug/inbox`, params: { teamSlug } })}
            />
          ) : archivedView ? (
            // Wide: the same page in view `archived`, a back link to Chats.
            <Link
              to="/t/$teamSlug/threads"
              params={{ teamSlug }}
              search={{}}
              className="inline-flex max-w-full min-w-0 items-center gap-1.5 self-start text-sm text-muted-foreground hover:text-foreground"
              data-testid="chats-back"
            >
              <UiBackIcon className="size-4 shrink-0" />
              <span className="truncate">{COPY.title}</span>
            </Link>
          ) : null}
          {!archivedView && (
            <AgentBubbles
              bubbles={bubbles}
              onOpen={(agentId) =>
                void navigate({ to: `/t/$teamSlug/agents/$agentId`, params: { teamSlug, agentId } })
              }
            />
          )}
          <ChatFilterChips value={filter} counts={list.counts} onChange={setFilter} />
          <div ref={listRef}>
            <ChatList
              rows={list.rows}
              agentOf={cliOf}
              loading={!team || !isReady}
              entering={entering}
              emptyTitle={archivedView ? COPY.archived.empty : undefined}
              archived={!archivedView && filter === `all` ? list.archived : 0}
              onOpenArchived={() =>
                void navigate({ to: `/t/$teamSlug/threads`, params: { teamSlug }, search: { view: `archived` } })
              }
              selection={selectedIds}
              onRowClick={onRowClick}
              onSelectToggle={isMobile ? undefined : onSelectToggle}
            />
          </div>
        </div>
      </div>
      {selection && selectionRows.length > 0 && (
        // The bar FLOATS bottom-centre like the issue list's: md+ over the
        // list's bottom edge (click-through strip), phones in the tab bar's
        // slot (the dock positions itself there).
        <div className="pointer-events-none absolute inset-x-0 bottom-4 z-10 flex justify-center px-4">
          <div className="pointer-events-auto">
            <ChatSelectionBar rows={selectionRows} view={view} onClear={clearSelection} />
          </div>
        </div>
      )}
    </div>
  )
}

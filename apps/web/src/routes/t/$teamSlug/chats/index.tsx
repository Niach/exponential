import { useMemo } from "react"
import { createFileRoute, Link, redirect, useNavigate } from "@tanstack/react-router"
import {
  chatSections,
  CHATS_COPY,
  type ChatListView,
  type ChatSelectionRow,
} from "@exp/domain-contract/chats"
import {
  Button,
  ChatList,
  ChatsHeader,
  conceptIcon,
  FAB_FOOT_CLASS,
  NeedsYouEntryRow,
  useIsMobile,
} from "@exp/ui"
import { PAGE_END_CLEARANCE } from "@/components/team/mobile-tab-bar"
import { ChatSelectionBar } from "@/components/chat/chat-selection-bar"
import { useSession } from "@/hooks/use-session"
import { useTeamBySlug } from "@/hooks/use-team-data"
import { useChatList } from "@/hooks/use-chat-list"
import { useEnsureAssistant } from "@/hooks/use-ensure-assistant"
import { useEnteringRows } from "@/hooks/use-entering-rows"
import { useIssueSearch } from "@/hooks/use-issue-search"
import { useNeedsYou } from "@/hooks/use-needs-you"
import { useTeamSessions } from "@/hooks/use-session-tree"
import { useTeamPermissions } from "@/hooks/use-team-permissions"
import { useUnreadNotificationCount } from "@/hooks/use-unread-notifications"
import { useChatListSelection } from "@/components/chat/use-chat-list-selection"
import { pageTitle } from "@/lib/page-title"
import { canSteerSession } from "@/lib/session-account-switch"
import { CHATS_LIST_COPY } from "@/lib/session-tree"

// Chat 58cb32df + #EXP-1380 (chats.json LIST + WIDE): the product calls
// them CHATS, and every form factor paints the SAME list of the ONE active
// team: (S-3) the agents pinned on top, then (#EXP-1733) the apps, then (#FEED-191) `chatSections` under
// their headers (right-click / long-press = the row's menu through
// `menuProps`). Phones put the large headline (search + inbox) on top, then the
// Needs you entry row; wide keeps the list in one column at the page's list
// width.
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
const UiBackIcon = conceptIcon(`ui-back`)

export const Route = createFileRoute(`/t/$teamSlug/chats/`)({
  head: () => ({ meta: [{ title: pageTitle(CHATS_LIST_COPY.title) }] }),
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
  component: ChatsPage,
})

function ChatsPage() {
  const isMobile = useIsMobile()
  const { teamSlug } = Route.useParams()
  const view: ChatListView = Route.useSearch().view === `archived` ? `archived` : `list`
  const archivedView = view === `archived`
  const navigate = useNavigate()
  const team = useTeamBySlug(teamSlug)
  const { data: authSession } = useSession()
  const me = authSession?.user?.id ?? null
  const { list, agentRows, appRows, cliOf, waitingOf, wallOf, marksOf, isReady } = useChatList(team?.id, me, view)
  const needsYou = useNeedsYou(team?.id, me)
  // S-3: the viewer's Assistant exists once the list mounts (first use).
  useEnsureAssistant(team?.id)
  const entering = useEnteringRows(list.rows.map((row) => row.sessionId))
  const unread = useUnreadNotificationCount(team?.id)
  const issueSearch = useIssueSearch()
  const { rows: sessionRows } = useTeamSessions(team?.id)
  const { isOwner } = useTeamPermissions(team)

  // ── selection ──
  // #FEED-191: selection follows the DISPLAYED order (the sections; Archived flat).
  const order = useMemo(
    () => (archivedView ? list.rows : chatSections(list.rows).flatMap((section) => section.rows)).map((row) => row.sessionId),
    [list.rows, archivedView]
  )
  const openChat = (chatId: string) =>
    void navigate({ to: `/t/$teamSlug/sessions/$sessionId`, params: { teamSlug, sessionId: chatId } })
  const { selection, selectedIds, clear: clearSelection, listRef, onRowClick, onSelectToggle } =
    useChatListSelection(order, view, openChat)

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

  // #EXP-1733: an app row opens the app's chat (its editor).
  const openApp = (appSlug: string) =>
    void navigate({ to: `/t/$teamSlug/app-editor/$appSlug`, params: { teamSlug, appSlug } })

  const openAgent = (agentId: string) =>
    void navigate({ to: `/t/$teamSlug/agents/$agentId`, params: { teamSlug, agentId } })

  const toChats = () => void navigate({ to: `/t/$teamSlug/chats`, params: { teamSlug }, search: {} })

  return (
    // EXP-771: the SCROLLER is full width, the reading column lives inside it.
    <div className="relative h-full">
      <div className="h-full overflow-y-auto">
        <div
          className={`mx-auto flex w-full max-w-3xl flex-col gap-4 px-4 ${
            isMobile ? `pt-[max(1rem,env(safe-area-inset-top))]` : `pt-4 pb-4`
          } ${PAGE_END_CLEARANCE} ${selection ? `md:pb-20` : ``}`}
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
              inboxCount={unread}
              onSearch={issueSearch.open}
              onInbox={() => void navigate({ to: `/t/$teamSlug/inbox`, params: { teamSlug } })}
            />
          ) : archivedView ? (
            // Wide: the same page in view `archived`, a back link to Chats.
            <Link
              to="/t/$teamSlug/chats"
              params={{ teamSlug }}
              search={{}}
              className="inline-flex max-w-full min-w-0 items-center gap-1.5 self-start text-sm text-muted-foreground hover:text-foreground"
              data-testid="chats-back"
            >
              <UiBackIcon className="size-4 shrink-0" />
              <span className="truncate">{COPY.title}</span>
            </Link>
          ) : null}
          <div ref={listRef}>
            <ChatList
              rows={list.rows}
              agentRows={agentRows}
              onOpenAgent={openAgent}
              appRows={appRows}
              onOpenApp={openApp}
              agentOf={cliOf}
              waitingOf={waitingOf}
              wallOf={wallOf}
              marksOf={marksOf}
              loading={!team || !isReady}
              entering={entering}
              emptyTitle={archivedView ? COPY.archived.empty : undefined}
              flat={archivedView}
              entry={
                isMobile && !archivedView && needsYou.rows.length > 0 ? (
                  <NeedsYouEntryRow
                    count={needsYou.rows.length}
                    onOpen={() => void navigate({ to: `/t/$teamSlug/needs-you`, params: { teamSlug } })}
                  />
                ) : undefined
              }
              archived={archivedView ? 0 : list.archived}
              onOpenArchived={() =>
                void navigate({ to: `/t/$teamSlug/chats`, params: { teamSlug }, search: { view: `archived` } })
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
        // #EXP-1872: THE pinned foot: centred beside the Assistant FAB.
        <div className={`pointer-events-none absolute inset-x-0 bottom-4 z-10 flex justify-center px-4 ${FAB_FOOT_CLASS}`}>
          <div className="pointer-events-auto">
            <ChatSelectionBar rows={selectionRows} view={view} onClear={clearSelection} />
          </div>
        </div>
      )}
    </div>
  )
}

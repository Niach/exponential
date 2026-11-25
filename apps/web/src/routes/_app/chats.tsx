import { useMemo, type PointerEvent as ReactPointerEvent } from "react"
import { createFileRoute, Link, useNavigate } from "@tanstack/react-router"
import { startListDrag } from "@exponential_app/eui/web"
import { SIDEBAR_TREE_MAX_DEPTH } from "@exp/domain-contract/sidebar-tree"
import { chatSections, CHATS_COPY, type ChatListView, type ChatSelectionRow } from "@exp/domain-contract/chats"
import { Button, ChatList, ChatsHeader, conceptIcon, FAB_FOOT_CLASS, NeedsYouEntryRow, useIsMobile } from "@exp/ui"
import { ChatSelectionBar } from "@/components/chat/chat-selection-bar"
import { useCrossTeamChatList } from "@/components/chat/cross-team-chat-list"
import { useChatListSelection } from "@/components/chat/use-chat-list-selection"
import { PAGE_END_CLEARANCE } from "@/components/team/mobile-tab-bar"
import { useMemberTeams } from "@/components/home/use-member-teams"
import { useEnsureAssistant } from "@/hooks/use-ensure-assistant"
import { useEnteringRows } from "@/hooks/use-entering-rows"
import { useIssueSearch } from "@/hooks/use-issue-search"
import { useMemberNeedsYouCounts } from "@/hooks/use-app-icon-badge"
import { useUnreadNotificationCount } from "@/hooks/use-unread-notifications"
import { useTeamMemberships } from "@/hooks/use-team-data"
import { SIDEBAR_TEAM_LIST_ATTR, sidebarTeamDrop } from "@/hooks/use-sidebar-tree"
import { pageTitle } from "@/lib/page-title"
import { canSteerSession } from "@/lib/session-account-switch"
import { CHATS_LIST_COPY } from "@/lib/session-tree"

// #EXP-2027 (sidebar-tree.json ROOT `chats`): the TEAM-LESS Chats list,
// every width (#EXP-2109: the phones' Chats tab, under THE page header with
// the Needs you entry row), over EVERY member team. Each row opens in its own team (`/t/<its team>/sessions/<id>`, an
// agent's chat and an app's editor alike). Selection and the Archived view
// (`?view=archived`) too. A root row drags onto its team's list in the
// sidebar like an Active row (sidebar-tree.json `drop`, `sidebarDropSession`
// through the list's registered drop); md+ only, never while selecting.

const COPY = CHATS_COPY
const UiBackIcon = conceptIcon(`ui-back`)

/** The wide layout (md+), where the sidebar's team lists are a drop target. */
const WIDE_QUERY = `(min-width: 768px)`

/** A Chats row's drag onto its team's list in the sidebar: the rows dropped
 *  between are that list's (`SIDEBAR_TEAM_LIST_ATTR`), the dragged one this
 *  row (a list row of the same session steps aside: the drop is then a plain
 *  move). Another team's list never takes it. */
function startChatRowDrag(
  event: ReactPointerEvent<HTMLElement>,
  session: { id: string; teamId: string; parentId: string | null }
) {
  const element = event.currentTarget
  startListDrag(
    event.nativeEvent,
    session.id,
    {
      rows: () => {
        const list = document.querySelector(`[${SIDEBAR_TEAM_LIST_ATTR}="${session.teamId}"]`)
        const own = [...(list?.querySelectorAll<HTMLElement>(`[data-reorder-key]`) ?? [])].filter(
          (el) => el.getAttribute(`data-reorder-key`) !== session.id
        )
        return [...own, element]
      },
      maxDepth: SIDEBAR_TREE_MAX_DEPTH,
      color: `var(--primary)`,
    },
    { move: (context) => sidebarTeamDrop(session.teamId)?.(session, context) }
  )
}

export const Route = createFileRoute(`/_app/chats`)({
  head: () => ({ meta: [{ title: pageTitle(CHATS_LIST_COPY.title) }] }),
  validateSearch: (search: Record<string, unknown>): { view?: `archived` } =>
    search.view === `archived` ? { view: `archived` } : {},
  component: ChatsRoute,
})

function ChatsRoute() {
  const { user, defaultTeamSlug } = Route.useRouteContext()
  const me = user.id
  const isMobile = useIsMobile()
  const view: ChatListView = Route.useSearch().view === `archived` ? `archived` : `list`
  const archivedView = view === `archived`
  const navigate = useNavigate()
  const memberTeams = useMemberTeams(me)
  const { memberships } = useTeamMemberships(me)
  const { list, agentRows, appRows, cliOf, waitingOf, wallOf, marksOf, teamOf, sessionRows, isReady } =
    useCrossTeamChatList(memberTeams.teamIds, me, view)
  // S-3: the viewer's Assistant exists once the list mounts (first use; no
  // current team here).
  useEnsureAssistant(null)
  const unread = useUnreadNotificationCount()
  const issueSearch = useIssueSearch()
  const needsYou = useMemberNeedsYouCounts()
  const entering = useEnteringRows(list.rows.map((row) => row.sessionId))

  /** A row's team slug (the default team for one outside the member teams:
   *  the viewer's Assistant, whose home is their personal team). */
  const slugOf = (id: string) => {
    const teamId = teamOf(id)
    return (teamId ? memberTeams.byId.get(teamId)?.slug : undefined) ?? defaultTeamSlug
  }

  const order = useMemo(
    () => (archivedView ? list.rows : chatSections(list.rows).flatMap((section) => section.rows)).map((row) => row.sessionId),
    [list.rows, archivedView]
  )
  const openChat = (chatId: string) =>
    void navigate({ to: `/t/$teamSlug/sessions/$sessionId`, params: { teamSlug: slugOf(chatId), sessionId: chatId } })
  const { selection, selectedIds, clear, listRef, onRowClick, onSelectToggle } = useChatListSelection(
    order,
    view,
    openChat
  )

  const owned = useMemo(
    () => new Set(memberships.filter((row) => row.role === `owner`).map((row) => row.teamId)),
    [memberships]
  )
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
          canDelete: session.userId === me || owned.has(session.teamId),
        },
      ]
    })
  }, [selection, sessionRows, list.rows, me, owned])

  const sessionById = useMemo(() => new Map(sessionRows.map((row) => [row.id, row])), [sessionRows])
  const onRowPointerDown = (sessionId: string, event: ReactPointerEvent<HTMLButtonElement>) => {
    if (selection || !window.matchMedia(WIDE_QUERY).matches) return
    const session = sessionById.get(sessionId)
    if (!session || session.parentId) return
    startChatRowDrag(event, { id: session.id, teamId: session.teamId, parentId: null })
  }
  /** A row's team mark (none outside the member teams: the Assistant). */
  const markOf = (id: string) => {
    const team = memberTeams.byId.get(teamOf(id) ?? ``)
    return team ? { iconUrl: team.iconUrl, name: team.name } : null
  }

  const assistants = new Set(agentRows.filter((row) => row.assistant).map((row) => row.agentId))
  const openApp = (appId: string, appSlug: string) =>
    void navigate({ to: `/t/$teamSlug/app-editor/$appSlug`, params: { teamSlug: slugOf(appId), appSlug } })
  const appIdBySlug = new Map(appRows.map((row) => [row.slug, row.appId]))
  const openAgent = (agentId: string) =>
    void navigate({ to: `/t/$teamSlug/agents/$agentId`, params: { teamSlug: slugOf(agentId), agentId } })

  return (
    <div className="relative h-full max-md:h-dvh">
      <div className="h-full overflow-y-auto">
        <div className={`mx-auto flex w-full max-w-3xl flex-col gap-4 px-4 ${
            isMobile ? `` : `pt-4 pb-4`
          } ${PAGE_END_CLEARANCE} ${selection ? `md:pb-20` : ``}`}>
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
                    onClick={() => void navigate({ to: `/chats`, search: {} })}
                  >
                    <UiBackIcon className="size-5" />
                  </Button>
                ) : undefined
              }
              inboxCount={unread}
              onSearch={issueSearch.open}
              onInbox={() => void navigate({ to: `/inbox` })}
            />
          ) : archivedView ? (
            <Link
              to="/chats"
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
              // An app row hands its slug; its app names the team.
              onOpenApp={(slug) => openApp(appIdBySlug.get(slug) ?? ``, slug)}
              agentOf={cliOf}
              waitingOf={waitingOf}
              wallOf={wallOf}
              marksOf={marksOf}
              teamOf={markOf}
              // The Assistant is nobody's team's (chats.json PHONE SHELL).
              agentTeamOf={(id) => (assistants.has(id) ? null : markOf(id))}
              appTeamOf={markOf}
              onRowPointerDown={archivedView ? undefined : onRowPointerDown}
              loading={memberTeams.teamIds.length === 0 || !isReady}
              entering={entering}
              emptyTitle={archivedView ? COPY.archived.empty : undefined}
              flat={archivedView}
              archived={archivedView ? 0 : list.archived}
              onOpenArchived={() => void navigate({ to: `/chats`, search: { view: `archived` } })}
              selection={selectedIds}
              onRowClick={onRowClick}
              onSelectToggle={isMobile ? undefined : onSelectToggle}
              entry={
                isMobile && !archivedView && needsYou.total > 0 ? (
                  <NeedsYouEntryRow count={needsYou.total} onOpen={() => void navigate({ to: `/needs-you` })} />
                ) : undefined
              }
            />
          </div>
        </div>
      </div>
      {selection && selectionRows.length > 0 && (
        // #EXP-1872: THE pinned foot, bottom-centre over the list.
        <div className={`pointer-events-none absolute inset-x-0 bottom-4 z-10 flex justify-center px-4 ${FAB_FOOT_CLASS}`}>
          <div className="pointer-events-auto">
            <ChatSelectionBar rows={selectionRows} view={view} onClear={clear} />
          </div>
        </div>
      )}
    </div>
  )
}

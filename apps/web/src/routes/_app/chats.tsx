import { useMemo } from "react"
import { createFileRoute, Link, useNavigate } from "@tanstack/react-router"
import { chatSections, CHATS_COPY, type ChatListView, type ChatSelectionRow } from "@exp/domain-contract/chats"
import { ChatList, conceptIcon, FAB_FOOT_CLASS } from "@exp/ui"
import { ChatSelectionBar } from "@/components/chat/chat-selection-bar"
import { useCrossTeamChatList } from "@/components/chat/cross-team-chat-list"
import { useChatListSelection } from "@/components/chat/use-chat-list-selection"
import { useMemberTeams } from "@/components/home/use-member-teams"
import { useEnsureAssistant } from "@/hooks/use-ensure-assistant"
import { useEnteringRows } from "@/hooks/use-entering-rows"
import { useTeamMemberships } from "@/hooks/use-team-data"
import { pageTitle } from "@/lib/page-title"
import { canSteerSession } from "@/lib/session-account-switch"
import { CHATS_LIST_COPY } from "@/lib/session-tree"

// #EXP-2027 (sidebar-tree.json ROOT `chats`): the TEAM-LESS Chats list of web
// md+: the team's Chats page (routes/t/$teamSlug/chats) over EVERY member
// team. Each row opens in its own team (`/t/<its team>/sessions/<id>`, an
// agent's chat and an app's editor alike). Selection and the Archived view
// (`?view=archived`) as on the team's page; phones keep that page (the `_app`
// layout forwards them).

const COPY = CHATS_COPY
const UiBackIcon = conceptIcon(`ui-back`)

export const Route = createFileRoute(`/_app/chats`)({
  head: () => ({ meta: [{ title: pageTitle(CHATS_LIST_COPY.title) }] }),
  validateSearch: (search: Record<string, unknown>): { view?: `archived` } =>
    search.view === `archived` ? { view: `archived` } : {},
  component: ChatsRoute,
})

function ChatsRoute() {
  const { user, defaultTeamSlug } = Route.useRouteContext()
  const me = user.id
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

  const openApp = (appId: string, appSlug: string) =>
    void navigate({ to: `/t/$teamSlug/app-editor/$appSlug`, params: { teamSlug: slugOf(appId), appSlug } })
  const appIdBySlug = new Map(appRows.map((row) => [row.slug, row.appId]))
  const openAgent = (agentId: string) =>
    void navigate({ to: `/t/$teamSlug/agents/$agentId`, params: { teamSlug: slugOf(agentId), agentId } })

  return (
    <div className="relative h-full">
      <div className="h-full overflow-y-auto">
        <div className={`mx-auto flex w-full max-w-3xl flex-col gap-4 px-4 pt-4 pb-4 ${selection ? `pb-20` : ``}`}>
          {archivedView && (
            <Link
              to="/chats"
              search={{}}
              className="inline-flex max-w-full min-w-0 items-center gap-1.5 self-start text-sm text-muted-foreground hover:text-foreground"
              data-testid="chats-back"
            >
              <UiBackIcon className="size-4 shrink-0" />
              <span className="truncate">{COPY.title}</span>
            </Link>
          )}
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
              loading={memberTeams.teamIds.length === 0 || !isReady}
              entering={entering}
              emptyTitle={archivedView ? COPY.archived.empty : undefined}
              flat={archivedView}
              archived={archivedView ? 0 : list.archived}
              onOpenArchived={() => void navigate({ to: `/chats`, search: { view: `archived` } })}
              selection={selectedIds}
              onRowClick={onRowClick}
              onSelectToggle={onSelectToggle}
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

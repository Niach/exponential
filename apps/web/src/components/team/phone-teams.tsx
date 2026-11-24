import { useMemo, useState, type ReactNode } from "react"
import { Link, useNavigate, useRouter } from "@tanstack/react-router"
import { eq, and, useLiveQuery } from "@tanstack/react-db"
import { CHATS_COPY, railAgents } from "@exp/domain-contract/chats"
import { NEEDS_YOU_COPY } from "@exp/domain-contract/needs-you"
import { rowBadge } from "@exp/domain-contract/nav-badges"
import type { SidebarEntry } from "@exp/domain-contract/sidebar-folders"
import {
  phoneTeamMenu,
  phoneTeamPage,
  SIDEBAR_TEAMS_MENU,
  SIDEBAR_TREE_COPY,
  sidebarTeamItems,
  type PhoneTeamRow,
  type SidebarNewKind,
} from "@exp/domain-contract/sidebar-tree"
import {
  Badge,
  BoardGlyph,
  Button,
  FabButton,
  GlassGroup,
  HEADER_BUTTON_CLASS,
  Menu,
  MobileDetailHeader,
  SkeletonRows,
  TeamAvatar,
  conceptIcon,
  type MenuEntry,
} from "@exp/ui"
import type { Team } from "@/db/schema"
import { useCrossTeamChatList } from "@/components/chat/cross-team-chat-list"
import { Empty } from "@/components/empty"
import { GettingStartedSection } from "@/components/getting-started/getting-started-section"
import { CreateTeamDialog } from "@/components/create-team-dialog"
import { JoinTeamDialog } from "@/components/join-team-dialog"
import { useMemberTeams } from "@/components/home/use-member-teams"
import { MobilePageHeader, PushedTitle } from "@/components/team/mobile-page-header"
import { PAGE_END_CLEARANCE } from "@/components/team/mobile-tab-bar"
import { PHONE_ROW_CLASS } from "@/components/team/phone-settings"
import { AgentIcon } from "@/components/team/sidebar-agents"
import { AppIcon } from "@/components/team/sidebar-apps"
import { ChatRunMark } from "@/components/team/sidebar-running"
import { useMemberNeedsYouCounts } from "@/hooks/use-app-icon-badge"
import { useApps } from "@/hooks/use-apps"
import { useSession } from "@/hooks/use-session"
import { useChatsList } from "@/hooks/use-session-tree"
import { sidebarTeamSlice, useSidebarTeamsData } from "@/hooks/use-sidebar-teams-data"
import { useTeamBoardsWithReady, useTeamBySlug } from "@/hooks/use-team-data"
import { useTeamPermissions } from "@/hooks/use-team-permissions"
import { newPagePath, type NewPageKind } from "@/lib/app-paths"
import { emptyStateCopy } from "@/lib/empty-states"
import { sidebarLayoutCollection } from "@/lib/collections"
import type { ChatsListRow } from "@/lib/session-tree"

// #EXP-2109 (sidebar-tree.json `phone`, ×3 with iOS and Android): the phones'
// TEAMS tab, the sidebar tree one level per page: Teams › a team's one mixed
// list (boards, agents, apps, placed sessions, in the member's own order) › a
// folder's items. Read-only: the sidebar is arranged on wide screens.

const AddIcon = conceptIcon(`ui-add`)
const ChevronIcon = conceptIcon(`ui-chevron-right`)
const FolderIcon = conceptIcon(`ui-folder`)
const InviteIcon = conceptIcon(`ui-invite`)
const MoreIcon = conceptIcon(`ui-more`)
const SettingsIcon = conceptIcon(`nav-settings`)

const NEW_KIND_ICON: Record<SidebarNewKind, ReturnType<typeof conceptIcon>> = {
  board: conceptIcon(`nav-boards`),
  agent: conceptIcon(`settings-agents`),
  app: conceptIcon(`nav-apps`),
  session: conceptIcon(`action-chat`),
}

function RowBody({ glyph, label, trailing }: { glyph: ReactNode; label: string; trailing?: ReactNode }) {
  return (
    <>
      <span className="flex size-6 shrink-0 items-center justify-center">{glyph}</span>
      <span className="min-w-0 flex-1 truncate">{label}</span>
      {trailing}
      <ChevronIcon className="size-4 shrink-0 text-foreground/50" />
    </>
  )
}

/** The Teams tab's root. */
export function PhoneTeams({ me }: { me: string }) {
  const { teams } = useMemberTeams(me)
  const needsYou = useMemberNeedsYouCounts()
  const [createOpen, setCreateOpen] = useState(false)
  const [joinOpen, setJoinOpen] = useState(false)
  const entries: MenuEntry[] = SIDEBAR_TEAMS_MENU.map((id) =>
    id === `new-team`
      ? { kind: `item`, id, label: SIDEBAR_TREE_COPY.newTeam, icon: AddIcon, onSelect: () => setCreateOpen(true) }
      : { kind: `item`, id, label: SIDEBAR_TREE_COPY.joinTeam, icon: InviteIcon, onSelect: () => setJoinOpen(true) }
  )
  return (
    <div className="h-full overflow-y-auto" data-testid="phone-teams">
      <div
        className={`mx-auto flex w-full max-w-xl flex-col gap-4 px-4 pt-[max(1rem,env(safe-area-inset-top))] ${PAGE_END_CLEARANCE}`}
      >
        <MobilePageHeader
          title={CHATS_COPY.headers.teams}
          trailing={
            <Menu
              align="end"
              aria-label={SIDEBAR_TREE_COPY.teams}
              title={SIDEBAR_TREE_COPY.teams}
              entries={entries}
              trigger={
                <FabButton emphasis="primary" className="size-11" aria-label={SIDEBAR_TREE_COPY.teams} data-testid="teams-add">
                  <AddIcon className="size-5" />
                </FabButton>
              }
            />
          }
        />
        {teams.length === 0 ? (
          <SkeletonRows rowHeight={52} />
        ) : (
          <GlassGroup data-testid="teams-list">
            {teams.map((team) => (
              <Link key={team.id} to="/t/$teamSlug/team" params={{ teamSlug: team.slug }} className={PHONE_ROW_CLASS} data-testid="teams-row">
                <RowBody
                  glyph={<TeamAvatar iconUrl={team.iconUrl} size={24} />}
                  label={team.name}
                  trailing={<Badge count={needsYou.byTeam.get(team.id) ?? 0} tone="attention" title={NEEDS_YOU_COPY.entry} />}
                />
              </Link>
            ))}
          </GlassGroup>
        )}
      </div>
      <CreateTeamDialog open={createOpen} onOpenChange={setCreateOpen} />
      <JoinTeamDialog open={joinOpen} onOpenChange={setJoinOpen} />
    </div>
  )
}

/** The member's layout rows of one team (section `team`). */
function useTeamEntries(teamId: string | undefined): { entries: SidebarEntry[]; isReady: boolean } {
  const { data, isReady } = useLiveQuery(
    (query) =>
      teamId
        ? query
            .from({ rows: sidebarLayoutCollection })
            .where(({ rows }) => and(eq(rows.teamId, teamId), eq(rows.section, `team`)))
        : undefined,
    [teamId]
  )
  const entries = useMemo(
    () =>
      (data ?? []).map(
        (row): SidebarEntry => ({
          id: row.id,
          kind: row.kind === `folder` ? `folder` : `item`,
          targetId: row.targetId,
          parentId: row.parentId,
          name: row.name,
          sortOrder: row.sortOrder,
        })
      ),
    [data]
  )
  return { entries, isReady: Boolean(teamId) && isReady }
}

/** A team's page (`folderId` null) or one of its folders. */
export function PhoneTeam({ teamSlug, folderId }: { teamSlug: string; folderId: string | null }) {
  const navigate = useNavigate()
  const router = useRouter()
  const team = useTeamBySlug(teamSlug)
  const back = () => {
    if (folderId !== null) void navigate({ to: `/t/$teamSlug/team`, params: { teamSlug }, search: {} })
    else if (router.history.canGoBack()) router.history.back()
    else void navigate({ to: `/teams` })
  }
  if (!team) {
    return (
      <div className="flex h-full flex-col">
        <MobileDetailHeader title="" onBack={() => void navigate({ to: `/teams` })} />
        <div className="px-4 pt-6">
          <SkeletonRows rowHeight={52} />
        </div>
      </div>
    )
  }
  return <TeamPage team={team} folderId={folderId} onBack={folderId === null ? () => void navigate({ to: `/teams` }) : back} />
}

function TeamPage({ team, folderId, onBack }: { team: Team; folderId: string | null; onBack: () => void }) {
  const navigate = useNavigate()
  const { data: session } = useSession()
  const me = session?.user?.id
  const ids = useMemo(() => [team.id], [team.id])
  const { boards, boardsReady } = useTeamBoardsWithReady(team.id)
  const { apps } = useApps(team.id)
  const { groups } = useChatsList(ids)
  const chatRows = useMemo(() => {
    const map = new Map<string, ChatsListRow>()
    for (const group of groups) for (const row of group.rows) map.set(row.sessionId, row)
    return map
  }, [groups])
  // A placed session's count badge = its Chats row's.
  const { list } = useCrossTeamChatList(ids, me)
  const unread = useMemo(() => new Map(list.rows.map((row) => [row.sessionId, rowBadge(row)])), [list.rows])
  const permissions = useTeamPermissions(team)
  const data = sidebarTeamSlice(useSidebarTeamsData(ids, me, chatRows), team.id)
  const { entries, isReady } = useTeamEntries(team.id)
  const agents = useMemo(
    () => railAgents(data.agents, apps.map((app) => ({ agentId: app.row?.agentId ?? null }))),
    [data.agents, apps]
  )
  const items = useMemo(
    () =>
      sidebarTeamItems({
        teamId: team.id,
        boards,
        // `listAgents` order IS the default order.
        agents: agents.map((agent, index) => ({
          id: agent.agentId,
          sortOrder: index,
          role: agent.assistant ? `assistant` : `agent`,
        })),
        apps,
        sessions: data.sessions,
        entries,
      }),
    [team.id, boards, agents, apps, data.sessions, entries]
  )
  const rows = phoneTeamPage({ items, entries, folderId })
  const folderName = folderId === null ? null : (entries.find((entry) => entry.id === folderId)?.name ?? SIDEBAR_TREE_COPY.newFolder)
  const params = { teamSlug: team.slug }

  const rowOf = (row: PhoneTeamRow): ReactNode => {
    switch (row.kind) {
      case `folder`:
        return (
          <Link key={row.id} to="/t/$teamSlug/team" params={params} search={{ folder: row.id }} className={PHONE_ROW_CLASS} data-testid="team-folder-row">
            <RowBody glyph={<FolderIcon className="size-5 text-muted-foreground" />} label={row.name ?? ``} />
          </Link>
        )
      case `board`: {
        const board = boards.find((entry) => entry.id === row.id)
        return board ? (
          <Link key={row.id} to="/t/$teamSlug/boards/$boardSlug" params={{ ...params, boardSlug: board.slug }} className={PHONE_ROW_CLASS} data-testid="team-board-row">
            <RowBody glyph={<BoardGlyph board={board} className="size-5" />} label={board.name} />
          </Link>
        ) : null
      }
      case `agent`: {
        const agent = agents.find((entry) => entry.agentId === row.id)
        const badge = data.badges.get(row.id)
        return agent ? (
          <Link key={row.id} to="/t/$teamSlug/agents/$agentId" params={{ ...params, agentId: agent.agentId }} className={PHONE_ROW_CLASS} data-testid="team-agent-row">
            <RowBody
              glyph={<AgentIcon agent={agent} className="size-5" />}
              label={agent.name}
              trailing={badge && <Badge count={badge.count} tone={badge.tone} />}
            />
          </Link>
        ) : null
      }
      case `app`: {
        const app = apps.find((entry) => entry.id === row.id)
        return app ? (
          <Link key={row.id} to="/t/$teamSlug/apps/$appSlug" params={{ ...params, appSlug: app.slug }} className={PHONE_ROW_CLASS} data-testid="team-app-row">
            <RowBody glyph={<AppIcon app={app} className="size-5" />} label={app.name} />
          </Link>
        ) : null
      }
      default: {
        const chat = data.chatRows.get(row.id)
        const badge = unread.get(row.id)
        return (
          <Link key={row.id} to="/t/$teamSlug/sessions/$sessionId" params={{ ...params, sessionId: row.id }} className={PHONE_ROW_CLASS} data-testid="team-session-row">
            <RowBody
              glyph={<ChatRunMark row={{ sessionId: row.id, needsYou: chat?.needsYou ?? false, busy: chat?.busy ?? false }} ringClassName="ring-background" />}
              label={data.titles.get(row.id) ?? chat?.title ?? ``}
              trailing={badge && <Badge count={badge.count} tone={badge.tone} />}
            />
          </Link>
        )
      }
    }
  }

  return (
    <div className="flex h-full min-h-0 flex-col pt-[env(safe-area-inset-top)]" data-testid="phone-team">
      <MobileDetailHeader
        title={
          <PushedTitle
            glyph={
              folderId === null ? (
                <TeamAvatar iconUrl={team.iconUrl} size={24} />
              ) : (
                <FolderIcon className="size-5 text-muted-foreground" />
              )
            }
          >
            {folderId === null ? team.name : folderName}
          </PushedTitle>
        }
        onBack={onBack}
        backLabel={folderId === null ? CHATS_COPY.headers.teams : team.name}
        menu={folderId === null ? <TeamMenu team={team} /> : undefined}
      />
      <div className={`min-h-0 flex-1 overflow-y-auto px-4 pt-6 ${PAGE_END_CLEARANCE}`}>
        {!boardsReady || !isReady ? (
          <SkeletonRows rowHeight={52} />
        ) : rows && rows.length > 0 ? (
          <GlassGroup data-testid="team-items">{rows.map(rowOf)}</GlassGroup>
        ) : folderId === null ? (
          // Nothing yet: a board is the first step; the member's progress.
          <>
            <Empty id="boards">
              {permissions.isOwner && (
                <Button variant="outline" size="sm" onClick={() => void navigate({ href: newPagePath(team.slug, `board`) })}>
                  {emptyStateCopy(`boards`).action}
                </Button>
              )}
            </Empty>
            {permissions.isMember && <GettingStartedSection team={team} teamSlug={team.slug} />}
          </>
        ) : null}
      </div>
    </div>
  )
}

function TeamMenu({ team }: { team: Team }) {
  const navigate = useNavigate()
  const { isOwner } = useTeamPermissions(team)
  const entries = phoneTeamMenu({ owner: isOwner }).map(
    (item): MenuEntry =>
      item.id === `new`
        ? {
            kind: `submenu`,
            id: item.id,
            label: SIDEBAR_TREE_COPY.new,
            icon: AddIcon,
            entries: item.children.map((kind) => ({
              kind: `item` as const,
              id: kind,
              label: SIDEBAR_TREE_COPY[kind],
              icon: NEW_KIND_ICON[kind],
              onSelect: () => void navigate({ href: newPagePath(team.slug, kind as NewPageKind) }),
            })),
          }
        : {
            kind: `item`,
            id: item.id,
            label: SIDEBAR_TREE_COPY.settings,
            icon: SettingsIcon,
            onSelect: () => void navigate({ to: `/t/$teamSlug/settings`, params: { teamSlug: team.slug } }),
          }
  )
  return (
    <Menu
      align="end"
      aria-label={SIDEBAR_TREE_COPY.teamActions}
      title={team.name}
      entries={entries}
      trigger={
        <Button variant="ghost" size="icon" className={HEADER_BUTTON_CLASS} aria-label={SIDEBAR_TREE_COPY.teamActions} data-testid="team-menu">
          <MoreIcon className="size-4" />
        </Button>
      }
    />
  )
}

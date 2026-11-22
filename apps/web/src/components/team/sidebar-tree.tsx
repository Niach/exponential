import { memo, useCallback, useEffect, useMemo, type ReactNode } from "react"
import { Link, useNavigate } from "@tanstack/react-router"
import { CHATS_COPY, railAgents } from "@exp/domain-contract/chats"
import { NEEDS_YOU_COPY, NEEDS_YOU_TEST_IDS } from "@exp/domain-contract/needs-you"
import type { CountBadgeValue } from "@exp/domain-contract/nav-badges"
import type { SidebarEntry } from "@exp/domain-contract/sidebar-folders"
import {
  SIDEBAR_DEVICES_OPEN_KEY,
  SIDEBAR_TEAMS_MENU,
  SIDEBAR_TREE_COPY,
  sidebarRootRows,
  sidebarTeamItems,
  sidebarTeamMenu,
  sidebarTeamOpenKey,
  type SidebarItemKind,
  type SidebarNewKind,
  type SidebarRootRow,
} from "@exp/domain-contract/sidebar-tree"
import {
  Badge,
  conceptIcon,
  getBoardIcon,
  LiveDot,
  Menu,
  menuProps,
  SidebarGroup,
  SidebarGroupAction,
  SidebarGroupContent,
  SidebarGroupLabel,
  SidebarMenu,
  SidebarMenuAction,
  SidebarMenuButton,
  SidebarMenuItem,
  SidebarMenuTrailing,
  TeamAvatar,
  toast,
  TREE_INDENT,
  TreeGuides,
  treeGuides,
  type MenuEntry,
  type MenuSessionProps,
} from "@exp/ui"
import type { Board, Team } from "@/db/schema"
import type { ChatsListRow } from "@/lib/session-tree"
import { newPagePath, type NewPageKind } from "@/lib/app-paths"
import { sidebarLayoutCollection } from "@/lib/collections"
import { trpc } from "@/lib/trpc-client"
import { cn } from "@/lib/utils"
import { useApps } from "@/hooks/use-apps"
import { useTeamBoards } from "@/hooks/use-team-data"
import { sidebarTeamSlice, useSidebarTeamsData, type SidebarChatMark, type SidebarTeamSlice } from "@/hooks/use-sidebar-teams-data"
import { useDeviceGlyphs } from "@/hooks/use-device-glyphs"
import { useSidebarFolders } from "@/hooks/use-sidebar-folders"
import {
  registerSidebarTeamDrop,
  SIDEBAR_TEAM_LIST_ATTR,
  useSidebarOpenFlag,
  type SidebarDevice,
} from "@/hooks/use-sidebar-tree"
import { SidebarFolderRows, type SidebarRowItemProps } from "@/components/team/sidebar-folders"
import { SidebarAgentRow } from "@/components/team/sidebar-agents"
import { SidebarAppRow } from "@/components/team/sidebar-apps"
import { ChatRunMark } from "@/components/team/sidebar-running"
import { NavCountBadge } from "@/components/team/sidebar-shared"
import { DRAFTS_LABEL, NavDraftsIcon } from "@/components/team/sidebar-nav-entries"

// #EXP-2027 (fixtures/sidebar-tree.json, its `$comment` is the spec): the
// web md+ main menu as ONE tree, with no active team. The root rows
// (`sidebarRootRows`: Home, Needs you and Drafts while they count, Chats,
// Devices with each device a row), the `Teams` label (its + = New team /
// Join team), then one row per team (`sidebarTeams`) holding that team's ONE
// mixed list (`sidebarTeamItems` through `SidebarFolderRows`, section `team`):
// boards, agents, apps and the root sessions the member placed. A team row
// folds on a press; its `…` and right-click open THE team menu
// (`sidebarTeamMenu`). Open flags are device-local (open by default).

const NavHomeIcon = conceptIcon(`nav-home`)
const NavNeedsYouIcon = conceptIcon(`nav-needs-you`)
const NavChatsIcon = conceptIcon(`nav-chats`)
const NavDevicesIcon = conceptIcon(`nav-devices`)
const NavSettingsIcon = conceptIcon(`nav-settings`)
const ChevronIcon = conceptIcon(`ui-chevron-right`)
const AddIcon = conceptIcon(`ui-add`)
const MoreIcon = conceptIcon(`ui-more`)
const FolderIcon = conceptIcon(`ui-folder`)
const InviteIcon = conceptIcon(`ui-invite`)
const RemoveIcon = conceptIcon(`ui-minus`)
const DeviceDefaultIcon = conceptIcon(`ui-device-default`)

/** The New submenu's glyph per kind (the draft: board, agent, app, chat). */
const NEW_KIND_ICON: Record<SidebarNewKind, ReturnType<typeof conceptIcon>> = {
  board: conceptIcon(`nav-boards`),
  agent: conceptIcon(`settings-agents`),
  app: conceptIcon(`nav-apps`),
  session: conceptIcon(`action-chat`),
}

/** The guide gutter starts at the menu button's own left padding. */
const GUIDE_BASE = 8

/** The team menu's kind (`menuProps`), one per team list's host. */
export const sidebarTeamMenuKind = (teamId: string) => `sidebar-team:${teamId}`
/** A placed session's menu (Remove from sidebar), one per team list's host. */
export const sidebarSessionMenuKind = (teamId: string) => `sidebar-session:${teamId}`

/** Session › New: Home's composer with the team as its Teams pick. */
export const newSessionHref = (teamId: string) => `/home?team=${encodeURIComponent(teamId)}`

/** The pathname's place in the tree: what each row reads to light up. */
export function sidebarActive(pathname: string) {
  const team = /^\/t\/([^/]+)\/(boards|agents|apps|app-editor|sessions)\/([^/]+)/.exec(pathname)
  const device = /^\/devices\/([^/]+)/.exec(pathname)
  return {
    pathname,
    teamSlug: team?.[1] ?? null,
    board: team?.[2] === `boards` ? team[3] : null,
    agent: team?.[2] === `agents` ? team[3] : null,
    app: team?.[2] === `apps` || team?.[2] === `app-editor` ? team[3] : null,
    session: team?.[2] === `sessions` ? team[3] : null,
    device: device ? decodeURIComponent(device[1]) : null,
  }
}
export type SidebarActive = ReturnType<typeof sidebarActive>

/* ─── root rows ─────────────────────────────────────────────────────────── */

export function SidebarRootRows({
  active,
  needsYou,
  drafts,
  chatsBadge,
  devices,
  teams,
  onAddDevice,
}: {
  active: SidebarActive
  needsYou: number
  drafts: number
  chatsBadge: CountBadgeValue
  devices: readonly SidebarDevice[]
  /** Member teams by id: a teammate's server wears its team's mark. */
  teams: ReadonlyMap<string, Team>
  onAddDevice: () => void
}) {
  const rows = sidebarRootRows({ needsYou, drafts })
  const row = (kind: SidebarRootRow): ReactNode => {
    switch (kind) {
      case `home`:
        return (
          <SidebarMenuItem key={kind}>
            <SidebarMenuButton asChild density="compact" isActive={active.pathname === `/home`}>
              <Link to="/home" data-testid="sidebar-home">
                <NavHomeIcon className="h-4 w-4" />
                <span>{SIDEBAR_TREE_COPY.home}</span>
              </Link>
            </SidebarMenuButton>
          </SidebarMenuItem>
        )
      case `needs-you`:
        return (
          <SidebarMenuItem key={kind}>
            <SidebarMenuButton asChild density="compact" isActive={active.pathname === `/needs-you`}>
              <Link to="/needs-you" data-testid={NEEDS_YOU_TEST_IDS.entry}>
                <NavNeedsYouIcon className="h-4 w-4" />
                <span>{NEEDS_YOU_COPY.entry}</span>
              </Link>
            </SidebarMenuButton>
            <SidebarMenuTrailing className="pointer-events-none">
              <Badge count={needsYou} tone="attention" data-testid="needs-you-count-badge" />
            </SidebarMenuTrailing>
          </SidebarMenuItem>
        )
      case `chats`:
        return (
          <SidebarMenuItem key={kind}>
            <SidebarMenuButton asChild density="compact" isActive={active.pathname === `/chats`}>
              <Link to="/chats" data-testid="nav-chats">
                <NavChatsIcon className="h-4 w-4" />
                <span>{CHATS_COPY.title}</span>
              </Link>
            </SidebarMenuButton>
            <NavCountBadge value={chatsBadge} data-testid="nav-chats-badge" />
          </SidebarMenuItem>
        )
      case `drafts`:
        return (
          <SidebarMenuItem key={kind}>
            <SidebarMenuButton asChild density="compact" isActive={active.pathname === `/drafts`}>
              <Link to="/drafts" data-testid="nav-drafts">
                <NavDraftsIcon className="h-4 w-4" />
                <span>{DRAFTS_LABEL}</span>
              </Link>
            </SidebarMenuButton>
            <SidebarMenuTrailing className="pointer-events-none">
              <Badge count={drafts} tone="muted" data-testid="drafts-count-badge" />
            </SidebarMenuTrailing>
          </SidebarMenuItem>
        )
      case `devices`:
        return <SidebarDevicesRows key={kind} active={active} devices={devices} teams={teams} onAdd={onAddDevice} />
    }
  }
  return <SidebarMenu>{rows.map(row)}</SidebarMenu>
}

/** Devices: an expandable row with no nav target (a press folds it, its
 *  hover + adds a device), then one row per device (`sidebarDeviceRows`). */
function SidebarDevicesRows({
  active,
  devices,
  teams,
  onAdd,
}: {
  active: SidebarActive
  devices: readonly SidebarDevice[]
  teams: ReadonlyMap<string, Team>
  onAdd: () => void
}) {
  const [open, setOpen] = useSidebarOpenFlag(SIDEBAR_DEVICES_OPEN_KEY)
  const glyphOf = useDeviceGlyphs()
  const shown = open ? devices : []
  const guides = treeGuides([0, ...shown.map(() => 1)])
  return (
    <>
      <SidebarMenuItem data-testid="sidebar-devices">
        <SidebarMenuButton density="compact" aria-expanded={open} onClick={() => setOpen(!open)}>
          <NavDevicesIcon className="h-4 w-4" />
          <span>{SIDEBAR_TREE_COPY.devices}</span>
        </SidebarMenuButton>
        <SidebarMenuAction
          showOnHover
          className="right-7"
          title={SIDEBAR_TREE_COPY.addDevice}
          aria-label={SIDEBAR_TREE_COPY.addDevice}
          data-testid="sidebar-add-device"
          onClick={onAdd}
        >
          <AddIcon />
        </SidebarMenuAction>
        <SidebarMenuTrailing className="pointer-events-none text-muted-foreground">
          <ChevronIcon className={cn(`size-4 transition-transform`, open && `rotate-90`)} />
        </SidebarMenuTrailing>
      </SidebarMenuItem>
      {shown.map((device, index) => {
        const Glyph = glyphOf(device.id)?.icon ?? DeviceDefaultIcon
        const team = device.teamId ? teams.get(device.teamId) : undefined
        return (
          <SidebarMenuItem
            key={device.id}
            data-testid="sidebar-device-row"
            data-muted={device.muted || undefined}
            style={{ paddingLeft: `${TREE_INDENT}px` }}
          >
            <TreeGuides guide={guides[index + 1]} base={GUIDE_BASE} />
            <SidebarMenuButton
              asChild
              density="compact"
              isActive={active.device === device.id}
              className={cn(device.muted && `text-muted-foreground`)}
            >
              <Link to="/devices/$deviceId" params={{ deviceId: device.id }}>
                <Glyph className="h-4 w-4 shrink-0" />
                <span className="min-w-0 flex-1 truncate">{device.name}</span>
              </Link>
            </SidebarMenuButton>
            <SidebarMenuTrailing className="pointer-events-none group-hover/menu-item:opacity-0">
              {device.mine ? (
                !device.muted && <LiveDot tone="live" className="size-2" />
              ) : (
                <span title={team?.name} data-testid="sidebar-device-team" className="flex">
                  <TeamAvatar iconUrl={team?.iconUrl} size={14} />
                </span>
              )}
            </SidebarMenuTrailing>
            <SidebarMenuAction asChild showOnHover title={SIDEBAR_TREE_COPY.deviceSettings}>
              <Link
                to="/devices/$deviceId/settings"
                params={{ deviceId: device.id }}
                aria-label={`${SIDEBAR_TREE_COPY.deviceSettings}: ${device.name}`}
              >
                <NavSettingsIcon />
              </Link>
            </SidebarMenuAction>
          </SidebarMenuItem>
        )
      })}
    </>
  )
}

/* ─── teams ─────────────────────────────────────────────────────────────── */

export function SidebarTeamsGroup({
  teams,
  owners,
  needsYou,
  chatRows,
  me,
  active,
  onNewTeam,
  onJoinTeam,
}: {
  /** `sidebarTeams` order. */
  teams: readonly Team[]
  /** The teams the viewer owns. */
  owners: ReadonlySet<string>
  /** Needs you per team (nav-badges.json). */
  needsYou: ReadonlyMap<string, number>
  /** Every member team's Chats rows by session (a placed session's mark). */
  chatRows: ReadonlyMap<string, ChatsListRow>
  me: string | null | undefined
  active: SidebarActive
  onNewTeam: () => void
  onJoinTeam: () => void
}) {
  // #PERF-13: every team's lists read ONCE, each section its own slice.
  const teamIds = useMemo(() => teams.map((team) => team.id), [teams])
  const data = useSidebarTeamsData(teamIds, me, chatRows)
  const teamsMenu: MenuEntry[] = SIDEBAR_TEAMS_MENU.map((id) =>
    id === `new-team`
      ? { kind: `item`, id, label: SIDEBAR_TREE_COPY.newTeam, icon: AddIcon, onSelect: onNewTeam }
      : { kind: `item`, id, label: SIDEBAR_TREE_COPY.joinTeam, icon: InviteIcon, onSelect: onJoinTeam }
  )
  return (
    <SidebarGroup data-testid="sidebar-teams">
      <SidebarGroupLabel>{SIDEBAR_TREE_COPY.teams}</SidebarGroupLabel>
      <Menu
        align="start"
        aria-label={SIDEBAR_TREE_COPY.teams}
        title={SIDEBAR_TREE_COPY.teams}
        entries={teamsMenu}
        trigger={
          <SidebarGroupAction title={SIDEBAR_TREE_COPY.teams} aria-label={SIDEBAR_TREE_COPY.teams} data-testid="sidebar-teams-add">
            <AddIcon className="h-4 w-4" />
          </SidebarGroupAction>
        }
      />
      <SidebarGroupContent className="flex flex-col gap-1">
        {teams.map((team) => (
          <SidebarTeamSection
            key={team.id}
            team={team}
            owner={owners.has(team.id)}
            needsYou={needsYou.get(team.id) ?? 0}
            data={sidebarTeamSlice(data, team.id)}
            active={active}
          />
        ))}
      </SidebarGroupContent>
    </SidebarGroup>
  )
}

/** One team: its row and (open) its list. #PERF-13: its session lists are
 *  `data`, the group's one read; memoised, a synced row that moved none of
 *  THIS team's rows renders nothing here. */
const SidebarTeamSection = memo(function SidebarTeamSection({
  team,
  owner,
  needsYou,
  data,
  active,
}: {
  team: Team
  owner: boolean
  needsYou: number
  data: SidebarTeamSlice
  active: SidebarActive
}) {
  const navigate = useNavigate()
  const [open, setOpen] = useSidebarOpenFlag(sidebarTeamOpenKey(team.id))
  const boards = useTeamBoards(team.id)
  const { agents, badges, titles, chatRows } = data
  const { apps } = useApps(team.id)
  // #FEED-239 / #EXP-1837 (`railAgents`): an app's agent shows as its app,
  // the Assistant never.
  const agentRows = useMemo(
    () => railAgents(agents, apps.map((app) => ({ agentId: app.row?.agentId ?? null }))),
    [agents, apps]
  )
  const sessionInputs = data.sessions

  const itemsOf = useCallback(
    (entries: readonly SidebarEntry[]) =>
      sidebarTeamItems({
        teamId: team.id,
        boards,
        // `listAgents` order IS the default order.
        agents: agentRows.map((agent, index) => ({
          id: agent.agentId,
          sortOrder: index,
          role: agent.assistant ? `assistant` : `agent`,
        })),
        apps,
        sessions: sessionInputs,
        entries,
      }).map((item) => item.id),
    [team.id, boards, agentRows, apps, sessionInputs]
  )
  const folders = useSidebarFolders(team.id, `team`, itemsOf)

  // An Active row's drag drops into this list.
  const { dropSession } = folders
  useEffect(() => registerSidebarTeamDrop(team.id, dropSession), [team.id, dropSession])

  const boardsById = useMemo(() => new Map(boards.map((board) => [board.id, board])), [boards])
  const agentsById = useMemo(() => new Map(agentRows.map((agent) => [agent.agentId, agent])), [agentRows])
  const appsById = useMemo(() => new Map(apps.map((app) => [app.id, app])), [apps])
  // An item's kind is never stored (sidebar-tree.json): its id resolves
  // against the team's boards, agents and apps, else it is a session.
  const kindOf = (id: string): SidebarItemKind =>
    boardsById.has(id) ? `board` : agentsById.has(id) ? `agent` : appsById.has(id) ? `app` : `session`
  const here = active.teamSlug === team.slug

  const teamMenuEntries = (): MenuEntry[] =>
    sidebarTeamMenu({ owner }).flatMap((item): MenuEntry[] => {
      if (item.id === `new`) {
        return [
          {
            kind: `submenu`,
            id: item.id,
            label: SIDEBAR_TREE_COPY.new,
            icon: AddIcon,
            entries: item.children.map((kind) => ({
              kind: `item` as const,
              id: kind,
              label: SIDEBAR_TREE_COPY[kind],
              icon: NEW_KIND_ICON[kind],
              onSelect: () =>
                void navigate({
                  href: kind === `session` ? newSessionHref(team.id) : newPagePath(team.slug, kind as NewPageKind),
                }),
            })),
          },
        ]
      }
      if (item.id === `new-folder`) {
        return [
          {
            kind: `item`,
            id: item.id,
            label: SIDEBAR_TREE_COPY.newFolder,
            icon: FolderIcon,
            onSelect: () => {
              setOpen(true)
              folders.addFolder()
            },
          },
        ]
      }
      return [
        { kind: `separator`, id: `settings-separator` },
        {
          kind: `item`,
          id: item.id,
          label: SIDEBAR_TREE_COPY.settings,
          icon: NavSettingsIcon,
          onSelect: () => void navigate({ to: `/t/$teamSlug/settings`, params: { teamSlug: team.slug } }),
        },
      ]
    })

  const teamMenu = ({ target, open: menuOpen, onOpenChange }: MenuSessionProps) => (
    <Menu
      mode="pointer"
      anchor={target.anchor}
      open={menuOpen}
      onOpenChange={onOpenChange}
      returnFocus={target.origin}
      aria-label={SIDEBAR_TREE_COPY.teamActions}
      title={team.name}
      entries={teamMenuEntries()}
    />
  )

  const sessionMenu = ({ target, open: menuOpen, onOpenChange }: MenuSessionProps) => (
    <Menu
      mode="pointer"
      anchor={target.anchor}
      open={menuOpen}
      onOpenChange={onOpenChange}
      returnFocus={target.origin}
      aria-label={titles.get(target.id) ?? SIDEBAR_TREE_COPY.session}
      title={titles.get(target.id) ?? SIDEBAR_TREE_COPY.session}
      entries={[
        {
          kind: `item`,
          id: `remove`,
          label: SIDEBAR_TREE_COPY.removeFromSidebar,
          icon: RemoveIcon,
          onSelect: () => removeFromSidebar(target.id),
        },
      ]}
    />
  )

  const renderTeamRow = (itemProps: SidebarRowItemProps) => (
    <SidebarMenuItem {...itemProps} data-testid="sidebar-team-row">
      <SidebarMenuButton
        density="compact"
        aria-expanded={open}
        onClick={() => setOpen(!open)}
        {...menuProps(sidebarTeamMenuKind(team.id), team.id)}
      >
        <TeamAvatar iconUrl={team.iconUrl} size={16} />
        <span className="min-w-0 flex-1 truncate">{team.name}</span>
      </SidebarMenuButton>
      <SidebarMenuTrailing className="pointer-events-none gap-1 text-muted-foreground">
        {needsYou > 0 && (
          <Badge
            count={needsYou}
            tone="attention"
            title={NEEDS_YOU_COPY.entry}
            data-testid="sidebar-team-needs-you"
            className="group-hover/menu-item:invisible group-has-[[data-state=open]]/menu-item:invisible"
          />
        )}
        <ChevronIcon className={cn(`size-4 transition-transform`, open && `rotate-90`)} />
      </SidebarMenuTrailing>
      <Menu
        align="start"
        side="right"
        aria-label={SIDEBAR_TREE_COPY.teamActions}
        title={team.name}
        entries={teamMenuEntries()}
        trigger={
          <SidebarMenuAction
            showOnHover
            className="right-7"
            title={SIDEBAR_TREE_COPY.teamActions}
            aria-label={SIDEBAR_TREE_COPY.teamActions}
            data-testid="sidebar-team-menu-button"
          >
            <MoreIcon />
          </SidebarMenuAction>
        }
      />
    </SidebarMenuItem>
  )

  const renderItem = (id: string, itemProps: SidebarRowItemProps, guides: ReactNode): ReactNode => {
    switch (kindOf(id)) {
      case `board`: {
        const board = boardsById.get(id)
        return board ? (
          <SidebarBoardRow
            key={id}
            board={board}
            teamSlug={team.slug}
            owner={owner}
            active={here && active.board === board.slug}
            itemProps={itemProps}
            guides={guides}
          />
        ) : null
      }
      case `agent`: {
        const agent = agentsById.get(id)
        return agent ? (
          <SidebarAgentRow
            key={id}
            agent={agent}
            teamSlug={team.slug}
            active={here && active.agent === agent.agentId}
            badge={badges.get(agent.agentId)}
            itemProps={itemProps}
            guides={guides}
          />
        ) : null
      }
      case `app`: {
        const app = appsById.get(id)
        return app ? (
          <SidebarAppRow
            key={id}
            app={app}
            teamSlug={team.slug}
            active={here && active.app === app.slug}
            itemProps={itemProps}
            guides={guides}
          />
        ) : null
      }
      default:
        return (
          <SidebarSessionRow
            key={id}
            sessionId={id}
            teamId={team.id}
            teamSlug={team.slug}
            title={titles.get(id) ?? chatRows.get(id)?.title ?? ``}
            row={chatRows.get(id)}
            active={here && active.session === id}
            itemProps={itemProps}
            guides={guides}
          />
        )
    }
  }

  return (
    <SidebarFolderRows
      folders={folders}
      teamId={team.id}
      section="team"
      team={{ open, render: renderTeamRow }}
      renderItem={renderItem}
      menus={{ [sidebarTeamMenuKind(team.id)]: teamMenu, [sidebarSessionMenuKind(team.id)]: sessionMenu }}
      testId="sidebar-team-items"
      listProps={{ [SIDEBAR_TEAM_LIST_ATTR]: team.id }}
    />
  )
})

/** `Remove from sidebar`: the caller's entry for a placed session goes. */
function removeFromSidebar(targetId: string) {
  trpc.sidebarLayout.remove
    .mutate({ targetId })
    .then(({ txId }) => sidebarLayoutCollection.utils.awaitTxId(txId))
    .catch(() => toast.error(`Could not save the sidebar`))
}

/** A board: its icon in its colour, the board link, an owner's hover gear. */
function SidebarBoardRow({
  board,
  teamSlug,
  owner,
  active,
  itemProps,
  guides,
}: {
  board: Board
  teamSlug: string
  owner: boolean
  active: boolean
  itemProps: SidebarRowItemProps
  guides: ReactNode
}) {
  const TypeIcon = getBoardIcon(board)
  return (
    <SidebarMenuItem {...itemProps} data-testid="sidebar-board-row">
      {guides}
      <SidebarMenuButton asChild density="compact" isActive={active}>
        <Link to="/t/$teamSlug/boards/$boardSlug" params={{ teamSlug, boardSlug: board.slug }}>
          <TypeIcon className="h-4 w-4 shrink-0" style={{ color: board.color }} />
          <span className="min-w-0 flex-1 truncate">{board.name}</span>
        </Link>
      </SidebarMenuButton>
      {owner && (
        <SidebarMenuAction asChild showOnHover title="Board settings">
          <Link
            to="/t/$teamSlug/settings/boards/$boardId"
            params={{ teamSlug, boardId: board.id }}
            aria-label={`Settings for ${board.name}`}
          >
            <NavSettingsIcon />
          </Link>
        </SidebarMenuAction>
      )}
    </SidebarMenuItem>
  )
}

/** A placed root session: its run mark + title (like an Active row), its
 *  chat; its menu holds Remove from sidebar. */
function SidebarSessionRow({
  sessionId,
  teamId,
  teamSlug,
  title,
  row,
  active,
  itemProps,
  guides,
}: {
  sessionId: string
  teamId: string
  teamSlug: string
  title: string
  row: SidebarChatMark | undefined
  active: boolean
  itemProps: SidebarRowItemProps
  guides: ReactNode
}) {
  return (
    <SidebarMenuItem {...itemProps} data-testid="sidebar-session-row">
      {guides}
      <SidebarMenuButton asChild density="compact" isActive={active}>
        <Link
          to="/t/$teamSlug/sessions/$sessionId"
          params={{ teamSlug, sessionId }}
          {...menuProps(sidebarSessionMenuKind(teamId), sessionId)}
        >
          <ChatRunMark row={{ sessionId, needsYou: row?.needsYou ?? false, busy: row?.busy ?? false }} />
          <span
            data-unread={row?.unread || undefined}
            className={cn(`min-w-0 flex-1 truncate`, row?.unread && `font-semibold`)}
          >
            {title}
          </span>
        </Link>
      </SidebarMenuButton>
    </SidebarMenuItem>
  )
}

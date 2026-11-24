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
  MenuGestureHost,
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
import { AssistantMark } from "@/components/team/assistant-mark"
import { openHandlers, sidebarMenuEntries, SidebarPointerMenu, type SidebarMenuHandlers } from "@/components/team/sidebar-menus"
import { NEW_TAB_COPY, newTabTeamMenu, type NewTabKind, type SidebarMenuKind } from "@/lib/sidebar-menus"
import { openNewRunPage } from "@/hooks/use-work-tabs"
import { useAgents } from "@/hooks/use-session-tree"
import { useAgentBadges } from "@/components/team/sidebar-agents"

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
const DeviceDefaultIcon = conceptIcon(`ui-device-default`)

/** The New submenu's glyph per kind (the draft: board, agent, app, chat). */
export const NEW_KIND_ICON: Record<SidebarNewKind, ReturnType<typeof conceptIcon>> = {
  board: conceptIcon(`nav-boards`),
  agent: conceptIcon(`settings-agents`),
  app: conceptIcon(`nav-apps`),
  session: conceptIcon(`action-chat`),
}

/** The guide gutter starts at the menu button's own left padding. */
const GUIDE_BASE = 8

/** The team menu's kind (`menuProps`), one per team list's host. */
export const sidebarTeamMenuKind = (teamId: string) => `sidebar-team:${teamId}`
/** #EXP-2124: a team item's menu (board, app, placed session), one per team
 *  list's host; the id is the item's, its kind resolves like its row. */
export const sidebarItemMenuKind = (teamId: string) => `sidebar-item:${teamId}`
/** #EXP-2124: the root rows' menu (the Assistant, Home, Chats, Devices and
 *  each device); the id is the row's kind, a device's `device:<id>`. */
export const SIDEBAR_ROOT_MENU_KIND = `sidebar-root`
const DEVICE_MENU_PREFIX = `device:`

/** New › of a team, the tab strip's + and the team menu alike. */
export function newTeamEntries(
  team: Pick<Team, `id` | `slug`>,
  owner: boolean,
  go: (href: string) => void
): MenuEntry[] {
  return newTabTeamMenu({ owner }).map((kind: NewTabKind) => ({
    kind: `item` as const,
    id: kind,
    label: NEW_TAB_COPY[kind],
    icon: NEW_KIND_ICON[kind],
    onSelect: () => go(kind === `session` ? newSessionHref(team.id) : newPagePath(team.slug, kind as NewPageKind)),
  }))
}

/** The viewer's Assistant, as the root row and the new-tab menu read it. */
export interface SidebarAssistant {
  agentId: string
  name: string
  /** The team its chat route runs under. */
  teamSlug: string
  badge?: CountBadgeValue
}

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

/** The viewer's Assistant under the layout's team (the FAB's own read). */
export function useSidebarAssistant(
  team: Pick<Team, `id`> | null | undefined,
  teamSlug: string,
  me: string | null | undefined
): SidebarAssistant | null {
  const { agents } = useAgents(team?.id, me)
  const assistant = useMemo(() => agents.find((agent) => agent.assistant) ?? null, [agents])
  const rows = useMemo(() => (assistant ? [assistant] : []), [assistant])
  const badge = useAgentBadges(team?.id, rows, me).get(assistant?.agentId ?? ``)
  return useMemo(
    () => (assistant ? { agentId: assistant.agentId, name: assistant.name, teamSlug, badge } : null),
    [assistant, teamSlug, badge]
  )
}

/* ─── root rows ─────────────────────────────────────────────────────────── */

export function SidebarRootRows({
  active,
  needsYou,
  drafts,
  chatsBadge,
  devices,
  teams,
  assistant,
  onAddDevice,
}: {
  active: SidebarActive
  /** #EXP-2124: the viewer's Assistant, the first row. */
  assistant: SidebarAssistant | null
  needsYou: number
  drafts: number
  chatsBadge: CountBadgeValue
  devices: readonly SidebarDevice[]
  /** Member teams by id: a teammate's server wears its team's mark. */
  teams: ReadonlyMap<string, Team>
  onAddDevice: () => void
}) {
  const rows = sidebarRootRows({ needsYou, drafts })
  const navigate = useNavigate()
  const assistantHref = assistant
    ? `/t/${encodeURIComponent(assistant.teamSlug)}/agents/${encodeURIComponent(assistant.agentId)}`
    : null
  // #EXP-2124: every root row's menu off THE table.
  const rootMenu = (session: MenuSessionProps): ReactNode => {
    const id = session.target.id
    const go = (href: string) => () => void navigate({ href })
    const newChat = () => openNewRunPage(navigate)
    if (id.startsWith(DEVICE_MENU_PREFIX)) {
      const device = devices.find((entry) => entry.id === id.slice(DEVICE_MENU_PREFIX.length))
      if (!device) return null
      const href = `/devices/${encodeURIComponent(device.id)}`
      return (
        <SidebarPointerMenu
          session={session}
          title={device.name}
          entries={sidebarMenuEntries(`device`, { mine: device.mine }, { ...openHandlers(href, go(href)), settings: go(`${href}/settings`) })}
        />
      )
    }
    const kind = id as SidebarMenuKind
    const handlers: SidebarMenuHandlers =
      kind === `assistant` && assistantHref
        ? { ...openHandlers(assistantHref, go(assistantHref)), settings: go(`/settings/assistant`) }
        : kind === `devices`
          ? { "add-device": onAddDevice }
          : { ...openHandlers(`/${kind}`, go(`/${kind}`)), "new-chat": newChat }
    const title =
      kind === `assistant` ? (assistant?.name ?? ``) : kind === `devices` ? SIDEBAR_TREE_COPY.devices : kind === `chats` ? CHATS_COPY.title : kind
    return <SidebarPointerMenu session={session} title={title} entries={sidebarMenuEntries(kind, {}, handlers)} />
  }
  const row = (kind: SidebarRootRow): ReactNode => {
    switch (kind) {
      case `home`:
        return (
          <SidebarMenuItem key={kind}>
            <SidebarMenuButton asChild density="compact" isActive={active.pathname === `/home`}>
              <Link to="/home" data-testid="sidebar-home" {...menuProps(SIDEBAR_ROOT_MENU_KIND, kind)}>
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
              <Link to="/needs-you" data-testid={NEEDS_YOU_TEST_IDS.entry} {...menuProps(SIDEBAR_ROOT_MENU_KIND, kind)}>
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
              <Link to="/chats" data-testid="nav-chats" {...menuProps(SIDEBAR_ROOT_MENU_KIND, kind)}>
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
              <Link to="/drafts" data-testid="nav-drafts" {...menuProps(SIDEBAR_ROOT_MENU_KIND, kind)}>
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
  return (
    <MenuGestureHost menus={{ [SIDEBAR_ROOT_MENU_KIND]: rootMenu }}>
      <SidebarMenu>
        {/* #EXP-2124: the Assistant (its name + the brand mark), above Home. */}
        {assistant && (
          <SidebarMenuItem>
            <SidebarMenuButton asChild density="compact" isActive={active.agent === assistant.agentId}>
              <Link
                to="/t/$teamSlug/agents/$agentId"
                params={{ teamSlug: assistant.teamSlug, agentId: assistant.agentId }}
                data-testid="sidebar-assistant"
                {...menuProps(SIDEBAR_ROOT_MENU_KIND, `assistant`)}
              >
                <AssistantMark />
                <span className="min-w-0 flex-1 truncate">{assistant.name}</span>
              </Link>
            </SidebarMenuButton>
            {assistant.badge && <NavCountBadge value={assistant.badge} data-testid="sidebar-assistant-badge" />}
          </SidebarMenuItem>
        )}
        {rows.map(row)}
      </SidebarMenu>
    </MenuGestureHost>
  )
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
        <SidebarMenuButton
          density="compact"
          aria-expanded={open}
          onClick={() => setOpen(!open)}
          {...menuProps(SIDEBAR_ROOT_MENU_KIND, `devices`)}
        >
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
              <Link
                to="/devices/$deviceId"
                params={{ deviceId: device.id }}
                {...menuProps(SIDEBAR_ROOT_MENU_KIND, `${DEVICE_MENU_PREFIX}${device.id}`)}
              >
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

  const go = (href: string) => void navigate({ href })
  const teamMenuEntries = (): MenuEntry[] =>
    sidebarMenuEntries(
      `team`,
      { owner },
      {
        new: newTeamEntries(team, owner, go),
        "new-folder": () => {
          setOpen(true)
          folders.addFolder()
        },
        settings: () => void navigate({ to: `/t/$teamSlug/settings`, params: { teamSlug: team.slug } }),
        leave: () => void navigate({ to: `/t/$teamSlug/settings/members`, params: { teamSlug: team.slug } }),
      }
    )

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

  // #EXP-2124: a board's, an app's and a placed session's menu off THE table.
  const itemMenu = (session: MenuSessionProps): ReactNode => {
    const id = session.target.id
    const kind = kindOf(id)
    const base = `/t/${encodeURIComponent(team.slug)}`
    const move: MenuEntry[] = [
      ...folders.rows
        .filter((row) => row.group)
        .map((row) => ({
          kind: `item` as const,
          id: row.id,
          label: row.name ?? SIDEBAR_TREE_COPY.newFolder,
          icon: FolderIcon,
          onSelect: () => folders.move({ id, into: row.id }),
        })),
      { kind: `separator`, id: `new-folder-separator` },
      {
        kind: `item`,
        id: `new-folder`,
        label: SIDEBAR_TREE_COPY.newFolder,
        icon: AddIcon,
        onSelect: () => void toast(`${SIDEBAR_TREE_COPY.newFolder}: with the build`),
      },
    ]
    if (kind === `board`) {
      const board = boardsById.get(id)
      if (!board) return null
      const href = `${base}/boards/${encodeURIComponent(board.slug)}`
      return (
        <SidebarPointerMenu
          session={session}
          title={board.name}
          entries={sidebarMenuEntries(`board`, { owner }, {
            ...openHandlers(href, () => go(href)),
            move,
            settings: () => go(`${base}/settings/boards/${board.id}`),
          })}
        />
      )
    }
    if (kind === `app`) {
      const app = appsById.get(id)
      if (!app) return null
      const href = `${base}/apps/${encodeURIComponent(app.slug)}`
      const editor = `${base}/app-editor/${encodeURIComponent(app.slug)}`
      return (
        <SidebarPointerMenu
          session={session}
          title={app.name}
          entries={sidebarMenuEntries(`app`, { owner }, {
            ...openHandlers(href, () => go(href)),
            edit: () => go(editor),
            move,
            settings: () => go(`${editor}?tab=settings`),
          })}
        />
      )
    }
    const href = `${base}/sessions/${id}`
    const title = titles.get(id) ?? SIDEBAR_TREE_COPY.session
    return (
      <SidebarPointerMenu
        session={session}
        title={title}
        entries={sidebarMenuEntries(`session`, { canDelete: true }, {
          ...openHandlers(href, () => go(href)),
          move,
          "remove-from-sidebar": () => removeFromSidebar(id),
        })}
      />
    )
  }

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
            teamId={team.id}
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
            menu={menuProps(sidebarItemMenuKind(team.id), app.id)}
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
      menus={{ [sidebarTeamMenuKind(team.id)]: teamMenu, [sidebarItemMenuKind(team.id)]: itemMenu }}
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
  teamId,
  owner,
  active,
  itemProps,
  guides,
}: {
  board: Board
  teamSlug: string
  teamId: string
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
        <Link
          to="/t/$teamSlug/boards/$boardSlug"
          params={{ teamSlug, boardSlug: board.slug }}
          {...menuProps(sidebarItemMenuKind(teamId), board.id)}
        >
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
          {...menuProps(sidebarItemMenuKind(teamId), sessionId)}
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

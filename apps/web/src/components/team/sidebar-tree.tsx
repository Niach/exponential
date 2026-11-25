import { memo, useCallback, useEffect, useMemo, useState, type ReactNode } from "react"
import { Link, useNavigate } from "@tanstack/react-router"
import { CHATS_COPY, railAgents } from "@exp/domain-contract/chats"
import { NEEDS_YOU_COPY, NEEDS_YOU_TEST_IDS } from "@exp/domain-contract/needs-you"
import type { CountBadgeValue } from "@exp/domain-contract/nav-badges"
import type { SidebarEntry } from "@exp/domain-contract/sidebar-folders"
import { SIDEBAR_MENU_COPY, sidebarMoveTargets, type SidebarMenuKind } from "@exp/domain-contract/sidebar-menus"
import {
  SIDEBAR_DEVICES_OPEN_KEY,
  SIDEBAR_TREE_COPY,
  sidebarRootRows,
  sidebarTeamItems,
  sidebarTeamOpenKey,
  type SidebarItemKind,
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
import { failedAction, SKIP_ERROR_TOAST } from "@/lib/action-error"
import { sidebarLayoutCollection } from "@/lib/collections"
import { trpc } from "@/lib/trpc-client"
import { cn } from "@/lib/utils"
import { NEW_RUN_TAB, type WorkTab } from "@/lib/work-tabs"
import { useApps } from "@/hooks/use-apps"
import { useTeamBoardsWithReady } from "@/hooks/use-team-data"
import { sidebarTeamSlice, useSidebarTeamsData, type SidebarChatMark, type SidebarTeamSlice } from "@/hooks/use-sidebar-teams-data"
import { useDeviceGlyphs } from "@/hooks/use-device-glyphs"
import { useSidebarFolders, type SidebarFolders } from "@/hooks/use-sidebar-folders"
import { useOpenNewDraft } from "@/hooks/use-open-new-draft"
import { openNewRunPage } from "@/hooks/use-work-tabs"
import type { SidebarAssistant } from "@/hooks/use-sidebar-assistant"
import {
  registerSidebarTeamDrop,
  SIDEBAR_TEAM_LIST_ATTR,
  useSidebarOpenFlag,
  type SidebarDevice,
} from "@/hooks/use-sidebar-tree"
import { SidebarFolderRows, SidebarNameInput, useSidebarTreeDensity, type SidebarRowItemProps } from "@/components/team/sidebar-folders"
import { SidebarAgentRow } from "@/components/team/sidebar-agents"
import { SidebarAppRow } from "@/components/team/sidebar-apps"
import { ChatRunMark } from "@/components/team/sidebar-running"
import { NavCountBadge } from "@/components/team/sidebar-shared"
import { DRAFTS_LABEL, NavDraftsIcon } from "@/components/team/sidebar-nav-entries"
import { AssistantMark } from "@/components/team/assistant-mark"
import {
  headerMenuEntries,
  newTeamEntries,
  openHandlers,
  sidebarMenuEntries,
  SidebarPointerMenu,
} from "@/components/team/sidebar-menus"
import { useBoardArchive } from "@/components/team/board-archive"
import { useAgentSends } from "@/components/chat/chat-menu"

// #EXP-2027 (fixtures/sidebar-tree.json, its `$comment` is the spec): the
// web md+ main menu as ONE tree, with no active team. The root rows
// (`sidebarRootRows`: #EXP-2124 the Assistant first, then Home, Needs you and
// Drafts while they count, Chats, Devices with each device a row), the
// `Teams` label (its + = New team / Join team), then one row per team
// (`sidebarTeams`) holding that team's ONE mixed list (`sidebarTeamItems`
// through `SidebarFolderRows`, section `team`): boards, agents, apps and the
// root sessions the member placed. A team row folds on a press. Open flags
// are device-local (open by default).
//
// #EXP-2124 (fixtures/sidebar-menus.json): EVERY row's right-click (a
// long-press on touch) opens ITS kind's menu off THE table
// (`sidebarMenuEntries`): the root rows through one host here, a team's rows
// through its List's host, the team row's `…` the same menu as its
// right-click.

const NavHomeIcon = conceptIcon(`nav-home`)
const NavNeedsYouIcon = conceptIcon(`nav-needs-you`)
const NavChatsIcon = conceptIcon(`nav-chats`)
const NavDevicesIcon = conceptIcon(`nav-devices`)
const NavSettingsIcon = conceptIcon(`nav-settings`)
const ChevronIcon = conceptIcon(`ui-chevron-right`)
const AddIcon = conceptIcon(`ui-add`)
const MoreIcon = conceptIcon(`ui-more`)
const FolderIcon = conceptIcon(`ui-folder`)
const DeviceDefaultIcon = conceptIcon(`ui-device-default`)

/** The guide gutter starts at the menu button's own left padding. */
const GUIDE_BASE = 8

/** The team menu's kind (`menuProps`), one per team list's host. */
export const sidebarTeamMenuKind = (teamId: string) => `sidebar-team:${teamId}`
/** #EXP-2124: a team item's menu (board, agent, app, placed session), one
 *  per team list's host; the id is the item's, its kind resolves like its
 *  row. */
export const sidebarItemMenuKind = (teamId: string) => `sidebar-item:${teamId}`
/** #EXP-2124: the root rows' menu (the Assistant, Home, Needs you, Chats,
 *  Drafts, Devices and each device); the id is the row's kind, a device's
 *  `device:<id>`. */
export const SIDEBAR_ROOT_MENU_KIND = `sidebar-root`
/** #EXP-2124: the Teams label's menu (New team, Join team). */
export const SIDEBAR_TEAMS_MENU_KIND = `sidebar-teams`
const DEVICE_MENU_PREFIX = `device:`

export type { SidebarAssistant }

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

/** The team-less root pages (a tab each, sidebar-menus.json `tabs`). */
const ROOT_PAGES = {
  home: { href: `/home`, title: SIDEBAR_TREE_COPY.home, tab: NEW_RUN_TAB as WorkTab },
  "needs-you": { href: `/needs-you`, title: NEEDS_YOU_COPY.entry, tab: { kind: `page`, page: `needs-you`, from: null } as WorkTab },
  chats: { href: `/chats`, title: CHATS_COPY.title, tab: { kind: `page`, page: `chats`, from: null } as WorkTab },
  drafts: { href: `/drafts`, title: DRAFTS_LABEL, tab: { kind: `page`, page: `drafts`, from: null } as WorkTab },
} as const

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
  const rows = sidebarRootRows({ needsYou, drafts, assistant: assistant !== null })
  const navigate = useNavigate()
  // #EXP-2124: the device whose name is being edited in place (Rename).
  const [renaming, setRenaming] = useState<string | null>(null)
  const rootMenu = (session: MenuSessionProps): ReactNode => {
    const id = session.target.id
    const go = (href: string) => () => void navigate({ href })
    if (id.startsWith(DEVICE_MENU_PREFIX)) {
      const device = devices.find((entry) => entry.id === id.slice(DEVICE_MENU_PREFIX.length))
      if (!device) return null
      const href = `/devices/${encodeURIComponent(device.id)}`
      return (
        <SidebarPointerMenu
          session={session}
          title={device.name}
          entries={sidebarMenuEntries(`device`, { mine: device.mine }, {
            ...openHandlers(href, go(href), { kind: `device`, deviceId: device.id, from: null }),
            // After the menu has closed and handed focus back to the row,
            // or that focus would blur (and end) the rename at once.
            rename: () => void setTimeout(() => setRenaming(device.id), 0),
            settings: go(`${href}/settings`),
          })}
        />
      )
    }
    if (id === `assistant`) {
      return assistant ? <AssistantMenu session={session} assistant={assistant} /> : null
    }
    if (id === `devices`) {
      return (
        <SidebarPointerMenu
          session={session}
          title={SIDEBAR_TREE_COPY.devices}
          entries={sidebarMenuEntries(`devices`, {}, { "add-device": onAddDevice })}
        />
      )
    }
    const page = ROOT_PAGES[id as keyof typeof ROOT_PAGES]
    if (!page) return null
    return (
      <SidebarPointerMenu
        session={session}
        title={page.title}
        entries={sidebarMenuEntries(id as SidebarMenuKind, {}, {
          ...openHandlers(page.href, go(page.href), page.tab),
          "new-chat": () => openNewRunPage(navigate),
        })}
      />
    )
  }
  const row = (kind: SidebarRootRow): ReactNode => {
    switch (kind) {
      case `assistant`:
        return assistant ? (
          <SidebarMenuItem key={kind}>
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
        ) : null
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
        return (
          <SidebarDevicesRows
            key={kind}
            active={active}
            devices={devices}
            teams={teams}
            onAdd={onAddDevice}
            renaming={renaming}
            onRenamed={() => setRenaming(null)}
          />
        )
    }
  }
  return (
    <MenuGestureHost menus={{ [SIDEBAR_ROOT_MENU_KIND]: rootMenu }}>
      <SidebarMenu>{rows.map(row)}</SidebarMenu>
    </MenuGestureHost>
  )
}

/** The Assistant row's menu: its chat, Compact / New conversation (THE
 *  agent sends) and Settings > Assistant. */
function AssistantMenu({ session, assistant }: { session: MenuSessionProps; assistant: SidebarAssistant }) {
  const navigate = useNavigate()
  const sends = useAgentSends(assistant.agentId)
  const go = (href: string) => () => void navigate({ href })
  return (
    <SidebarPointerMenu
      session={session}
      title={assistant.name}
      entries={sidebarMenuEntries(`assistant`, {}, {
        ...openHandlers(assistant.href, go(assistant.href), {
          kind: `agent`,
          agentId: assistant.agentId,
          from: null,
          teamId: assistant.teamId,
          teamSlug: assistant.teamSlug,
        }),
        ...sends,
        settings: go(`/settings/assistant`),
      })}
    />
  )
}

/** #EXP-2124 device Rename: the name in place (the device settings' rename). */
export function renameDevice(deviceId: string, label: string, current: string, onRenamed?: () => void) {
  const next = label.trim()
  if (!next || next === current) return
  void trpc.devices.rename
    .mutate({ deviceId, label: next }, SKIP_ERROR_TOAST)
    .then(() => onRenamed?.())
    .catch((error: unknown) => failedAction(`rename`, `the device`, error))
}

/** Devices: an expandable row with no nav target (a press folds it, its
 *  hover + adds a device), then one row per device (`sidebarDeviceRows`). */
function SidebarDevicesRows({
  active,
  devices,
  teams,
  onAdd,
  renaming,
  onRenamed,
}: {
  active: SidebarActive
  devices: readonly SidebarDevice[]
  teams: ReadonlyMap<string, Team>
  onAdd: () => void
  /** The device whose name is in edit. */
  renaming: string | null
  onRenamed: () => void
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
        const editing = renaming === device.id
        return (
          <SidebarMenuItem
            key={device.id}
            data-testid="sidebar-device-row"
            data-muted={device.muted || undefined}
            style={{ paddingLeft: `${TREE_INDENT}px` }}
          >
            <TreeGuides guide={guides[index + 1]} base={GUIDE_BASE} />
            {editing ? (
              <SidebarMenuButton density="compact">
                <Glyph className="h-4 w-4 shrink-0" />
                <SidebarNameInput
                  label={SIDEBAR_MENU_COPY.rename}
                  testId="sidebar-device-name"
                  initial={device.name}
                  onSave={(name) => {
                    onRenamed()
                    renameDevice(device.id, name, device.name)
                  }}
                  onCancel={onRenamed}
                />
              </SidebarMenuButton>
            ) : (
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
            )}
            {!editing && (
              <>
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
              </>
            )}
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
  // #EXP-2124: the label's + and its right-click are THE table's `teams` row.
  const teamsMenu = () => sidebarMenuEntries(`teams`, {}, { "new-team": onNewTeam, "join-team": onJoinTeam })
  return (
    <SidebarGroup data-testid="sidebar-teams">
      <MenuGestureHost
        menus={{
          [SIDEBAR_TEAMS_MENU_KIND]: (session) => (
            <SidebarPointerMenu session={session} title={SIDEBAR_TREE_COPY.teams} entries={teamsMenu()} />
          ),
        }}
      >
        <SidebarGroupLabel {...menuProps(SIDEBAR_TEAMS_MENU_KIND, `teams`)}>{SIDEBAR_TREE_COPY.teams}</SidebarGroupLabel>
      </MenuGestureHost>
      <Menu
        align="start"
        aria-label={SIDEBAR_TREE_COPY.teams}
        title={SIDEBAR_TREE_COPY.teams}
        entries={teamsMenu()}
        trigger={
          <SidebarGroupAction title={SIDEBAR_TREE_COPY.teams} aria-label={SIDEBAR_TREE_COPY.teams} data-testid="sidebar-teams-add">
            <AddIcon className="h-4 w-4" />
          </SidebarGroupAction>
        }
      />
      <SidebarGroupContent className="flex flex-col gap-1">
        <SidebarTeamSections teams={teams} owners={owners} needsYou={needsYou} chatRows={chatRows} me={me} active={active} />
      </SidebarGroupContent>
    </SidebarGroup>
  )
}

/** Every team's tree (the sidebar's Teams group and, at touch density, the
 *  phones' Teams tab). #PERF-13: the lists are read ONCE, each section its
 *  own slice. */
export function SidebarTeamSections({
  teams,
  owners,
  needsYou,
  chatRows,
  me,
  active,
  sessionBadges,
  onCount,
}: {
  teams: readonly Team[]
  owners: ReadonlySet<string>
  needsYou: ReadonlyMap<string, number>
  chatRows: ReadonlyMap<string, ChatsListRow>
  me: string | null | undefined
  active: SidebarActive
  /** #EXP-2109: a placed session's count badge (touch density). */
  sessionBadges?: ReadonlyMap<string, CountBadgeValue>
  /** #EXP-2109: a team's top-level node count, once its lists are in. */
  onCount?: (teamId: string, count: number) => void
}) {
  const teamIds = useMemo(() => teams.map((team) => team.id), [teams])
  const data = useSidebarTeamsData(teamIds, me, chatRows)
  return teams.map((team) => (
    <SidebarTeamSection
      key={team.id}
      team={team}
      owner={owners.has(team.id)}
      needsYou={needsYou.get(team.id) ?? 0}
      data={sidebarTeamSlice(data, team.id)}
      active={active}
      sessionBadges={sessionBadges}
      onCount={onCount}
    />
  ))
}

/** #EXP-2124 Move to folder ›: the team's folders (into that folder; not the
 *  one holding it), then New folder (a new folder holding it, its name in
 *  edit) and, for an item inside a folder, Top level. */
export function moveEntries(folders: Pick<SidebarFolders, `rows` | `move` | `moveToNewFolder`>, id: string): MenuEntry[] {
  const targets = sidebarMoveTargets(folders.rows, id)
  const holder = targets.topLevel
  const into = targets.folders
    .map(
      (row): MenuEntry => ({
        kind: `item`,
        id: `folder-${row.id}`,
        label: row.name ?? SIDEBAR_TREE_COPY.newFolder,
        icon: FolderIcon,
        onSelect: () => folders.move({ id, into: row.id }),
      })
    )
  return [
    ...into,
    ...(into.length > 0 ? [{ kind: `separator` as const, id: `move-separator` }] : []),
    {
      kind: `item`,
      id: `new-folder`,
      label: SIDEBAR_MENU_COPY[`new-folder`],
      icon: FolderIcon,
      "data-testid": `sidebar-move-new-folder`,
      onSelect: () => folders.moveToNewFolder(id),
    },
    ...(holder
      ? [
          {
            kind: `item` as const,
            id: `top-level`,
            label: SIDEBAR_MENU_COPY[`top-level`],
            "data-testid": `sidebar-move-top-level`,
            onSelect: () => folders.move({ id, after: holder as string }),
          },
        ]
      : []),
  ]
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
  sessionBadges,
  onCount,
}: {
  team: Team
  owner: boolean
  needsYou: number
  data: SidebarTeamSlice
  active: SidebarActive
  sessionBadges?: ReadonlyMap<string, CountBadgeValue>
  onCount?: (teamId: string, count: number) => void
}) {
  const navigate = useNavigate()
  const density = useSidebarTreeDensity()
  const touch = density === `touch`
  const [open, setOpen] = useSidebarOpenFlag(sidebarTeamOpenKey(team.id))
  const { boards, boardsReady } = useTeamBoardsWithReady(team.id)
  const { agents, badges, titles, chatRows } = data
  const { apps } = useApps(team.id)
  const openNewDraft = useOpenNewDraft(team.slug)
  // #EXP-2124 board Archive: the board settings' archive behind its confirm.
  const here = active.teamSlug === team.slug
  const boardArchive = useBoardArchive((board) => here && active.board === board.slug)
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

  const count = boardsReady ? folders.items.length : null
  useEffect(() => {
    if (count !== null) onCount?.(team.id, count)
  }, [onCount, team.id, count])

  // An Active row's drag (and its Add to sidebar) drops into this list.
  const { dropSession } = folders
  useEffect(() => registerSidebarTeamDrop(team.id, dropSession), [team.id, dropSession])

  const boardsById = useMemo(() => new Map(boards.map((board) => [board.id, board])), [boards])
  const agentsById = useMemo(() => new Map(agentRows.map((agent) => [agent.agentId, agent])), [agentRows])
  const appsById = useMemo(() => new Map(apps.map((app) => [app.id, app])), [apps])
  // An item's kind is never stored (sidebar-tree.json): its id resolves
  // against the team's boards, agents and apps, else it is a session.
  const kindOf = (id: string): SidebarItemKind =>
    boardsById.has(id) ? `board` : agentsById.has(id) ? `agent` : appsById.has(id) ? `app` : `session`
  const base = `/t/${encodeURIComponent(team.slug)}`
  const tabTeam = { teamId: team.id, teamSlug: team.slug }

  const go = (href: string) => void navigate({ href })
  // #EXP-2124: the team row's right-click and its `…` alike.
  const teamMenuEntries = (): MenuEntry[] =>
    sidebarMenuEntries(`team`, { owner }, {
      new: newTeamEntries(team, owner, go),
      "new-folder": () => {
        setOpen(true)
        folders.addFolder()
      },
      settings: () => void navigate({ to: `/t/$teamSlug/settings`, params: { teamSlug: team.slug } }),
    })

  const teamMenu = (session: MenuSessionProps) => (
    <SidebarPointerMenu session={session} title={team.name} entries={teamMenuEntries()} />
  )

  // #EXP-2124: a board's, an agent's, an app's and a placed session's menu
  // off THE table.
  const itemMenu = (session: MenuSessionProps): ReactNode => {
    const id = session.target.id
    const move = moveEntries(folders, id)
    switch (kindOf(id)) {
      case `board`: {
        const board = boardsById.get(id)
        if (!board) return null
        const href = `${base}/boards/${encodeURIComponent(board.slug)}`
        return (
          <SidebarPointerMenu
            session={session}
            title={board.name}
            entries={sidebarMenuEntries(`board`, { owner }, {
              ...openHandlers(href, () => go(href), { kind: `board`, boardId: board.id, from: null, ...tabTeam }),
              "new-issue": () => openNewDraft({ boardId: board.id }),
              move,
              settings: () => go(`${base}/settings/boards/${board.id}`),
              archive: () => boardArchive.ask(board),
            })}
          />
        )
      }
      case `app`: {
        const app = appsById.get(id)
        if (!app) return null
        const href = `${base}/apps/${encodeURIComponent(app.slug)}`
        const editor = `${base}/app-editor/${encodeURIComponent(app.slug)}`
        return (
          <SidebarPointerMenu
            session={session}
            title={app.name}
            entries={sidebarMenuEntries(`app`, { owner }, {
              ...openHandlers(href, () => go(href), { kind: `app`, appId: app.id, from: null, ...tabTeam }),
              edit: () => go(editor),
              move,
              settings: () => go(`${editor}?tab=settings`),
            })}
          />
        )
      }
      case `agent`: {
        const agent = agentsById.get(id)
        if (!agent) return null
        return (
          <AgentItemMenu
            session={session}
            agentId={agent.agentId}
            name={agent.name}
            teamSlug={team.slug}
            tab={{ kind: `agent`, agentId: agent.agentId, from: null, ...tabTeam }}
            owner={owner}
            move={move}
          />
        )
      }
      default: {
        const href = `${base}/sessions/${encodeURIComponent(id)}`
        return (
          <SidebarPointerMenu
            session={session}
            title={titles.get(id) ?? chatRows.get(id)?.title ?? SIDEBAR_TREE_COPY.session}
            entries={sidebarMenuEntries(`session`, {}, {
              ...openHandlers(href, () => go(href), { kind: `chat`, chatId: id, from: null, ...tabTeam }),
              move,
              "remove-from-sidebar": () => removeFromSidebar(id),
            })}
          />
        )
      }
    }
  }

  const renderTeamRow = (itemProps: SidebarRowItemProps) => (
    <SidebarMenuItem {...itemProps} data-testid="sidebar-team-row" data-team-slug={team.slug}>
      <SidebarMenuButton
        density={density}
        className={touch ? `pr-36!` : `group-hover/menu-item:pr-18!`}
        aria-expanded={open}
        onClick={() => setOpen(!open)}
        {...menuProps(sidebarTeamMenuKind(team.id), team.id)}
      >
        <TeamAvatar iconUrl={team.iconUrl} size={touch ? 20 : 16} />
        <span className="min-w-0 flex-1 truncate">{team.name}</span>
      </SidebarMenuButton>
      <SidebarMenuTrailing className={cn(`pointer-events-none gap-1 text-muted-foreground`, touch && `right-4 justify-end`)}>
        {needsYou > 0 && (
          <Badge
            count={needsYou}
            tone="attention"
            title={NEEDS_YOU_COPY.entry}
            data-testid="sidebar-team-needs-you"
            className={
              touch ? `mr-18` : `group-hover/menu-item:invisible group-has-[[data-state=open]]/menu-item:invisible`
            }
          />
        )}
        <ChevronIcon className={cn(`size-4 transition-transform`, open && `rotate-90`)} />
      </SidebarMenuTrailing>
      {/* header-actions.json `rows.team`: the +, then the `…`. */}
      <Menu
        align="start"
        side="right"
        aria-label={SIDEBAR_TREE_COPY.new}
        title={team.name}
        entries={newTeamEntries(team, owner, go)}
        trigger={
          <SidebarMenuAction
            showOnHover
            className={touch ? `inset-y-0 right-19 my-auto top-0!` : `right-12`}
            title={SIDEBAR_TREE_COPY.new}
            aria-label={`${SIDEBAR_TREE_COPY.new}: ${team.name}`}
            data-testid="sidebar-team-new-button"
          >
            <AddIcon />
          </SidebarMenuAction>
        }
      />
      <Menu
        align="start"
        side="right"
        aria-label={SIDEBAR_TREE_COPY.teamActions}
        title={team.name}
        entries={headerMenuEntries(teamMenuEntries())}
        trigger={
          <SidebarMenuAction
            showOnHover
            className={touch ? `inset-y-0 right-10 my-auto top-0!` : `right-7`}
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

  const itemMenuProps = (id: string) => menuProps(sidebarItemMenuKind(team.id), id)

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
            menu={itemMenuProps(board.id)}
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
            menu={itemMenuProps(agent.agentId)}
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
            menu={itemMenuProps(app.id)}
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
            teamSlug={team.slug}
            menu={itemMenuProps(id)}
            title={titles.get(id) ?? chatRows.get(id)?.title ?? ``}
            row={chatRows.get(id)}
            badge={sessionBadges?.get(id)}
            active={here && active.session === id}
            itemProps={itemProps}
            guides={guides}
          />
        )
    }
  }

  return (
    <>
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
      {boardArchive.dialog}
    </>
  )
})

/** An agent row's menu: its chat, THE agent sends, Move to folder and (an
 *  owner) its settings as the screen's opened item. */
function AgentItemMenu({
  session,
  agentId,
  name,
  teamSlug,
  tab,
  owner,
  move,
}: {
  session: MenuSessionProps
  agentId: string
  name: string
  teamSlug: string
  tab: WorkTab
  owner: boolean
  move: MenuEntry[]
}) {
  const navigate = useNavigate()
  const sends = useAgentSends(agentId)
  const href = `/t/${encodeURIComponent(teamSlug)}/agents/${encodeURIComponent(agentId)}`
  return (
    <SidebarPointerMenu
      session={session}
      title={name}
      entries={sidebarMenuEntries(`agent`, { owner }, {
        ...openHandlers(href, () => void navigate({ href }), tab),
        ...sends,
        move,
        settings: () =>
          void navigate({ to: `/t/$teamSlug/agents/$agentId`, params: { teamSlug, agentId }, search: { open: `settings` } }),
      })}
    />
  )
}

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
  menu,
  active,
  itemProps,
  guides,
}: {
  board: Board
  teamSlug: string
  owner: boolean
  /** #EXP-2124: the row's context-menu target (`menuProps`). */
  menu: ReturnType<typeof menuProps>
  active: boolean
  itemProps: SidebarRowItemProps
  guides: ReactNode
}) {
  const density = useSidebarTreeDensity()
  const TypeIcon = getBoardIcon(board)
  return (
    <SidebarMenuItem {...itemProps} data-testid="sidebar-board-row">
      {guides}
      <SidebarMenuButton asChild density={density} isActive={active}>
        <Link to="/t/$teamSlug/boards/$boardSlug" params={{ teamSlug, boardSlug: board.slug }} {...menu}>
          <TypeIcon className="h-4 w-4 shrink-0" style={{ color: board.color }} />
          <span className="min-w-0 flex-1 truncate">{board.name}</span>
        </Link>
      </SidebarMenuButton>
      {owner && density === `compact` && (
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
 *  chat. */
function SidebarSessionRow({
  sessionId,
  teamSlug,
  menu,
  title,
  row,
  badge,
  active,
  itemProps,
  guides,
}: {
  sessionId: string
  teamSlug: string
  menu: ReturnType<typeof menuProps>
  title: string
  row: SidebarChatMark | undefined
  badge?: CountBadgeValue
  active: boolean
  itemProps: SidebarRowItemProps
  guides: ReactNode
}) {
  const density = useSidebarTreeDensity()
  return (
    <SidebarMenuItem {...itemProps} data-testid="sidebar-session-row">
      {guides}
      <SidebarMenuButton asChild density={density} isActive={active}>
        <Link to="/t/$teamSlug/sessions/$sessionId" params={{ teamSlug, sessionId }} {...menu}>
          <ChatRunMark row={{ sessionId, needsYou: row?.needsYou ?? false, busy: row?.busy ?? false }} />
          <span
            data-unread={row?.unread || undefined}
            className={cn(`min-w-0 flex-1 truncate`, row?.unread && `font-semibold`)}
          >
            {title}
          </span>
        </Link>
      </SidebarMenuButton>
      {density === `touch` && badge && badge.count > 0 && (
        <SidebarMenuTrailing className="pointer-events-none right-4">
          <Badge count={badge.count} tone={badge.tone} />
        </SidebarMenuTrailing>
      )}
    </SidebarMenuItem>
  )
}

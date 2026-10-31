import { APPS_COPY } from "@exp/domain-contract/apps"
import {
  SETTINGS_NAV_CHILDREN,
  SETTINGS_NAV_GROUPS,
  settingsNavItemVisible,
  settingsNavPage,
  settingsNavRow,
} from "@exp/domain-contract/settings-nav"
import { ASSISTANT_COPY } from "@exp/domain-contract/agent-settings"
// Shared model for the team-settings pages (EXP-146): the grouped nav
// definition consumed by the layout route (sidebar) and the index route
// (redirect to the first visible section), plus the per-section access guard
// and the data hook every section page needs.
import { useEffect, useState } from "react"
import { type LucideIcon } from "lucide-react"
import { useSession } from "@/hooks/use-session"
import { useTeamBySlug, useTeamUsers } from "@/hooks/use-team-data"
import {
  useTeamPermissions,
  type TeamPermissions,
} from "@/hooks/use-team-permissions"
import { getRuntimeConfig, type RuntimeConfig } from "@/lib/runtime-config"
import { conceptIcon } from "@exp/ui"
import { AGENT_SETTINGS_COPY } from "@/lib/session-tree"

export interface SettingsNavContext {
  isCloud: boolean
}

// Kept to the settings sub-route literals (not the all-routes union) so
// `<Link to={item.to} params={{ teamSlug }}>` type-checks.
export type SettingsSectionPath =
  | `/t/$teamSlug/settings/general`
  | `/t/$teamSlug/settings/members`
  | `/t/$teamSlug/settings/issues`
  | `/t/$teamSlug/settings/labels`
  | `/t/$teamSlug/settings/statuses`
  | `/t/$teamSlug/settings/billing`
  | `/t/$teamSlug/settings/storage`
  | `/t/$teamSlug/settings/import`
  | `/t/$teamSlug/settings/boards/archived`
  | `/t/$teamSlug/settings/repositories`
  | `/t/$teamSlug/settings/prompts`
  | `/t/$teamSlug/settings/widget`
  | `/t/$teamSlug/settings/mcp`
  | `/t/$teamSlug/settings/account`
  | `/t/$teamSlug/settings/notifications`
  | `/t/$teamSlug/settings/security`

export interface SettingsNavItem {
  /** The settings-nav.json page id (`rows` id or `nav.pages` key). */
  id: string
  label: string
  to: SettingsSectionPath
  icon: LucideIcon
  visible: (
    permissions: TeamPermissions,
    context: SettingsNavContext
  ) => boolean
  // EXP-630: sub-pages nested under this entry (the sidebar folds them,
  // the phone strip flattens them right after their parent).
  children?: SettingsNavItem[]
}

/** Every entry in nav order, sub-pages right after their parent. */
export function flattenSettingsNav(items: SettingsNavItem[]): SettingsNavItem[] {
  return items.flatMap((item) => [item, ...(item.children ?? [])])
}

/** S-3: the web's viewer for settings-nav.json `visible` (General, Widget,
 *  Storage, Import, Archived boards = owners; Plan & Billing = owners on a
 *  cloud instance; the rest every member). */
function fixtureVisible(id: string): SettingsNavItem[`visible`] {
  return (permissions, context) =>
    settingsNavItemVisible(id, `web`, { isOwner: permissions.isOwner, isCloud: context.isCloud })
}

// The web route + concept of every settings-nav.json page the web draws.
// The label and icon are literals (lib/icons.test.ts pairs them with the
// desktop nav); -shared.test.tsx locks them to the fixture's, and the ORDER
// and grouping come from the fixture (`SETTINGS_NAV` below), so neither can
// drift. EXP-317: every icon is a registry concept.
const SETTINGS_PAGES: Record<string, Omit<SettingsNavItem, `visible` | `children`>> = {
  general: {
    id: `general`,
    label: `General`,
    to: `/t/$teamSlug/settings/general`,
    icon: conceptIcon(`settings-general`),
  },
  members: {
    id: `members`,
    label: `Members`,
    to: `/t/$teamSlug/settings/members`,
    icon: conceptIcon(`settings-members`),
  },
  // EXP-630: the estimate scale + PR automation, Labels and Statuses its
  // sub-pages (`nav.children`).
  issues: {
    id: `issues`,
    label: `Issues`,
    to: `/t/$teamSlug/settings/issues`,
    icon: conceptIcon(`settings-issues`),
  },
  labels: {
    id: `labels`,
    label: `Labels`,
    to: `/t/$teamSlug/settings/labels`,
    icon: conceptIcon(`settings-labels`),
  },
  statuses: {
    id: `statuses`,
    label: `Statuses`,
    to: `/t/$teamSlug/settings/statuses`,
    icon: conceptIcon(`settings-statuses`),
  },
  repositories: {
    id: `repositories`,
    label: `Repositories`,
    to: `/t/$teamSlug/settings/repositories`,
    icon: conceptIcon(`settings-repositories`),
  },
  prompts: {
    id: `prompts`,
    label: `Prompts`,
    to: `/t/$teamSlug/settings/prompts`,
    icon: conceptIcon(`nav-actions`),
  },
  widget: {
    id: `widget`,
    label: `Widget`,
    to: `/t/$teamSlug/settings/widget`,
    icon: conceptIcon(`settings-widget`),
  },
  storage: {
    id: `storage`,
    label: `Storage`,
    to: `/t/$teamSlug/settings/storage`,
    icon: conceptIcon(`settings-storage`),
  },
  billing: {
    id: `billing`,
    label: `Plan & Billing`,
    to: `/t/$teamSlug/settings/billing`,
    icon: conceptIcon(`settings-billing`),
  },
  // EXP-630: the tracker-import wizard, web-only like Billing.
  import: {
    id: `import`,
    label: `Import`,
    to: `/t/$teamSlug/settings/import`,
    icon: conceptIcon(`settings-import`),
  },
  // EXP-500: the archived boards' restore list.
  "archived-boards": {
    id: `archived-boards`,
    label: `Archived boards`,
    to: `/t/$teamSlug/settings/boards/archived`,
    icon: conceptIcon(`ui-archive`),
  },
  // EXP-792 / #VAPP-185: every member reads it; writes are owner-only inside.
  mcp: {
    id: `mcp`,
    label: `MCP`,
    to: `/t/$teamSlug/settings/mcp`,
    icon: conceptIcon(`settings-mcp`),
  },
  account: {
    id: `account`,
    label: `Account`,
    to: `/t/$teamSlug/settings/account`,
    icon: conceptIcon(`settings-account`),
  },
  notifications: {
    id: `notifications`,
    label: `Notifications`,
    to: `/t/$teamSlug/settings/notifications`,
    icon: conceptIcon(`settings-notifications`),
  },
  // EXP-862: API keys AND passkeys.
  security: {
    id: `security`,
    label: `Security`,
    to: `/t/$teamSlug/settings/security`,
    icon: conceptIcon(`settings-api`),
  },
}

/** The web page of a settings-nav.json page id (undefined = not on web). */
export function settingsPageItem(id: string): SettingsNavItem | undefined {
  const page = SETTINGS_PAGES[id]
  if (!page) return undefined
  const children = (SETTINGS_NAV_CHILDREN[id] ?? [])
    .map((child) => settingsPageItem(child))
    .filter((child): child is SettingsNavItem => child !== undefined)
  return { ...page, visible: fixtureVisible(id), ...(children.length > 0 ? { children } : {}) }
}

/** One entry of a group in order: a page, or an injected `@list` drawn at
 *  render off synced data (`@boards`, `@new-board`, `@assistant`, `@agents`,
 *  `@new-agent`, `@apps`, `@new-app`). */
export type SettingsNavEntry = { kind: `page`; item: SettingsNavItem } | { kind: `list`; list: string }

export interface SettingsNavGroupDef {
  /** settings-nav.json group id. */
  id: string
  group: string
  /** The static pages (sub-pages nested in `children`). */
  items: SettingsNavItem[]
  /** Every entry in the fixture's order, injected lists included. */
  entries: SettingsNavEntry[]
}

// S-3 (settings-nav.json `nav`, ×4): the groups and their order ARE the
// fixture's (platform `web`: no This device group, no About: the web's
// lives in the account menu). Team (General first, Import closing it),
// Boards (rows, New board, Archived boards), Agents (the viewer's Assistant,
// the team agents, New agent, MCP servers), Apps, Personal. Personal stays
// last: the index redirect lands on the first visible item, a team page.
export const SETTINGS_NAV: SettingsNavGroupDef[] = SETTINGS_NAV_GROUPS.filter(
  (group) => !group.platforms || group.platforms.includes(`web`)
).map((group) => {
  const entries: SettingsNavEntry[] = group.items.flatMap((id): SettingsNavEntry[] => {
    if (id.startsWith(`@`)) return [{ kind: `list`, list: id }]
    const item = settingsPageItem(id)
    if (!item) return []
    const page = settingsNavPage(id)
    if (page?.platforms && !page.platforms.includes(`web`)) return []
    return [{ kind: `page`, item }]
  })
  return {
    id: group.id,
    group: group.label,
    items: entries.flatMap((entry) => (entry.kind === `page` ? [entry.item] : [])),
    entries,
  }
})

/** Whether an injected `@list` shows for the viewer (`nav.newEntries`: the
 *  New board / New agent / New app entries are owners'). */
export function settingsNavListVisible(list: string, permissions: TeamPermissions): boolean {
  return settingsNavItemVisible(list, `web`, { isOwner: permissions.isOwner, isCloud: false })
}

// EXP-862: the group whose rows are injected per board, and the label of
// the muted entry that opens the create dialog. Byte-identical with the
// desktop nav's ("New board", `settings/mod.rs`), which is why both live
// here rather than being typed twice in the two web surfaces that render
// them (the sidebar panel and the mobile strip).
export const SETTINGS_BOARDS_GROUP = `Boards`
export const NEW_BOARD_LABEL = `New board`

// EXP-1293: the same injection for team agents (fixture agent-settings.json).
// Members see the agent rows (the pages read-only); "New agent" is owner-only.
export const SETTINGS_AGENTS_GROUP = `Agents`
export const NEW_AGENT_LABEL = AGENT_SETTINGS_COPY.newAgent

// VAPP-110: the same injection for the team's apps (fixture apps.json).
export const SETTINGS_APPS_GROUP = `Apps`
export const NEW_APP_LABEL = APPS_COPY.newApp

// S-3 (settings-nav.json `@assistant`, agent-settings.json `assistant`): the
// viewer's OWN Assistant heads the Agents group and opens its own page,
// `/settings/assistant` (it resolves the Assistant through
// `agents.ensureAssistant`), titled `copy.title`.
export const SETTINGS_ASSISTANT_SECTION = `assistant`
export const ASSISTANT_LABEL = ASSISTANT_COPY.title

// #EXP-1381: the settings route a pathname sits on: `section` = the segment
// after `/settings/` (a settings-nav.json row id or a SETTINGS_NAV page),
// `sub` = the one after it (a board/agent/app/prompt id, `new`, `archived`).
export function settingsRouteOf(pathname: string): { section: string | null; sub: string | null } {
  const match = /\/settings\/([^/?#]+)(?:\/([^/?#]+))?/.exec(pathname)
  return { section: match?.[1] ?? null, sub: match?.[2] ?? null }
}

/** The pages under a section that list rows (their sub-pages go back to them). */
export const SETTINGS_LIST_SECTIONS = [`boards`, `agents`, `prompts`, `apps`] as const

/** A section's label: the settings-nav.json row's (the phones' row title AND
 *  page title) where the fixture has it, else SETTINGS_NAV's (the group label
 *  for the injected Boards / Agents / Apps lists). */
export function settingsSectionLabel(section: string, sub: string | null = null): string | null {
  const nested = sub ? flatSettingsNavItems().find((item) => item.to === `/t/$teamSlug/settings/${section}/${sub}`) : undefined
  if (nested) return nested.label
  if (section === SETTINGS_ASSISTANT_SECTION) return ASSISTANT_LABEL
  const row = settingsNavRow(section)
  if (row) return row.label
  const item = flatSettingsNavItems().find((entry) => entry.to === `/t/$teamSlug/settings/${section}`)
  if (item) return item.label
  const group = SETTINGS_NAV.find((entry) => entry.group.toLowerCase() === section)
  return group?.group ?? null
}

function flatSettingsNavItems(): SettingsNavItem[] {
  return SETTINGS_NAV.flatMap((group) => flattenSettingsNav(group.items))
}

// Everything a settings section page needs. `resolved` flips true only once
// the current user's own member row has synced — permissions are transiently
// all-false while shapes load, so guards must treat !resolved as loading,
// never as denied.
export function useSettingsPage(teamSlug: string) {
  const { data: session } = useSession()
  const team = useTeamBySlug(teamSlug)
  const { members, userMap } = useTeamUsers(team?.id)
  const permissions = useTeamPermissions(team)
  const [config, setConfig] = useState<RuntimeConfig | null>(null)

  useEffect(() => {
    void getRuntimeConfig().then(setConfig)
  }, [])

  const currentUserId = session?.user?.id
  const resolved = Boolean(
    team &&
    currentUserId &&
    members.some((member) => member.userId === currentUserId)
  )

  return {
    session,
    team,
    members,
    userMap,
    permissions,
    config,
    resolved,
  }
}

// Access guard for owner-gated section pages reached by direct URL. A notice
// beats a redirect: no bounce loops, and transient loading renders nothing.
export function SettingsSectionGuard({
  resolved,
  allowed,
  children,
}: {
  resolved: boolean
  allowed: boolean
  children: React.ReactNode
}) {
  if (!resolved) return null
  if (!allowed) {
    return (
      <p className="text-sm text-muted-foreground">
        Only the team owner can manage this section.
      </p>
    )
  }
  return <>{children}</>
}

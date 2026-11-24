import type { ReactNode } from "react"
import {
  newTabMenu,
  SIDEBAR_MENU_COPY,
  SIDEBAR_MENU_ICONS,
  SIDEBAR_MENU_SHORTCUTS,
  SIDEBAR_NEW_COPY,
  SIDEBAR_NEW_ICONS,
  sidebarItemMenu,
  sidebarMenuDestructive,
  sidebarNewKinds,
  type SidebarMenuAction,
  type SidebarMenuInput,
  type SidebarMenuKind,
} from "@exp/domain-contract/sidebar-menus"
import {
  conceptIcon,
  Menu,
  TeamAvatar,
  tidyMenuEntries,
  toast,
  type MenuEntry,
  type MenuSessionProps,
} from "@exp/ui"
import type { IconConcept } from "@exp/icons"
import type { Team } from "@/db/schema"
import { newPagePath, type NewPageKind } from "@/lib/app-paths"
import type { WorkTab } from "@/lib/work-tabs"
import { openWorkTabInBackground } from "@/hooks/use-work-tabs"
import { AssistantMark } from "@/components/team/assistant-mark"

// #EXP-2124 (fixtures/sidebar-menus.json, its `$comment` is the spec): every
// sidebar row's right-click / long-press menu, built from THE table
// (`sidebarItemMenu`) into THE `Menu`'s rows. A row hands over what each verb
// does: a handler, or a submenu's rows (New ›, Move to folder ›). A verb the
// row hands nothing for is not drawn (an Active row already placed has no Add
// to sidebar), and the separators around it tidy away.

/** What a verb does: a handler, or a submenu's rows. */
export type SidebarMenuHandlers = Partial<Record<SidebarMenuAction, (() => void) | readonly MenuEntry[]>>

const iconOf = (action: SidebarMenuAction) =>
  conceptIcon((SIDEBAR_MENU_ICONS as Record<string, string>)[action] as IconConcept)

const applePlatform = () => typeof navigator !== `undefined` && /Mac|iPhone|iPad/.test(navigator.platform)

/** `copy-link`: the page's absolute URL on the clipboard, then the toast. */
export function copySidebarLink(href: string) {
  const url = new URL(href, window.location.origin).toString()
  void navigator.clipboard
    .writeText(url)
    .then(() => toast.success(SIDEBAR_MENU_COPY.linkCopied))
    .catch(() => undefined)
}

/** Open, Open in new tab (the row's page as a BACKGROUND tab) and Copy link
 *  for a row that is a page. */
export function openHandlers(href: string, open: () => void, tab: WorkTab | null): SidebarMenuHandlers {
  return {
    open,
    ...(tab ? { "open-new-tab": () => openWorkTabInBackground(tab) } : {}),
    "copy-link": () => copySidebarLink(href),
  }
}

/** A kind's menu rows for the viewer, off THE table. */
export function sidebarMenuEntries(
  kind: SidebarMenuKind,
  input: SidebarMenuInput,
  handlers: SidebarMenuHandlers
): MenuEntry[] {
  const rows = sidebarItemMenu(kind, input).flatMap((id, index): MenuEntry[] => {
    if (id === `separator`) return [{ kind: `separator`, id: `separator-${index}` }]
    const handler = handlers[id]
    if (!handler) return []
    const base = {
      id,
      label: SIDEBAR_MENU_COPY[id],
      icon: iconOf(id),
      destructive: sidebarMenuDestructive(kind, id),
      "data-testid": `sidebar-menu-${id}`,
    }
    if (typeof handler !== `function`) return [{ kind: `submenu`, ...base, entries: handler }]
    const shortcut = SIDEBAR_MENU_SHORTCUTS[id]
    return [
      {
        kind: `item`,
        ...base,
        ...(shortcut ? { shortcut: applePlatform() ? shortcut.apple : shortcut.other } : {}),
        onSelect: handler,
      },
    ]
  })
  return tidyMenuEntries(rows)
}

/** A row's pointer menu (`MenuGestureHost` session). */
export function SidebarPointerMenu({
  session,
  title,
  entries,
}: {
  session: MenuSessionProps
  title: string
  entries: MenuEntry[]
}): ReactNode {
  if (entries.length === 0) return null
  return (
    <Menu
      mode="pointer"
      anchor={session.target.anchor}
      open={session.open}
      onOpenChange={session.onOpenChange}
      returnFocus={session.target.origin}
      aria-label={title}
      title={title}
      entries={entries}
      data-testid="sidebar-item-menu"
    />
  )
}

/** Chat › New: Home's composer with the team as its Teams pick. */
export const newSessionHref = (teamId: string) => `/home?team=${encodeURIComponent(teamId)}`

/** THE New list of a team (`sidebarNewKinds`): the team menu's New › and the
 *  new-tab menu's team submenu alike. */
export function newTeamEntries(
  team: Pick<Team, `id` | `slug`>,
  owner: boolean,
  go: (href: string) => void
): MenuEntry[] {
  return sidebarNewKinds({ owner }).map((kind) => ({
    kind: `item` as const,
    id: kind,
    label: SIDEBAR_NEW_COPY[kind],
    icon: conceptIcon(SIDEBAR_NEW_ICONS[kind] as IconConcept),
    "data-testid": `sidebar-new-${kind}`,
    onSelect: () => go(kind === `session` ? newSessionHref(team.id) : newPagePath(team.slug, kind as NewPageKind)),
  }))
}

/** THE new-tab menu (`newTabMenu`): the Assistant, then one submenu per team
 *  (in sidebar order) holding THE New list. */
export function newTabMenuEntries({
  assistant,
  teams,
  owners,
  go,
}: {
  assistant: { name: string; href: string } | null
  /** Member teams in sidebar order. */
  teams: readonly Pick<Team, `id` | `slug` | `name` | `iconUrl`>[]
  owners: ReadonlySet<string>
  go: (href: string) => void
}): MenuEntry[] {
  const byId = new Map(teams.map((team) => [team.id, team]))
  return newTabMenu({
    assistant: assistant !== null,
    teams: teams.map((team) => ({ id: team.id, owner: owners.has(team.id) })),
  }).flatMap((row): MenuEntry[] => {
    if (row.id === `separator`) return [{ kind: `separator`, id: `teams-separator` }]
    if (row.id === `assistant` && assistant) {
      return [
        {
          kind: `item`,
          id: `assistant`,
          label: assistant.name,
          icon: <AssistantMark />,
          "data-testid": `new-tab-assistant`,
          onSelect: () => go(assistant.href),
        },
      ]
    }
    const team = byId.get(row.id.slice(`team:`.length))
    if (!team) return []
    return [
      {
        kind: `submenu`,
        id: row.id,
        label: team.name,
        icon: <TeamAvatar iconUrl={team.iconUrl} size={16} />,
        "data-testid": `new-tab-team-${team.id}`,
        entries: newTeamEntries(team, owners.has(team.id), go),
      },
    ]
  })
}

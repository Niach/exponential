import type { ReactNode } from "react"
import { conceptIcon, Menu, toast, type MenuEntry, type MenuIcon, type MenuSessionProps } from "@exp/ui"
import {
  SIDEBAR_MENU_COPY,
  SIDEBAR_MENU_DESTRUCTIVE,
  SIDEBAR_MENU_SHORTCUTS,
  sidebarItemMenu,
  type SidebarMenuAction,
  type SidebarMenuInput,
  type SidebarMenuKind,
} from "@/lib/sidebar-menus"

// #EXP-2124 DRAFT: every sidebar row's right-click / long-press menu, built
// from THE table (`sidebarItemMenu`) into THE menu's rows. A row hands over
// what each verb does; a verb with rows is a submenu (New ›, Move to folder ›).

const ICONS: Record<SidebarMenuAction, MenuIcon> = {
  open: conceptIcon(`ui-arrow-right`),
  "open-new-tab": conceptIcon(`ui-external-link`),
  "new-chat": conceptIcon(`action-chat`),
  new: conceptIcon(`ui-add`),
  "new-folder": conceptIcon(`ui-folder`),
  "new-issue": conceptIcon(`nav-create-issue`),
  "new-team": conceptIcon(`ui-add`),
  "join-team": conceptIcon(`ui-invite`),
  "add-device": conceptIcon(`ui-add`),
  compact: conceptIcon(`coding-compact`),
  "new-conversation": conceptIcon(`action-chat`),
  edit: conceptIcon(`ui-edit`),
  rename: conceptIcon(`ui-edit`),
  "copy-link": conceptIcon(`ui-link`),
  move: conceptIcon(`ui-folder`),
  ungroup: conceptIcon(`ui-minus`),
  "add-to-sidebar": conceptIcon(`ui-add`),
  "remove-from-sidebar": conceptIcon(`ui-minus`),
  settings: conceptIcon(`nav-settings`),
  archive: conceptIcon(`ui-archive`),
  delete: conceptIcon(`ui-delete`),
  remove: conceptIcon(`ui-delete`),
  leave: conceptIcon(`ui-sign-out`),
}

/** What a verb does: a handler, or a submenu's rows. */
export type SidebarMenuHandlers = Partial<Record<SidebarMenuAction, (() => void) | MenuEntry[]>>

const applePlatform = () => typeof navigator !== `undefined` && /Mac|iPhone|iPad/.test(navigator.platform)

/** Open, and Open in new tab, for a row that is a link. */
export function openHandlers(href: string, open: () => void): SidebarMenuHandlers {
  return {
    open,
    "open-new-tab": () => window.open(href, `_blank`, `noopener`),
    "copy-link": () => void navigator.clipboard.writeText(new URL(href, window.location.origin).toString()),
  }
}

export function sidebarMenuEntries(
  kind: SidebarMenuKind,
  input: SidebarMenuInput,
  handlers: SidebarMenuHandlers
): MenuEntry[] {
  return sidebarItemMenu(kind, input).map((id, index): MenuEntry => {
    if (id === `separator`) return { kind: `separator`, id: `separator-${index}` }
    const handler = handlers[id]
    const base = { id, label: SIDEBAR_MENU_COPY[id], icon: ICONS[id], destructive: SIDEBAR_MENU_DESTRUCTIVE.has(id) }
    if (Array.isArray(handler)) return { kind: `submenu`, ...base, entries: handler }
    const shortcut = SIDEBAR_MENU_SHORTCUTS[id]
    return {
      kind: `item`,
      ...base,
      ...(shortcut ? { shortcut: applePlatform() ? shortcut.apple : shortcut.other } : {}),
      // The draft wires what exists today; the rest lands with the build.
      onSelect: handler ?? (() => void toast(`${SIDEBAR_MENU_COPY[id]}: with the build`)),
    }
  })
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

// #EXP-1638: the app icon's long-press menu. Pure and locked by
// `fixtures/app-shortcuts.json`, replayed by iOS ExpCore `AppShortcuts.swift`
// and Android `domain/AppShortcuts.kt`. The fixture's `$comment` holds the
// rules; this is the reference.

import fixture from "../fixtures/app-shortcuts.json" with { type: "json" }

export const APP_SHORTCUTS = fixture

export interface AppShortcutChat {
  id: string
  title: string
  updatedAt: string
  /** The agent a conversation session belongs to: never a recent entry. */
  agentId: string | null
  /** A tree key or a sealed title: never handed to the OS. */
  sealed?: boolean
}

export interface AppShortcut {
  id: string
  title: string
  icon: string
}

const STATIC: Record<string, AppShortcut> = {
  "new-issue": { id: `new-issue`, title: fixture.copy.newIssue, icon: fixture.icons.newIssue },
  "new-chat": { id: `new-chat`, title: fixture.copy.newChat, icon: fixture.icons.newChat },
  search: { id: `search`, title: fixture.copy.search, icon: fixture.icons.search },
}

/** The statics, then the most recent plain chats, capped at `maxEntries`. */
export function appShortcuts(input: { chats: AppShortcutChat[] }): AppShortcut[] {
  const statics = fixture.statics.map((id) => STATIC[id]!)
  const recent = input.chats
    .filter((chat) => !chat.agentId && !chat.sealed)
    .sort((a, b) => b.updatedAt.localeCompare(a.updatedAt))
    .map((chat) => ({ id: `${fixture.recentChatPrefix}${chat.id}`, title: chat.title, icon: fixture.icons.chat }))
  return [...statics, ...recent].slice(0, fixture.maxEntries)
}

export type AppShortcutTarget =
  | { kind: `new-issue` }
  | { kind: `new-chat` }
  | { kind: `search` }
  | { kind: `chat`; id: string }

/** The destination a shortcut id names, null = the normal start screen. */
export function parseAppShortcut(id: string): AppShortcutTarget | null {
  if (id === `new-issue` || id === `new-chat` || id === `search`) return { kind: id }
  if (!id.startsWith(fixture.recentChatPrefix)) return null
  const rest = id.slice(fixture.recentChatPrefix.length)
  return rest ? { kind: `chat`, id: rest } : null
}

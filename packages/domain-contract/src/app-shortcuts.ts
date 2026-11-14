// #EXP-1638 / #EXP-1830: the app icon's long-press menu. Pure and locked by
// `fixtures/app-shortcuts.json`, replayed by iOS ExpCore `AppShortcuts.swift`
// and Android `domain/AppShortcuts.kt`. The fixture's `$comment` holds the
// rules; this is the reference.

import fixture from "../fixtures/app-shortcuts.json" with { type: "json" }

export const APP_SHORTCUTS = fixture

export interface AppShortcutChat {
  id: string
  title: string
  updatedAt: string
  /** The agent a conversation session belongs to: the agent entry. */
  agentId: string | null
  /** A tree key or a sealed title: a plain chat never handed to the OS. */
  sealed?: boolean
}

export interface AppShortcutAgent {
  id: string
  name: string
  icon: string | null
  color: string | null
}

export interface AppShortcutsInput {
  assistant: { id: string; name: string } | null
  agents: AppShortcutAgent[]
  /** The viewer's session pins, in sidebar order. */
  pins: string[]
  chats: AppShortcutChat[]
}

export interface AppShortcut {
  id: string
  title: string
  icon: string
  color: string | null
}

const STATIC: Record<string, AppShortcut> = {
  "new-issue": { id: `new-issue`, title: fixture.copy.newIssue, icon: fixture.icons.newIssue, color: null },
  "new-chat": { id: `new-chat`, title: fixture.copy.newChat, icon: fixture.icons.newChat, color: null },
  search: { id: `search`, title: fixture.copy.search, icon: fixture.icons.search, color: null },
}

/** #EXP-1830: the chat entries: the Assistant, pins, then recent chats, deduplicated. */
export function chatEntries(input: AppShortcutsInput): AppShortcut[] {
  const agents = new Map(input.agents.map((agent) => [agent.id, agent]))
  const entry = (chat: AppShortcutChat): AppShortcut | null => {
    if (chat.agentId) {
      const agent = agents.get(chat.agentId)
      if (!agent) return null
      if (agent.id === input.assistant?.id) return assistantEntry(input.assistant)
      return {
        id: `${fixture.agentPrefix}${agent.id}`,
        title: agent.name,
        icon: agent.icon ?? fixture.icons.agent,
        color: agent.color,
      }
    }
    if (chat.sealed) return null
    return { id: `${fixture.recentChatPrefix}${chat.id}`, title: chat.title, icon: fixture.icons.chat, color: null }
  }
  const byId = new Map(input.chats.map((chat) => [chat.id, chat]))
  const pinned = input.pins.flatMap((id) => {
    const chat = byId.get(id)
    return chat ? [chat] : []
  })
  const recent = [...input.chats].sort((a, b) => b.updatedAt.localeCompare(a.updatedAt))
  const out: AppShortcut[] = []
  const seen = new Set<string>()
  const push = (item: AppShortcut | null) => {
    if (!item || seen.has(item.id)) return
    seen.add(item.id)
    out.push(item)
  }
  if (input.assistant) push(assistantEntry(input.assistant))
  for (const chat of pinned) push(entry(chat))
  for (const chat of recent) push(entry(chat))
  return out
}

function assistantEntry(assistant: { id: string; name: string }): AppShortcut {
  return { id: `${fixture.agentPrefix}${assistant.id}`, title: assistant.name, icon: fixture.icons.assistant, color: null }
}

/** The statics, then the chat entries, capped at `maxEntries`. */
export function appShortcuts(input: AppShortcutsInput): AppShortcut[] {
  const statics = fixture.statics.map((id) => STATIC[id]!)
  return [...statics, ...chatEntries(input)].slice(0, fixture.maxEntries)
}

export type AppShortcutTarget =
  | { kind: `new-issue` }
  | { kind: `new-chat` }
  | { kind: `search` }
  | { kind: `chat`; id: string }
  | { kind: `agent`; id: string }

/** The destination a shortcut id names, null = the normal start screen. */
export function parseAppShortcut(id: string): AppShortcutTarget | null {
  if (id === `new-issue` || id === `new-chat` || id === `search`) return { kind: id }
  if (id.startsWith(fixture.agentPrefix)) {
    const rest = id.slice(fixture.agentPrefix.length)
    return rest ? { kind: `agent`, id: rest } : null
  }
  if (!id.startsWith(fixture.recentChatPrefix)) return null
  const rest = id.slice(fixture.recentChatPrefix.length)
  return rest ? { kind: `chat`, id: rest } : null
}

const SHARE_AGENT = `share-agent:`
const SHARE_CHAT = `share-chat:`

/** #EXP-1830 (Android): the app shortcut id a shortcut's LAUNCHER intent carries. */
export function launcherShortcutId(id: string): string | null {
  if (id.startsWith(SHARE_AGENT)) {
    const rest = id.slice(SHARE_AGENT.length)
    return rest ? `${fixture.agentPrefix}${rest}` : null
  }
  if (id.startsWith(SHARE_CHAT)) {
    const rest = id.slice(SHARE_CHAT.length)
    return rest ? `${fixture.recentChatPrefix}${rest}` : null
  }
  return parseAppShortcut(id) ? id : null
}

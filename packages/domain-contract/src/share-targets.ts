// EXP-1635: sharing INTO Exponential. Pure and locked by
// `fixtures/share-targets.json`, replayed by iOS ExpCore `ShareTargets.swift`
// and Android `domain/ShareTargets.kt`. The fixture's `$comment` holds the
// rules; this is the reference.

import fixture from "../fixtures/share-targets.json" with { type: "json" }
import { orderedAgents, type AgentRoleLike } from "./agent-settings"

export const SHARE_TARGETS = fixture

export interface ShareTargetAgent extends AgentRoleLike {
  id: string
  name: string
  sortOrder: number
}

export interface ShareTargetChat {
  id: string
  title: string
  updatedAt: string
  /** The agent a conversation session belongs to: never a recent chat row. */
  agentId: string | null
  /** A tree key or a sealed title: never handed to the OS. */
  sealed?: boolean
}

/** The Share to page: the fixed rows, then the agents in agent row order. */
export function shareTargets<T extends ShareTargetAgent>(input: {
  agents: T[]
  me: string | null | undefined
}): { rows: string[]; agents: T[] } {
  return {
    rows: [fixture.ids.newIssue, fixture.ids.newChat],
    agents: orderedAgents(input.agents, input.me),
  }
}

/** What a shared payload leaves in the composer. */
export function shareChatText(input: {
  text?: string | null
  subject?: string | null
  url?: string | null
  hasImages: boolean
}): string {
  const text = input.text?.trim() ?? ``
  const url = input.url?.trim() ?? ``
  const body = url && !text.includes(url) ? [text, url].filter(Boolean).join(`\n\n`) : text
  const subject = input.subject?.trim() ?? ``
  if (input.hasImages || !subject || body.includes(subject)) return body
  return [subject, body].filter(Boolean).join(`\n\n`)
}

/** The system share sheet's rows: agents, then recent chats, capped. */
export function directShareEntries<T extends ShareTargetAgent>(input: {
  agents: T[]
  chats: ShareTargetChat[]
  me: string | null | undefined
}): { id: string; title: string }[] {
  const agents = orderedAgents(input.agents, input.me).map((agent) => ({
    id: `${fixture.directShare.agentPrefix}${agent.id}`,
    title: agent.name,
  }))
  const chats = input.chats
    .filter((chat) => !chat.agentId && !chat.sealed)
    .sort((a, b) => b.updatedAt.localeCompare(a.updatedAt))
    .map((chat) => ({ id: `${fixture.directShare.chatPrefix}${chat.id}`, title: chat.title }))
  return [...agents, ...chats].slice(0, fixture.directShare.maxEntries)
}

export type DirectShareTarget = { kind: `agent` | `chat`; id: string }

/** The destination a direct-share id names, null = fall back to step 1. */
export function parseDirectShareId(id: string): DirectShareTarget | null {
  const { agentPrefix, chatPrefix } = fixture.directShare
  const kind = id.startsWith(agentPrefix) ? `agent` : id.startsWith(chatPrefix) ? `chat` : null
  if (!kind) return null
  const rest = id.slice((kind === `agent` ? agentPrefix : chatPrefix).length)
  return rest ? { kind, id: rest } : null
}

/** New issue's boards on the picked team, and the one it opens on. */
export function shareIssueBoards<T extends { id: string; teamId: string }>(input: {
  teamId: string | null | undefined
  boards: readonly T[]
  lastBoardId: string | null | undefined
}): { boards: T[]; board: T | null } {
  const boards = input.teamId ? input.boards.filter((board) => board.teamId === input.teamId) : []
  const board = boards.find((row) => row.id === input.lastBoardId) ?? boards[0] ?? null
  return { boards, board }
}

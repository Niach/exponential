// #EXP-1642: issues and chats in the phone's own search (CoreSpotlight,
// AppSearch). Pure and locked by `fixtures/system-search.json`, replayed by
// iOS ExpCore `SystemSearch.swift` and Android `domain/SystemSearch.kt`.
// The fixture's `$comment` holds the rules; this is the reference.

import fixture from "../fixtures/system-search.json" with { type: "json" }

export const SYSTEM_SEARCH = fixture

export interface SystemSearchIssue {
  id: string
  identifier: string
  title: string
  boardId: string
  updatedAt: string
  trashed?: boolean
  archived?: boolean
}

export interface SystemSearchChat {
  id: string
  title: string
  updatedAt: string
  /** The agent a conversation session belongs to: never indexed. */
  agentId: string | null
  /** A tree key or a sealed title: never handed to the OS. */
  sealed?: boolean
}

export interface SystemSearchItem {
  id: string
  title: string
  subtitle: string
  keywords: string[]
}

const newestFirst = (a: { updatedAt: string }, b: { updatedAt: string }) => b.updatedAt.localeCompare(a.updatedAt)

/** The index of one account: live issues, then plain chats, each capped
 *  (the fixture's caps by default; clients pass `productionCaps`). */
export function systemSearchItems(
  input: { boards: { id: string; name: string }[]; issues: SystemSearchIssue[]; chats: SystemSearchChat[] },
  caps: { maxIssues: number; maxChats: number } = fixture,
): SystemSearchItem[] {
  const boards = new Map(input.boards.map((board) => [board.id, board.name]))
  const issues = input.issues
    .filter((issue) => !issue.trashed && !issue.archived)
    .sort(newestFirst)
    .slice(0, caps.maxIssues)
    .map((issue) => {
      const board = boards.get(issue.boardId)
      return {
        id: `${fixture.issuePrefix}${issue.id}`,
        title: issue.title,
        subtitle: board
          ? fixture.copy.issueSubtitle.replace(`{identifier}`, issue.identifier).replace(`{board}`, board)
          : issue.identifier,
        keywords: board ? [issue.identifier, board] : [issue.identifier],
      }
    })
  const chats = input.chats
    .filter((chat) => !chat.agentId && !chat.sealed)
    .sort(newestFirst)
    .slice(0, caps.maxChats)
    .map((chat) => ({
      id: `${fixture.chatPrefix}${chat.id}`,
      title: chat.title,
      subtitle: fixture.copy.chatSubtitle,
      keywords: [],
    }))
  return [...issues, ...chats]
}

export type SystemSearchTarget = { kind: `issue` | `chat`; id: string }

/** The destination a result id names, null = the normal start screen. */
export function parseSystemSearchId(id: string): SystemSearchTarget | null {
  const kind = id.startsWith(fixture.issuePrefix) ? `issue` : id.startsWith(fixture.chatPrefix) ? `chat` : null
  if (!kind) return null
  const rest = id.slice((kind === `issue` ? fixture.issuePrefix : fixture.chatPrefix).length)
  return rest ? { kind, id: rest } : null
}

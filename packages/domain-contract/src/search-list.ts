// #EDE-99 (fixtures/search-list.json, its `$comment` = the spec ×4): the
// global search's CHATS section over the chat store's `searchAll`. Pure:
// which sections show, the chat rows, what a pick opens and where find in
// chat starts. Mirrors: desktop `domain::search_list` (the IDE and eDE),
// iOS ExpCore, Android `domain/SearchList.kt`.
import fixture from "../fixtures/search-list.json" with { type: "json" }
import type { ChatHit } from "./chat-store"
import { chatTimeLabel } from "./chats"
import { findEventKey, findInChatQuery, findMessageKey, type FindStop } from "./find-in-chat"

export const SEARCH_LIST_CHATS = fixture.chats
export const SEARCH_LIST_CHATS_COPY = fixture.chats.copy

export type GlobalSectionKey = `issues` | `chats`

export interface GlobalSection {
  key: GlobalSectionKey
  count: number
  loading: boolean
}

/** `chats` null = no chat store on this client. */
export function globalSections(input: {
  query: string
  issues: number
  chats: { rows: number; loading: boolean } | null
}): { titled: boolean; sections: GlobalSection[]; count: number | null; empty: boolean } {
  const blank = findInChatQuery(input.query) === ``
  if (blank || input.chats === null) {
    return {
      titled: false,
      sections: [{ key: `issues`, count: input.issues, loading: false }],
      count: blank ? null : input.issues,
      empty: false,
    }
  }
  const sections: GlobalSection[] = []
  if (input.issues > 0) sections.push({ key: `issues`, count: input.issues, loading: false })
  if (input.chats.rows > 0) sections.push({ key: `chats`, count: input.chats.rows, loading: input.chats.loading })
  const count = input.issues + input.chats.rows
  return { titled: true, sections, count, empty: count === 0 && !input.chats.loading }
}

export interface GlobalChatRow {
  key: string
  rootId: string
  icon: string
  title: string
  subtitle: string
  highlight: string
  trailing: string
}

/** The Chats rows: the store's order, roots of the active team only, at most `limit`. */
export function globalChatRows(input: {
  query: string
  now: string
  utcOffsetMinutes: number
  chats: readonly { rootId: string; count: number; hit: ChatHit }[]
  /** The active team's synced roots I can see. */
  roots: Readonly<Record<string, { title: string; agent: boolean }>>
  /** Authors of synced messages by id (`user` leads with You). */
  messageAuthors: Readonly<Record<string, string>>
  limit: number
}): GlobalChatRow[] {
  const copy = fixture.chats.copy
  const highlight = findInChatQuery(input.query)
  const rows: GlobalChatRow[] = []
  for (const chat of input.chats) {
    if (rows.length >= input.limit) break
    const root = input.roots[chat.rootId]
    if (!root) continue
    const hit = chat.hit
    // #FEED-300: a system input (a schedule tick, a relay, a teammate's
    // line) is never the person's own words.
    const person =
      !hit.system &&
      (hit.kind === `user_message` || (hit.kind === `message` && !!hit.messageId && input.messageAuthors[hit.messageId] === `user`))
    rows.push({
      key: `chat:${chat.rootId}`,
      rootId: chat.rootId,
      icon: root.agent ? fixture.chats.icons.agent : fixture.chats.icons.chat,
      title: root.title,
      subtitle: `${person ? copy.you : ``}${hit.snippet}`,
      highlight,
      trailing: chatTimeLabel(new Date(hit.at).toISOString(), input.now, input.utcOffsetMinutes),
    })
  }
  return rows
}

export interface GlobalChatTarget {
  key: string
  output: boolean
}

/** A pick: the session to open and find in chat's query + start target. */
export function globalChatPick(input: { query: string; hit: ChatHit }): {
  sessionId: string
  find: { query: string; target: GlobalChatTarget }
} {
  const { hit } = input
  const key = hit.kind === `message` ? findMessageKey(hit.messageId ?? ``) : findEventKey(hit.sessionId, hit.seq ?? 0)
  return { sessionId: hit.sessionId, find: { query: findInChatQuery(input.query), target: { key, output: hit.kind === `tool_output` } } }
}

/** Find in chat's start for a pick: the target's first stop, else the newest. */
export function globalChatStart(stops: readonly FindStop[], target: GlobalChatTarget): number | null {
  if (stops.length === 0) return null
  const at = stops.findIndex((s) => s.key === target.key && s.output === target.output)
  return at >= 0 ? at : stops.length - 1
}

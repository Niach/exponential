// EXP-1341: THE display title of any session ×4 (never `Session N`).
// fixtures/session-title.json holds the rules + cases; mirrors: desktop
// `domain::session_title`, iOS ExpCore `SessionTitle.swift`, Android
// `domain/SessionTitle.kt`. The server's `deriveSessionTitle` calls this.

import fixture from "../fixtures/session-title.json" with { type: "json" }

export const SESSION_TITLE_COPY = {
  titles: fixture.titles,
  titleMax: fixture.titleMax,
  untitled: fixture.untitled,
}

export interface TitleSession {
  id: string
  rootId: string
  name: string | null
  title: string | null
  actionName: string | null
}

export interface TitleLink {
  sessionId: string
  issueId: string
  sortOrder: number
  /** The tree session working on it (null = the root's own). */
  assignedSessionId?: string | null
}

export interface TitleIssue {
  id: string
  identifier: string
  title: string
}

export interface TitleMessage {
  id: string
  sessionId: string
  authorKind: string
  authorSessionId?: string | null
  body: string
  card: unknown
  createdAt: string
}

const ms = (iso: string): number => {
  const value = Date.parse(iso)
  return Number.isNaN(value) ? 0 : value
}

/** Cut to `max` characters, ending in `…`. */
function clip(text: string, max: number): string {
  return text.length > max ? `${text.slice(0, Math.max(0, max - 1)).trimEnd()}…` : text
}

/** The first usable line of a body: non-blank, not an image embed. */
function firstLine(body: string): string | null {
  return (
    body
      .split(`\n`)
      .map((part) => part.trim())
      .find((part) => part.length > 0 && !part.startsWith(`![`)) ?? null
  )
}

/** A root's links, or the links ASSIGNED to a child, in link order. */
export function sessionIssues<I extends TitleIssue>(
  session: Pick<TitleSession, `id` | `rootId`>,
  links: readonly TitleLink[],
  issues: readonly I[]
): I[] {
  const byId = new Map(issues.map((issue) => [issue.id, issue]))
  const isRoot = session.id === session.rootId
  return links
    .filter((link) =>
      isRoot ? link.sessionId === session.id : (link.assignedSessionId ?? null) === session.id
    )
    .sort(
      (a, b) =>
        a.sortOrder - b.sortOrder || (a.issueId < b.issueId ? -1 : a.issueId > b.issueId ? 1 : 0)
    )
    .map((link) => byId.get(link.issueId))
    .filter((issue): issue is I => issue !== undefined)
}

/** The first prompt's usable line: a person's text or another session's input. */
export function firstPrompt(
  sessionId: string,
  messages: readonly TitleMessage[]
): string | null {
  const inputs = messages
    .filter(
      (m) =>
        m.sessionId === sessionId &&
        (m.card === null || m.card === undefined) &&
        (m.authorKind === `user` || (!!m.authorSessionId && m.authorSessionId !== sessionId))
    )
    .sort((a, b) => ms(a.createdAt) - ms(b.createdAt) || (a.id < b.id ? -1 : a.id > b.id ? 1 : 0))
  for (const message of inputs) {
    const line = firstLine(message.body)
    if (line) return line
  }
  return null
}

function issueLabel(issues: readonly TitleIssue[]): string {
  const { titles, titleMax } = SESSION_TITLE_COPY
  const first = issues[0]
  // Function replacers: a title holding `$&`, `$$`, `` $` `` or `$'` lands
  // LITERALLY, as the natives insert it (`String.replace` would expand it).
  const template = (issues.length > 1 ? titles.issues : titles.issue)
    .replace(`{identifier}`, () => first.identifier)
    .replace(`{more}`, () => String(issues.length - 1))
  const title = first.title.trim()
  const room = titleMax - template.replace(`{title}`, ``).length
  const clipped = clip(title, Math.max(1, room))
  return template.replace(`{title}`, () => clipped)
}

/** THE display title (fixture `session-title.json` rungs). */
export function sessionTitle(
  session: TitleSession,
  links: readonly TitleLink[],
  issues: readonly TitleIssue[],
  messages: readonly TitleMessage[]
): string {
  const named = session.name?.trim() || session.title?.trim()
  if (named) return named
  const own = sessionIssues(session, links, issues)
  if (own.length > 0) return issueLabel(own)
  const action = session.actionName?.trim()
  if (action) return SESSION_TITLE_COPY.titles.action.replace(`{action}`, () => action)
  const prompt = firstPrompt(session.id, messages)
  return prompt ? clip(prompt, SESSION_TITLE_COPY.titleMax) : SESSION_TITLE_COPY.untitled
}

/** Every given session's title, keyed by id (the tree's card names). */
export function sessionTitles(
  sessions: readonly TitleSession[],
  links: readonly TitleLink[],
  issues: readonly TitleIssue[],
  messages: readonly TitleMessage[]
): Map<string, string> {
  return new Map(sessions.map((s) => [s.id, sessionTitle(s, links, issues, messages)]))
}

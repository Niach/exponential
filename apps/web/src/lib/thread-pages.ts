// EXP-1343 (session-thread.json): phones page through EVERY strip tab by
// swipe AND tap, in strip order — conversation · each issue chip then its
// nested issues · each session chip then its nested sessions · stories
// (stories.json STORIES TAB: always the last page); the `+` is never a
// page. A page is a string key so the pager (`useFaceSwipe`) can compare
// them by value.

export type ThreadPage = `conversation` | `stories` | `issue:${string}` | `session:${string}`

type StripTab =
  | { kind: `add` }
  | { kind: `conversation` }
  | { kind: `issue`; issueId: string; nested: readonly { issueId: string }[] }
  | { kind: `session`; sessionId: string; nested: readonly { sessionId: string }[] }

/** The thread's pages in strip order, nested chips flattened after theirs,
 *  the Stories tab last. */
export function threadPages(tabs: readonly StripTab[]): ThreadPage[] {
  const issues: ThreadPage[] = []
  const sessions: ThreadPage[] = []
  for (const tab of tabs) {
    if (tab.kind === `issue`) {
      issues.push(`issue:${tab.issueId}`, ...tab.nested.map((n) => `issue:${n.issueId}` as const))
    } else if (tab.kind === `session`) {
      sessions.push(
        `session:${tab.sessionId}`,
        ...tab.nested.map((n) => `session:${n.sessionId}` as const)
      )
    }
  }
  return [`conversation`, ...issues, ...sessions, `stories`]
}

/** The page a thread tab shows. */
export function threadPageOf(
  tab:
    | { kind: `conversation` }
    | { kind: `stories` }
    | { kind: `issue`; issueId: string }
    | { kind: `session`; sessionId: string }
): ThreadPage {
  if (tab.kind === `issue`) return `issue:${tab.issueId}`
  if (tab.kind === `session`) return `session:${tab.sessionId}`
  return tab.kind
}

/** The thread route's search for a page (`?tab=`). */
export function threadPageSearch(
  page: ThreadPage
): { tab?: `issue` | `stories` | `session`; issue?: string; session?: string } {
  if (page === `conversation`) return {}
  if (page === `stories`) return { tab: `stories` }
  if (page.startsWith(`issue:`)) return { tab: `issue`, issue: page.slice(`issue:`.length) }
  return { tab: `session`, session: page.slice(`session:`.length) }
}

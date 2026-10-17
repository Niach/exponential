// EXP-1343 (session-thread.json): phones page through EVERY strip tab by
// swipe AND tap, in strip order — conversation · each issue chip then its
// nested issues · each session chip then its nested sessions · guide; the
// `+` is never a page. A page is a string key so the pager
// (`useFaceSwipe`) can compare them by value.

export type ThreadPage = `conversation` | `guide` | `issue:${string}` | `session:${string}`

type StripTab =
  | { kind: `add` }
  | { kind: `conversation` }
  | { kind: `issue`; issueId: string; nested: readonly { issueId: string }[] }
  | { kind: `session`; sessionId: string; nested: readonly { sessionId: string }[] }
  | { kind: `guide` }

/** The thread's pages in strip order, nested chips flattened after theirs. */
export function threadPages(tabs: readonly StripTab[]): ThreadPage[] {
  const issues: ThreadPage[] = []
  const sessions: ThreadPage[] = []
  let guide = false
  for (const tab of tabs) {
    if (tab.kind === `issue`) {
      issues.push(`issue:${tab.issueId}`, ...tab.nested.map((n) => `issue:${n.issueId}` as const))
    } else if (tab.kind === `session`) {
      sessions.push(
        `session:${tab.sessionId}`,
        ...tab.nested.map((n) => `session:${n.sessionId}` as const)
      )
    } else if (tab.kind === `guide`) {
      guide = true
    }
  }
  return [`conversation`, ...issues, ...sessions, ...(guide ? [`guide` as const] : [])]
}

/** The page a thread tab shows. */
export function threadPageOf(
  tab:
    | { kind: `conversation` }
    | { kind: `guide` }
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
): { tab?: `issue` | `guide` | `session`; issue?: string; session?: string } {
  if (page === `conversation`) return {}
  if (page === `guide`) return { tab: `guide` }
  if (page.startsWith(`issue:`)) return { tab: `issue`, issue: page.slice(`issue:`.length) }
  return { tab: `session`, session: page.slice(`session:`.length) }
}

// EXP-1343 / #EXP-1433 (session-thread.json STRIP): phones page through the
// strip by swipe AND tap — Conversation · (chips: the ONE issue | group: the
// item opened from the group's menu, only while it is open) · Stories
// (stories.json STORIES TAB: always the last page). The `+` and the group
// tab are never pages. A page is a string key so the pager (`useFaceSwipe`)
// can compare them by value.

export type ThreadPage = `conversation` | `stories` | `issue:${string}` | `session:${string}`

/** The thread's pages: `single` = the chips mode's one issue (its id), else
 *  null; `open` = the issue / session page open from the group (null when
 *  none is). */
export function threadPages({
  single,
  open,
}: {
  single: string | null
  open: ThreadPage | null
}): ThreadPage[] {
  const middle: ThreadPage[] = single
    ? [`issue:${single}`]
    : open && open !== `conversation` && open !== `stories`
      ? [open]
      : []
  return [`conversation`, ...middle, `stories`]
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

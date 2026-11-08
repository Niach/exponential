// EXP-1343 / #EXP-1433 (session-chat.json STRIP): phones page through the
// strip by swipe AND tap — Conversation · (#EXP-1662: the plan) · (chips: the ONE issue | group: the
// item opened from the group's menu, only while it is open) · Results
// (results-tab.json: always the last page). The `+` and the group
// tab are never pages. A page is a string key so the pager (`useFaceSwipe`)
// can compare them by value.

export type ChatPage = `conversation` | `plan` | `results` | `issue:${string}` | `session:${string}`

/** The chat's pages: `single` = the chips mode's one issue (its id), else
 *  null; `open` = the issue / session page open from the group (null when
 *  none is). */
export function chatPages({
  single,
  open,
  plan = false,
}: {
  single: string | null
  open: ChatPage | null
  /** #EXP-1662: the chat holds a plan (its page follows Conversation). */
  plan?: boolean
}): ChatPage[] {
  const middle: ChatPage[] = single
    ? [`issue:${single}`]
    : open && open !== `conversation` && open !== `results` && open !== `plan`
      ? [open]
      : []
  return [`conversation`, ...(plan ? ([`plan`] as const) : []), ...middle, `results`]
}

/** The page a chat tab shows. */
export function chatPageOf(
  tab:
    | { kind: `conversation` }
    | { kind: `results` }
    | { kind: `plan` }
    | { kind: `issue`; issueId: string }
    | { kind: `session`; sessionId: string }
): ChatPage {
  if (tab.kind === `issue`) return `issue:${tab.issueId}`
  if (tab.kind === `session`) return `session:${tab.sessionId}`
  return tab.kind
}

/** The chat route's search for a page (`?tab=`). */
export function chatPageSearch(
  page: ChatPage
): { tab?: `issue` | `results` | `session` | `plan`; issue?: string; session?: string } {
  if (page === `conversation`) return {}
  if (page === `results`) return { tab: `results` }
  if (page === `plan`) return { tab: `plan` }
  if (page.startsWith(`issue:`)) return { tab: `issue`, issue: page.slice(`issue:`.length) }
  return { tab: `session`, session: page.slice(`session:`.length) }
}

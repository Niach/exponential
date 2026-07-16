/* ─── Shared web-app demo state: context, types ───
   Mirrors ide/state.tsx: one context object owns the interactive state, the
   view components read it through useWeb(). */
import { createContext, useContext } from "react"
import type { FilterTab } from "../ide/data"

/* Embedding views the docs can request. */
export type WebView = `board` | `issue` | `inbox` | `support`

/* Sidebar nav targets that actually switch the main pane. Search, Reviews
   and Agents render for fidelity but stay inert — the demo keeps only the
   panes that carry a full recreation. */
export type WebNav = `project` | `my-issues` | `inbox` | `support`

export type WebApi = {
  interactive: boolean

  nav: WebNav
  setNav: (nav: WebNav) => void

  /* Non-null renders the full-page issue detail in place of the board. */
  openIssueId: string | null
  openIssue: (id: string) => void
  closeIssue: () => void

  filter: FilterTab
  setFilter: (filter: FilterTab) => void
  collapsedGroups: Set<string>
  toggleGroup: (status: string) => void

  inboxRead: Set<string>
  markInboxRead: (id: string) => void
  markAllInboxRead: () => void

  selectedThreadId: string | null
  selectThread: (id: string) => void
  threadFilter: `open` | `resolved`
  setThreadFilter: (filter: `open` | `resolved`) => void
  threadRead: Set<string>
}

export const WebContext = createContext<WebApi | null>(null)

export function useWeb(): WebApi {
  const api = useContext(WebContext)
  if (!api) throw new Error(`useWeb must be used inside <WebDemo>`)
  return api
}

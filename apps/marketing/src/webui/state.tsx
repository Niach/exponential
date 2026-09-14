/* ─── Shared web-app demo state: context, types ───
   Mirrors ide/state.tsx: one context object owns the interactive state, the
   view components read it through useWeb(). */
import { createContext, useContext } from "react"
import type { Issue } from "../ide/data"
import type { DemoAgent } from "./data"

/* Embedding views the docs can request. */
export type WebView = `board` | `issue` | `run` | `inbox` | `support` | `agent`

/* Sidebar nav targets that actually switch the main pane. Devices, Actions,
   Automations and Reviews render for fidelity but stay inert — the demo keeps
   only the panes that carry a full recreation. */
export type WebNav = `project` | `inbox` | `support` | `agent`

/* My Issues is a TAB of the Inbox page (EXP-186, `?tab=my-issues`), never a
   route or a sidebar entry. */
export type InboxTab = `inbox` | `my-issues`

/* EXP-877: the faces of ONE work tab — the issue, its run, the run's diff. */
export type WorkFace = `issue` | `run` | `diff`

export type WebApi = {
  interactive: boolean

  /* Scene-injected extra issue (EXP-602: the collab demo files a widget
     report onto the board) — rendered first in its status group with an
     entrance animation. Null everywhere else. */
  injectedIssue: Issue | null

  nav: WebNav
  setNav: (nav: WebNav) => void

  inboxTab: InboxTab
  setInboxTab: (tab: InboxTab) => void

  /* Non-null renders the work view (issue / run / diff face) in the card. */
  openIssueId: string | null
  face: WorkFace
  setFace: (face: WorkFace) => void
  /* Opens the issue face (a list click), or the run face of a live tab. */
  openIssue: (id: string, face?: WorkFace) => void
  closeIssue: () => void
  /* Every play button routes to the Agent page composer with the issue
     chipped (`?issues=`, EXP-825). */
  startCoding: (issueId: string) => void
  agentSeedId: string | null

  /* EXP-870 work tabs: the ordinary (closable) tabs, in open order. The live
     runs' tabs are derived from the sessions and can never be closed. */
  tabIds: string[]
  closeTab: (id: string) => void
  /* EXP-877: agent groups folded down to their brand mark. */
  foldedAgents: Set<DemoAgent>
  toggleAgentFold: (agent: DemoAgent, folded?: boolean) => void

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

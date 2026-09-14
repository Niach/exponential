/* ─── WebDemo — pixel-faithful, usable recreation of the Exponential web app ───
   Same house pattern as ide/Ide.tsx: fixed 1100×680 canvas auto-scaled to
   the container via the shared useDemoScale hook, one context object owns
   the interactive state, fixtures come from the shared universe in ide/data.
   EXP-870 layout: the sidebar, then the content column — the 44px work-tabs
   band on the bare ground, and the cutout card flush under it. */
import { useMemo, useState } from "react"
import type { Issue } from "../ide/data"
import { toggledSet } from "../ide/state"
import { INBOX_ITEMS } from "../ide/data"
import { useDemoScale } from "../lib/use-demo-scale"
import {
  WebContext,
  type InboxTab,
  type WebApi,
  type WebNav,
  type WebView,
  type WorkFace,
} from "./state"
import { SUPPORT_THREADS, sessionFor, type DemoAgent } from "./data"
import { WebSidebar } from "./Sidebar"
import { WebBoard } from "./Board"
import { WebWorkTabs } from "./WorkTabs"
import { WebAgentPage } from "./AgentPage"
import { WebIssueDetail } from "./IssueDetail"
import { WebInbox } from "./Inbox"
import { WebSupportInbox } from "./SupportInbox"

const BASE_W = 1100
const WEB_H = 680

type InitState = {
  nav: WebNav
  openIssueId: string | null
  face: WorkFace
  selectedThreadId: string | null
}

const initialState = (view: WebView): InitState => {
  const base = { openIssueId: null, face: `issue` as WorkFace, selectedThreadId: SUPPORT_THREADS[0].id }
  switch (view) {
    case `issue`:
      return { ...base, nav: `project`, openIssueId: `EXP-8` }
    case `run`:
      return { ...base, nav: `project`, openIssueId: `EXP-8`, face: `run` }
    case `inbox`:
      return { ...base, nav: `inbox` }
    case `support`:
      return { ...base, nav: `support` }
    case `agent`:
      return { ...base, nav: `agent` }
    default:
      return { ...base, nav: `project` }
  }
}

export type WebDemoProps = {
  view?: WebView
  interactive?: boolean
  className?: string
  /* Scene-injected extra board issue (EXP-602 collab demo). */
  injectedIssue?: Issue | null
}

export function WebDemo({
  view = `board`,
  interactive = true,
  className,
  injectedIssue = null,
}: WebDemoProps) {
  const init = useMemo(() => initialState(view), [view])

  const [nav, setNav] = useState<WebNav>(init.nav)
  const [inboxTab, setInboxTab] = useState<InboxTab>(`inbox`)
  const [openIssueId, setOpenIssueId] = useState<string | null>(init.openIssueId)
  const [face, setFace] = useState<WorkFace>(init.face)
  const [tabIds, setTabIds] = useState<string[]>([])
  const [foldedAgents, setFoldedAgents] = useState<Set<DemoAgent>>(new Set())
  const [agentSeedId, setAgentSeedId] = useState<string | null>(null)
  const [collapsedGroups, setCollapsedGroups] = useState<Set<string>>(new Set())
  const [inboxRead, setInboxRead] = useState<Set<string>>(new Set())
  const [selectedThreadId, setSelectedThreadId] = useState<string | null>(init.selectedThreadId)
  const [threadFilter, setThreadFilter] = useState<`open` | `resolved`>(`open`)
  const [threadRead, setThreadRead] = useState<Set<string>>(new Set())

  const api: WebApi = {
    interactive,
    injectedIssue,
    nav,
    setNav,
    inboxTab,
    setInboxTab,
    openIssueId,
    face,
    setFace,
    openIssue: (id, nextFace = `issue`) => {
      setOpenIssueId(id)
      setFace(nextFace)
      /* A live run already owns its (permanent) tab; anything else opens an
         ordinary one at the end of the strip. */
      if (!sessionFor(id)) {
        setTabIds((prev) => (prev.includes(id) ? prev : [...prev, id]))
      }
      /* Navigating to a chip in a folded group unfolds it. */
      const agent = sessionFor(id)?.agent
      if (agent) setFoldedAgents((prev) => {
        if (!prev.has(agent)) return prev
        const next = new Set(prev)
        next.delete(agent)
        return next
      })
    },
    closeIssue: () => setOpenIssueId(null),
    startCoding: (id) => {
      setAgentSeedId(id)
      setOpenIssueId(null)
      setNav(`agent`)
    },
    agentSeedId,
    tabIds,
    closeTab: (id) => {
      setTabIds((prev) => prev.filter((t) => t !== id))
      if (openIssueId === id) setOpenIssueId(null)
    },
    foldedAgents,
    toggleAgentFold: (agent, folded) =>
      setFoldedAgents((prev) => {
        const next = new Set(prev)
        const fold = folded ?? !prev.has(agent)
        if (fold) next.add(agent)
        else next.delete(agent)
        return next
      }),
    collapsedGroups,
    toggleGroup: (status) => setCollapsedGroups((prev) => toggledSet(prev, status)),
    inboxRead,
    markInboxRead: (id) => setInboxRead((prev) => new Set(prev).add(id)),
    markAllInboxRead: () =>
      setInboxRead((prev) => {
        const next = new Set(prev)
        INBOX_ITEMS.forEach((n) => next.add(n.id))
        return next
      }),
    selectedThreadId,
    selectThread: (id) => {
      setSelectedThreadId(id)
      setThreadRead((prev) => new Set(prev).add(id))
    },
    threadFilter,
    setThreadFilter,
    threadRead,
  }

  const main = openIssueId ? (
    <WebIssueDetail issueId={openIssueId} />
  ) : nav === `project` ? (
    <WebBoard />
  ) : nav === `agent` ? (
    <WebAgentPage key={agentSeedId ?? `chat`} />
  ) : nav === `inbox` ? (
    <WebInbox />
  ) : (
    <WebSupportInbox />
  )

  const { ref, scale } = useDemoScale(BASE_W)

  return (
    <div
      ref={ref}
      className={`web-scale${className ? ` ${className}` : ``}`}
      style={{ height: Math.round(WEB_H * scale) }}
    >
      <WebContext.Provider value={api}>
        <div
          className={`web-root${interactive ? `` : ` is-static`}`}
          style={scale < 1 ? { width: BASE_W, transform: `scale(${scale})` } : undefined}
        >
          <WebSidebar />
          {/* team/app-shell.ts: the content COLUMN — the work-tabs band on
              the bare page ground, the cutout card flush under it (EXP-870;
              the agent dock band is gone since EXP-818). */}
          <div className="web-maincol">
            <div className="web-tabsband">
              <WebWorkTabs />
            </div>
            <div className="web-main">{main}</div>
          </div>
        </div>
      </WebContext.Provider>
    </div>
  )
}

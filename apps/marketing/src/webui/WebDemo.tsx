/* ─── WebDemo — pixel-faithful, usable recreation of the Exponential web app ───
   Same house pattern as ide/Ide.tsx: fixed 1100×680 canvas auto-scaled to
   the container via the shared useDemoScale hook, one context object owns
   the interactive state, fixtures come from the shared universe in ide/data. */
import { useMemo, useState } from "react"
import type { FilterTab } from "../ide/data"
import { toggledSet } from "../ide/state"
import { INBOX_ITEMS } from "../ide/data"
import { useDemoScale } from "../lib/use-demo-scale"
import { WebContext, type WebApi, type WebNav, type WebView } from "./state"
import { SUPPORT_THREADS } from "./data"
import { WebSidebar } from "./Sidebar"
import { WebBoard, WebMyIssues } from "./Board"
import { WebIssueDetail } from "./IssueDetail"
import { WebInbox } from "./Inbox"
import { WebSupportInbox } from "./SupportInbox"

const BASE_W = 1100
const WEB_H = 680

type InitState = {
  nav: WebNav
  openIssueId: string | null
  selectedThreadId: string | null
}

const initialState = (view: WebView): InitState => {
  switch (view) {
    case `issue`:
      return { nav: `project`, openIssueId: `EXP-8`, selectedThreadId: SUPPORT_THREADS[0].id }
    case `inbox`:
      return { nav: `inbox`, openIssueId: null, selectedThreadId: SUPPORT_THREADS[0].id }
    case `support`:
      return { nav: `support`, openIssueId: null, selectedThreadId: SUPPORT_THREADS[0].id }
    default:
      return { nav: `project`, openIssueId: null, selectedThreadId: SUPPORT_THREADS[0].id }
  }
}

export type WebDemoProps = {
  view?: WebView
  interactive?: boolean
  className?: string
}

export function WebDemo({ view = `board`, interactive = true, className }: WebDemoProps) {
  const init = useMemo(() => initialState(view), [view])

  const [nav, setNav] = useState<WebNav>(init.nav)
  const [openIssueId, setOpenIssueId] = useState<string | null>(init.openIssueId)
  const [filter, setFilter] = useState<FilterTab>(`all`)
  const [collapsedGroups, setCollapsedGroups] = useState<Set<string>>(new Set())
  const [inboxRead, setInboxRead] = useState<Set<string>>(new Set())
  const [selectedThreadId, setSelectedThreadId] = useState<string | null>(init.selectedThreadId)
  const [threadFilter, setThreadFilter] = useState<`open` | `resolved`>(`open`)
  const [threadRead, setThreadRead] = useState<Set<string>>(new Set())

  const api: WebApi = {
    interactive,
    nav,
    setNav,
    openIssueId,
    openIssue: (id) => setOpenIssueId(id),
    closeIssue: () => setOpenIssueId(null),
    filter,
    setFilter,
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

  const main =
    nav === `project` ? (
      openIssueId ? (
        <WebIssueDetail issueId={openIssueId} />
      ) : (
        <WebBoard />
      )
    ) : nav === `my-issues` ? (
      openIssueId ? (
        <WebIssueDetail issueId={openIssueId} />
      ) : (
        <WebMyIssues />
      )
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
          <div className="web-main">{main}</div>
        </div>
      </WebContext.Provider>
    </div>
  )
}

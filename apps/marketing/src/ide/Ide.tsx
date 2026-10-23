/* ─── IdeDemo — a usable recreation of the Exponential desktop IDE ───
   Shell (shell.rs, EXP-723/1192): the left column — the 34px traffic-light
   strip, the fixed team header and the rail — full height on the window
   ground; right of it the 44px work-tabs band over the content CARD (10px
   margins left/right/bottom, radius 12, the card hairline over the panel
   wash). The card shows the active tab (an issue or a thread), else the
   rail destination's screen. Flow (EXP-1294): Start on an issue opens Home
   with the issue as the subject; the send starts ONE thread, opened as a
   tab; its turn ends on a question that waits in the Dock. ─── */
import { useEffect, useMemo, useState } from "react"
import {
  CHANGES,
  COMMITS,
  INBOX_ITEMS,
  codingScriptFor,
  getIssue,
  questionFor,
  type FeedRow,
} from "./data"
import {
  IdeContext,
  prefersReducedMotion,
  toggledSet,
  useIde,
  type IdeApi,
  type IdeView,
  type ScriptPos,
  type Tab,
  type ThreadView,
  type Tool,
} from "./state"
import { Topbar } from "./Topbar"
import { Rail } from "./Rail"
import { InboxDetail, InboxScreen, ThreadsScreen } from "./Sidebar"
import { BoardPanel } from "./Board"
import { IssueDetail } from "./IssueDetail"
import { FilesPanel, FileTab } from "./Files"
import { SourceControlScreen } from "./SourceControl"
import { SessionScreen } from "./Session"
import { ChatScreen } from "./Chat"
import { useDemoScale } from "../lib/use-demo-scale"

const BASE_W = 1440
const IDE_H = 900

const THREAD_KEY = `thread:1`
const issueTab = (id: string, origin?: `inbox`): Tab => ({ key: `issue:${id}`, kind: `issue`, ref: id, origin })

type InitState = { tool: Tool; tabs: Tab[]; active: string | null }

const initialState = (view: IdeView): InitState => {
  switch (view) {
    case `issue`:
      return { tool: `board`, tabs: [issueTab(`EXP-8`)], active: `issue:EXP-8` }
    case `files`:
      return { tool: `files`, tabs: [], active: null }
    case `source-control`:
      return { tool: `source-control`, tabs: [], active: null }
    default:
      return { tool: `board`, tabs: [], active: null }
  }
}

/* The card's content: the active tab, else the rail destination. */
function MainView() {
  const { tool, tabs, active } = useIde()
  const tab = tabs.find((t) => t.key === active)
  if (tab) {
    if (tab.kind === `thread`) {
      return (
        <div className="ide-center">
          <SessionScreen />
        </div>
      )
    }
    if (tab.origin === `inbox`) return <InboxDetail issueId={tab.ref} />
    return (
      <div className="ide-center">
        <IssueDetail key={tab.ref} issueId={tab.ref} />
      </div>
    )
  }
  switch (tool) {
    case `agent`:
      return (
        <div className="ide-center">
          <ChatScreen />
        </div>
      )
    case `inbox`:
      return <InboxScreen />
    case `threads`:
      return (
        <div className="ide-center">
          <ThreadsScreen />
        </div>
      )
    case `files`:
      return (
        <>
          <FilesPanel />
          <div className="ide-center">
            <FileTab />
          </div>
        </>
      )
    case `source-control`:
      return <SourceControlScreen />
    default:
      return (
        <div className="ide-center">
          <BoardPanel />
        </div>
      )
  }
}

export type IdeDemoProps = {
  view?: IdeView
  interactive?: boolean
  className?: string
}

export function IdeDemo({ view = `board`, interactive = true, className }: IdeDemoProps) {
  const init = useMemo(() => initialState(view), [view])

  const [tool, setToolState] = useState<Tool>(init.tool)
  const [tabs, setTabs] = useState<Tab[]>(init.tabs)
  const [active, setActive] = useState<string | null>(init.active)
  const [collapsedGroups, setCollapsedGroups] = useState<Set<string>>(new Set())
  const [expandedDirs, setExpandedDirs] = useState<Set<string>>(new Set([`apps`]))
  const [selectedFile, setSelectedFile] = useState<string | null>(null)
  const [inboxRead, setInboxRead] = useState<Set<string>>(new Set())
  const [subject, setSubject] = useState<string[]>([])

  /* The one thread a send starts: its script, its position, its state. */
  const [threadIssue, setThreadIssue] = useState<string | null>(null)
  const [firstMessage, setFirstMessage] = useState(``)
  const [threadState, setThreadState] = useState<ThreadView[`state`]>(`working`)
  const [extraRows, setExtraRows] = useState<FeedRow[]>([])
  const [scriptPos, setScriptPos] = useState<ScriptPos>({ done: 0, chars: 0 })
  const [runId, setRunId] = useState(0)

  /* The send's own words (the optional instructions) lead as the person's
     bubble; then the session's work. */
  const script = useMemo<FeedRow[]>(
    () =>
      threadIssue
        ? [
            ...(firstMessage ? [{ kind: `user` as const, text: firstMessage }] : []),
            ...codingScriptFor(getIssue(threadIssue)),
          ]
        : [],
    [threadIssue, firstMessage],
  )

  /* The transcript streams the way a session's does: prose types out, Trace
     lines land whole. The turn ends on the question (EXP-674), which waits
     in the Dock. Instant under prefers-reduced-motion. Stop ends the TURN
     where it is; the session stays (it waits for the next message). */
  useEffect(() => {
    if (!threadIssue || threadState !== `working`) return undefined
    if (prefersReducedMotion()) {
      setScriptPos({ done: script.length, chars: 0 })
      const t = window.setTimeout(() => setThreadState(`needs_you`), 400)
      return () => window.clearTimeout(t)
    }
    let done = 0
    let chars = 0
    let t: number
    const tick = () => {
      if (done >= script.length) {
        setThreadState(`needs_you`)
        return
      }
      const row = script[done]
      if (row.kind === `narration` && chars < row.text.length) {
        chars += Math.max(2, Math.round(row.text.length / 60))
        setScriptPos({ done, chars: Math.min(chars, row.text.length) })
        t = window.setTimeout(tick, 22)
        return
      }
      done += 1
      chars = 0
      setScriptPos({ done, chars: 0 })
      const next = script[done]
      t = window.setTimeout(tick, !next ? 600 : next.kind === `narration` ? 480 : 360)
    }
    setScriptPos({ done: 0, chars: 0 })
    t = window.setTimeout(tick, 450)
    return () => window.clearTimeout(t)
  }, [threadIssue, threadState, script, runId])

  const typingRow = threadState === `working` ? script[scriptPos.done] : undefined
  const thread: ThreadView | null = threadIssue
    ? {
        issueId: threadIssue,
        state: threadState,
        rows: [...script.slice(0, scriptPos.done), ...extraRows],
        typing: typingRow?.kind === `narration` && scriptPos.chars > 0 ? typingRow.text.slice(0, scriptPos.chars) : null,
        question: threadState === `needs_you` ? questionFor() : null,
      }
    : null

  const ensureTab = (tab: Tab) =>
    setTabs((prev) => {
      const at = prev.findIndex((t) => t.key === tab.key)
      if (at < 0) return [...prev, tab]
      const next = [...prev]
      next[at] = tab
      return next
    })

  const setTool = (next: Tool) => {
    setToolState(next)
    setActive(null)
  }

  const openThread = () => {
    if (!threadIssue) return
    ensureTab({ key: THREAD_KEY, kind: `thread`, ref: THREAD_KEY })
    setActive(THREAD_KEY)
  }

  const api: IdeApi = {
    interactive,
    tool,
    setTool,
    tabs,
    active,
    selectTab: setActive,
    closeTab: (key) => {
      const idx = tabs.findIndex((t) => t.key === key)
      const next = tabs.filter((t) => t.key !== key)
      setTabs(next)
      if (active === key) {
        setActive(next.length > 0 ? next[Math.min(idx, next.length - 1)].key : null)
      }
    },
    openIssue: (id, origin) => {
      const tab = issueTab(id, origin)
      ensureTab(tab)
      setActive(tab.key)
    },
    collapsedGroups,
    toggleGroup: (status) => setCollapsedGroups((prev) => toggledSet(prev, status)),
    expandedDirs,
    toggleDir: (path) => setExpandedDirs((prev) => toggledSet(prev, path)),
    selectedFile,
    selectFile: setSelectedFile,
    changes: CHANGES,
    commits: COMMITS,
    inboxRead,
    markInboxRead: (id) => setInboxRead((prev) => new Set(prev).add(id)),
    markAllInboxRead: () =>
      setInboxRead((prev) => {
        const next = new Set(prev)
        INBOX_ITEMS.forEach((n) => next.add(n.id))
        return next
      }),
    subject,
    start: (issueId) => {
      setSubject([issueId])
      setTool(`agent`)
    },
    removeSubject: (issueId) => setSubject((prev) => prev.filter((id) => id !== issueId)),
    send: (text) => {
      /* The demo scripts ONE kind of thread: an issue's (Home's send stays
         disabled without an issue subject). */
      const issueId = subject[0]
      if (!issueId) return
      setThreadIssue(issueId)
      setFirstMessage(text.trim())
      setThreadState(`working`)
      setExtraRows([])
      setRunId((n) => n + 1)
      setSubject([])
      ensureTab({ key: THREAD_KEY, kind: `thread`, ref: THREAD_KEY })
      setActive(THREAD_KEY)
    },
    thread,
    threadKey: THREAD_KEY,
    openThread,
    stopThread: () => {
      if (threadState === `working`) setThreadState(`waiting`)
    },
    answer: (text) => {
      if (threadState !== `needs_you`) return
      setExtraRows([
        { kind: `user`, text },
        {
          kind: `narration`,
          text: `Done: ${text.charAt(0).toLowerCase()}${text.slice(1)}. Pushed the follow-up and opened the PR; it is ready for review.`,
        },
      ])
      setThreadState(`waiting`)
    },
    steer: (text) => {
      const body = text.trim()
      if (!body) return
      if (threadState === `needs_you`) {
        api.answer(body)
        return
      }
      /* A message to the idle session: its bubble, then the agent's reply.
         (While a turn runs the demo's field is read-only.) */
      if (threadState !== `waiting`) return
      setExtraRows((prev) => [
        ...prev,
        { kind: `user`, text: body },
        { kind: `narration`, text: `On it: I'll fold that into the same branch and push it to the open PR.` },
      ])
    },
  }

  const { ref, scale } = useDemoScale(BASE_W)

  return (
    <div
      ref={ref}
      className={`ide-scale${className ? ` ${className}` : ``}`}
      style={{ height: Math.round(IDE_H * scale) }}
    >
      <IdeContext.Provider value={api}>
        <div
          className={`ide-root${interactive ? `` : ` is-static`}`}
          style={scale < 1 ? { width: BASE_W, transform: `scale(${scale})` } : undefined}
        >
          <Rail />
          <div className="ide-main">
            <Topbar />
            <div className="ide-panel">
              <MainView />
            </div>
          </div>
        </div>
      </IdeContext.Provider>
    </div>
  )
}

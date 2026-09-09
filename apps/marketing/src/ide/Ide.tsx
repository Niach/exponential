/* ─── IdeDemo — pixel-faithful, usable recreation of the Exponential desktop IDE ─── */
import { useEffect, useMemo, useRef, useState } from "react"
import {
  CHANGES,
  COMMITS,
  INBOX_ITEMS,
  batchCodingScriptFor,
  codingScriptFor,
  getIssue,
  type Change,
  type Commit,
  type FilterTab,
} from "./data"
import {
  IdeContext,
  prefersReducedMotion,
  toggledSet,
  useIde,
  type CodingState,
  type CodingTarget,
  type IdeApi,
  type IdeView,
  type ScriptPos,
  type Tab,
  type Tool,
} from "./state"
import { Topbar } from "./Topbar"
import { Rail } from "./Rail"
import { ReviewsScreen, SidebarPanel } from "./Sidebar"
import { IssueDetail } from "./IssueDetail"
import { FileTab } from "./Files"
import { ScTab } from "./SourceControl"
import { SessionScreen } from "./Session"
import { StartCodingDialog } from "./StartCodingDialog"
import { IcInbox } from "./icons"
import { useDemoScale } from "../lib/use-demo-scale"

const BASE_W = 1440
const IDE_H = 900

const issueTab = (id: string): Tab => ({ key: `issue:${id}`, kind: `issue`, label: id, ref: id })

type InitState = {
  tool: Tool
  tabs: Tab[]
  active: string | null
  selectedFile: string | null
}

const initialState = (view: IdeView): InitState => {
  switch (view) {
    case `issue`:
      return { tool: `issues`, tabs: [issueTab(`EXP-8`)], active: `issue:EXP-8`, selectedFile: null }
    case `files`:
      return { tool: `files`, tabs: [], active: null, selectedFile: null }
    case `source-control`:
      return { tool: `source-control`, tabs: [], active: null, selectedFile: null }
    default:
      return { tool: `issues`, tabs: [], active: null, selectedFile: null }
  }
}

function EmptyState() {
  return (
    <div className="ide-empty">
      <IcInbox size={18} className="ide-c-muted" />
      <span className="ide-empty-title">Nothing open</span>
      <span className="ide-empty-sub">Pick an issue from the sidebar. It opens as a tab here.</span>
    </div>
  )
}

/* EXP-288: the center is driven by the active TOOL for Files / Source
   Control (they are tab-less full-page modes) and by the open issue tab
   otherwise. Only issues get a tab chip in the titlebar. */
function CenterArea() {
  const { tool, tabs, active } = useIde()
  /* EXP-706: Reviews is a rail SCREEN — it replaces the tool window and the
     center together, exactly like Devices/Actions/Automations. */
  if (tool === `reviews`) {
    return (
      <div className="ide-center">
        <ReviewsScreen />
      </div>
    )
  }
  if (tool === `files`) {
    return (
      <div className="ide-center">
        <FileTab />
      </div>
    )
  }
  if (tool === `source-control`) {
    return (
      <div className="ide-center">
        <ScTab />
      </div>
    )
  }
  const openTabs = tabs.filter((t) => t.kind === `issue`)
  if (openTabs.length === 0) {
    return (
      <div className="ide-center">
        <EmptyState />
      </div>
    )
  }
  return (
    <div className="ide-center">
      {openTabs.map((tab) => (
        <div key={tab.key} className="ide-tabpane" hidden={tab.key !== active}>
          <IssueDetail issueId={tab.ref} />
        </div>
      ))}
    </div>
  )
}

export type IdeDemoProps = {
  view?: IdeView
  interactive?: boolean
  className?: string
}

export function IdeDemo({ view = `board`, interactive = true, className }: IdeDemoProps) {
  const init = useMemo(() => initialState(view), [view])

  const [tool, setTool] = useState<Tool>(init.tool)
  const [tabs, setTabs] = useState<Tab[]>(init.tabs)
  const [active, setActive] = useState<string | null>(init.active)
  const [filter, setFilter] = useState<FilterTab>(`all`)
  const [collapsedGroups, setCollapsedGroups] = useState<Set<string>>(new Set())
  const [expandedDirs, setExpandedDirs] = useState<Set<string>>(new Set([`apps`]))
  const [selectedFile, setSelectedFile] = useState<string | null>(init.selectedFile)
  const [viewedBranch, setViewedBranch] = useState(`master`)
  const [changes] = useState<Change[]>(CHANGES)
  const [commits] = useState<Commit[]>(COMMITS)
  const [coding, setCoding] = useState<CodingState>(`idle`)
  const [codingTarget, setCodingTarget] = useState<CodingTarget | null>(null)
  const [pendingCoding, setPendingCoding] = useState<CodingTarget | null>(null)
  const [runId, setRunId] = useState(0)
  const [scriptPos, setScriptPos] = useState<ScriptPos>({ done: 0, chars: 0 })
  const [sessionOpen, setSessionOpen] = useState(false)
  const [inboxRead, setInboxRead] = useState<Set<string>>(new Set())
  const [mergedReviews, setMergedReviews] = useState<Set<string>>(new Set())
  const [goneReviews, setGoneReviews] = useState<Set<string>>(new Set())
  const mergeTimers = useRef<number[]>([])

  /* Clear pending merge animate-out timers on unmount */
  useEffect(
    () => () => {
      mergeTimers.current.forEach((t) => window.clearTimeout(t))
    },
    [],
  )

  /* The scripted Claude session — single-issue or batch, by target kind. */
  const codingScript = useMemo(() => {
    if (!codingTarget) return []
    return codingTarget.kind === `issue`
      ? codingScriptFor(getIssue(codingTarget.id))
      : batchCodingScriptFor(codingTarget.issueIds.map(getIssue))
  }, [codingTarget])

  /* The transcript streams in the way a run's does: prose types out, tool
     rows land whole. The run does not "finish" — its turn ends on the
     question card and it holds for a reply (EXP-674). Instant when
     prefers-reduced-motion. */
  useEffect(() => {
    if (coding !== `running`) return undefined
    if (prefersReducedMotion()) {
      setScriptPos({ done: codingScript.length, chars: 0 })
      const t = window.setTimeout(() => setCoding(`waiting`), 500)
      return () => window.clearTimeout(t)
    }
    let done = 0
    let chars = 0
    let t: number
    const tick = () => {
      if (done >= codingScript.length) {
        setCoding(`waiting`)
        return
      }
      const row = codingScript[done]
      if (row.kind === `narration` && chars < row.text.length) {
        chars += Math.max(2, Math.round(row.text.length / 60))
        setScriptPos({ done, chars: Math.min(chars, row.text.length) })
        t = window.setTimeout(tick, 22)
        return
      }
      done += 1
      chars = 0
      setScriptPos({ done, chars: 0 })
      const next = codingScript[done]
      const delay = !next ? 700 : next.kind === `narration` ? 520 : 380
      t = window.setTimeout(tick, delay)
    }
    setScriptPos({ done: 0, chars: 0 })
    t = window.setTimeout(tick, 450)
    return () => window.clearTimeout(t)
  }, [coding, runId, codingScript])

  const openTab = (tab: Tab) => {
    setTabs((prev) => (prev.some((t) => t.key === tab.key) ? prev : [...prev, tab]))
    setActive(tab.key)
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
    openIssue: (id) => openTab(issueTab(id)),
    openFile: (path) => {
      setTool(`files`)
      setSelectedFile(path)
    },
    openSourceControl: () => setTool(`source-control`),
    filter,
    setFilter,
    collapsedGroups,
    toggleGroup: (status) => setCollapsedGroups((prev) => toggledSet(prev, status)),
    expandedDirs,
    toggleDir: (path) => setExpandedDirs((prev) => toggledSet(prev, path)),
    selectedFile,
    selectFile: setSelectedFile,
    viewedBranch,
    viewBranch: setViewedBranch,
    changes,
    commits,
    inboxRead,
    markInboxRead: (id) => setInboxRead((prev) => new Set(prev).add(id)),
    markAllInboxRead: () =>
      setInboxRead((prev) => {
        const next = new Set(prev)
        INBOX_ITEMS.forEach((n) => next.add(n.id))
        return next
      }),
    mergedReviews,
    goneReviews,
    mergeReview: (issueId) => {
      setMergedReviews((prev) => new Set(prev).add(issueId))
      mergeTimers.current.push(
        window.setTimeout(
          () => setGoneReviews((prev) => new Set(prev).add(issueId)),
          900,
        ),
      )
    },
    coding,
    codingTarget,
    codingScript,
    pendingCoding,
    requestCoding: (target) => setPendingCoding(target),
    cancelStartCoding: () => setPendingCoding(null),
    confirmStartCoding: (target) => {
      setCodingTarget(target)
      setPendingCoding(null)
      setCoding(`running`)
      setRunId((n) => n + 1)
      /* A start opens the run's screen, the way a launch navigates to it. */
      setSessionOpen(true)
    },
    stopCoding: () => setCoding(`ended`),
    scriptPos,
    sessionOpen,
    openSession: () => setSessionOpen(true),
    closeSession: () => setSessionOpen(false),
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
          {/* The labelled rail is the ONE full-height column (it carries
              its own titlebar strip); the decoration band and the cutout
              panel with its panes live in the content column right of it
              (shell.rs, EXP-723). The bottom session bar (EXP-769/791)
              carries terminal tabs alone and takes no height while none is
              open, which is this demo's whole story: it never draws. */}
          <Rail />
          <div className="ide-main">
            <Topbar />
            <div className="ide-panel">
              <div className="ide-main-top">
                {sessionOpen ? (
                  /* EXP-791: a session fills the center — no tool window
                     beside it, exactly like Reviews. */
                  <SessionScreen />
                ) : (
                  <>
                    {tool !== `reviews` && <SidebarPanel />}
                    <CenterArea />
                  </>
                )}
              </div>
            </div>
          </div>
          {pendingCoding && <StartCodingDialog />}
        </div>
      </IdeContext.Provider>
    </div>
  )
}

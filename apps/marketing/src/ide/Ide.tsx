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
  type DockTab,
  type IdeApi,
  type IdeView,
  type ScriptPos,
  type Tab,
  type Tool,
} from "./state"
import { Topbar } from "./Topbar"
import { Rail } from "./Rail"
import { SidebarPanel } from "./Sidebar"
import { IssueDetail } from "./IssueDetail"
import { FileTab } from "./Files"
import { ScTab } from "./SourceControl"
import { TerminalDock } from "./Terminal"
import { StartCodingDialog } from "./StartCodingDialog"
import { IcInbox, IcX } from "./icons"

const BASE_W = 960
const IDE_H = 640

const issueTab = (id: string): Tab => ({ key: `issue:${id}`, kind: `issue`, label: id, ref: id })
const fileTab = (path: string): Tab => ({
  key: `file:${path}`,
  kind: `file`,
  label: path.split(`/`).pop() ?? path,
  ref: path,
})
const scTab = (): Tab => ({ key: `sc`, kind: `sc`, label: `Source Control`, ref: `` })

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
      return {
        tool: `files`,
        tabs: [fileTab(`package.json`)],
        active: `file:package.json`,
        selectedFile: `package.json`,
      }
    case `source-control`:
      return { tool: `source-control`, tabs: [scTab()], active: `sc`, selectedFile: null }
    default:
      return { tool: `issues`, tabs: [], active: null, selectedFile: null }
  }
}

function useIdeScale() {
  const ref = useRef<HTMLDivElement | null>(null)
  const [scale, setScale] = useState(1)
  useEffect(() => {
    const el = ref.current
    if (!el || typeof ResizeObserver === `undefined`) return undefined
    const ro = new ResizeObserver((entries) => {
      const w = entries[0]?.contentRect.width ?? BASE_W
      setScale(w >= BASE_W ? 1 : Math.max(w / BASE_W, 0.3))
    })
    ro.observe(el)
    return () => ro.disconnect()
  }, [])
  return { ref, scale }
}

function EmptyState() {
  return (
    <div className="ide-empty">
      <IcInbox size={24} className="ide-c-muted" />
      <span className="ide-empty-title">Nothing open</span>
      <span className="ide-empty-sub">Pick an issue from the sidebar — it opens as a tab here.</span>
    </div>
  )
}

function CenterArea() {
  const { tabs, active, selectTab, closeTab, interactive } = useIde()
  if (tabs.length === 0) {
    return (
      <div className="ide-center">
        <EmptyState />
      </div>
    )
  }
  return (
    <div className="ide-center">
      <div className="ide-tabbar">
        {tabs.map((tab) => (
          <div
            key={tab.key}
            className={`ide-tab${tab.key === active ? ` is-active` : ``}${interactive ? ` is-click` : ``}`}
            onClick={interactive ? () => selectTab(tab.key) : undefined}
          >
            <span className={`ide-tab-title${tab.kind === `issue` ? ` ide-mono` : ``}`}>
              {tab.label}
            </span>
            <button
              className={`ide-tab-x${interactive ? ` is-click` : ``}`}
              type="button"
              title="Close tab"
              onClick={
                interactive
                  ? (e) => {
                      e.stopPropagation()
                      closeTab(tab.key)
                    }
                  : undefined
              }
            >
              <IcX size={11} />
            </button>
          </div>
        ))}
      </div>
      <div className="ide-tabpanes">
        {tabs.map((tab) => (
          <div key={tab.key} className="ide-tabpane" hidden={tab.key !== active}>
            {tab.kind === `issue` ? (
              <IssueDetail issueId={tab.ref} />
            ) : tab.kind === `file` ? (
              <FileTab />
            ) : (
              <ScTab />
            )}
          </div>
        ))}
      </div>
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
  const [changes, setChanges] = useState<Change[]>(CHANGES)
  const [staged, setStaged] = useState<Set<string>>(new Set())
  const [commits, setCommits] = useState<Commit[]>(COMMITS)
  const [ahead, setAhead] = useState(0)
  const [coding, setCoding] = useState<CodingState>(`idle`)
  const [codingTarget, setCodingTarget] = useState<CodingTarget | null>(null)
  const [pendingCoding, setPendingCoding] = useState<CodingTarget | null>(null)
  const [codedIssues, setCodedIssues] = useState<Set<string>>(new Set())
  const [runId, setRunId] = useState(0)
  const [scriptPos, setScriptPos] = useState<ScriptPos>({ done: 0, chars: 0 })
  const [dockOpen, setDockOpen] = useState(false)
  const [dockTab, setDockTab] = useState<DockTab>(`shell`)
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

  /* Typed-out Claude session. Instant when prefers-reduced-motion. */
  useEffect(() => {
    if (coding !== `running` || !codingTarget) return undefined
    const finish = () => {
      setCoding(`ended`)
      /* A batch run ships every checked issue in its one combined PR. */
      const finished = codingTarget.kind === `issue` ? [codingTarget.id] : codingTarget.issueIds
      setCodedIssues((prev) => {
        const next = new Set(prev)
        finished.forEach((id) => next.add(id))
        return next
      })
    }
    if (prefersReducedMotion()) {
      setScriptPos({ done: codingScript.length, chars: 0 })
      const t = window.setTimeout(finish, 500)
      return () => window.clearTimeout(t)
    }
    let done = 0
    let chars = 0
    let t: number
    const tick = () => {
      if (done >= codingScript.length) {
        finish()
        return
      }
      const line = codingScript[done]
      if (line.kind === `cmd` && chars < line.text.length) {
        chars += 1
        setScriptPos({ done, chars })
        t = window.setTimeout(tick, 18)
        return
      }
      done += 1
      chars = 0
      setScriptPos({ done, chars: 0 })
      const next = codingScript[done]
      const delay = !next ? 700 : next.kind === `cmd` ? 500 : next.kind === `claude` ? 550 : 420
      t = window.setTimeout(tick, delay)
    }
    setScriptPos({ done: 0, chars: 0 })
    t = window.setTimeout(tick, 450)
    return () => window.clearTimeout(t)
  }, [coding, runId, codingTarget, codingScript])

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
    openFile: (path) => openTab(fileTab(path)),
    openSourceControl: () => {
      setTool(`source-control`)
      openTab(scTab())
    },
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
    staged,
    toggleStaged: (path) => setStaged((prev) => toggledSet(prev, path)),
    commits,
    commitAll: (message, push) => {
      if (changes.length === 0 || message.length === 0) return
      setCommits((prev) => [{ subject: message, meta: `niach · just now` }, ...prev])
      setChanges([])
      setStaged(new Set())
      setAhead(push ? 0 : ahead + 1)
    },
    ahead,
    push: () => setAhead(0),
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
    codedIssues,
    pendingCoding,
    requestCoding: (target) => setPendingCoding(target),
    cancelStartCoding: () => setPendingCoding(null),
    confirmStartCoding: (target) => {
      setCodingTarget(target)
      setPendingCoding(null)
      setCoding(`running`)
      setRunId((n) => n + 1)
      setDockOpen(true)
      setDockTab(`claude`)
    },
    stopCoding: () => setCoding(`ended`),
    scriptPos,
    dockOpen,
    setDockOpen,
    dockTab,
    setDockTab,
  }

  const { ref, scale } = useIdeScale()

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
          <Topbar />
          <div className="ide-body">
            <Rail />
            {/* The terminal dock spans sidebar + center; only the icon rail
                stays full-height, matching the real IDE. */}
            <div className="ide-main">
              <div className="ide-main-top">
                <SidebarPanel />
                <CenterArea />
              </div>
              <TerminalDock />
            </div>
          </div>
          {pendingCoding && <StartCodingDialog />}
        </div>
      </IdeContext.Provider>
    </div>
  )
}

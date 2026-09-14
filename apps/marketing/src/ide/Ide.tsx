/* ─── IdeDemo — a usable recreation of the Exponential desktop IDE (0.14.42) ───
   Shell (shell.rs, EXP-870/877): the left column is the rail, 272px and
   labelled, or — while a list is folded in beside an open issue — the 48px
   icon rail plus that ListNav. Right of it: the 44px work-tabs band over the
   cutout panel. An issue and its run are ONE top tab (Issue | Run | +N −M in
   the work header); every live run of mine owns a tab, clustered by agent. */
import { useEffect, useMemo, useRef, useState } from "react"
import {
  BATCH_RUN_TITLE,
  CHANGES,
  COMMITS,
  INBOX_ITEMS,
  ISSUES,
  REVIEW_RUN,
  REVIEW_RUN_FILES,
  SCRIPTED_RUN_FILES,
  batchCodingScriptFor,
  codingScriptFor,
  getIssue,
  reviewRunScript,
  type Change,
  type Commit,
  type FeedRow,
  type FilterTab,
} from "./data"
import {
  IdeContext,
  isLive,
  prefersReducedMotion,
  toggledSet,
  useIde,
  type CodingState,
  type CodingTarget,
  type Face,
  type IdeApi,
  type IdeView,
  type RunId,
  type RunView,
  type ScriptPos,
  type Tab,
  type Tool,
} from "./state"
import { Topbar } from "./Topbar"
import { Rail } from "./Rail"
import { InboxScreen, ListNav, ReviewsScreen } from "./Sidebar"
import { BoardPanel } from "./Board"
import { IssueDetail } from "./IssueDetail"
import { FilesPanel, FileTab } from "./Files"
import { ScPanel, ScTab } from "./SourceControl"
import { DiffFace, SessionScreen } from "./Session"
import { ChatScreen } from "./Chat"
import { useDemoScale } from "../lib/use-demo-scale"

const BASE_W = 1440
const IDE_H = 900

const issueTab = (id: string): Tab => ({ key: `issue:${id}`, kind: `issue`, label: id, ref: id })
const BATCH_TAB: Tab = { key: `run:batch`, kind: `run`, label: BATCH_RUN_TITLE, ref: `batch` }
const REVIEW_TAB = issueTab(REVIEW_RUN.issueId)

type InitState = {
  tool: Tool
  tabs: Tab[]
  active: string | null
  selectedFile: string | null
}

const initialState = (view: IdeView): InitState => {
  switch (view) {
    case `issue`:
      return {
        tool: `issues`,
        tabs: [REVIEW_TAB, issueTab(`EXP-8`)],
        active: `issue:EXP-8`,
        selectedFile: null,
      }
    case `files`:
      return { tool: `files`, tabs: [REVIEW_TAB], active: null, selectedFile: null }
    case `source-control`:
      return { tool: `source-control`, tabs: [REVIEW_TAB], active: null, selectedFile: null }
    default:
      return { tool: `issues`, tabs: [REVIEW_TAB], active: null, selectedFile: null }
  }
}

/* One top tab's content, by its face. */
function WorkTab({ tab }: { tab: Tab }) {
  const { faceOf, runForTab } = useIde()
  const run = runForTab(tab.key)
  const face = faceOf(tab.key)
  if (run && face === `diff`) return <DiffFace tabKey={tab.key} />
  if (run && (face === `run` || tab.kind === `run`)) return <SessionScreen tabKey={tab.key} />
  return <IssueDetail issueId={tab.ref} />
}

/* The main view: the active tab, else the rail destination's screen. */
function MainView() {
  const { tool, tabs, active } = useIde()
  const tab = tabs.find((t) => t.key === active)
  if (tab) {
    return (
      <div className="ide-center">
        <WorkTab key={tab.key} tab={tab} />
      </div>
    )
  }
  switch (tool) {
    case `agent`:
      return <ChatScreen />
    case `reviews`:
      return (
        <div className="ide-center">
          <ReviewsScreen />
        </div>
      )
    case `inbox`:
      return (
        <div className="ide-center">
          <InboxScreen />
        </div>
      )
    case `files`:
      return (
        <>
          <div className="ide-sidebar">
            <FilesPanel />
          </div>
          <div className="ide-center">
            <FileTab />
          </div>
        </>
      )
    case `source-control`:
      return (
        <>
          <div className="ide-sidebar">
            <ScPanel />
          </div>
          <div className="ide-center">
            <ScTab />
          </div>
        </>
      )
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
  const [faces, setFaces] = useState<Record<string, Face>>({})
  const [listNav, setListNav] = useState(false)
  const [filter, setFilter] = useState<FilterTab>(`all`)
  const [collapsedGroups, setCollapsedGroups] = useState<Set<string>>(new Set())
  const [expandedDirs, setExpandedDirs] = useState<Set<string>>(new Set([`apps`]))
  const [selectedFile, setSelectedFile] = useState<string | null>(init.selectedFile)
  const [viewedBranch, setViewedBranch] = useState(`master`)
  const [changes] = useState<Change[]>(CHANGES)
  const [commits] = useState<Commit[]>(COMMITS)
  const [coding, setCoding] = useState<CodingState>(`idle`)
  const [codingTarget, setCodingTarget] = useState<CodingTarget | null>(null)
  const [chips, setChips] = useState<string[]>([])
  const [runId, setRunId] = useState(0)
  const [scriptPos, setScriptPos] = useState<ScriptPos>({ done: 0, chars: 0 })
  const [extraRows, setExtraRows] = useState<Record<RunId, FeedRow[]>>({
    scripted: [],
    review: [],
  })
  const [reviewStopped, setReviewStopped] = useState(false)
  const [foldedAgents, setFoldedAgents] = useState<Set<string>>(new Set())
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

  /* The scripted run — single-issue or batch, by target kind. */
  const codingScript = useMemo(() => {
    if (!codingTarget) return []
    return codingTarget.kind === `issue`
      ? codingScriptFor(getIssue(codingTarget.id))
      : batchCodingScriptFor(codingTarget.issueIds.map(getIssue))
  }, [codingTarget])

  /* The transcript streams in the way a run's does: prose types out, tool
     rows land whole. The run does not "finish" — its turn ends on the
     question card and it holds for an answer (EXP-674). Instant when
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

  /* The Codex run's PR merging ends it (EXP-498), as does its Stop. */
  const reviewState: CodingState =
    reviewStopped || mergedReviews.has(REVIEW_RUN.issueId) ? `ended` : `review`

  const scriptedTabKey = codingTarget
    ? codingTarget.kind === `issue`
      ? `issue:${codingTarget.id}`
      : BATCH_TAB.key
    : null

  const runs: RunView[] = []
  if (codingTarget && coding !== `idle` && scriptedTabKey) {
    const issue = codingTarget.kind === `issue` ? getIssue(codingTarget.id) : null
    const rows = [...codingScript, ...extraRows.scripted]
    runs.push({
      id: `scripted`,
      agent: `claude`,
      state: coding,
      issueId: issue?.id ?? null,
      title: issue ? issue.title : BATCH_RUN_TITLE,
      tabKey: scriptedTabKey,
      rows,
      pos:
        coding === `running`
          ? scriptPos
          : { done: rows.length, chars: 0 },
      files: SCRIPTED_RUN_FILES,
      model: `Fable`,
    })
  }
  /* A newer scripted run on the same issue takes its tab. */
  if (scriptedTabKey !== REVIEW_TAB.key) {
    const issue = getIssue(REVIEW_RUN.issueId)
    const rows = [...reviewRunScript(issue), ...extraRows.review]
    runs.push({
      id: `review`,
      agent: REVIEW_RUN.agent,
      state: reviewState,
      issueId: issue.id,
      title: issue.title,
      tabKey: REVIEW_TAB.key,
      rows,
      pos: { done: rows.length, chars: 0 },
      files: REVIEW_RUN_FILES,
      model: `GPT-5.5`,
    })
  }

  const ensureTab = (tab: Tab) =>
    setTabs((prev) => (prev.some((t) => t.key === tab.key) ? prev : [...prev, tab]))

  const setTool = (next: Tool) => {
    setToolState(next)
    setActive(null)
    setListNav(false)
  }

  const api: IdeApi = {
    interactive,
    tool,
    setTool,
    tabs,
    active,
    selectTab: setActive,
    closeTab: (key) => {
      const run = runs.find((r) => r.tabKey === key)
      /* A live run's tab cannot be closed. */
      if (run && isLive(run.state)) return
      const idx = tabs.findIndex((t) => t.key === key)
      const next = tabs.filter((t) => t.key !== key)
      setTabs(next)
      if (active === key) {
        setActive(next.length > 0 ? next[Math.min(idx, next.length - 1)].key : null)
      }
    },
    faceOf: (key) => faces[key] ?? (key === BATCH_TAB.key ? `run` : `issue`),
    setFace: (key, face) => setFaces((prev) => ({ ...prev, [key]: face })),
    openIssue: (id, fromList = false) => {
      const tab = issueTab(id)
      ensureTab(tab)
      setActive(tab.key)
      setListNav(fromList && (tool === `issues` || tool === `inbox`))
    },
    openFile: (path) => {
      setTool(`files`)
      setSelectedFile(path)
    },
    openSourceControl: () => setTool(`source-control`),
    listNav,
    closeListNav: () => {
      setActive(null)
      setListNav(false)
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
      /* A PR merge ends the live run that opened it (EXP-498). */
      const scripted = runs.find((r) => r.id === `scripted`)
      if (scripted && isLive(scripted.state) && (scripted.issueId ?? `batch`) === issueId) {
        setCoding(`ended`)
      }
      mergeTimers.current.push(
        window.setTimeout(() => setGoneReviews((prev) => new Set(prev).add(issueId)), 900),
      )
    },
    coding,
    codingTarget,
    codingScript,
    scriptPos,
    composerOpen: tool === `agent` && active === null,
    chips,
    openComposer: (issueIds) => {
      setChips(issueIds)
      setTool(`agent`)
    },
    closeComposer: () => {
      if (tool === `agent`) setTool(`issues`)
    },
    toggleChip: (issueId) =>
      setChips((prev) =>
        prev.includes(issueId) ? prev.filter((id) => id !== issueId) : [...prev, issueId],
      ),
    submitComposer: () => {
      /* Stable board order, like the real launcher's prompt sections. */
      const ids = ISSUES.filter((issue) => chips.includes(issue.id)).map((i) => i.id)
      if (ids.length === 0) return
      const target: CodingTarget =
        ids.length === 1 ? { kind: `issue`, id: ids[0] } : { kind: `batch`, issueIds: ids }
      const tab = target.kind === `issue` ? issueTab(target.id) : BATCH_TAB
      setCodingTarget(target)
      setExtraRows((prev) => ({ ...prev, scripted: [] }))
      setCoding(`running`)
      setRunId((n) => n + 1)
      setChips([])
      /* A start follows the run in: its tab, on the Run face. */
      ensureTab(tab)
      setFaces((prev) => ({ ...prev, [tab.key]: `run` }))
      setActive(tab.key)
      setListNav(false)
    },
    runs,
    runForTab: (key) => runs.find((r) => r.tabKey === key) ?? null,
    openRun: (id) => {
      const run = runs.find((r) => r.id === id)
      if (!run) return
      const tab = run.issueId ? issueTab(run.issueId) : BATCH_TAB
      ensureTab(tab)
      setFaces((prev) => ({ ...prev, [tab.key]: `run` }))
      setActive(tab.key)
      setListNav(false)
    },
    stopRun: (id) => {
      if (id === `scripted`) setCoding(`ended`)
      else setReviewStopped(true)
    },
    stopCoding: () => setCoding(`ended`),
    answerQuestion: (answer) => {
      if (coding !== `waiting`) return
      const target = codingTarget
      const pr =
        target?.kind === `batch`
          ? `the combined PR`
          : `the PR`
      setExtraRows((prev) => ({
        ...prev,
        scripted: [
          { kind: `user`, text: answer },
          {
            kind: `narration`,
            text: `Done: ${answer.charAt(0).toLowerCase()}${answer.slice(1)}. Pushed the follow-up to ${pr}; it is ready for review.`,
          },
        ],
      }))
      setCoding(`review`)
    },
    steer: (id, text) => {
      const body = text.trim()
      if (!body) return
      if (id === `scripted` && coding === `waiting`) {
        api.answerQuestion(body)
        return
      }
      setExtraRows((prev) => ({ ...prev, [id]: [...prev[id], { kind: `user`, text: body }] }))
    },
    foldedAgents,
    toggleAgentFold: (agent) => setFoldedAgents((prev) => toggledSet(prev, agent)),
  }

  const { ref, scale } = useDemoScale(BASE_W)
  /* EXP-870: a list beside an open issue folds the rail to its icon column. */
  const compact = listNav && active !== null

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
          {/* The LEFT COLUMN is full height; the traffic lights float over
              its strip. The bottom terminal bar takes no height while no
              terminal is open, so it never draws here. */}
          <div className={`ide-left${compact ? ` is-compact` : ``}`}>
            <div className="ide-rail-strip">
              <span className="ide-lights">
                <i style={{ background: `#ff5f57` }} />
                <i style={{ background: `#febc2e` }} />
                <i style={{ background: `#28c840` }} />
              </span>
            </div>
            <div className="ide-left-body">
              <Rail compact={compact} />
              {compact && <ListNav />}
            </div>
          </div>
          <div className="ide-main">
            <Topbar />
            <div className="ide-panel">
              <div className="ide-main-top">
                <MainView />
              </div>
            </div>
          </div>
        </div>
      </IdeContext.Provider>
    </div>
  )
}

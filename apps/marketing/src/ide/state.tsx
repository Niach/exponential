/* ─── Shared IDE demo state: context, types, helpers ─── */
import { createContext, useContext } from "react"
import type { AgentKind, Change, Commit, FeedRow, FilterTab, RunFile } from "./data"

/* The rail destination the main view shows while no top tab is active.
   Board and Inbox are LIST screens: an issue picked from one opens beside
   it, with the list folded into the left column (EXP-870). */
export type Tool =
  | `issues`
  | `files`
  | `source-control`
  | `inbox`
  /* EXP-706: a rail SCREEN, not a docked tool window. */
  | `reviews`
  /* EXP-825: the Agent page, the composer over the Running/Recent bands. */
  | `agent`
export type IdeView = `board` | `issue` | `files` | `source-control`

/* `issue` = an issue and its run as ONE top tab (EXP-870); `run` = an
   issue-less run's Run-only tab (a batch). `file`/`sc` are legacy kinds. */
export type TabKind = `issue` | `file` | `sc` | `run`
export type Tab = { key: string; kind: TabKind; label: string; ref: string }

/* Which face of a top tab is up (work_header.rs `Face`): the Diff face is
   the run's full-page diff (EXP-877), never a side pane. */
export type Face = `issue` | `run` | `diff`

/* A run's phase: `running` = the agent's turn is going, `waiting` = its turn
   ended on a question it holds for (EXP-674), `review` = idle with its PR
   open, `ended` = stopped (or its PR merged, EXP-498). `idle` = no run. */
export type CodingState = `idle` | `running` | `waiting` | `review` | `ended`
/* A coding run targets ONE issue or a BATCH of issues (EXP-106) — one
   session, one exp/batch-<id8> branch, one combined PR. */
export type CodingTarget =
  | { kind: `issue`; id: string }
  | { kind: `batch`; issueIds: string[] }
export type ScriptPos = { done: number; chars: number }

/* The two runs the demo knows: the one the composer starts (scripted) and
   the Codex run already waiting for review. */
export type RunId = `scripted` | `review`

export type RunView = {
  id: RunId
  agent: AgentKind
  state: CodingState
  /* The issue the run is on; null for a batch. */
  issueId: string | null
  title: string
  /* The top tab it lives in. */
  tabKey: string
  rows: FeedRow[]
  /* Rows fully shown, plus the typing row's character count. */
  pos: ScriptPos
  files: RunFile[]
  /* The model the footer pin names. */
  model: string
}

export const isLive = (state: CodingState): boolean =>
  state === `running` || state === `waiting` || state === `review`

export type IdeApi = {
  interactive: boolean

  tool: Tool
  /* A rail navigation: the main view becomes that screen, no tab active. */
  setTool: (tool: Tool) => void

  tabs: Tab[]
  active: string | null
  selectTab: (key: string) => void
  closeTab: (key: string) => void
  faceOf: (key: string) => Face
  setFace: (key: string, face: Face) => void
  /* `fromList` = picked from a list screen: the list folds in beside it. */
  openIssue: (id: string, fromList?: boolean) => void
  openFile: (path: string) => void
  openSourceControl: () => void

  /* EXP-870: the ListNav is up (the rail is the compact icon column). */
  listNav: boolean
  closeListNav: () => void

  filter: FilterTab
  setFilter: (filter: FilterTab) => void
  collapsedGroups: Set<string>
  toggleGroup: (status: string) => void

  expandedDirs: Set<string>
  toggleDir: (path: string) => void
  selectedFile: string | null
  selectFile: (path: string) => void

  viewedBranch: string
  viewBranch: (name: string) => void
  changes: Change[]
  commits: Commit[]

  inboxRead: Set<string>
  markInboxRead: (id: string) => void
  markAllInboxRead: () => void

  mergedReviews: Set<string>
  goneReviews: Set<string>
  mergeReview: (issueId: string) => void

  coding: CodingState
  codingTarget: CodingTarget | null
  codingScript: FeedRow[]
  scriptPos: ScriptPos
  /* EXP-825: every play button navigates to the Agent page with its issues
     already chipped, and the send starts the run (1 chip = a single run,
     2+ = a batch). */
  composerOpen: boolean
  chips: string[]
  openComposer: (issueIds: string[]) => void
  closeComposer: () => void
  toggleChip: (issueId: string) => void
  submitComposer: () => void

  /* Every run with a tab or a row: live ones auto-own a tab (EXP-870). */
  runs: RunView[]
  runForTab: (key: string) => RunView | null
  openRun: (id: RunId) => void
  stopRun: (id: RunId) => void
  /* Keeps the scripted run's older name working. */
  stopCoding: () => void
  answerQuestion: (answer: string) => void
  steer: (id: RunId, text: string) => void

  /* The strip's agent clusters a chevron has folded to their mark. */
  foldedAgents: Set<string>
  toggleAgentFold: (agent: AgentKind) => void
}

export const IdeContext = createContext<IdeApi | null>(null)

export function useIde(): IdeApi {
  const api = useContext(IdeContext)
  if (!api) throw new Error(`useIde must be used inside <IdeDemo>`)
  return api
}

export const prefersReducedMotion = (): boolean =>
  typeof window !== `undefined` &&
  typeof window.matchMedia === `function` &&
  window.matchMedia(`(prefers-reduced-motion: reduce)`).matches

export const toggledSet = (set: Set<string>, value: string): Set<string> => {
  const next = new Set(set)
  if (next.has(value)) {
    next.delete(value)
  } else {
    next.add(value)
  }
  return next
}

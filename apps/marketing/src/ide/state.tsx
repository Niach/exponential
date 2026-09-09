/* ─── Shared IDE demo state: context, types, helpers ─── */
import { createContext, useContext } from "react"
import type { Change, Commit, FeedRow, FilterTab } from "./data"

export type Tool =
  | `issues`
  | `files`
  | `source-control`
  | `inbox`
  /* EXP-706: a rail SCREEN, not a docked tool window. */
  | `reviews`
export type IdeView = `board` | `issue` | `files` | `source-control`

export type TabKind = `issue` | `file` | `sc`
export type Tab = { key: string; kind: TabKind; label: string; ref: string }

/* The run's phase, as the session header reads it: `running` = the agent is
   working, `waiting` = its turn ended on a question and it is holding for
   your reply (a person-started run has no idle bound, EXP-674), `ended` =
   killed from the header. */
export type CodingState = `idle` | `running` | `waiting` | `ended`
/* A coding run targets ONE issue or a BATCH of issues (EXP-106) — one
   session, one exp/batch-<id8> branch, one combined PR. */
export type CodingTarget =
  | { kind: `issue`; id: string }
  | { kind: `batch`; issueIds: string[] }
export type ScriptPos = { done: number; chars: number }

export type IdeApi = {
  interactive: boolean

  tool: Tool
  setTool: (tool: Tool) => void

  tabs: Tab[]
  active: string | null
  selectTab: (key: string) => void
  closeTab: (key: string) => void
  openIssue: (id: string) => void
  openFile: (path: string) => void
  openSourceControl: () => void

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
  /* Start-coding dialog beat: request opens the dialog (pre-seeding its
     picker), confirm launches whatever the picker settled on. */
  pendingCoding: CodingTarget | null
  requestCoding: (target: CodingTarget) => void
  cancelStartCoding: () => void
  confirmStartCoding: (target: CodingTarget) => void
  stopCoding: () => void
  scriptPos: ScriptPos

  /* EXP-791: a run is a full-width center SCREEN, opened from the rail's
     Sessions section or the issue's Watch — never a dock. `sessionOpen`
     is "the center is showing the run", not "a run exists". */
  sessionOpen: boolean
  openSession: () => void
  closeSession: () => void
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

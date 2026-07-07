/* ─── Shared IDE demo state: context, types, helpers ─── */
import { createContext, useContext } from "react"
import type { Change, Commit, FilterTab, ScriptLine } from "./data"

export type Tool =
  | `issues`
  | `files`
  | `source-control`
  | `inbox`
  | `my-issues`
  | `reviews`
export type IdeView = `board` | `issue` | `files` | `source-control`

export type TabKind = `issue` | `file` | `sc`
export type Tab = { key: string; kind: TabKind; label: string; ref: string }

export type CodingState = `idle` | `running` | `ended`
export type DockTab = `shell` | `claude`
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
  staged: Set<string>
  toggleStaged: (path: string) => void
  commits: Commit[]
  commitAll: (message: string, push: boolean) => void
  ahead: number

  inboxRead: Set<string>
  markInboxRead: (id: string) => void
  markAllInboxRead: () => void

  mergedReviews: Set<string>
  goneReviews: Set<string>
  mergeReview: (issueId: string) => void

  coding: CodingState
  codingIssueId: string | null
  codingScript: ScriptLine[]
  codedIssues: Set<string>
  startCoding: (issueId: string) => void
  stopCoding: () => void
  scriptPos: ScriptPos

  dockOpen: boolean
  setDockOpen: (open: boolean) => void
  dockTab: DockTab
  setDockTab: (tab: DockTab) => void
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

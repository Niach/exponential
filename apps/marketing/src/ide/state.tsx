/* ─── Shared IDE demo state: context, types, helpers ─── */
import { createContext, useContext } from "react"
import type { Change, Commit, FeedRow, Question } from "./data"

/* The rail destination the main view shows while no top tab is active
   (navigation.rs `Screen`): Home ("Agent"), the Inbox, the Chats page
   (`Screen::Threads`, the rail's Chats entry), a
   board's list, and the footer Computer menu's Files / Source Control. */
export type Tool = `agent` | `inbox` | `threads` | `board` | `files` | `source-control`

export type IdeView = `board` | `issue` | `files` | `source-control`

/* A top tab (EXP-923: a flat strip of what you opened): an issue's detail
   or a thread. `origin` = the Inbox lends its list as a second sidebar to
   the detail it opened (EXP-1192); nothing else does. */
export type Tab = { key: string; kind: `issue` | `thread`; ref: string; origin?: `inbox` }

/* session-card.json states the demo's thread moves through. */
export type ThreadState = `working` | `needs_you` | `waiting` | `ended`

export type ThreadView = {
  issueId: string
  state: ThreadState
  /* The rows on screen, in order. */
  rows: FeedRow[]
  /* The prose row still typing out (its text so far), while working. */
  typing: string | null
  question: Question | null
}

export type ScriptPos = { done: number; chars: number }

export type IdeApi = {
  interactive: boolean

  tool: Tool
  /* A rail navigation: the main view becomes that screen, no tab active. */
  setTool: (tool: Tool) => void

  tabs: Tab[]
  active: string | null
  selectTab: (key: string) => void
  closeTab: (key: string) => void
  openIssue: (id: string, origin?: `inbox`) => void

  collapsedGroups: Set<string>
  toggleGroup: (status: string) => void

  expandedDirs: Set<string>
  toggleDir: (path: string) => void
  selectedFile: string | null
  selectFile: (path: string) => void
  changes: Change[]
  commits: Commit[]

  inboxRead: Set<string>
  markInboxRead: (id: string) => void
  markAllInboxRead: () => void

  /* EXP-1294: Start opens Home with the issue as the composer's subject. */
  subject: string[]
  start: (issueId: string) => void
  removeSubject: (issueId: string) => void
  /* The send: ONE thread on the device, opened as a tab. */
  send: (text: string) => void

  thread: ThreadView | null
  threadKey: string
  openThread: () => void
  /* chats.json STOP: the header's Stop pill = `sessions.end` on the root
     (the whole chat ends; a later message resumes it). */
  stopThread: () => void
  /* The composer's round Stop while a turn runs = `sessions.stop`, the
     TURN interrupt (the session stays and waits). */
  stopTurn: () => void
  answer: (text: string) => void
  steer: (text: string) => void
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

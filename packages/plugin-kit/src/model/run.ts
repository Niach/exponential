// The session ("run") wire row the list view-models read
// (`exponential_sessions {op: "list" | "get"}`).

export interface RunResult {
  topic: string
  text?: string
  files?: string[]
  label?: string
  url?: string
}

export interface RunDetail {
  id: string
  createdAt?: string
  /** The run's page in the app (sessions_get only). */
  url?: string
  issueId?: string | null
  issueIdentifier?: string | null
  issueTitle?: string | null
  /** The session's derived title (session-title.json). */
  title?: string | null
  agent?: string | null
  status: string
  branch?: string | null
  prUrl?: string | null
  prNumber?: number | null
  prState?: string | null
  agentBusy?: boolean | null
  agentCaption?: string | null
  needsInput?: boolean | null
  results?: RunResult[]
}

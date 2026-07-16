// Pure state model for the getting-started checklist (EXP-141) — no React.
// The hook (use-getting-started-progress) gathers the signals; this derives
// what each entry looks like. Kept pure so the order/lock/done rules are unit
// tested without rendering.

export type EntryKey =
  | `github`
  | `project`
  | `coding`
  | `feedback-board`
  | `widget`
  | `mcp`

export type EntryState = `done` | `available` | `locked`

export interface GettingStartedSignals {
  /** integrations.github.status → installed (workspace has a linked App install). */
  githubInstalled: boolean
  /** Any live (non-archived, non-trashed) project. */
  hasProject: boolean
  /** Any live project with a repository attached. */
  hasRepoProject: boolean
  /** Any coding_sessions row in the workspace (running or ended). */
  hasCodingSession: boolean
  /** Any live public project (feedback board). */
  hasPublicProject: boolean
  /** Any live public project with the helpdesk enabled. */
  hasHelpdeskProject: boolean
  /** widgets.list non-empty (owner-only signal — false for members). */
  hasWidget: boolean
  /** An MCP OAuth grant exists OR the user holds a personal API key. */
  mcpConnected: boolean
}

export interface GettingStartedEntry {
  key: EntryKey
  state: EntryState
  /** For locked entries: the step whose completion unlocks this one. */
  lockedBy?: EntryKey
}

// A public feedback board's most likely next steps are the feedback track, so
// it leads there; everywhere else the coding loop comes first.
export function gettingStartedEntryOrder(projectIsPublic?: boolean): EntryKey[] {
  return projectIsPublic
    ? [`feedback-board`, `widget`, `mcp`, `github`, `project`, `coding`]
    : [`github`, `project`, `coding`, `feedback-board`, `widget`, `mcp`]
}

// Derive every entry's state. Completion always wins over locking (a signal
// that exists proves the prereq was satisfiable), and the widget entry is for
// owners only — widgets.list is an owner-only procedure, so members neither
// see the entry nor count it in the total.
export function deriveEntryStates(
  signals: GettingStartedSignals,
  { canManageWidgets }: { canManageWidgets: boolean }
): { entries: GettingStartedEntry[]; done: number; total: number } {
  const entries: GettingStartedEntry[] = []

  entries.push({
    key: `github`,
    state: signals.githubInstalled ? `done` : `available`,
  })

  entries.push({
    key: `project`,
    state: signals.hasProject ? `done` : `available`,
  })

  // Coding needs a repo-backed project; when locked, point at whichever of
  // its two feeder steps is still missing (GitHub first — without it the
  // project step can't attach a repo either).
  if (signals.hasCodingSession) {
    entries.push({ key: `coding`, state: `done` })
  } else if (signals.hasRepoProject) {
    entries.push({ key: `coding`, state: `available` })
  } else {
    entries.push({
      key: `coding`,
      state: `locked`,
      lockedBy: signals.githubInstalled ? `project` : `github`,
    })
  }

  entries.push({
    key: `feedback-board`,
    state: signals.hasPublicProject ? `done` : `available`,
  })

  if (canManageWidgets) {
    if (signals.hasWidget) {
      entries.push({ key: `widget`, state: `done` })
    } else if (signals.hasProject) {
      entries.push({ key: `widget`, state: `available` })
    } else {
      entries.push({ key: `widget`, state: `locked`, lockedBy: `project` })
    }
  }

  entries.push({
    key: `mcp`,
    state: signals.mcpConnected ? `done` : `available`,
  })

  return {
    entries,
    done: entries.filter((entry) => entry.state === `done`).length,
    total: entries.length,
  }
}

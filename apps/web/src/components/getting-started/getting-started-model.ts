// Pure state model for the getting-started checklist (EXP-141) — no React.
// The hook (use-getting-started-progress) gathers the signals; this derives
// what each entry looks like. Kept pure so the order/lock/done rules are unit
// tested without rendering.

export type EntryKey =
  | `desktop`
  | `github`
  | `invite`
  | `board`
  | `coding`
  | `action`
  | `server`
  | `widget`
  | `helpdesk`
  | `mcp`

export type EntryState = `done` | `available` | `locked`

export interface GettingStartedSignals {
  /** The synced devices shape (own rows) has a desktop-kind device — the IDE
   * registered. */
  hasDesktopDevice: boolean
  /** The synced devices shape (own rows) has a server-kind device — the CLI
   * daemon registered. */
  hasServerDevice: boolean
  /** integrations.github.status → installed (team has a linked App install). */
  githubInstalled: boolean
  /** members.length > 1 or any invite row — live Electric. */
  hasInvitedTeam: boolean
  /** Any live (non-trashed) board. */
  hasBoard: boolean
  /** Any live board with a repository attached. */
  hasRepoBoard: boolean
  /** Any coding_sessions row in the team (running or ended). */
  hasCodingSession: boolean
  /** Any synced `actions` row in the team (EXP-548) — the builtins are not rows. */
  hasAction: boolean
  /** The team-level helpdesk switch (teams.helpdeskEnabled). */
  helpdeskEnabled: boolean
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

// Derive every entry's state, in the single static display order
// desktop → github → invite → board → coding → action → server → widget →
// helpdesk → mcp. Completion always wins over locking (a signal that exists
// proves the prereq was satisfiable). The invite entry is for members who can
// mint invites (canManageMembers — teamInvites.create is owner/admin-only),
// and the action, widget and helpdesk entries are for owners only — action
// writes, widgets.list and the helpdesk switch are owner-only surfaces; the
// others neither see those entries nor count them in the total.
//
// The desktop IDE mirrors this function byte-for-byte in
// `crates/ui/src/getting_started.rs` (`derive_entries`, EXP-548: same
// entries, order, gating and lock chain) — change both together.
export function deriveEntryStates(
  signals: GettingStartedSignals,
  {
    canManageWidgets,
    isOwner,
    canManageMembers,
  }: { canManageWidgets: boolean; isOwner: boolean; canManageMembers: boolean }
): { entries: GettingStartedEntry[]; done: number; total: number } {
  const entries: GettingStartedEntry[] = []

  entries.push({
    key: `desktop`,
    state: signals.hasDesktopDevice ? `done` : `available`,
  })

  entries.push({
    key: `github`,
    state: signals.githubInstalled ? `done` : `available`,
  })

  if (canManageMembers) {
    entries.push({
      key: `invite`,
      state: signals.hasInvitedTeam ? `done` : `available`,
    })
  }

  entries.push({
    key: `board`,
    state: signals.hasBoard ? `done` : `available`,
  })

  // Coding needs a repo-backed board and a machine to run on; when locked,
  // point at whichever feeder step is still missing, in display order
  // (desktop first — without any machine nothing can run; then GitHub —
  // without it the board step can't attach a repo either).
  const hasDevice = signals.hasDesktopDevice || signals.hasServerDevice
  if (signals.hasCodingSession) {
    entries.push({ key: `coding`, state: `done` })
  } else if (signals.hasRepoBoard && hasDevice) {
    entries.push({ key: `coding`, state: `available` })
  } else {
    entries.push({
      key: `coding`,
      state: `locked`,
      lockedBy: !hasDevice
        ? `desktop`
        : signals.githubInstalled
          ? `board`
          : `github`,
    })
  }

  // EXP-548: actions are authored by the builtin creator run, which — like
  // any coding session — needs a machine; the desktop step is the feeder.
  if (isOwner) {
    if (signals.hasAction) {
      entries.push({ key: `action`, state: `done` })
    } else if (hasDevice) {
      entries.push({ key: `action`, state: `available` })
    } else {
      entries.push({ key: `action`, state: `locked`, lockedBy: `desktop` })
    }
  }

  entries.push({
    key: `server`,
    state: signals.hasServerDevice ? `done` : `available`,
  })

  if (canManageWidgets) {
    if (signals.hasWidget) {
      entries.push({ key: `widget`, state: `done` })
    } else if (signals.hasBoard) {
      entries.push({ key: `widget`, state: `available` })
    } else {
      entries.push({ key: `widget`, state: `locked`, lockedBy: `board` })
    }
  }

  if (isOwner) {
    entries.push({
      key: `helpdesk`,
      state: signals.helpdeskEnabled ? `done` : `available`,
    })
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

// EXP-548: the checklist has no dismissal — it simply disappears (sidebar
// entry, empty-board block, IDE rail entry alike) once every visible entry is
// done. Loading counts as incomplete-unknown and is handled by the callers.
export function isGettingStartedComplete({
  done,
  total,
}: {
  done: number
  total: number
}): boolean {
  return total > 0 && done === total
}

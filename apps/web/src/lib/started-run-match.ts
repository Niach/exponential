// EXP-536: a remote start is only a COMMAND — the desktop inserts the
// `coding_sessions` row a moment later — so every surface that sends one waits
// for that row to sync in and then opens the live session (the agent dock on
// desktop web, its full-viewport takeover on mobile web). These are the
// matching rules, mirrored on Android (`domain/StartedRunMatch.kt`) and iOS
// (`Domain/StartedRunMatch.swift`); a mismatch there is a start that never
// navigates.

/** What a post-send session watch is looking for among the synced rows. */
export type StartedRunKey =
  /** An action run — keyed by the display-name SNAPSHOT, never the action id:
   * the builtin "Create action" row carries `action_id` NULL. */
  | { kind: `action`; actionName: string }
  /** A single-issue session — the desktop stamps `issue_id`. */
  | { kind: `issue`; issueId: string }
  /** A batch run: `issue_id` NULL (it spans issues) and no action name.
   * Nothing narrower exists — a batch row is unmatchable by issue server-side
   * too — so the userId + startedAt cut carries the identity. */
  | { kind: `batch` }

/** Clock-skew slack on the desktop-written `started_at`. */
export const STARTED_RUN_SKEW_MS = 120_000

/** How long a start may take to surface its row before the watch gives up.
 * The "waiting for the desktop" caption shares it. */
export const STARTED_RUN_DEADLINE_MS = 180_000

/** The key for a Start-coding send: 1 id is a plain session, 2+ a batch. */
export function startedRunKeyForIssues(issueIds: string[]): StartedRunKey | null {
  if (issueIds.length === 0) return null
  if (issueIds.length >= 2) return { kind: `batch` }
  return { kind: `issue`, issueId: issueIds[0] }
}

/** The `coding_sessions` columns the rules read — structurally satisfied by a
 * synced `CodingSession` row. `startedAt` also accepts its wire string so the
 * rules stay usable before deserialization. */
export interface StartedRunCandidate {
  issueId: string | null
  actionName: string | null
  userId: string
  startedAt: Date | string
}

export function matchesStartedRun(
  session: StartedRunCandidate,
  key: StartedRunKey,
  userId: string,
  cutoffMs: number
): boolean {
  if (session.userId !== userId) return false
  const startedAt = new Date(session.startedAt).getTime()
  // Fail CLOSED on an unparseable stamp: not navigating beats navigating into
  // somebody else's run.
  if (!Number.isFinite(startedAt) || startedAt < cutoffMs) return false
  switch (key.kind) {
    case `action`:
      return session.actionName === key.actionName
    case `issue`:
      return session.issueId === key.issueId && session.actionName == null
    case `batch`:
      return session.issueId == null && session.actionName == null
  }
}

/** The first row that IS this start, or undefined while none has synced. */
export function findStartedRun<T extends StartedRunCandidate & { id: string }>(
  sessions: T[],
  key: StartedRunKey,
  userId: string,
  cutoffMs: number
): T | undefined {
  return sessions.find((session) => matchesStartedRun(session, key, userId, cutoffMs))
}

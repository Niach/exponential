// Status-derived column management, shared by issues.update/bulkUpdate,
// statuses.delete's reassignment writer, and the PR lifecycle automation
// (EXP-319 — pr-sync.ts must not import from lib/trpc/issues.ts, which
// itself imports pr-sync). Mutates setValues in place. Only applies the
// duplicate-clear rule when the caller hasn't already decided duplicate
// linkage (setValues.duplicateOfId === undefined) — update's duplicateOfId
// input block runs BEFORE this. setValues.status is always the ANCHOR enum
// (resolveStatusWrite), so the terminal rules below cover custom statuses
// too: a custom completed status anchors `done` and stamps completedAt;
// moving between two same-category customs keeps the anchor stable,
// deliberately preserving completedAt.
export function applyStatusDerivations(
  setValues: Record<string, unknown>,
  current: { status: string; duplicateOfId: string | null }
): void {
  if (
    setValues.status !== undefined &&
    setValues.status !== `duplicate` &&
    current.duplicateOfId !== null &&
    setValues.duplicateOfId === undefined
  ) {
    // Moving off 'duplicate' via a plain status change also unmarks.
    setValues.duplicateOfId = null
  }

  const nextStatus = setValues.status as string | undefined
  if (
    nextStatus === `done` ||
    nextStatus === `cancelled` ||
    nextStatus === `duplicate`
  ) {
    // Only an actual transition stamps completedAt — a redundant write of the
    // same terminal status must not clobber the original completion time.
    if (nextStatus !== current.status) {
      setValues.completedAt = new Date()
    }
  } else if (nextStatus) {
    setValues.completedAt = null
  }
}

import { useEffect } from "react"

// EXP-1154: the issue route's hand-offs to the run that owns a LIVE diff.
// Pure pieces so the route's wiring stays testable without mounting it.

export type RunFaceView = `diff` | `results`

/** The session route's search for a hand-off: the list origin, the face and
 *  (on `diff`) the file in focus, which the run's Changes face seeds from. */
export function runFaceSearch(input: {
  from?: string
  view?: RunFaceView
  file?: string
}): { from?: string; view?: RunFaceView; file?: string } {
  return {
    ...(input.from ? { from: input.from } : {}),
    ...(input.view ? { view: input.view } : {}),
    ...(input.view === `diff` && input.file ? { file: input.file } : {}),
  }
}

/** The run a `?view=diff` on the ISSUE must hand off to: the run owns its
 *  live diff, so the issue never draws the PR files under the live diff's
 *  counts. Null = the issue keeps its own PR-files face. */
export function liveDiffHandoff(input: {
  showChanges: boolean
  liveDiff: boolean
  runId: string | null | undefined
}): string | null {
  return input.showChanges && input.liveDiff && input.runId
    ? input.runId
    : null
}

/** Redirects a deep-linked issue `?view=diff` to the run's Changes face
 *  once the run turns out to own a live diff (`liveDiffHandoff`). */
export function useLiveDiffHandoff(
  input: Parameters<typeof liveDiffHandoff>[0] & { file?: string },
  goRun: (runId: string, view: `diff`, file?: string) => void
) {
  const runId = liveDiffHandoff(input)
  const { file } = input
  useEffect(() => {
    if (runId) goRun(runId, `diff`, file)
  }, [runId, file, goRun])
  return runId !== null
}

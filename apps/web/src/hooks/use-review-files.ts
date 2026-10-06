import { useCallback, useEffect, useState } from "react"
import { fromPullFile, type DiffFile } from "@exp/domain-contract/diff"
import type { CodingSession, Issue } from "@/db/schema"
import { trpc } from "@/lib/trpc-client"

// EXP-706: the review's file list is fetched by the caller, not by the diff
// component — the header prints the file count and the +/- totals, so it
// needs the files before they are rendered. Two tiers behind one state
// machine: the PR diff (`issues.prFiles`) and, for a pushed branch with no
// PR yet, `repositories.branchDiff` (which answers null when nothing was
// ever pushed). EXP-893 lifts it out of the review route: the phone's
// Changes face draws the same files for an issue whose PR is open and whose
// run published no live diff (`enabled` keeps the fetch off until then).
//
// EXP-895: GitHub's `PullFile` stops at the transport boundary — every caller
// gets the shared `DiffFile` model (`fromPullFile`, which parses the patch
// into hunks and keeps GitHub's counts only when there are none).

export type ReviewFilesState =
  | { kind: `loading` }
  | { kind: `files`; files: DiffFile[] }
  | { kind: `none` } // no PR and the branch was never pushed (GitHub 404)
  | { kind: `error`; message: string }

export function useReviewFiles(
  issue: Pick<Issue, `id` | `prNumber`> | null,
  options: { enabled?: boolean } = {}
) {
  const issueId = issue?.id ?? null
  const hasPr = issue?.prNumber != null
  const fetchFiles = useCallback(
    (id: string): Promise<DiffFile[] | null> =>
      hasPr
        ? trpc.issues.prFiles
            .query({ issueId: id })
            .then((res) => res.files.map(fromPullFile))
        : trpc.repositories.branchDiff
            .query({ issueId: id })
            .then((res) => res?.files.map(fromPullFile) ?? null),
    [hasPr]
  )
  return useFilesState(issueId, options.enabled ?? true, fetchFiles)
}

/** EXP-1194: the PR files of a RUN with no issue to key them on (a chat or
 *  action run's chore PR, `codingSessions.prFiles`). Same states; no PR = no
 *  branch-diff tier (a run's PR is stamped by `pr_open`). */
export function useSessionPrFiles(
  session: Pick<CodingSession, `id` | `prUrl`> | null,
  options: { enabled?: boolean } = {}
) {
  const sessionId = session?.prUrl ? session.id : null
  const fetchFiles = useCallback(
    (id: string): Promise<DiffFile[] | null> =>
      trpc.codingSessions.prFiles
        .query({ sessionId: id })
        .then((res) => (res.prNumber == null ? null : res.files.map(fromPullFile))),
    []
  )
  return useFilesState(sessionId, options.enabled ?? true, fetchFiles)
}

function useFilesState(
  key: string | null,
  enabled: boolean,
  fetchFiles: (key: string) => Promise<DiffFile[] | null>
) {
  const [state, setState] = useState<ReviewFilesState>({ kind: `loading` })
  // EXP-1154: the route is reused across issues, so another issue's files
  // must never outlive a param change, even while the fetch is disabled
  // (issue B's Guide counting off A's files). Reset during render, before
  // anyone reads the stale state.
  const [stateFor, setStateFor] = useState(key)
  if (stateFor !== key) {
    setStateFor(key)
    setState({ kind: `loading` })
  }

  const load = useCallback(() => {
    if (!key || !enabled) return
    let cancelled = false
    setState({ kind: `loading` })
    fetchFiles(key)
      .then((files) => {
        if (cancelled) return
        setState(files ? { kind: `files`, files } : { kind: `none` })
      })
      .catch((err: unknown) => {
        if (cancelled) return
        setState({
          kind: `error`,
          message: err instanceof Error ? err.message : `Failed to load changes`,
        })
      })
    return () => {
      cancelled = true
    }
  }, [key, enabled, fetchFiles])

  useEffect(() => load(), [load])

  return { state, reload: load }
}

import { useCallback, useEffect, useState } from "react"
import type { Issue } from "@/db/schema"
import type { PullFile } from "@/components/diff-view"
import { trpc } from "@/lib/trpc-client"

// EXP-706: the review's file list is fetched by the caller, not by the diff
// component — the header prints the file count and the +/- totals, so it
// needs the files before they are rendered. Two tiers behind one state
// machine: the PR diff (`issues.prFiles`) and, for a pushed branch with no
// PR yet, `repositories.branchDiff` (which answers null when nothing was
// ever pushed). EXP-893 lifts it out of the review route: the phone's
// Changes face draws the same files for an issue whose PR is open and whose
// run published no live diff (`enabled` keeps the fetch off until then).

export type ReviewFilesState =
  | { kind: `loading` }
  | { kind: `files`; files: PullFile[] }
  | { kind: `none` } // no PR and the branch was never pushed (GitHub 404)
  | { kind: `error`; message: string }

export function useReviewFiles(
  issue: Pick<Issue, `id` | `prNumber`> | null,
  options: { enabled?: boolean } = {}
) {
  const enabled = options.enabled ?? true
  const issueId = issue?.id ?? null
  const hasPr = issue?.prNumber != null
  const [state, setState] = useState<ReviewFilesState>({ kind: `loading` })

  const load = useCallback(() => {
    if (!issueId || !enabled) return
    let cancelled = false
    setState({ kind: `loading` })
    const request: Promise<PullFile[] | null> = hasPr
      ? trpc.issues.prFiles.query({ issueId }).then((res) => res.files)
      : trpc.repositories.branchDiff
          .query({ issueId })
          .then((res) => res?.files ?? null)
    request
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
  }, [issueId, hasPr, enabled])

  useEffect(() => load(), [load])

  return { state, reload: load }
}

import { useCallback, useEffect, useState } from "react"
import { fromPullFile, type DiffFile } from "@exp/domain-contract/diff"
import { trpc } from "@/lib/trpc-client"

// EXP-706 / EXP-1293: a pull request's changed files, fetched by the caller
// (the Guide's Changes rows count them before the diff renders). ONE hook
// keyed by the `pull_requests` row: `pullRequests.files`. GitHub's file
// shape stops at the transport boundary (`fromPullFile` parses the patch
// into hunks, EXP-895).

export type ReviewFilesState =
  | { kind: `loading` }
  | { kind: `files`; files: DiffFile[] }
  | { kind: `none` }
  | { kind: `error`; message: string }

export function usePullRequestFiles(
  pullRequestId: string | null | undefined
): { state: ReviewFilesState; reload: () => void } {
  const key = pullRequestId ?? null
  const [state, setState] = useState<ReviewFilesState>({ kind: `loading` })
  // EXP-1154: another PR's files never outlive a key change. Reset during
  // render, before anyone reads them.
  const [stateFor, setStateFor] = useState(key)
  if (stateFor !== key) {
    setStateFor(key)
    setState({ kind: `loading` })
  }

  const [attempt, setAttempt] = useState(0)
  useEffect(() => {
    if (!key) return
    let cancelled = false
    setState({ kind: `loading` })
    trpc.pullRequests.files
      .query({ pullRequestId: key })
      .then((res) => {
        if (cancelled) return
        const files = res.files.map((file) =>
          fromPullFile({
            filename: file.path,
            status: `modified`,
            additions: file.additions,
            deletions: file.deletions,
            patch: file.patch,
          })
        )
        setState(files.length > 0 ? { kind: `files`, files } : { kind: `none` })
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
  }, [key, attempt])

  const reload = useCallback(() => setAttempt((value) => value + 1), [])
  return { state, reload }
}

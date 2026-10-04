import { useCallback, useEffect, useState } from "react"
import type { Issue } from "@/db/schema"
import { trpc } from "@/lib/trpc-client"

// EXP-1154: the pull request's title + body as GitHub holds them
// (`issues.prDescription`, never a synced column). The Results face shows it
// as ONE unnumbered group when the issue has an open PR and no run published
// a report (EXP-1139's review-page card went with that page; the fetch moved
// here). `enabled` keeps the request off until that face is on screen.

export type PrDescriptionState =
  | { kind: `idle` }
  | { kind: `loading` }
  | { kind: `ready`; title: string; body: string; state: string | null }
  | { kind: `error`; message: string }

export function usePrDescription(
  issue: Pick<Issue, `id` | `prNumber`> | null,
  options: { enabled?: boolean } = {}
) {
  const enabled = options.enabled ?? true
  const issueId = issue?.id ?? null
  const prNumber = issue?.prNumber ?? null
  const [state, setState] = useState<PrDescriptionState>({ kind: `idle` })

  const load = useCallback(() => {
    if (!issueId || prNumber == null || !enabled) return
    let cancelled = false
    setState({ kind: `loading` })
    trpc.issues.prDescription
      .query({ issueId })
      .then((res) => {
        if (cancelled) return
        if (res.title === null) {
          setState({ kind: `error`, message: `No pull request is linked.` })
          return
        }
        setState({
          kind: `ready`,
          title: res.title,
          body: res.body ?? ``,
          state: res.state,
        })
      })
      .catch((err: unknown) => {
        if (cancelled) return
        setState({
          kind: `error`,
          message:
            err instanceof Error
              ? err.message
              : `Failed to load the pull request`,
        })
      })
    return () => {
      cancelled = true
    }
  }, [issueId, prNumber, enabled])

  useEffect(() => load(), [load])

  return { state, reload: load }
}

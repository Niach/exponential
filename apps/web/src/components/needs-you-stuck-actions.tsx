import { useCallback, useEffect, useRef, useState, type ReactNode } from "react"
import { NeedsYouStuckButtons, type StuckAction } from "@exp/ui"
import {
  usePullRequestApproval,
  usePullRequestClose,
  usePullRequestTrainRows,
} from "@/components/session-merge-button"
import type { NeedsYouListRow } from "@/hooks/use-needs-you"
import type { PullRequestRow } from "@/lib/session-tree"

// #EXP-1710 (merge-train.json `stuckRow`): a stuck PR's Needs you actions as
// THE Button sm row (`NeedsYouStuckButtons`, the styleguide specimen's own):
// Retry (= Resume auto-merge), Merge now (owners: THE shared alert card
// `merge-now-pr`, through the `…` menu's own `usePullRequestTrainRows`, so a
// PR in an open stack asks `titleStack` exactly as the menu does), Close PR
// (close-pr.json).

export function NeedsYouStuckActions({ row }: { row: NeedsYouListRow }) {
  const stuck = row.stuck
  if (!stuck) return null
  return <StuckActions stuck={stuck} />
}

function StuckActions({ stuck }: { stuck: NonNullable<NeedsYouListRow[`stuck`]> }) {
  const pr = stuck.pullRequest
  const train = usePullRequestTrainRows(pr)
  const close = usePullRequestClose(pr, stuck.actions.includes(`close-pr`))
  const run = { retry: train.resume, "merge-now": train.mergeNow, "close-pr": close.entry } as const
  const actions = stuck.actions.filter((id): id is StuckAction => run[id]?.kind === `item`)
  return (
    <NeedsYouStuckButtons
      actions={actions}
      onAction={(id) => {
        const entry = run[id]
        if (entry?.kind === `item`) entry.onSelect?.()
      }}
    >
      {train.dialog}
      {close.dialog}
    </NeedsYouStuckButtons>
  )
}

/** #EXP-1710 (needs-you.json): an `approval` ready PR row's press is Needs
 *  approval, never the chat: THE pill's confirm (`usePullRequestApproval`,
 *  the plain or the stack confirm), Merge queues it, Cancel leaves the row.
 *  `open` wraps the list's own press: any other row opens as before. Render
 *  `dialog` once. */
export function useNeedsYouPress(
  open: (row: NeedsYouListRow) => void
): { press: (row: NeedsYouListRow) => void; dialog: ReactNode } {
  const [pending, setPending] = useState<PullRequestRow | null>(null)
  const press = useCallback(
    (row: NeedsYouListRow) => {
      if (row.approve && !row.stuck) setPending(row.approve)
      else open(row)
    },
    [open]
  )
  const dialog = pending ? (
    <ApprovalConfirm key={pending.id} pr={pending} onSettled={() => setPending(null)} />
  ) : null
  return { press, dialog }
}

function ApprovalConfirm({ pr, onSettled }: { pr: PullRequestRow; onSettled: () => void }) {
  const approval = usePullRequestApproval(pr, onSettled)
  const asked = useRef(false)
  const { ready, request } = approval
  // The stack read decides which confirm asks: wait for it, then ask once.
  useEffect(() => {
    if (!ready || asked.current) return
    asked.current = true
    request()
  }, [ready, request])
  return <>{approval.dialog}</>
}

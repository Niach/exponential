import { MERGE_TRAIN_COPY, MERGE_TRAIN_TEST_IDS } from "@exp/domain-contract/merge-train"
import { Button } from "@exp/ui"
import { usePullRequestClose, usePullRequestTrainRows } from "@/components/session-merge-button"
import type { NeedsYouListRow } from "@/hooks/use-needs-you"

// #EXP-1710 (merge-train.json `stuckRow`): a stuck PR's Needs you actions as
// THE Button sm row: Retry (= Resume auto-merge), Merge now (owners: THE
// shared alert card `merge-now-pr`), Close PR (close-pr.json).

export function NeedsYouStuckActions({ row }: { row: NeedsYouListRow }) {
  const stuck = row.stuck
  if (!stuck) return null
  return <StuckActions stuck={stuck} />
}

function StuckActions({ stuck }: { stuck: NonNullable<NeedsYouListRow[`stuck`]> }) {
  const pr = stuck.pullRequest
  const train = usePullRequestTrainRows(pr)
  const close = usePullRequestClose(pr, stuck.actions.includes(`close-pr`))
  const copy = MERGE_TRAIN_COPY.needsYou
  const labels = { retry: copy.retry, "merge-now": copy.mergeNow, "close-pr": copy.close } as const
  const run = { retry: train.resume, "merge-now": train.mergeNow, "close-pr": close.entry } as const
  return (
    <div
      className="mt-2 flex flex-wrap items-center gap-2"
      data-testid={MERGE_TRAIN_TEST_IDS.stuckRow}
      onClick={(event) => event.stopPropagation()}
    >
      {stuck.actions.map((id) => {
        const entry = run[id]
        if (!entry || entry.kind !== `item`) return null
        return (
          <Button
            key={id}
            type="button"
            size="sm"
            variant="outline"
            data-action={id}
            onClick={() => entry.onSelect?.()}
          >
            {labels[id]}
          </Button>
        )
      })}
      {train.dialog}
      {close.dialog}
    </div>
  )
}

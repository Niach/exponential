import { useState } from "react"
import { Button, conceptIcon, Menu } from "@exp/ui"
import {
  PullRequestStatusPill,
  usePullRequestClose,
  usePullRequestTrainRows,
} from "@/components/session-merge-button"
import { usePullRequestGuideEntries } from "@/hooks/use-pull-request-checks"
import {
  guidePullRequest,
  THREAD_TEST_IDS,
  type PullRequestRow,
} from "@/lib/session-tree"

// EXP-1293 Threads v2 / EXP-1306: the thread's ONE pull request,
// `guidePullRequest` (session-thread.json, ×4). S-3: the
// Guide TAB is gone (its body, Changes rows and StackRail with it); what
// stays is the header's Merge pill on every form factor (phones included)
// and the `…` (#EXP-1710: merge-train.json `mergeMenuEntries`). EXP-1307:
// Close PR (EXP-1338: no GitHub entry).

const MoreIcon = conceptIcon(`ui-more`)

/** The Guide's PR (`guidePullRequest` over the root's synced rows). */
export function threadGuidePullRequest(
  pullRequests: readonly PullRequestRow[]
): PullRequestRow | null {
  const byId = new Map(pullRequests.map((pr) => [pr.id, pr]))
  const picked = guidePullRequest(
    pullRequests.map((pr) => ({
      id: pr.id,
      number: pr.number,
      state: pr.state,
      createdAt: new Date(pr.createdAt).toISOString(),
    }))
  )
  return picked ? (byId.get(picked.id) ?? null) : null
}

/** The thread header's `…` on the Guide (#EXP-1710 `mergeMenuEntries`, ×4):
 *  the Checks submenu whenever the PR has check rows (any viewer), Merge now
 *  (owners), Hold | Resume auto-merge and Close PR (members; EXP-1338: no
 *  GitHub entry, only the agent opens PRs). */
export function ThreadGuideMenu({
  pr,
  canClose,
  isOwner = false,
  className,
}: {
  pr: PullRequestRow
  canClose: boolean
  isOwner?: boolean
  className?: string
}) {
  const [open, setOpen] = useState(false)
  const close = usePullRequestClose(pr, canClose)
  const train = usePullRequestTrainRows(pr)
  const entries = usePullRequestGuideEntries(
    pr,
    { member: canClose, owner: isOwner && canClose },
    { "close-pr": close.entry, "merge-now": train.mergeNow, hold: train.hold, resume: train.resume }
  )
  if (entries.length === 0) return null
  return (
    <>
      <Menu
        open={open}
        onOpenChange={setOpen}
        align="end"
        aria-label={`#${pr.number}`}
        title={`#${pr.number}`}
        contentClassName="w-64"
        entries={entries}
        trigger={
          <Button
            type="button"
            variant="ghost"
            size="icon"
            aria-label="More"
            data-testid={THREAD_TEST_IDS.guideMenu}
            className={className}
          >
            <MoreIcon />
          </Button>
        }
      />
      {close.dialog}
      {train.dialog}
    </>
  )
}

/** #EXP-1710: the header's merge STATUS pill of the thread's PR (every form
 *  factor, any viewer; a closed PR has none). */
export function ThreadGuideMerge({
  pr,
  canMerge,
  className,
}: {
  pr: PullRequestRow | null
  /** A member (approves in `approval` mode). */
  canMerge: boolean
  className?: string
}) {
  if (!pr || pr.state === `closed`) return null
  return (
    <div className={className ?? `flex shrink-0 items-center`} data-testid={THREAD_TEST_IDS.guideMerge}>
      <PullRequestStatusPill pr={pr} member={canMerge} />
    </div>
  )
}

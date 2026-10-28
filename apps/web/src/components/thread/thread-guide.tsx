import { useState } from "react"
import { Button, conceptIcon, Menu, type MenuEntry } from "@exp/ui"
import { PullRequestMergePill, usePullRequestClose } from "@/components/session-merge-button"
import {
  guideMenuEntries,
  guidePullRequest,
  THREAD_TEST_IDS,
  type PullRequestRow,
} from "@/lib/session-tree"

// EXP-1293 Threads v2 / EXP-1306: the thread's ONE pull request,
// `guidePullRequest` (session-thread.json, ×4). S-3 (stories.json): the
// Guide TAB is gone (its body, Changes rows and StackRail with it); what
// stays is the header's Merge pill on every form factor (phones included)
// and the `…` (`guideMenuEntries`). EXP-1307: Close PR (EXP-1338: no
// GitHub entry).

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

/** The thread header's `…` on the Guide (`guideMenuEntries`, ×4): Close PR
 *  while the PR is open and the viewer is a member (EXP-1338: no GitHub
 *  entry, only the agent opens PRs). */
export function ThreadGuideMenu({
  pr,
  canClose,
  className,
}: {
  pr: PullRequestRow
  canClose: boolean
  className?: string
}) {
  const [open, setOpen] = useState(false)
  const close = usePullRequestClose(pr, canClose)
  const entries = guideMenuEntries({ state: pr.state, member: canClose }).flatMap(
    (): MenuEntry[] => (close.entry ? [close.entry] : [])
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
    </>
  )
}

/** The header's Merge pill of the thread's open PR (S-3: every form
 *  factor; the phones' bottom capsule is gone). */
export function ThreadGuideMerge({
  pr,
  canMerge,
  className,
}: {
  pr: PullRequestRow | null
  canMerge: boolean
  className?: string
}) {
  if (!pr || pr.state !== `open` || !canMerge) return null
  return (
    <div className={className ?? `flex shrink-0 items-center`} data-testid={THREAD_TEST_IDS.guideMerge}>
      <PullRequestMergePill pr={pr} canMerge={canMerge} />
    </div>
  )
}

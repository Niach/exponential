import { useEffect, useState, type ReactNode } from "react"
import { eq, useLiveQuery } from "@tanstack/react-db"
import closePrCopy from "@exp/domain-contract/fixtures/close-pr.json"
import type { Issue } from "@/db/schema"
import { pullRequestIssueCollection } from "@/lib/collections"
import { useIssuePullRequests } from "@/hooks/use-session-tree"
import { mergeFailure } from "@/lib/merge-failure"
import { trpc } from "@/lib/trpc-client"
import { failedAction, SKIP_ERROR_TOAST } from "@/lib/action-error"
import { Prompt } from "@exp/ui"

// EXP-1154: CLOSE PR (without merging) lives in the issue's actions menu
// (`…`, md+ and phone): a destructive item above Delete issue, members only
// while the PR is open. One hook owns the gate, the confirm and the
// `pullRequests.close` mutation; the copy is the ×4 fixture `close-pr.json`. EXP-1293: the PR is the issue's OPEN
// `pull_requests` row (via `pull_request_issues`). A refusal is a toast with
// the merge-failure message; success needs nothing, the Electric echo flips
// the row's `state` and the item goes.

/** The confirm body: the fixture body, plus the batch line when the PR links
 *  `others` more issues. */
export function closePrBody(others: number): string {
  return others > 0
    ? `${closePrCopy.body} ${closePrCopy.batchLine.replace(`{n}`, String(others))}`
    : closePrCopy.body
}

export const CLOSE_PR_COPY = closePrCopy

export function useClosePr(
  issue: Pick<Issue, `id`>,
  { readOnly = false }: { readOnly?: boolean } = {}
): {
  canClose: boolean
  closing: boolean
  request: () => void
  dialog: ReactNode
} {
  // The issue's open pull request (newest first when several are open).
  const openPr =
    useIssuePullRequests(issue.id).find((pr) => pr.state === `open`) ?? null
  const pullRequestId = openPr?.id ?? null
  const canClose = !readOnly && pullRequestId !== null
  const [open, setOpen] = useState(false)
  const [closing, setClosing] = useState(false)
  // Every issue sharing this PR (a batch links several).
  const { data: linkedRows } = useLiveQuery(
    (query) =>
      canClose && pullRequestId
        ? query
            .from({ links: pullRequestIssueCollection })
            .where(({ links }) => eq(links.pullRequestId, pullRequestId))
        : undefined,
    [canClose, pullRequestId]
  )
  const others = Math.max(0, (linkedRows?.length ?? 1) - 1)

  // The spinner holds until the echo moves the row on (a refusal drops it).
  const updatedAt = openPr ? String(openPr.updatedAt) : null
  useEffect(() => {
    setClosing(false)
  }, [updatedAt])

  const confirm = () => {
    if (!pullRequestId) return
    setOpen(false)
    setClosing(true)
    trpc.pullRequests.close
      .mutate({ pullRequestId }, SKIP_ERROR_TOAST)
      .catch((error: unknown) => {
        // A destructive answer: the toast offers no Retry.
        failedAction(
          `close`,
          `the PR`,
          mergeFailure(error, `The pull request could not be closed`).message
        )
        setClosing(false)
      })
  }

  const dialog = canClose ? (
    <Prompt
      open={open}
      onOpenChange={setOpen}
      data-testid="close-pr-confirm"
      title={closePrCopy.title}
      body={closePrBody(others)}
      actions={[
        { label: `Cancel`, role: `cancel` },
        { label: closePrCopy.confirm, role: `destructive`, onSelect: confirm },
      ]}
    />
  ) : null

  return {
    canClose,
    closing,
    // Deferred past the menu's close + focus restore, so the dialog's focus
    // trap does not fight Radix.
    request: () => setTimeout(() => setOpen(true), 0),
    dialog,
  }
}

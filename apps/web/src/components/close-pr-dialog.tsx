import { useEffect, useState, type ReactNode } from "react"
import { eq, useLiveQuery } from "@tanstack/react-db"
import closePrCopy from "@exp/domain-contract/fixtures/close-pr.json"
import type { Issue } from "@/db/schema"
import { issueCollection } from "@/lib/collections"
import { mergeFailure } from "@/lib/merge-failure"
import { trpc } from "@/lib/trpc-client"
import { Prompt, toast } from "@exp/ui"

// EXP-1154: CLOSE PR (without merging) moved off the deleted Reviews detail
// page into the issue's actions menu (`…`, md+ and phone): a destructive item
// above Delete issue, members only while the PR is open. One hook owns the
// gate, the confirm and the `issues.closePr` mutation; the copy is the ×4
// fixture `close-pr.json`. A refusal is a toast with the merge-failure
// message; success needs nothing, the Electric echo flips `prState` and the
// item goes.

/** The confirm body: the fixture body, plus the batch line when the PR links
 *  `others` more issues. */
export function closePrBody(others: number): string {
  return others > 0
    ? `${closePrCopy.body} ${closePrCopy.batchLine.replace(`{n}`, String(others))}`
    : closePrCopy.body
}

export const CLOSE_PR_COPY = closePrCopy

export function useClosePr(
  issue: Pick<Issue, `id` | `prUrl` | `prNumber` | `prState` | `updatedAt`>,
  { readOnly = false }: { readOnly?: boolean } = {}
): {
  canClose: boolean
  closing: boolean
  request: () => void
  dialog: ReactNode
} {
  const canClose =
    !readOnly && issue.prState === `open` && issue.prNumber != null
  const [open, setOpen] = useState(false)
  const [closing, setClosing] = useState(false)
  // Every issue sharing this PR (a batch run links several).
  const { data: linkedRows } = useLiveQuery(
    (query) =>
      canClose && issue.prUrl
        ? query
            .from({ issues: issueCollection })
            .where(({ issues }) => eq(issues.prUrl, issue.prUrl))
        : undefined,
    [canClose, issue.prUrl]
  )
  const others = Math.max(0, (linkedRows?.length ?? 1) - 1)

  // The spinner holds until the echo moves the row on (a refusal drops it).
  const updatedAt = issue.updatedAt
  useEffect(() => {
    setClosing(false)
  }, [updatedAt])

  const confirm = () => {
    setOpen(false)
    setClosing(true)
    trpc.issues.closePr
      .mutate({ issueId: issue.id }, { context: { skipErrorToast: true } })
      .catch((error: unknown) => {
        toast.error(
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
        { label: `Cancel` },
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

import type { ReactNode } from "react"
import { ListRow } from "@exp/ui"
import { issueMenuProps } from "@/components/issue-context-menu/attr"

// THE Reviews queue's pull-request row. A flat `ListRow` on a 4-column grid:
// the lead glyph (a batch PR's batch glyph, else the open-PR glyph), the
// mono label, the title over its branch, and ONE trailing slot (Merge, Fix
// conflicts).

/** What a review row needs of its representative issue. */
export interface ReviewPrRowIssue {
  id: string
  identifier: string
  title: string
  branch: string | null
  prNumber: number | null
}

/** The mono label: a batch PR names itself by its number (its issues are
 *  many), a single-issue PR by its issue's identifier. */
export function reviewPrRowLabel(
  issue: Pick<ReviewPrRowIssue, `identifier` | `prNumber`>,
  isBatch: boolean
): string {
  return isBatch && issue.prNumber ? `#${issue.prNumber}` : issue.identifier
}

export function ReviewPrRow({
  issue,
  issues,
  lead,
  trailing,
  footer,
  onOpen,
}: {
  /** The PR's representative issue (branch, number). */
  issue: ReviewPrRowIssue
  /** Every issue on this PR — more than one = a batch PR. */
  issues: readonly Pick<ReviewPrRowIssue, `identifier`>[]
  /** The lead cell's glyph; the cell is ALWAYS drawn (EXP-916). */
  lead: ReactNode
  /** The ONE trailing control; absent = an empty cell. */
  trailing?: ReactNode
  /** A full-width line under the row (a merge refusal). */
  footer?: ReactNode
  onOpen: () => void
}) {
  const isBatch = issues.length > 1
  return (
    <ListRow
      interactive
      className="group/row grid grid-cols-[1.5rem_4.5rem_1fr_auto] gap-0"
      onClick={onOpen}
      data-testid={`review-row-${issue.identifier}`}
      {...issueMenuProps(issue.id)}
    >
      <span className="flex size-4 shrink-0 items-center justify-center">
        {lead}
      </span>
      <span className="truncate font-mono text-xs text-muted-foreground">
        {reviewPrRowLabel(issue, isBatch)}
      </span>
      {/* EXP-698: pr-3 IS the gap to the trailing control — on a phone the
          two used to sit 8px apart, which read as one blob. */}
      <div className="min-w-0 pr-3">
        <div className="truncate text-sm">
          {isBatch ? (
            <>
              {`${issues.length} issues`}
              <span className="ml-2 font-mono text-xs text-muted-foreground">
                {issues.map((linked) => linked.identifier).join(`, `)}
              </span>
            </>
          ) : (
            issue.title
          )}
        </div>
        {issue.branch && (
          <div className="truncate font-mono text-xs text-muted-foreground">
            {issue.branch}
          </div>
        )}
      </div>
      {trailing ?? <span />}
      {footer}
    </ListRow>
  )
}

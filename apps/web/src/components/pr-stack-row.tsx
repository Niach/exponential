import type { CSSProperties, ReactNode } from "react"
import {
  cn,
  ListRow,
  TREE_BASE,
  TREE_INDENT,
  TreeGuides,
  type TreeGuide,
} from "@exp/ui"
import { stackedOnCaption } from "@/lib/pr-stack"
import { issueMenuProps } from "@/components/issue-context-menu/attr"

// SLOP-16 r3: THE pull-request row of a stack — the Reviews queue's row and
// the "Related work" dialog's "Pull requests" band render this one component,
// so the two can never drift. A flat `ListRow` on a 4-column grid: the lead
// glyph (a batch PR's batch glyph, else the open-PR glyph), the mono label,
// the title over its branch / `on top of #…` caption, and ONE trailing slot
// (Merge, Merge stack, the PR state pill…). Nested bottom-up: 14px per
// stacked level (`TREE_INDENT`), the EXP-965 connector off the visible
// depths.

/** What a stack row needs of its representative issue. */
export interface PrStackRowIssue {
  id: string
  identifier: string
  title: string
  branch: string | null
  prNumber: number | null
}

/** The mono label: a batch PR names itself by its number (its issues are
 *  many), a single-issue PR by its issue's identifier. */
export function prStackRowLabel(
  issue: Pick<PrStackRowIssue, `identifier` | `prNumber`>,
  isBatch: boolean
): string {
  return isBatch && issue.prNumber ? `#${issue.prNumber}` : issue.identifier
}

export function PrStackRow({
  issue,
  issues,
  depth,
  guide,
  lead,
  stackedOn = null,
  listBatchIdentifiers = true,
  trailing,
  footer,
  onOpen,
  className,
  style,
}: {
  /** The PR's representative issue (branch, number). */
  issue: PrStackRowIssue
  /** Every issue on this PR — more than one = a batch PR. */
  issues: readonly Pick<PrStackRowIssue, `identifier`>[]
  depth: number
  guide: TreeGuide | null | undefined
  /** The lead cell's glyph; the cell is ALWAYS drawn (EXP-916). */
  lead: ReactNode
  /** The identifier of the PR this one stacks on (an upper member). */
  stackedOn?: string | null
  /** A batch row lists its issues' identifiers beside `N issues`; off where
   *  the same view lists those issues already (no duplicated information). */
  listBatchIdentifiers?: boolean
  /** The ONE trailing control; absent = an empty cell. */
  trailing?: ReactNode
  /** A full-width line under the row (a merge refusal). */
  footer?: ReactNode
  onOpen: () => void
  className?: string
  style?: CSSProperties
}) {
  const isBatch = issues.length > 1
  return (
    <ListRow
      interactive
      className={cn(
        `group/row relative grid grid-cols-[1.5rem_4.5rem_1fr_auto] gap-0`,
        className
      )}
      // EXP-897: 14px per stacked level, the ×4 indent.
      style={{ paddingLeft: `${TREE_BASE + depth * TREE_INDENT}px`, ...style }}
      onClick={onOpen}
      data-testid={`review-row-${issue.identifier}`}
      {...issueMenuProps(issue.id)}
    >
      <TreeGuides guide={guide ?? null} />
      <span className="flex size-4 shrink-0 items-center justify-center">
        {lead}
      </span>
      <span className="truncate font-mono text-xs text-muted-foreground">
        {prStackRowLabel(issue, isBatch)}
      </span>
      {/* EXP-698: pr-3 IS the gap to the trailing control — on a phone the
          two used to sit 8px apart, which read as one blob. */}
      <div className="min-w-0 pr-3">
        <div className="truncate text-sm">
          {isBatch ? (
            <>
              {`${issues.length} issues`}
              {listBatchIdentifiers && (
                <span className="ml-2 font-mono text-xs text-muted-foreground">
                  {issues.map((linked) => linked.identifier).join(`, `)}
                </span>
              )}
            </>
          ) : (
            issue.title
          )}
        </div>
        {/* EXP-897: the caption line carries the branch and an upper row's
            foundation. */}
        <div className="flex min-w-0 flex-wrap items-center gap-2">
          {issue.branch && (
            <span className="truncate font-mono text-xs text-muted-foreground">
              {issue.branch}
            </span>
          )}
          {stackedOn && (
            <span className="shrink-0 text-xs text-muted-foreground">
              {stackedOnCaption(stackedOn)}
            </span>
          )}
        </div>
      </div>
      {trailing ?? <span />}
      {footer}
    </ListRow>
  )
}

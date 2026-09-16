import { useMemo, type ReactNode } from "react"
import { conceptIcon, Button } from "@exp/ui"
import { contract } from "@exp/domain-contract"
import { summaryLabel, totals, type DiffFile } from "@exp/domain-contract/diff"
import { cn } from "@/lib/utils"
import type { SessionMergeTargetProps } from "@/hooks/use-agents-data"
import { PrGithubButton } from "@/components/pr-github-button"
import { SessionMergePill } from "@/components/session-merge-button"

// EXP-916: the REVIEWS header, and only Reviews. A run's Changes face lost its
// own bar — its merge, its GitHub link and its PR state live in the work header
// it already wears — so this stopped being "the header of a Changes surface"
// and became the review page's identity line plus its actions:
//
//   MET-12 · exp/MET-12 · Open · 2 files +5 −0 │ ✕ Reject · Merge PR · GitHub
//
// The counts are `summaryLabel` — the very words the file tree heads its list
// with, so the page says its size once and says it the same way ×4. Every
// action label is the contract's.

const UiLoadingIcon = conceptIcon(`ui-loading`)
const PrClosedIcon = conceptIcon(`pr-closed`)

export function ChangesTopBar({
  identifier,
  files,
  branch,
  prState,
  prUrl,
  merge,
  /** Close the PR without merging — the review's own action. */
  onClosePr,
  closing = false,
  trailing,
  className,
}: {
  /** The representative issue's identifier (`MET-12`) — what this review IS. */
  identifier?: string | null
  files: readonly DiffFile[]
  branch?: string | null
  /** `null`/absent = there is no pull request yet (a pushed branch). */
  prState?: string | null
  prUrl?: string | null
  /** The merge TARGET (an issue XOR a run). Absent = nothing to merge here. */
  merge?: (SessionMergeTargetProps & { steerEnabled: boolean }) | null
  onClosePr?: () => void
  closing?: boolean
  /** Anything the surface adds after GitHub (the PR-graph badge). */
  trailing?: ReactNode
  className?: string
}) {
  const summary = useMemo(() => {
    const sum = totals(files)
    return summaryLabel(sum.files, sum.additions, sum.deletions)
  }, [files])
  const prStateLabel = prState == null ? `No pull request` : prState
  const isOpen = prState === `open`

  return (
    <div
      className={cn(
        `hidden min-w-0 items-center gap-3 border-b border-border px-4 py-2 text-xs md:flex`,
        className
      )}
      data-testid="changes-top-bar"
    >
      {identifier && (
        <span className="shrink-0 font-mono font-medium">{identifier}</span>
      )}
      {branch && (
        <span className="min-w-0 truncate font-mono text-muted-foreground">
          {branch}
        </span>
      )}
      <span className="shrink-0 capitalize text-muted-foreground">
        {prStateLabel}
      </span>
      <span
        className="min-w-0 truncate text-muted-foreground"
        data-testid="changes-file-count"
      >
        {summary}
      </span>
      <div className="ml-auto flex shrink-0 items-center gap-2">
        {isOpen && onClosePr && (
          <Button
            variant="glass"
            size="icon-sm"
            className="rounded-full"
            aria-label={contract.diffUi.closePr}
            title={contract.diffUi.closePr}
            data-testid="changes-close-pr"
            disabled={closing}
            onClick={onClosePr}
          >
            {closing ? (
              <UiLoadingIcon className="size-4 animate-spin" />
            ) : (
              <PrClosedIcon className="size-4" />
            )}
          </Button>
        )}
        {merge && (
          <SessionMergePill {...merge} label={contract.diffUi.mergePr} />
        )}
        {prUrl && <PrGithubButton prUrl={prUrl} variant="glass" />}
        {trailing}
      </div>
    </div>
  )
}

import { useMemo, type ReactNode } from "react"
import {
  conceptIcon,
  Button,
  DiffCounts,
} from "@exp/ui"
import { totals, type DiffFile } from "@exp/domain-contract/diff"
import { cn } from "@/lib/utils"
import type { SessionMergeTargetProps } from "@/hooks/use-agents-data"
import { MERGE_PR_LABEL } from "@/components/run-action-pills"
import { SessionMergePill } from "@/components/session-merge-button"

// EXP-895: the ONE header a Changes surface wears on md+ — the review page and
// the run's Changes face used to carry two hand-rolled variants of it. Reading
// order is the review layout's: what this is, how big it is, where it came
// from, then the actions.
//
//   Changes · +N −M · K files · branch · PR state │ ✕ close · Merge PR · GitHub
//
// The merge control is HERE and nowhere else while a Changes surface is up
// (EXP-895): exactly ONE `SessionMergePill`, so the run header's own Merge pill
// stands down while the face shows.

const GithubIcon = conceptIcon(`ui-github`)
const UiLoadingIcon = conceptIcon(`ui-loading`)
const UiCloseIcon = conceptIcon(`ui-close`)

export const CHANGES_TITLE = `Changes`

export function ChangesTopBar({
  files,
  branch,
  prState,
  prUrl,
  merge,
  /** Review only: close the PR without merging (the run's face has no such
   *  action — a chore PR is closed on GitHub). */
  onClosePr,
  closing = false,
  trailing,
  className,
}: {
  files: readonly DiffFile[]
  branch?: string | null
  /** `null`/absent = there is no pull request yet (a pushed branch). */
  prState?: string | null
  prUrl?: string | null
  /** The merge TARGET (an issue XOR a run). Absent = nothing to merge here. */
  merge?: (SessionMergeTargetProps & { steerEnabled: boolean }) | null
  onClosePr?: () => void
  closing?: boolean
  /** Anything the surface adds after GitHub (nothing does, yet). */
  trailing?: ReactNode
  className?: string
}) {
  const sum = useMemo(() => totals(files), [files])
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
      <span className="shrink-0 font-medium">{CHANGES_TITLE}</span>
      <DiffCounts additions={sum.additions} deletions={sum.deletions} />
      {/* The same words the file column heads its list with, minus the counts
          that already sit to the left of them. */}
      <span className="shrink-0 text-muted-foreground" data-testid="changes-file-count">
        {sum.files === 1 ? `1 file` : `${sum.files} files`}
      </span>
      {branch && (
        <span className="min-w-0 truncate font-mono text-muted-foreground">
          {branch}
        </span>
      )}
      <span className="shrink-0 capitalize text-muted-foreground">
        {prStateLabel}
      </span>
      <div className="ml-auto flex shrink-0 items-center gap-2">
        {isOpen && onClosePr && (
          <Button
            variant="glass"
            size="icon-sm"
            aria-label="Close pull request without merging"
            title="Close PR without merging"
            disabled={closing}
            onClick={onClosePr}
          >
            {closing ? (
              <UiLoadingIcon className="size-4 animate-spin" />
            ) : (
              <UiCloseIcon className="size-4" />
            )}
          </Button>
        )}
        {merge && <SessionMergePill {...merge} label={MERGE_PR_LABEL} />}
        {prUrl && (
          <Button
            variant="glass"
            size="icon-sm"
            aria-label="Open pull request on GitHub"
            title="Open PR on GitHub"
            data-testid="changes-github-link"
            onClick={() => window.open(prUrl, `_blank`, `noopener,noreferrer`)}
          >
            <GithubIcon className="size-4" />
          </Button>
        )}
        {trailing}
      </div>
    </div>
  )
}

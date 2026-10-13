import { useEffect, useRef, type ReactNode } from "react"
import type { DiffFile } from "@exp/domain-contract/diff"
import {
  conceptIcon,
  FEEDBACK_ERRORS,
  ListEmpty,
  Pill,
  SessionResultsView,
  SkeletonRows,
  type GuideChangesTarget,
  type SessionResultGroup,
} from "@exp/ui"
import type { ReviewFilesState } from "@/hooks/use-review-files"
import { failedAction } from "@/lib/action-error"

// EXP-1251 / EXP-1293: the GUIDE body: the Summary lead, the numbered
// sections (band, text, ONE `Changes · N files · +A −D ›` row, tiles,
// Earlier), the automatic `Other changes` section and the final `Show
// complete diff` row (`SessionResultsView`, `guideCoverage` behind it). A
// thread's Guide tab feeds it the root's guide entries over its pull
// request's files (`pullRequests.files`); a Changes row opens the section's
// diff as a page under the Guide (`guide-section-diff.tsx`).

const UiRefreshIcon = conceptIcon(`ui-refresh`)

export const GUIDE_NO_CHANGES = `No changes yet. Nothing has been pushed.`

/** The diff the Guide counts, from a files fetch: the files once loaded,
 *  else null (the coverage draws no Changes rows yet). */
export function guideFilesOf(state: ReviewFilesState | null | undefined): DiffFile[] | null {
  return state?.kind === `files` ? state.files : null
}

/** The Guide's body on every width: the report over the diff. */
export function GuideBody({
  groups,
  files,
  filesState,
  stack,
  onOpenChanges,
  onRetry,
  renderText,
}: {
  groups: readonly SessionResultGroup[]
  /** The diff the coverage counts (null = not loaded yet). */
  files: readonly DiffFile[] | null
  /** The files fetch, for its loading / none / error states when there is
   *  no report to stand on. */
  filesState?: ReviewFilesState | null
  /** A lead block above the report (the pull requests). */
  stack?: ReactNode
  /** A Changes row / Show complete diff: the section page. */
  onOpenChanges?: (target: GuideChangesTarget) => void
  onRetry?: () => void
  /** The app's GFM renderer. */
  renderText?: (text: string) => ReactNode
}) {
  const hasDiff = (files?.length ?? 0) > 0
  const showView = groups.length > 0 || hasDiff
  const filesError = filesState?.kind === `error` ? filesState.message : null
  const filesLoading = filesState?.kind === `loading`
  const waiting = !showView && filesLoading
  // FEED-79: the fetch runs on open, so a failed one is the empty state,
  // never a toast; the diff keeps its Retry, and only a failed Retry tap
  // (the person's own action) toasts.
  const retried = useRef(false)
  const retryFiles = onRetry
    ? () => {
        retried.current = true
        onRetry()
      }
    : undefined
  const retryRef = useRef(retryFiles)
  retryRef.current = retryFiles
  useEffect(() => {
    if (filesLoading) return
    const tapped = retried.current
    retried.current = false
    if (!tapped || !filesError) return
    const again = retryRef.current
    failedAction(`load`, `the changes`, filesError, {
      retry: again ? () => again() : undefined,
    })
  }, [filesLoading, filesError])
  return (
    <div data-testid="guide-body">
      {stack && <div className="px-7 pt-5 md:px-5">{stack}</div>}
      {waiting && <SkeletonRows dot={false} className="px-4 py-3 md:px-2" />}
      {!showView && !waiting && <ListEmpty>{GUIDE_NO_CHANGES}</ListEmpty>}
      {!showView && !waiting && filesError && retryFiles && (
        <div className="flex justify-center pb-6">
          <Pill mode="action" onClick={retryFiles}>
            <UiRefreshIcon className="size-3" />
            {FEEDBACK_ERRORS.retry}
          </Pill>
        </div>
      )}
      {showView && (
        <SessionResultsView
          groups={groups}
          attachmentSrc={(id) => `/api/attachments/${id}`}
          renderText={renderText}
          files={files}
          onOpenChanges={onOpenChanges}
        />
      )}
    </div>
  )
}

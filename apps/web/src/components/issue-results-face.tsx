import type { ReactNode } from "react"
import type { DiffFile } from "@exp/domain-contract/diff"
import type { Board, Issue } from "@/db/schema"
import {
  conceptIcon,
  MOBILE_WORK_BAR_CLEARANCE,
  MobileWorkBar,
  SessionResultsView,
  useIsMobile,
  type SessionResultGroup,
} from "@exp/ui"
import { cn } from "@/lib/utils"
import type { PrDescriptionState } from "@/hooks/use-pr-description"
import { renderResultText } from "@/components/agent-session"
import { IssueMobileHeader } from "@/components/issue-mobile-header"
import { MOBILE_DETAIL_SCREEN_CLASS } from "@/components/team/mobile-detail-header"
import { useMeasuredSize } from "@/hooks/use-detail-chrome"
import {
  FACE_BODY_TOUCH_CLASS,
  type FaceSwipeHandlers,
} from "@/components/mobile-face-tabs"
import { useIssuePropertyHandlers } from "@/hooks/use-issue-property-handlers"

// EXP-933: an ISSUE's Results face — the report (topic text + screenshots) of
// the run `issueResultsRun` picked, rendered ON the issue route. A teammate's
// run is not openable on the session route, and an agent message deep-links
// every recipient to the issue's `?view=results`, so the issue carries the
// report itself. EXP-1154: the report reads as the GUIDE (`SessionResultsView`:
// Summary lead, numbered sections with the files they touched; a file row
// opens the Changes face on it). An issue with an OPEN PR and no report shows
// the GitHub PR body instead, as ONE unnumbered group (`prDescriptionGroups`).

const UiLoadingIcon = conceptIcon(`ui-loading`)

/** The fallback's band label without a PR title. */
export const PR_DESCRIPTION_FALLBACK_TOPIC = `Pull request`
/** The fallback's body for a blank PR description. */
export const PR_DESCRIPTION_EMPTY = `No description.`

/** EXP-1154: the GitHub PR body as ONE Results group (band = the PR title,
 *  else `Pull request`; `No description.` when blank); [] until loaded. */
export function prDescriptionGroups(
  state: PrDescriptionState
): SessionResultGroup[] {
  if (state.kind !== `ready`) return []
  return [
    {
      topic: state.title.trim() || PR_DESCRIPTION_FALLBACK_TOPIC,
      text: state.body.trim() || PR_DESCRIPTION_EMPTY,
      entries: [],
      earlier: [],
      files: [],
    },
  ]
}

/** The md+ body (and the phone face's): the Guide in the work column. */
export function IssueResultsBody({
  groups,
  files,
  onOpenFile,
  numbered = true,
  loading = false,
  error = null,
}: {
  groups: readonly SessionResultGroup[]
  /** The loaded diff the file rows read their `+N −M` from. */
  files?: readonly DiffFile[] | null
  /** A file row opens the Changes face on that path. */
  onOpenFile?: (path: string) => void
  /** False for the PR-body fallback (one band, no `01 / 01`). */
  numbered?: boolean
  /** The PR-body fallback is still loading. */
  loading?: boolean
  error?: string | null
}) {
  const isMobile = useIsMobile()
  return (
    <div data-testid="issue-results-face">
      {groups.length === 0 && loading && (
        <div className="flex items-center gap-2 px-7 py-6 text-sm text-muted-foreground md:px-9">
          <UiLoadingIcon className="size-4 animate-spin" />
          Loading the pull request…
        </div>
      )}
      {groups.length === 0 && error && (
        <p className="px-7 py-6 text-sm text-destructive md:px-9">{error}</p>
      )}
      {groups.length > 0 && (
        <SessionResultsView
          groups={groups}
          attachmentSrc={(id) => `/api/attachments/${id}`}
          renderText={renderResultText}
          files={files}
          onOpenFile={onOpenFile}
          isMobile={isMobile}
          numbered={numbered}
        />
      )}
    </div>
  )
}

/** The phone face: the issue's header with the face tabs under it, the
 *  Guide, and (EXP-1154) the bar's centred white Merge capsule alone. */
export function IssueResultsFace({
  issue,
  board,
  teamSlug,
  teamId,
  readOnly,
  origin,
  body,
  merge,
  tabs,
  swipe,
}: {
  issue: Issue
  board: Board
  teamSlug: string
  teamId: string
  readOnly: boolean
  origin?: string
  /** The Guide (`IssueResultsBody`), built by the route. */
  body: ReactNode
  /** EXP-1154: the white Merge capsule (`MergeCapsule`), absent when the
   *  viewer may not merge. */
  merge?: ReactNode
  /** EXP-1150: the face strip (`MobileFaceTabs`) and the pager the root
   *  spreads (`useFaceSwipe`; EXP-1152: it moves the `data-face-body`). */
  tabs: ReactNode
  swipe?: FaceSwipeHandlers
}) {
  const handlers = useIssuePropertyHandlers({ issue, teamSlug, readOnly })
  // EXP-1162: the header band floats over the scroller, which pads by it.
  const [headerRef, headerSize] = useMeasuredSize()
  return (
    <div className={MOBILE_DETAIL_SCREEN_CLASS} {...swipe}>
      <IssueMobileHeader
        issue={issue}
        board={board}
        teamSlug={teamSlug}
        teamId={teamId}
        readOnly={readOnly}
        origin={origin}
        handlers={handlers}
        face="results"
        tabs={tabs}
        overlay
        headerRef={headerRef}
      />
      <div
        className={cn(
          `min-h-0 flex-1 overflow-y-auto overscroll-contain bg-card/40`,
          MOBILE_WORK_BAR_CLEARANCE,
          FACE_BODY_TOUCH_CLASS
        )}
        /* EXP-1152: the pager's body — it follows the finger. */
        data-face-body=""
        style={{ paddingTop: headerSize.height }}
      >
        {body}
      </div>
      {merge && <MobileWorkBar cluster capsule={merge} />}
      {handlers.duplicatePicker}
    </div>
  )
}

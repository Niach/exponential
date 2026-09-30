import type { ReactNode } from "react"
import type { Board, Issue } from "@/db/schema"
import {
  MOBILE_WORK_BAR_CLEARANCE,
  SessionResultsView,
  type SessionDotTone,
  type SessionResultGroup,
} from "@exp/ui"
import { cn } from "@/lib/utils"
import { renderResultText } from "@/components/agent-session"
import { IssueMobileHeader } from "@/components/issue-mobile-header"
import {
  FACE_BODY_TOUCH_CLASS,
  type FaceSwipeHandlers,
} from "@/components/mobile-face-tabs"
import { useIssuePropertyHandlers } from "@/hooks/use-issue-property-handlers"

// EXP-933: an ISSUE's Results face — the report (topic text + screenshots) of
// the run `issueResultsRun` picked, rendered ON the issue route. A teammate's
// run is not openable on the session route, and an agent message deep-links
// every recipient to the issue's `?view=results`, so the issue carries the
// report itself.

/** The md+ body: the report in the issue's work column. */
export function IssueResultsBody({
  groups,
}: {
  groups: readonly SessionResultGroup[]
}) {
  return (
    <div data-testid="issue-results-face">
      <SessionResultsView
        groups={groups}
        attachmentSrc={(id) => `/api/attachments/${id}`}
        renderText={renderResultText}
      />
    </div>
  )
}

/** The phone face: the issue's header with the face tabs under it, the
 *  report, and NO bar — the report has nothing to act on (EXP-1150). */
export function IssueResultsFace({
  issue,
  board,
  teamSlug,
  teamId,
  readOnly,
  origin,
  groups,
  tabs,
  swipe,
  dot,
}: {
  issue: Issue
  board: Board
  teamSlug: string
  teamId: string
  readOnly: boolean
  origin?: string
  groups: readonly SessionResultGroup[]
  /** EXP-1150: the face strip (`MobileFaceTabs`) and the pager the root
   *  spreads (`useFaceSwipe`; EXP-1152: it moves the `data-face-body`). */
  tabs: ReactNode
  swipe?: FaceSwipeHandlers
  dot?: { tone: SessionDotTone; connecting?: boolean } | null
}) {
  const handlers = useIssuePropertyHandlers({ issue, teamSlug, readOnly })
  return (
    <div className="flex h-full min-h-0 flex-col" {...swipe}>
      <IssueMobileHeader
        issue={issue}
        board={board}
        teamSlug={teamSlug}
        teamId={teamId}
        readOnly={readOnly}
        origin={origin}
        handlers={handlers}
        face="results"
        dot={dot}
        tabs={tabs}
      />
      <div
        className={cn(
          `min-h-0 flex-1 overflow-y-auto overscroll-contain bg-card/40`,
          MOBILE_WORK_BAR_CLEARANCE,
          FACE_BODY_TOUCH_CLASS
        )}
        /* EXP-1152: the pager's body — it follows the finger. */
        data-face-body=""
      >
        <IssueResultsBody groups={groups} />
      </div>
      {handlers.duplicatePicker}
    </div>
  )
}

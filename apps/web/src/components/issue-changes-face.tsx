import { useState, type ReactNode } from "react"
import type { Board, Issue } from "@/db/schema"
import {
  conceptIcon,
  PILL_PRIMARY_PAINT,
  type SessionDotTone,
  MOBILE_WORK_BAR_CLEARANCE,
  MOBILE_WORK_CAPSULE_CLASS,
  MobileWorkBar,
  ChangesFileSheet,
  PrGithubButton,
} from "@exp/ui"
import { cn } from "@/lib/utils"
import type { ReviewFilesState } from "@/hooks/use-review-files"
import { ChangesView } from "@/components/changes-view"
import { IssueMobileHeader } from "@/components/issue-mobile-header"
import { MOBILE_DETAIL_SCREEN_CLASS } from "@/components/team/mobile-detail-header"
import { useMeasuredSize } from "@/hooks/use-detail-chrome"
import {
  FACE_BODY_TOUCH_CLASS,
  type FaceSwipeHandlers,
} from "@/components/mobile-face-tabs"
import { PrGraphBadge } from "@/components/pr-graph-badge"
import { MERGE_PR_LABEL } from "@/components/run-action-pills"
import { SessionMergePill } from "@/components/session-merge-button"
import { useIssuePropertyHandlers } from "@/hooks/use-issue-property-handlers"

// EXP-893: the Changes FACE of an issue subject with NO shown run — the
// issue has an open PR (or a pushed branch), so its files are the face
// (`useReviewFiles`, the review route's own loader — EXP-952: called by the
// ISSUE ROUTE, which hands the state down, so the face switcher counts the
// same files before this face was ever opened). Same header as the
// Issue face, the diff in the column, and the bar: the file SHEET on the left
// (EXP-895 — GitHub moved up into the header's action slot, where a phone
// header has room for it). EXP-1150: no switcher circle and no Merge capsule
// — the face TABS sit in the header band with the Merge PR pill beside them
// on every face, and the body swipes between faces. A run's live diff draws
// the same face inside the session view.

const UiLoadingIcon = conceptIcon(`ui-loading`)

/** EXP-916: the phone's Merge PR capsule of the REVIEWS page — a SOLID white
 *  pill hugging its label (28px padding, a 20px glyph) in the bar's centred
 *  cluster, carrying `SessionMergePill`'s confirm and Fix-conflicts swap. It
 *  self-hides unless the PR is open. EXP-1150: the Work screen's Changes face
 *  no longer draws it — there the Merge pill rides the header band. */
export function MergeCapsule(props: {
  issueId?: string
  sessionId?: string
  prState: string | null
  prNumber: number | null
  branch: string | null
  updatedAt: string | Date | null
  steerEnabled: boolean
}) {
  return (
    <SessionMergePill
      {...props}
      label={MERGE_PR_LABEL}
      className={cn(
        MOBILE_WORK_CAPSULE_CLASS,
        `flex-none justify-center rounded-full px-7 font-medium [&_svg]:size-5`,
        PILL_PRIMARY_PAINT
      )}
    />
  )
}

export function IssueChangesFace({
  issue,
  board,
  teamSlug,
  teamId,
  readOnly,
  origin,
  filesState: state,
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
  /** EXP-952: the issue's PR / branch files, fetched by the route
   *  (`useReviewFiles`) — the switcher's `+N −M` reads the same list. */
  filesState: ReviewFilesState
  /** EXP-1150: the face strip (`MobileFaceTabs`) and the pager the root
   *  spreads (`useFaceSwipe`; EXP-1152: it moves the `data-face-body`). */
  tabs: ReactNode
  swipe?: FaceSwipeHandlers
  dot?: { tone: SessionDotTone; connecting?: boolean } | null
}) {
  const [selected, setSelected] = useState<string | null>(null)
  // The `…` menu's Move to board / Unmark duplicate, the same handlers the
  // issue face binds (`use-issue-property-handlers.ts`).
  const handlers = useIssuePropertyHandlers({ issue, teamSlug, readOnly })
  const files = state.kind === `files` ? state.files : []
  // EXP-1162: the header band floats over the scroller, which pads by it.
  const [headerRef, headerSize] = useMeasuredSize()

  return (
    <div
      className={MOBILE_DETAIL_SCREEN_CLASS}
      data-testid="issue-changes-face"
      {...swipe}
    >
      <IssueMobileHeader
        issue={issue}
        board={board}
        teamSlug={teamSlug}
        teamId={teamId}
        readOnly={readOnly}
        origin={origin}
        handlers={handlers}
        /* EXP-934: the `…` belongs to the Issue face; Changes keeps GitHub. */
        face="changes"
        action={
          issue.prUrl ? <PrGithubButton prUrl={issue.prUrl} /> : undefined
        }
        graphBadge={
          /* SLOP-16: the same "Related work" overlay every face opens:
             Blocked by · Same pull request · Pull request stack. */
          <PrGraphBadge
            teamId={teamId}
            teamSlug={teamSlug}
            issue={issue}
          />
        }
        dot={dot}
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
        {state.kind === `loading` && (
          <div className="flex items-center gap-2 px-4 py-6 text-sm text-muted-foreground">
            <UiLoadingIcon className="size-4 animate-spin" />
            Loading changes…
          </div>
        )}
        {state.kind === `none` && (
          <p className="px-4 py-6 text-sm text-muted-foreground">
            No changes yet — nothing has been pushed for this issue.
          </p>
        )}
        {state.kind === `error` && (
          <p className="px-4 py-6 text-sm text-destructive">{state.message}</p>
        )}
        {state.kind === `files` && (
          /* The file LIST is the bar's sheet on a phone, so the cards stand
             alone here (`nav="none"`). EXP-916: they start OPEN, like every
             other diff surface — only a file past the contract's collapse
             threshold folds itself. */
          <ChangesView
            files={files}
            nav="none"
            selected={selected}
            onSelect={setSelected}
            emptyLabel="No changes in this pull request."
          />
        )}
      </div>
      {files.length > 0 && (
        <MobileWorkBar
          /* EXP-916: the Reviews page's cluster — the file sheet alone here
             (EXP-1150: Merge rides the header band). */
          cluster
          leading={
            <ChangesFileSheet
              files={files}
              selected={selected}
              onSelect={setSelected}
            />
          }
        />
      )}
      {handlers.duplicatePicker}
    </div>
  )
}

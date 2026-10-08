import type { ReactNode } from "react"
import type { Board, Issue } from "@/db/schema"
import {
  conceptIcon,
  PILL_PRIMARY_PAINT,
  MOBILE_WORK_BAR_CLEARANCE,
  MOBILE_WORK_CAPSULE_CLASS,
  MobileWorkBar,
  ChangesFileSheet,
  Pill,
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
// (`useReviewFiles` — EXP-952: called by the ISSUE ROUTE, which hands the
// state down, so the face tabs count the same files before this face was
// ever opened). Same header as the Issue face, the diff in the column, GitHub
// in the header's action slot. EXP-1154: this IS the review of the PR (the
// Reviews detail page is gone), and Merge is back on the floating bar on
// every phone face: here the bar's centred cluster = the file SHEET + the
// white Merge capsule. md+ draws `IssueChangesBody` in the issue's work
// column. A run's live diff draws the same face inside the session view.

const UiLoadingIcon = conceptIcon(`ui-loading`)
const UiRefreshIcon = conceptIcon(`ui-refresh`)

/** EXP-916 / EXP-1154: the phone's Merge PR capsule — a SOLID white pill
 *  hugging its label (28px padding, a 20px glyph), carrying
 *  `SessionMergePill`'s confirm, stack choice and conflict recovery. It
 *  self-hides unless the PR is open. The Changes and Results faces put it in
 *  the bar's centred cluster; the Issue and Run faces'
 *  composer bars get the Merge circle instead (`MobileMergeCircle`). */
export function MergeCapsule(props: {
  issueId?: string
  sessionId?: string
  prState: string | null
  prNumber: number | null
  branch: string | null
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

/** EXP-1154: the Changes body of an issue — the PR / branch files
 *  (`useReviewFiles`), the cards alone (the file list is the sidebar's tree
 *  on md+, the bar's sheet on a phone). Every card starts OPEN (EXP-916). */
export function IssueChangesBody({
  state,
  selected,
  onSelect,
  onRetry,
}: {
  state: ReviewFilesState
  selected: string | null
  onSelect: (path: string) => void
  onRetry?: () => void
}) {
  return (
    <div data-testid="issue-changes-body">
      {state.kind === `loading` && (
        <div className="flex items-center gap-2 px-4 py-6 text-sm text-muted-foreground md:px-5">
          <UiLoadingIcon className="size-4 animate-spin" />
          Loading changes…
        </div>
      )}
      {state.kind === `none` && (
        <p className="px-4 py-6 text-sm text-muted-foreground md:px-5">
          No changes yet. Nothing has been pushed for this issue.
        </p>
      )}
      {state.kind === `error` && (
        <div className="flex flex-wrap items-center gap-2 px-4 py-6 text-sm text-destructive md:px-5">
          {`Couldn’t load changes: ${state.message}`}
          {onRetry && (
            <Pill mode="action" onClick={onRetry}>
              <UiRefreshIcon className="size-3" />
              Retry
            </Pill>
          )}
        </div>
      )}
      {state.kind === `files` && (
        <ChangesView
          files={state.files}
          nav="none"
          selected={selected}
          onSelect={onSelect}
          emptyLabel="No changes in this pull request."
        />
      )}
    </div>
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
  onRetry,
  selected,
  onSelect,
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
  /** EXP-952: the issue's PR / branch files, fetched by the route
   *  (`useReviewFiles`) — the tabs' `+N −M` reads the same list. */
  filesState: ReviewFilesState
  onRetry?: () => void
  /** EXP-1154: the file in focus, owned by the route so a Results file row
   *  (and `?file=`) can open this face on it. */
  selected: string | null
  onSelect: (path: string) => void
  /** EXP-1154: the bar's white Merge capsule (`MergeCapsule`), absent when
   *  the viewer may not merge. */
  merge?: ReactNode
  /** EXP-1150: the face strip (`MobileFaceTabs`) and the pager the root
   *  spreads (`useFaceSwipe`; EXP-1152: it moves the `data-face-body`). */
  tabs: ReactNode
  swipe?: FaceSwipeHandlers
}) {
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
        <IssueChangesBody
          state={state}
          selected={selected}
          onSelect={onSelect}
          onRetry={onRetry}
        />
      </div>
      {(files.length > 0 || merge) && (
        <MobileWorkBar
          /* EXP-916: the review cluster — the file sheet and (EXP-1154) the
             white Merge capsule, centred. */
          cluster
          leading={
            files.length > 0 ? (
              <ChangesFileSheet
                files={files}
                selected={selected}
                onSelect={onSelect}
              />
            ) : undefined
          }
          capsule={merge}
        />
      )}
      {handlers.duplicatePicker}
    </div>
  )
}

import { useMemo, type ReactNode } from "react"
import { and, eq, inArray, useLiveQuery } from "@tanstack/react-db"
import type { DiffFile } from "@exp/domain-contract/diff"
import {
  conceptIcon,
  GlassSectionHeader,
  MOBILE_WORK_BAR_CLEARANCE,
  MobileWorkBar,
  Pill,
  SessionResultsView,
  StackRail,
  type GuideChangesTarget,
  type SessionResultGroup,
} from "@exp/ui"
import type { Board, Issue } from "@/db/schema"
import { boardCollection, issueCollection } from "@/lib/collections"
import { stackView, type StackView } from "@/lib/pr-stack"
import type { PrDescriptionState } from "@/hooks/use-pr-description"
import type { ReviewFilesState } from "@/hooks/use-review-files"
import { useTeamBoardIds } from "@/hooks/use-team-issue-graph"
import { useMeasuredSize } from "@/hooks/use-detail-chrome"
import { useIssuePropertyHandlers } from "@/hooks/use-issue-property-handlers"
import { IssueMobileHeader } from "@/components/issue-mobile-header"
import { MOBILE_DETAIL_SCREEN_CLASS } from "@/components/team/mobile-detail-header"
import {
  FACE_BODY_TOUCH_CLASS,
  type FaceSwipeHandlers,
} from "@/components/work-faces"
import { cn } from "@/lib/utils"

// EXP-1251: the GUIDE face — Changes + Results merged into one face on every
// client. Top to bottom: the Stack card (only while the pull request sits in
// a linear open stack of 2+), the Summary lead, the numbered sections (band,
// text, ONE `Changes · N files · +A −D ›` row, tiles, Earlier), the automatic
// `Other changes` section and the final `Show complete diff` row
// (`SessionResultsView`, `guideCoverage` behind it). A Changes row opens the
// section's diff as a page under the Guide (`guide-section-diff.tsx`,
// `?view=guide&section=N`). With no report the open PR's GitHub body stands
// in as one unnumbered group (`prDescriptionGroups`); with neither, the diff
// alone is one `Changes` section.

const UiLoadingIcon = conceptIcon(`ui-loading`)
const UiRefreshIcon = conceptIcon(`ui-refresh`)
const PrStackIcon = conceptIcon(`pr-stack`)

/** The fallback's band label without a PR title. */
export const PR_DESCRIPTION_FALLBACK_TOPIC = `Pull request`
/** The fallback's body for a blank PR description. */
export const PR_DESCRIPTION_EMPTY = `No description.`
/** The Stack card's band. No count (list bands carry none). */
export const STACK_CARD_TITLE = `Stack`
export const GUIDE_NO_CHANGES = `No changes yet. Nothing has been pushed for this issue.`

/** EXP-1154: the GitHub PR body as ONE Guide group (band = the PR title,
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

/** The diff the Guide counts, from a files fetch: the files once loaded,
 *  else null (the coverage draws no Changes rows yet). */
export function guideFilesOf(state: ReviewFilesState | null | undefined): DiffFile[] | null {
  return state?.kind === `files` ? state.files : null
}

/**
 * EXP-1248: the Stack card of `issue`'s pull request: the team's synced
 * open-PR issues decide it (`stackView`), so it shows only while the PR sits
 * in a linear open stack. `memberTarget` resolves a rail row to the route
 * that opens it (the board slug lives on the board row).
 */
export function useIssueStack(
  issue: Issue | null | undefined,
  teamId: string | undefined
): {
  stack: StackView | null
  memberTarget: (issueId: string) => { boardSlug: string; identifier: string } | null
} {
  const active = Boolean(issue && teamId && issue.prState === `open`)
  const boardIds = useTeamBoardIds(active ? teamId : undefined)
  const { data: openRows } = useLiveQuery(
    (query) =>
      active && boardIds.length > 0
        ? query
            .from({ i: issueCollection })
            .where(({ i }) => and(inArray(i.boardId, boardIds), eq(i.prState, `open`)))
        : undefined,
    [active, boardIds.join(`,`)]
  )
  const { data: boardRows } = useLiveQuery(
    (query) =>
      active && teamId
        ? query.from({ b: boardCollection }).where(({ b }) => eq(b.teamId, teamId))
        : undefined,
    [active, teamId]
  )
  return useMemo(() => {
    const open = (openRows ?? []) as Issue[]
    const boards = new Map(((boardRows ?? []) as Board[]).map((board) => [board.id, board]))
    const byId = new Map(open.map((row) => [row.id, row]))
    return {
      stack: active && issue ? stackView(issue, open) : null,
      memberTarget: (issueId) => {
        const row = byId.get(issueId)
        const board = row ? boards.get(row.boardId) : undefined
        return row && board ? { boardSlug: board.slug, identifier: row.identifier } : null
      },
    }
  }, [active, issue, openRows, boardRows])
}

/** EXP-1248: the Guide's Stack card — the band (`pr-stack` glyph, no count)
 *  over the ONE stack rail (`StackRail`): top first, the current member on
 *  the active wash, the base branch last. A member row REPLACES the subject
 *  in place; hovering one offers `Merge through here`. */
export function GuideStackCard({
  stack,
  onOpen,
  onMergeThrough,
}: {
  stack: StackView
  onOpen?: (issueId: string) => void
  onMergeThrough?: (issueId: string) => void
}) {
  const members = stack.rows.map((row) => ({
    key: row.issueId,
    identifier: row.identifier,
    title: row.title,
    current: row.isCurrent,
  }))
  return (
    <div className="flex flex-col" data-testid="guide-stack-card">
      <GlassSectionHeader
        label={STACK_CARD_TITLE}
        className="-mx-3 w-auto md:mx-0"
        leading={<PrStackIcon className="size-4 shrink-0 text-muted-foreground" aria-hidden />}
      />
      <StackRail
        className="pt-1"
        members={members}
        baseBranch={stack.baseBranch ?? `main`}
        onOpen={
          onOpen
            ? (member) => {
                if (!member.current) onOpen(member.key)
              }
            : undefined
        }
        onMergeThrough={onMergeThrough ? (member) => onMergeThrough(member.key) : undefined}
      />
    </div>
  )
}

/** The Guide's body on every width (md+ in the work column, a phone inside
 *  `GuideFace`): the Stack card, then the report over the diff. */
export function GuideBody({
  groups,
  files,
  filesState,
  numbered = true,
  loading = false,
  error = null,
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
  /** False for the PR-body fallback (one band, no `01 / 01`). */
  numbered?: boolean
  /** The PR-body fallback is still loading. */
  loading?: boolean
  error?: string | null
  /** The Stack card (`GuideStackCard`), when the PR is in one. */
  stack?: ReactNode
  /** A Changes row / Show complete diff: the section page. */
  onOpenChanges?: (target: GuideChangesTarget) => void
  onRetry?: () => void
  /** The app's GFM renderer (`renderResultText`, agent-session.tsx). */
  renderText?: (text: string) => ReactNode
}) {
  const hasDiff = (files?.length ?? 0) > 0
  const showView = groups.length > 0 || hasDiff
  const waiting =
    !showView && (loading || filesState?.kind === `loading`) && !error
  return (
    <div data-testid="guide-body">
      {stack && <div className="px-7 pt-5 md:px-5">{stack}</div>}
      {waiting && (
        <div className="flex items-center gap-2 px-7 py-6 text-sm text-muted-foreground md:px-5">
          <UiLoadingIcon className="size-4 animate-spin" />
          Loading the pull request…
        </div>
      )}
      {!showView && error && (
        <p className="px-7 py-6 text-sm text-destructive md:px-5">{error}</p>
      )}
      {!showView && filesState?.kind === `error` && (
        <div className="flex flex-wrap items-center gap-2 px-7 py-6 text-sm text-destructive md:px-5">
          {`Couldn’t load changes: ${filesState.message}`}
          {onRetry && (
            <Pill mode="action" onClick={onRetry}>
              <UiRefreshIcon className="size-3" />
              Retry
            </Pill>
          )}
        </div>
      )}
      {!showView && !waiting && !error && filesState?.kind !== `error` && (
        <p className="px-7 py-6 text-sm text-muted-foreground md:px-5">
          {GUIDE_NO_CHANGES}
        </p>
      )}
      {showView && (
        <SessionResultsView
          groups={groups}
          attachmentSrc={(id) => `/api/attachments/${id}`}
          renderText={renderText}
          files={files}
          onOpenChanges={onOpenChanges}
          numbered={numbered}
        />
      )}
    </div>
  )
}

/** The phone's Guide face of an ISSUE subject: the issue's header with the
 *  face tabs under it, the Guide (or a section page) as the pager's body, and
 *  the floating bar: the section page's file sheet (`leading`) and the white
 *  Merge capsule. */
export function GuideFace({
  issue,
  board,
  teamSlug,
  teamId,
  readOnly,
  origin,
  body,
  action,
  graphBadge,
  leading,
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
  /** `GuideBody`, or the section page (`GuideSectionDiff`). */
  body: ReactNode
  /** The header's action slot (GitHub). */
  action?: ReactNode
  graphBadge?: ReactNode
  /** The bar's leading circle (the section page's file sheet). */
  leading?: ReactNode
  /** The white Merge capsule (`MergeCapsule`), absent when the viewer may not
   *  merge. */
  merge?: ReactNode
  tabs: ReactNode
  swipe?: FaceSwipeHandlers
}) {
  const handlers = useIssuePropertyHandlers({ issue, teamSlug, readOnly })
  // EXP-1162: the header band floats over the scroller, which pads by it.
  const [headerRef, headerSize] = useMeasuredSize()
  return (
    <div className={MOBILE_DETAIL_SCREEN_CLASS} data-testid="guide-face" {...swipe}>
      <IssueMobileHeader
        issue={issue}
        board={board}
        teamSlug={teamSlug}
        teamId={teamId}
        readOnly={readOnly}
        origin={origin}
        handlers={handlers}
        face="guide"
        action={action}
        graphBadge={graphBadge}
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
      {(leading || merge) && (
        <MobileWorkBar cluster leading={leading} capsule={merge} />
      )}
      {handlers.duplicatePicker}
    </div>
  )
}

import type { ReactNode, Ref } from "react"
import { useNavigate } from "@tanstack/react-router"
import type { Board, Issue } from "@/db/schema"
import { trpc } from "@/lib/trpc-client"
import { originListNavigation, parseOrigin } from "@/lib/detail-origin"
import { CollapsedTitle } from "@exp/ui"
import { faceShowsContextMenu, type WorkFaceKind } from "@/lib/work-faces"
import { issueUrlFor } from "@/components/issue-actions-menu"
import { IssueDetailMobileMenu } from "@/components/issue-detail-mobile-menu"
import { useClosePr } from "@/components/close-pr-dialog"
import { PinToggleButton } from "@/components/pin-toggle-button"
import { MobileDetailHeader } from "@/components/team/mobile-detail-header"
import type { useIssuePropertyHandlers } from "@/hooks/use-issue-property-handlers"

// EXP-893: the phone header of an ISSUE subject, identical on every face of
// its Work screen so nothing jumps when the face flips: round back on the
// left (to the list the issue was opened from), the mono identifier
// centred, then the face's own action (Stop / Resume on the
// Run face) and the `…` (Share · Move to board · Unmark duplicate · Close PR
// · Delete).
// Lifted out of `issue-detail-view.tsx` so the session route and the changes
// face render the very same bar. EXP-1150: the face TABS ride directly under
// it (`tabs`, `MobileFaceTabs`), in the same slot on every face.

export function IssueMobileHeader({
  issue,
  board,
  teamSlug,
  teamId,
  readOnly,
  origin,
  handlers,
  action,
  graphBadge,
  face = `issue`,
  tabs,
  collapsed = face !== `issue`,
  overlay = false,
  headerRef,
}: {
  issue: Issue
  board: Board
  teamSlug: string
  teamId: string
  readOnly: boolean
  /** EXP-934: which face this bar is heading. The `…` menu (and the pin
   *  beside it) act on the ISSUE, so they show on the Issue face alone
   *  (`faceShowsContextMenu`); every other face keeps Stop / Resume alone. */
  face?: WorkFaceKind
  /** EXP-851: the `?from=` token — Back returns THERE, else to the board. */
  origin?: string
  handlers: Pick<
    ReturnType<typeof useIssuePropertyHandlers>,
    `handleBoardChange` | `handleUnmarkDuplicate`
  >
  /** The face's trailing control before the `…` (Stop / Resume). */
  action?: ReactNode
  /** EXP-897: the stack / batch pill (`PrGraphBadge`), the face's own overlay
   *  as a SHEET. It LEADS the trailing cluster: it names what this work is
   *  part of, before the controls that act on it, exactly like the md+ work
   *  header. Renders nothing when the issue is in no stack and no batch. */
  graphBadge?: ReactNode
  /** EXP-1150: the face strip INSIDE the header band, under the title row
   *  (`MobileFaceTabs`) — absent with a single face. */
  tabs?: ReactNode
  /** EXP-1162: the title row scrolled away under the band — the header
   *  breaks into identifier over title (`isTitleCollapsed`). A face with no
   *  title row of its own (Run, Changes, Results) is always collapsed. */
  collapsed?: boolean
  /** EXP-1162: the band floats over the face's scroller (`MobileDetailHeader`
   *  `overlay`); `headerRef` is the band the scroller pads by. */
  overlay?: boolean
  headerRef?: Ref<HTMLDivElement>
}) {
  const navigate = useNavigate()
  // EXP-1154: the `…` carries Close PR (EXP-248's control, once the review page's).
  const closePr = useClosePr(issue, { readOnly })

  // EXP-851 / EXP-870: back returns to the LIST this issue was opened from
  // (`originListNavigation`, the list nav's own back row), else the board.
  const goBackToList = () => {
    void navigate(
      (originListNavigation(teamSlug, parseOrigin(origin)) ?? {
        to: `/t/$teamSlug/boards/$boardSlug`,
        params: { teamSlug, boardSlug: board.slug },
        search: {},
      }) as never
    )
  }

  // Delete is a hard delete (issues.delete cleans up attachments server-side);
  // once it commits, land back on the board.
  const handleDeleteIssue = async () => {
    await trpc.issues.delete.mutate({ id: issue.id })
    void navigate({
      to: `/t/$teamSlug/boards/$boardSlug`,
      params: { teamSlug, boardSlug: board.slug },
      search: {},
    })
  }

  return (
    <>
      <MobileDetailHeader
        below={tabs}
        overlay={overlay}
        ref={headerRef}
        /* EXP-1162: no state dot on the title — the face tabs carry it
           (`faceDots`). */
        title={
          collapsed ? (
            <CollapsedTitle
              align="center"
              // Only the Issue face BREAKS; the others never showed less.
              animate={face === `issue`}
              identifier={issue.identifier}
              title={issue.title}
            />
          ) : (
            <span className="font-mono">{issue.identifier}</span>
          )
        }
        backLabel="Back"
        onBack={goBackToList}
        menu={
          <div className="flex shrink-0 items-center gap-1">
            {graphBadge}
            {action}
            {/* EXP-934: the issue's own controls, on the issue's own face. */}
            {faceShowsContextMenu(face) && (
              <>
                {/* EXP-850 §10: ghost everywhere a pin toggle renders. */}
                <PinToggleButton
                  teamId={teamId}
                  kind="issue"
                  targetId={issue.id}
                  variant="ghost"
                />
                <IssueDetailMobileMenu
                  issueTitle={issue.title}
                  issueUrl={issueUrlFor(teamSlug, board.slug, issue.identifier)}
                  teamId={teamId}
                  boardId={issue.boardId}
                  issueIdentifier={issue.identifier}
                  duplicateOfId={issue.duplicateOfId ?? null}
                  readOnly={readOnly}
                  onDelete={handleDeleteIssue}
                  onMoveBoard={handlers.handleBoardChange}
                  onUnmarkDuplicate={handlers.handleUnmarkDuplicate}
                  closePr={
                    closePr.canClose
                      ? { onSelect: closePr.request, disabled: closePr.closing }
                      : undefined
                  }
                />
              </>
            )}
          </div>
        }
      />
      {closePr.dialog}
    </>
  )
}

import type { ReactNode, Ref } from "react"
import { useNavigate } from "@tanstack/react-router"
import type { Board, Issue } from "@/db/schema"
import { originListNavigation, parseOrigin } from "@/lib/detail-origin"
import { CollapsedTitle } from "@exp/ui"
import { IssueActionsMenu } from "@/components/issue-actions-menu"
import { PinToggleButton } from "@/components/pin-toggle-button"
import { MobileDetailHeader } from "@/components/team/mobile-detail-header"
import type { useIssuePropertyHandlers } from "@/hooks/use-issue-property-handlers"

// EXP-893: the phone header of an issue: round back on the left (to the
// list the issue was opened from), the mono identifier centred, then the
// related-work badge, the pin and the `…` (Share · Move to board · Unmark
// duplicate · Close PR · Delete). EXP-1293: one face, no tabs, no Stop.

export function IssueMobileHeader({
  issue,
  board,
  teamSlug,
  teamId,
  readOnly,
  origin,
  handlers,
  graphBadge,
  collapsed = false,
  overlay = false,
  headerRef,
}: {
  issue: Issue
  board: Board
  teamSlug: string
  teamId: string
  readOnly: boolean
  /** EXP-851: the `?from=` token — Back returns THERE, else to the board. */
  origin?: string
  handlers: Pick<
    ReturnType<typeof useIssuePropertyHandlers>,
    `handleBoardChange` | `handleUnmarkDuplicate`
  >
  /** EXP-897: the stack / batch pill (`PrGraphBadge`), the face's own overlay
   *  as a SHEET. It LEADS the trailing cluster: it names what this work is
   *  part of, before the controls that act on it, exactly like the md+ work
   *  header. Renders nothing when the issue is in no stack and no batch. */
  graphBadge?: ReactNode
  /** EXP-1162: the title row scrolled away under the band — the header
   *  breaks into identifier over title (`isTitleCollapsed`). */
  collapsed?: boolean
  /** EXP-1162: the band floats over the face's scroller (`MobileDetailHeader`
   *  `overlay`); `headerRef` is the band the scroller pads by. */
  overlay?: boolean
  headerRef?: Ref<HTMLDivElement>
}) {
  const navigate = useNavigate()

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

  return (
    <>
      <MobileDetailHeader
        overlay={overlay}
        ref={headerRef}
        title={
          collapsed ? (
            <CollapsedTitle
              align="center"
              animate
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
            {/* EXP-850 §10: ghost everywhere a pin toggle renders. */}
            <PinToggleButton
              teamId={teamId}
              kind="issue"
              targetId={issue.id}
              variant="ghost"
            />
            {/* THE issue `…` (the md+ header's, a bottom sheet here):
                Share · Add relation · Move to board · Unmark duplicate
                · Close PR · Delete. */}
            <IssueActionsMenu
              issue={issue}
              board={board}
              teamSlug={teamSlug}
              teamId={teamId}
              readOnly={readOnly}
              onMoveBoard={handlers.handleBoardChange}
              onUnmarkDuplicate={handlers.handleUnmarkDuplicate}
            />
          </div>
        }
      />
    </>
  )
}

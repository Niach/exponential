import type { ReactNode } from "react"
import { useNavigate } from "@tanstack/react-router"
import type { Board, Issue } from "@/db/schema"
import { trpc } from "@/lib/trpc-client"
import { originListNavigation, parseOrigin } from "@/lib/detail-origin"
import { SESSION_DOT_CLASS, type SessionDotTone } from "@exp/ui"
import { cn } from "@/lib/utils"
import { issueUrlFor } from "@/components/issue-actions-menu"
import { IssueDetailMobileMenu } from "@/components/issue-detail-mobile-menu"
import { PinToggleButton } from "@/components/pin-toggle-button"
import { MobileDetailHeader } from "@/components/team/mobile-detail-header"
import type { useIssuePropertyHandlers } from "@/hooks/use-issue-property-handlers"

// EXP-893: the phone header of an ISSUE subject, identical on every face of
// its Work screen so nothing jumps when the face flips: round back on the
// left (to the list the issue was opened from), the session-state dot + the
// mono identifier centred, then the face's own action (Stop / Resume on the
// Run face) and the `…` (Share · Move to board · Unmark duplicate · Delete).
// Lifted out of `issue-detail-view.tsx` so the session route and the changes
// face render the very same bar.

/** The small state dot that leads a title: a session's tone, pulsing while
 *  it connects. */
export function TitleStateDot({
  tone,
  connecting = false,
}: {
  tone: SessionDotTone
  connecting?: boolean
}) {
  return (
    <span
      aria-hidden
      data-testid="work-title-dot"
      className={cn(
        `mr-1.5 inline-block size-2 shrink-0 rounded-full align-middle`,
        SESSION_DOT_CLASS[tone],
        connecting && `animate-pulse`
      )}
    />
  )
}

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
  dot,
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
  /** The face's trailing control before the `…` (Stop / Resume). */
  action?: ReactNode
  /** EXP-897: the stack / batch pill (`PrGraphBadge`), the face's own overlay
   *  as a SHEET. It LEADS the trailing cluster — it names what this work is
   *  part of, before the controls that act on it, exactly like the md+ work
   *  header. Renders nothing when the issue is in no stack and no batch. */
  graphBadge?: ReactNode
  /** The shown session's state; absent = no dot (no live run). */
  dot?: { tone: SessionDotTone; connecting?: boolean } | null
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
    <MobileDetailHeader
      title={
        <>
          {dot && <TitleStateDot tone={dot.tone} connecting={dot.connecting} />}
          <span className="font-mono">{issue.identifier}</span>
        </>
      }
      backLabel="Back"
      onBack={goBackToList}
      menu={
        <div className="flex shrink-0 items-center gap-1">
          {graphBadge}
          {action}
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
          />
        </div>
      }
    />
  )
}

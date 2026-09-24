import type { ReactNode } from "react"
import { useNavigate } from "@tanstack/react-router"
import { eq, useLiveQuery } from "@tanstack/react-db"
import type { Issue, IssueLabel } from "@/db/schema"
import { issueCollection, issueLabelCollection } from "@/lib/collections"
import { trpc } from "@/lib/trpc-client"
import { type IssuePriority } from "@/lib/domain"
import { statusUpdatePayload, type StatusRowOption } from "@/lib/team-statuses"
import { useDuplicateInterception } from "@/hooks/use-duplicate-interception"

// EXP-698 r5 / EXP-877: ONE definition per property mutation — the header
// tray, the phone sheet and the phone `…` menu are renderings of the same
// panel, and a closure that exists twice is a rule that can disagree with
// itself. Lifted out of the detail view so the session route's issue face
// mutates through the very same handlers.

export interface IssuePropertyHandlers {
  /** The issue's current label ids (the tray's own live query). */
  issueLabelIds: string[]
  handleStatusChange: (next: StatusRowOption) => void
  /** The duplicate-status interception's picker — render it ONCE per hook. */
  duplicatePicker: ReactNode
  handlePriorityChange: (priority: IssuePriority) => Promise<void>
  handleAssigneeChange: (assigneeId: string | null) => Promise<void>
  handleToggleLabel: (labelId: string) => Promise<void>
  /** `YYYY-MM-DD`, or null to clear — the wire format (REV2-49). */
  handleDueDateSelect: (date: string | null) => Promise<void>
  /** EXP-630: story points, or null to clear. */
  handleEstimateChange: (estimate: number | null) => Promise<void>
  /** EXP-57: the server renumbers the issue in the target board, so both the
   *  board slug AND the identifier change — awaits the issues txId, then hops
   *  to the issue's new canonical URL. */
  handleBoardChange: (boardId: string) => Promise<void>
  handleUnmarkDuplicate: () => void
}

export function useIssuePropertyHandlers({
  issue,
  teamSlug,
  readOnly,
}: {
  /** `null` keeps the hook unconditional in a component that may have no
   *  issue yet (the session route); every handler is then a no-op. */
  issue: Pick<Issue, `id`> | null
  teamSlug: string
  readOnly: boolean
}): IssuePropertyHandlers {
  const navigate = useNavigate()
  const issueId = issue?.id ?? null

  const { data: issueLabels } = useLiveQuery(
    (query) =>
      issueId
        ? query
            .from({ issueLabels: issueLabelCollection })
            .where(({ issueLabels }) => eq(issueLabels.issueId, issueId))
        : undefined,
    [issueId]
  )
  const issueLabelIds = ((issueLabels ?? []) as IssueLabel[]).map(
    (row) => row.labelId
  )

  const { handleStatusChange, duplicatePicker } = useDuplicateInterception({
    issueId: issueId ?? ``,
    onStatusChange: async (next) => {
      if (readOnly || !issueId) return
      await trpc.issues.update.mutate({
        id: issueId,
        ...statusUpdatePayload(next),
      })
    },
  })

  const handleBoardChange = async (boardId: string) => {
    if (readOnly || !issueId) return
    const {
      txId,
      issue: moved,
      boardSlug,
    } = await trpc.issues.move.mutate({ id: issueId, boardId })
    await issueCollection.utils.awaitTxId(txId)
    void navigate({
      to: `/t/$teamSlug/boards/$boardSlug/issues/$issueIdentifier`,
      params: {
        teamSlug,
        boardSlug,
        issueIdentifier: moved.identifier,
      },
    })
  }

  const handlePriorityChange = async (priority: IssuePriority) => {
    if (readOnly || !issueId) return
    await trpc.issues.update.mutate({ id: issueId, priority })
  }

  const handleAssigneeChange = async (assigneeId: string | null) => {
    if (readOnly || !issueId) return
    await trpc.issues.update.mutate({ id: issueId, assigneeId })
  }

  const handleToggleLabel = async (labelId: string) => {
    if (readOnly || !issueId) return
    if (issueLabelIds.includes(labelId)) {
      await trpc.issueLabels.remove.mutate({ issueId, labelId })
      return
    }
    await trpc.issueLabels.add.mutate({ issueId, labelId })
  }

  // `YYYY-MM-DD` straight through: the picker already speaks the wire format,
  // so nothing here converts a `Date` (and nothing can shift a day by a zone).
  const handleDueDateSelect = async (date: string | null) => {
    if (readOnly || !issueId) return
    await trpc.issues.update.mutate({ id: issueId, dueDate: date })
  }

  const handleEstimateChange = async (estimate: number | null) => {
    if (readOnly || !issueId) return
    await trpc.issues.update.mutate({ id: issueId, estimate })
  }

  const handleUnmarkDuplicate = () => {
    if (!issueId) return
    void trpc.issues.update.mutate({ id: issueId, duplicateOfId: null })
  }

  return {
    issueLabelIds,
    handleStatusChange,
    duplicatePicker,
    handlePriorityChange,
    handleAssigneeChange,
    handleToggleLabel,
    handleDueDateSelect,
    handleEstimateChange,
    handleBoardChange,
    handleUnmarkDuplicate,
  }
}

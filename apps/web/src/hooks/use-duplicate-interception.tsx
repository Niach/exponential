import { useCallback, useState } from "react"
import type { IssueStatus } from "@/lib/domain"
import { trpc } from "@/lib/trpc-client"
import { IssuePickerDialog } from "@/components/issue-picker-dialog"

// Duplicate = status interception (masterplan §4.1 / L27). Selecting the
// `duplicate` status from any control opens the canonical-issue picker instead
// of firing a status write; confirming links `duplicateOfId` (the server keeps
// status='duplicate' in lockstep) and cancelling leaves the control untouched
// so it reverts to the issue's current status. Any other status flows straight
// through to the caller's normal handler. Shared by every status sink so the
// interception lives in exactly one place.
export function useDuplicateInterception({
  issueId,
  onStatusChange,
}: {
  issueId: string
  onStatusChange: (status: IssueStatus) => void | Promise<void>
}) {
  const [pickerOpen, setPickerOpen] = useState(false)

  const handleStatusChange = useCallback(
    (nextStatus: IssueStatus) => {
      if (nextStatus === `duplicate`) {
        // Defer past the menu/sheet close + focus restore so the dialog's
        // focus trap doesn't fight Radix.
        setTimeout(() => setPickerOpen(true), 0)
        return
      }

      void onStatusChange(nextStatus)
    },
    [onStatusChange]
  )

  const duplicatePicker = (
    <IssuePickerDialog
      open={pickerOpen}
      onOpenChange={setPickerOpen}
      excludeIssueIds={[issueId]}
      title="Mark as duplicate"
      placeholder="Search the canonical issue…"
      onPick={(canonical) => {
        // The server sets status='duplicate' atomically with the link.
        void trpc.issues.update.mutate({
          id: issueId,
          duplicateOfId: canonical.id,
        })
      }}
    />
  )

  return { handleStatusChange, duplicatePicker }
}

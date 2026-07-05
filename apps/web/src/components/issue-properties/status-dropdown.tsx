import { Button } from "@/components/ui/button"
import { OptionDropdownMenu } from "@/components/option-dropdown-menu"
import { trpc } from "@/lib/trpc-client"
import { useDuplicateInterception } from "@/hooks/use-duplicate-interception"
import {
  getIssueStatusConfig,
  issueStatusOptions,
  type IssueStatus,
} from "@/lib/domain"

export const statuses = issueStatusOptions

export function getStatusConfig(status: IssueStatus | string) {
  return getIssueStatusConfig(status)
}

export function StatusIcon({
  status,
  className,
}: {
  className?: string
  status: IssueStatus | string
}) {
  const config = getStatusConfig(status)
  const Icon = config.icon

  return <Icon className={`h-4 w-4 ${config.color} ${className ?? ``}`} />
}

export function StatusDropdown({
  issueId,
  status,
  disabled,
}: {
  issueId: string
  status: IssueStatus
  disabled?: boolean
}) {
  const { handleStatusChange, duplicatePicker } = useDuplicateInterception({
    issueId,
    onStatusChange: async (nextStatus) => {
      await trpc.issues.update.mutate({ id: issueId, status: nextStatus })
    },
  })

  return (
    <>
      <OptionDropdownMenu
        value={status}
        disabled={disabled}
        options={statuses}
        mobileTitle="Status"
        onSelect={handleStatusChange}
        renderTrigger={(selected) => {
          const Icon = selected.icon

          return (
            <Button
              variant="ghost"
              className="h-8 w-8 md:h-5 md:w-5 p-0"
              disabled={disabled}
              aria-label={`Change status (current: ${selected.label})`}
            >
              <Icon className={`h-3.5 w-3.5 ${selected.color}`} />
            </Button>
          )
        }}
      />
      {duplicatePicker}
    </>
  )
}

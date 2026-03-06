import { Button } from "@/components/ui/button"
import { OptionDropdownMenu } from "@/components/option-dropdown-menu"
import { trpc } from "@/lib/trpc-client"
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
}: {
  issueId: string
  status: IssueStatus
}) {
  return (
    <OptionDropdownMenu
      value={status}
      options={statuses}
      onSelect={async (nextStatus) => {
        await trpc.issues.update.mutate({
          id: issueId,
          status: nextStatus,
        })
      }}
      renderTrigger={(selected) => {
        const Icon = selected.icon

        return (
          <Button variant="ghost" className="h-5 w-5 p-0">
            <Icon className={`h-3.5 w-3.5 ${selected.color}`} />
          </Button>
        )
      }}
    />
  )
}

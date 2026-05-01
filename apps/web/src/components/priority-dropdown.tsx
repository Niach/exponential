import { Button } from "@/components/ui/button"
import { OptionDropdownMenu } from "@/components/option-dropdown-menu"
import { trpc } from "@/lib/trpc-client"
import {
  getIssuePriorityConfig,
  issuePriorityOptions,
  type IssuePriority,
} from "@/lib/domain"

export const priorities = issuePriorityOptions

export function getPriorityConfig(priority: IssuePriority | string) {
  return getIssuePriorityConfig(priority)
}

export function PriorityIcon({
  priority,
  className,
}: {
  className?: string
  priority: IssuePriority | string
}) {
  const config = getPriorityConfig(priority)
  const Icon = config.icon

  return <Icon className={`h-4 w-4 ${config.color} ${className ?? ``}`} />
}

export function PriorityDropdown({
  issueId,
  priority,
}: {
  issueId: string
  priority: IssuePriority
}) {
  return (
    <OptionDropdownMenu
      value={priority}
      options={priorities}
      onSelect={async (nextPriority) => {
        await trpc.issues.update.mutate({
          id: issueId,
          priority: nextPriority,
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

import { Button, PriorityPicker } from "@exp/ui"
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
  disabled,
}: {
  issueId: string
  priority: IssuePriority
  disabled?: boolean
}) {
  // EXP-958: the trigger draws the CONFIG row, not the picker's matched
  // option — `getPriorityConfig` already falls back to the lifecycle start of
  // the vocabulary for an unknown/forward-compat value (REV2-85), where a
  // match against the display-ordered table would be empty.
  const current = getPriorityConfig(priority)
  const TriggerIcon = current.icon

  return (
    <PriorityPicker
      options={priorities}
      value={priority}
      disabled={disabled}
      mobileTitle="Priority"
      width="sm"
      onChange={async (nextPriority) => {
        await trpc.issues.update.mutate({
          id: issueId,
          priority: nextPriority as IssuePriority,
        })
      }}
      trigger={
        <Button
          variant="ghost"
          className="h-8 w-8 md:h-5 md:w-5 p-0"
          disabled={disabled}
          aria-label={`Change priority (current: ${current.label})`}
        >
          <TriggerIcon className={`h-3.5 w-3.5 ${current.color}`} />
        </Button>
      }
    />
  )
}

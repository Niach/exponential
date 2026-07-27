import type { CSSProperties } from "react"
import { Button } from "@/components/ui/button"
import { OptionDropdownMenu } from "@/components/option-dropdown-menu"
import { trpc } from "@/lib/trpc-client"
import { useDuplicateInterception } from "@/hooks/use-duplicate-interception"
import { useTeamStatusesContext } from "@/hooks/use-team-statuses"
import { ICON_COMPONENTS } from "@/lib/icons.generated"
import {
  statusUpdatePayload,
  type StatusResolvable,
  type StatusRowOption,
} from "@/lib/team-statuses"
import {
  getIssueStatusConfig,
  type IssueOption,
  type IssueStatus,
} from "@/lib/domain"

// EXP-314 color contract: BUILTIN rows (and the constructed fallbacks) render
// today's Tailwind token classes keyed on the builtin key, so nothing about
// the default team's look changes. CUSTOM rows render their synced hex inline
// — the same treatment labels get.
export function statusColorClass(option: StatusRowOption): string {
  return option.builtinKey ? getIssueStatusConfig(option.builtinKey).color : ``
}

export function statusColorStyle(
  option: StatusRowOption
): CSSProperties | undefined {
  return option.builtinKey ? undefined : { color: option.colorHex }
}

/** The glyph for an already-resolved status row. */
export function StatusIcon({
  option,
  className,
}: {
  option: StatusRowOption
  className?: string
}) {
  const Icon = ICON_COMPONENTS[option.icon]
  return (
    <Icon
      className={`h-4 w-4 ${statusColorClass(option)} ${className ?? ``}`}
      style={statusColorStyle(option)}
    />
  )
}

/** The glyph for an issue, resolved against the team's synced status rows. */
export function IssueStatusIcon({
  issue,
  className,
}: {
  issue: StatusResolvable
  className?: string
}) {
  const { resolve } = useTeamStatusesContext()
  return <StatusIcon option={resolve(issue)} className={className} />
}

/**
 * Adapt a status row to the shared `IssueOption` menu vocabulary. `value` is
 * the row id (or `builtin:<key>` for a constructed fallback), which is what
 * every picker's `onSelect` hands back.
 */
export function toStatusMenuOption(
  option: StatusRowOption
): IssueOption<string> {
  return {
    value: option.id,
    label: option.name,
    icon: ICON_COMPONENTS[option.icon],
    color: statusColorClass(option),
    colorHex: option.builtinKey ? undefined : option.colorHex,
  }
}

export function toStatusMenuOptions(
  options: readonly StatusRowOption[]
): IssueOption<string>[] {
  return options.map(toStatusMenuOption)
}

export function StatusDropdown({
  issueId,
  status,
  statusId,
  disabled,
}: {
  issueId: string
  status: IssueStatus
  statusId: string | null
  disabled?: boolean
}) {
  const { options, byId, resolve } = useTeamStatusesContext()
  const current = resolve({ status, statusId })

  const { handleStatusChange, duplicatePicker } = useDuplicateInterception({
    issueId,
    onStatusChange: async (next) => {
      await trpc.issues.update.mutate({
        id: issueId,
        ...statusUpdatePayload(next),
      })
    },
  })

  const menuOptions = toStatusMenuOptions(options)

  return (
    <>
      <OptionDropdownMenu
        value={current.id}
        fallbackValue={current.id}
        disabled={disabled}
        options={menuOptions}
        mobileTitle="Status"
        onSelect={(id) => {
          const picked = byId.get(id)
          if (picked) handleStatusChange(picked)
        }}
        renderTrigger={(selected) => {
          const Icon = selected.icon

          return (
            <Button
              variant="ghost"
              className="h-8 w-8 md:h-5 md:w-5 p-0"
              disabled={disabled}
              aria-label={`Change status (current: ${selected.label})`}
            >
              <Icon
                className={`h-3.5 w-3.5 ${selected.color}`}
                style={selected.colorHex ? { color: selected.colorHex } : undefined}
              />
            </Button>
          )
        }}
      />
      {duplicatePicker}
    </>
  )
}

import type { CSSProperties } from "react"
import { Button, Combobox, ICON_COMPONENTS, StatusGlyph } from "@exp/ui"
import { trpc } from "@/lib/trpc-client"
import { useDuplicateInterception } from "@/hooks/use-duplicate-interception"
import { useTeamStatusesContext } from "@/hooks/use-team-statuses"
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

/**
 * The same color as a CONCRETE CSS value, for the surfaces that cannot take a
 * Tailwind class — today the `#IDENTIFIER` pill, whose glyph is a `::before`
 * mask tinted through a custom property (EXP-423).
 *
 * Builtin rows derive from their token class rather than a second color table:
 * `text-foreground`/`text-muted-foreground` are theme vars, everything else is
 * a Tailwind palette var (`text-yellow-500` → `var(--color-yellow-500)`).
 * Tailwind v4 only emits palette vars for palette utilities in use — the three
 * builtin status colors (yellow-500, green-500, blue-500) all are, via the
 * status pickers, and the pill's CSS carries a `--muted-foreground` fallback
 * either way.
 */
export function statusColorCssValue(option: StatusRowOption): string {
  if (!option.builtinKey) return option.colorHex
  const token = getIssueStatusConfig(option.builtinKey).color.replace(
    /^text-/,
    ``
  )
  return token === `foreground` || token === `muted-foreground`
    ? `var(--${token})`
    : `var(--color-${token})`
}

/** The glyph for an already-resolved status row. The DRAWING is `StatusGlyph`
 *  in @exp/ui (icon + colour class or hex, nothing else); this is the half
 *  that knows what a `StatusRowOption` is. */
export function StatusIcon({
  option,
  className,
}: {
  option: StatusRowOption
  className?: string
}) {
  return (
    <StatusGlyph
      icon={option.icon}
      colorClass={statusColorClass(option)}
      colorHex={option.builtinKey ? undefined : option.colorHex}
      className={`h-4 w-4 ${className ?? ``}`}
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
 * Adapt a status row to the shared `IssueOption` picker vocabulary. `value` is
 * the row id (or `builtin:<key>` for a constructed fallback), which is what
 * every picker's `onChange` hands back.
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
  // EXP-958: the trigger draws the issue's RESOLVED row, never the picker's
  // matched option — the resolver already falls back for an unknown value
  // (REV2-85), and some menus leave the current row out entirely (a duplicate
  // issue inside `creatableStatusOptions`), where a match would be empty.
  const trigger = toStatusMenuOption(current)
  const TriggerIcon = trigger.icon

  return (
    <>
      <Combobox
        searchable={false}
        value={current.id}
        disabled={disabled}
        options={menuOptions}
        mobileTitle="Status"
        width="sm"
        onChange={(id) => {
          if (!id) return
          const picked = byId.get(id)
          if (picked) handleStatusChange(picked)
        }}
        renderTrigger={() => (
          <Button
            variant="ghost"
            className="h-8 w-8 md:h-5 md:w-5 p-0"
            disabled={disabled}
            aria-label={`Change status (current: ${trigger.label})`}
          >
            <TriggerIcon
              className={`h-3.5 w-3.5 ${trigger.color}`}
              style={trigger.colorHex ? { color: trigger.colorHex } : undefined}
            />
          </Button>
        )}
      />
      {duplicatePicker}
    </>
  )
}

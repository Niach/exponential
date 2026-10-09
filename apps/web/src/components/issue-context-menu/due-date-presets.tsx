import type { Issue } from "@/db/schema"
import {
  formatDueDateMenuMeta,
  getDueDatePresets,
  matchesDueDateValue,
} from "@/lib/issue-due-date"
import { formatDate } from "@/lib/utils"
import type { IssueMenuSlot, MenuEntry } from "@exp/ui"

/** The "Set due date" slot of the issue menu: the presets as rows (a square
 *  marker on the one the issue is set to, the date at the trailing edge),
 *  then "Clear due date" while one is set. */
export function dueDateSlot({
  dueDate,
  onApplyDueDate,
}: {
  dueDate: Issue[`dueDate`]
  onApplyDueDate: (date: Date | null) => void
}): IssueMenuSlot {
  const entries: MenuEntry[] = getDueDatePresets(new Date()).map((preset) => ({
    kind: `item`,
    id: preset.id,
    label: preset.label,
    icon: (
      <DueDatePresetIndicator active={matchesDueDateValue(preset.date, dueDate)} />
    ),
    value: (
      <span className="tabular-nums">{formatDueDateMenuMeta(preset.date)}</span>
    ),
    onSelect: () => onApplyDueDate(preset.date),
  }))
  if (dueDate) {
    entries.push(
      { kind: `separator` },
      {
        kind: `item`,
        id: `clear`,
        label: `Clear due date`,
        icon: <DueDatePresetIndicator active={false} muted />,
        value: `Remove`,
        onSelect: () => onApplyDueDate(null),
      }
    )
  }
  return {
    value: (
      <span className="tabular-nums">{dueDate ? formatDate(dueDate) : `None`}</span>
    ),
    contentClassName: `w-[15.5rem]`,
    entries,
  }
}

function DueDatePresetIndicator({
  active,
  muted,
}: {
  active: boolean
  muted?: boolean
}) {
  return (
    <span
      className={`flex size-4 shrink-0 items-center justify-center rounded-[5px] border ${
        active
          ? `border-cyan-400/70 bg-cyan-400/14`
          : muted
            ? `border-border/50 bg-transparent`
            : `border-border/70 bg-background/60`
      }`}
    >
      {active && <span className="size-1.5 rounded-full bg-cyan-300" />}
    </span>
  )
}

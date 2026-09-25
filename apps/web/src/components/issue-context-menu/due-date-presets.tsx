import { CalendarDays } from "lucide-react"
import type { Issue } from "@/db/schema"
import {
  formatDueDateMenuMeta,
  getDueDatePresets,
  matchesDueDateValue,
} from "@/lib/issue-due-date"
import { formatDate } from "@/lib/utils"
import {
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuShortcut,
  DropdownMenuSub,
  DropdownMenuSubContent,
  DropdownMenuSubTrigger,
} from "@exp/ui"

interface DueDateSubmenuProps {
  dueDate: Issue[`dueDate`]
  topLevelValueClass: string
  onApplyDueDate: (date: Date | null) => void
}

export function DueDateSubmenu({
  dueDate,
  topLevelValueClass,
  onApplyDueDate,
}: DueDateSubmenuProps) {
  const dueDatePresets = getDueDatePresets(new Date())
  const dueDateLabel = dueDate ? formatDate(dueDate) : `None`

  return (
    <DropdownMenuSub>
      <DropdownMenuSubTrigger>
        <CalendarDays />
        Set due date
        <DropdownMenuShortcut className={`${topLevelValueClass} tabular-nums`}>
          {dueDateLabel}
        </DropdownMenuShortcut>
      </DropdownMenuSubTrigger>
      <DropdownMenuSubContent className="w-[15.5rem] p-1">
        {dueDatePresets.map((preset) => (
          <DropdownMenuItem
            className="gap-3"
            key={preset.id}
            onSelect={() => {
              onApplyDueDate(preset.date)
            }}
          >
            <DueDatePresetIndicator
              active={matchesDueDateValue(preset.date, dueDate)}
            />
            <span>{preset.label}</span>
            <DropdownMenuShortcut className="min-w-[5.125rem] text-right normal-case tracking-normal tabular-nums">
              {formatDueDateMenuMeta(preset.date)}
            </DropdownMenuShortcut>
          </DropdownMenuItem>
        ))}

        {dueDate && (
          <>
            <DropdownMenuSeparator />
            <DropdownMenuItem
              className="gap-3"
              onSelect={() => {
                onApplyDueDate(null)
              }}
            >
              <DueDatePresetIndicator active={false} muted />
              Clear due date
              <DropdownMenuShortcut className="min-w-[5.125rem] text-right normal-case tracking-normal">
                Remove
              </DropdownMenuShortcut>
            </DropdownMenuItem>
          </>
        )}
      </DropdownMenuSubContent>
    </DropdownMenuSub>
  )
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

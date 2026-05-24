import { CalendarDays } from "lucide-react"
import type { Issue } from "@/db/schema"
import {
  formatDueDateMenuMeta,
  getDueDatePresets,
  matchesDueDateValue,
} from "@/lib/issue-due-date"
import { formatDate } from "@/lib/utils"
import {
  ContextMenuItem,
  ContextMenuSeparator,
  ContextMenuShortcut,
  ContextMenuSub,
  ContextMenuSubContent,
  ContextMenuSubTrigger,
} from "@/components/ui/context-menu"

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
    <ContextMenuSub>
      <ContextMenuSubTrigger>
        <CalendarDays className="size-4" />
        Set due date
        <ContextMenuShortcut className={`${topLevelValueClass} tabular-nums`}>
          {dueDateLabel}
        </ContextMenuShortcut>
      </ContextMenuSubTrigger>
      <ContextMenuSubContent className="w-[15.5rem] p-1">
        {dueDatePresets.map((preset) => (
          <ContextMenuItem
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
            <ContextMenuShortcut className="min-w-[5.125rem] text-right normal-case tracking-normal tabular-nums">
              {formatDueDateMenuMeta(preset.date)}
            </ContextMenuShortcut>
          </ContextMenuItem>
        ))}

        {dueDate && (
          <>
            <ContextMenuSeparator />
            <ContextMenuItem
              className="gap-3"
              onSelect={() => {
                onApplyDueDate(null)
              }}
            >
              <DueDatePresetIndicator active={false} muted />
              Clear due date
              <ContextMenuShortcut className="min-w-[5.125rem] text-right normal-case tracking-normal">
                Remove
              </ContextMenuShortcut>
            </ContextMenuItem>
          </>
        )}
      </ContextMenuSubContent>
    </ContextMenuSub>
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

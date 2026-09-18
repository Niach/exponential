import type { ReactNode } from "react"
import { Ellipsis } from "lucide-react"
import type { User } from "@/db/schema"
import { ISSUE_PRIORITY_FALLBACK, type IssuePriority } from "@/lib/domain"
import { useTeamStatusesContext } from "@/hooks/use-team-statuses"
import {
  creatableStatusOptions,
  type StatusRowOption,
} from "@/lib/team-statuses"
import { AssigneePicker } from "@/components/issue-properties/assignee-picker"
import { LabelPicker } from "@/components/issue-properties/label-picker"
import {
  priorities,
  PriorityIcon,
} from "@/components/issue-properties/priority-dropdown"
import { toStatusMenuOptions } from "@/components/issue-properties/status-dropdown"
import {
  OptionDropdownMenu,
  Button,
  conceptIcon,
  DatePicker,
  Pill,
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuTrigger,
} from "@exp/ui"

const DueDateGlyph = conceptIcon(`ui-due-date`)

export interface IssueEditorChipsProps {
  // EXP-314: the RESOLVED team status row (never a bare enum) so a custom
  // status survives a round-trip through the editor.
  status: StatusRowOption
  priority: IssuePriority
  assigneeId: string | null
  selectedLabelIds: string[]
  teamId: string
  users: User[]
  /** `YYYY-MM-DD`, or null for "no due date" (REV2-49: no time of day). */
  dueDate: string | null
  hideAssignee?: boolean
  hideDueDateChip?: boolean
  disableStatus?: boolean
  disabled?: boolean
  chipRowExtras?: ReactNode
  overflowMenuItems?: ReactNode
  onStatusChange: (status: StatusRowOption) => void | Promise<void>
  onPriorityChange: (priority: IssuePriority) => void | Promise<void>
  onAssigneeChange: (userId: string | null) => void | Promise<void>
  onToggleLabel: (labelId: string) => void | Promise<void>
  onDueDateSelect: (date: string | null) => void | Promise<void>
}

export function IssueEditorChips({
  status,
  priority,
  assigneeId,
  selectedLabelIds,
  teamId,
  users,
  dueDate,
  hideAssignee,
  hideDueDateChip,
  disableStatus,
  disabled,
  chipRowExtras,
  overflowMenuItems,
  onStatusChange,
  onPriorityChange,
  onAssigneeChange,
  onToggleLabel,
  onDueDateSelect,
}: IssueEditorChipsProps) {
  const { options, byId } = useTeamStatusesContext()
  const statusOptions = creatableStatusOptions(options)

  return (
    <>
      <OptionDropdownMenu
        value={status.id}
        fallbackValue={status.id}
        disabled={disabled || disableStatus}
        options={toStatusMenuOptions(statusOptions)}
        onSelect={(id) => {
          const picked = byId.get(id)
          if (picked) void onStatusChange(picked)
        }}
        mobileTitle="Status"
        renderTrigger={(selected) => {
          const Icon = selected.icon
          return (
            <Pill
              mode="action"
              disabled={disabled || disableStatus}
              leading={
                <Icon
                  className={`!h-3 !w-3 ${selected.color}`}
                  style={
                    selected.colorHex ? { color: selected.colorHex } : undefined
                  }
                />
              }
            >
              {selected.label}
            </Pill>
          )
        }}
      />

      <OptionDropdownMenu
        value={priority}
        fallbackValue={ISSUE_PRIORITY_FALLBACK}
        disabled={disabled}
        options={priorities}
        onSelect={onPriorityChange}
        mobileTitle="Priority"
        renderTrigger={(selected) => (
          <Pill
            mode="action"
            disabled={disabled}
            leading={
              <PriorityIcon priority={selected.value} className="!h-3 !w-3" />
            }
          >
            {selected.label}
          </Pill>
        )}
      />

      {!hideAssignee && (
        <AssigneePicker
          disabled={disabled}
          users={users}
          selectedUserId={assigneeId}
          onSelect={onAssigneeChange}
        />
      )}

      <LabelPicker
        disabled={disabled}
        teamId={teamId}
        selectedLabelIds={selectedLabelIds}
        onToggle={onToggleLabel}
      />

      {!hideDueDateChip && (
        <DatePicker
          value={dueDate}
          onChange={(date) => {
            void onDueDateSelect(date)
          }}
          disabled={disabled}
          align="start"
          renderTrigger={({ label }) => (
            <Pill
              mode="action"
              disabled={disabled}
              leading={<DueDateGlyph className="size-3" />}
            >
              {label}
            </Pill>
          )}
        />
      )}

      {chipRowExtras}

      {overflowMenuItems && (
        <DropdownMenu>
          <DropdownMenuTrigger asChild>
            <Button
              type="button"
              variant="ghost"
              size="icon-xs"
              aria-label="More options"
              disabled={disabled}
              className="text-muted-foreground shrink-0"
            >
              <Ellipsis className="size-3" />
            </Button>
          </DropdownMenuTrigger>
          <DropdownMenuContent align="start">
            {overflowMenuItems}
          </DropdownMenuContent>
        </DropdownMenu>
      )}
    </>
  )
}

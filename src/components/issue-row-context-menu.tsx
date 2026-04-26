import type { ReactNode } from "react"
import type { Issue, Label, User } from "@/db/schema"
import {
  formatDateForMutation,
  issuePriorityOptions,
  issueStatusOptions,
} from "@/lib/domain"
import {
  formatDueDateMenuMeta,
  getDueDatePresets,
  matchesDueDateValue,
} from "@/lib/issue-due-date"
import { trpc } from "@/lib/trpc-client"
import { formatDate, getInitials } from "@/lib/utils"
import { Avatar, AvatarFallback, AvatarImage } from "@/components/ui/avatar"
import {
  ContextMenu,
  ContextMenuCheckboxItem,
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuLabel,
  ContextMenuRadioGroup,
  ContextMenuRadioItem,
  ContextMenuSeparator,
  ContextMenuShortcut,
  ContextMenuSub,
  ContextMenuSubContent,
  ContextMenuSubTrigger,
  ContextMenuTrigger,
} from "@/components/ui/context-menu"
import {
  CalendarDays,
  CheckCheck,
  Copy,
  Flag,
  ListTodo,
  SquarePen,
  Tag,
  UserRound,
  X,
} from "lucide-react"

interface IssueRowContextMenuProps {
  children: ReactNode
  issue: Issue
  issueLabels: Label[]
  labels: Label[]
  onOpenIssue: () => void
  userMap: Map<string, User>
  users: User[]
}

export function IssueRowContextMenu({
  children,
  issue,
  issueLabels,
  labels,
  onOpenIssue,
  userMap,
  users,
}: IssueRowContextMenuProps) {
  const selectedAssignee = issue.assigneeId
    ? (userMap.get(issue.assigneeId) ?? null)
    : null
  const selectedLabelIds = new Set(issueLabels.map((label) => label.id))
  const orderedUsers = [...users].sort((left, right) => {
    if (left.id === issue.assigneeId) {
      return -1
    }

    if (right.id === issue.assigneeId) {
      return 1
    }

    return left.name.localeCompare(right.name)
  })
  const dueDatePresets = getDueDatePresets(new Date())

  const updateIssue = async (updates: {
    assigneeId?: Issue[`assigneeId`]
    dueDate?: Issue[`dueDate`]
    priority?: Issue[`priority`]
    status?: Issue[`status`]
  }) => {
    await trpc.issues.update.mutate({
      id: issue.id,
      ...updates,
    })
  }

  const applyDueDate = async (date: Date | null | undefined) => {
    await updateIssue({
      dueDate: formatDateForMutation(date),
    })
  }

  const toggleLabel = async (labelId: string) => {
    if (selectedLabelIds.has(labelId)) {
      await trpc.issueLabels.remove.mutate({
        issueId: issue.id,
        labelId,
      })
      return
    }

    await trpc.issueLabels.add.mutate({
      issueId: issue.id,
      labelId,
    })
  }

  const copyText = async (value: string) => {
    if (typeof navigator === `undefined` || !navigator.clipboard) {
      return
    }

    await navigator.clipboard.writeText(value)
  }

  const dueDateLabel = issue.dueDate ? formatDate(issue.dueDate) : `None`
  const statusLabel =
    issueStatusOptions.find((option) => option.value === issue.status)?.label ??
    `Status`
  const priorityLabel =
    issuePriorityOptions.find((option) => option.value === issue.priority)
      ?.label ?? `Priority`
  const labelsLabel =
    issueLabels.length > 0 ? `${issueLabels.length} selected` : `None`
  const topLevelValueClass = `w-[5.75rem] shrink-0 text-right normal-case tracking-normal truncate`

  return (
    <ContextMenu>
      <ContextMenuTrigger asChild>{children}</ContextMenuTrigger>
      <ContextMenuContent
        className="w-[17.5rem] rounded-xl border-border/60 bg-popover/95 p-1.5 shadow-2xl supports-[backdrop-filter]:bg-popover/90"
        collisionPadding={12}
      >
        <ContextMenuLabel className="rounded-lg bg-accent/40 px-3 py-2.5">
          <div className="flex items-center justify-between gap-3">
            <div className="min-w-0">
              <div className="truncate text-xs font-mono uppercase tracking-[0.24em] text-muted-foreground">
                {issue.identifier}
              </div>
              <div className="truncate text-sm font-medium text-foreground">
                {issue.title}
              </div>
            </div>
          </div>
        </ContextMenuLabel>

        <ContextMenuSeparator />

        <ContextMenuItem onSelect={onOpenIssue}>
          <SquarePen className="size-4" />
          Open issue
        </ContextMenuItem>

        <ContextMenuItem
          onSelect={() => {
            void updateIssue({
              status: issue.status === `done` ? `todo` : `done`,
            })
          }}
        >
          {issue.status === `done` ? (
            <ListTodo className="size-4" />
          ) : (
            <CheckCheck className="size-4" />
          )}
          {issue.status === `done` ? `Move to todo` : `Mark as done`}
        </ContextMenuItem>

        <ContextMenuItem
          onSelect={() => {
            void copyText(issue.identifier)
          }}
        >
          <Copy className="size-4" />
          Copy issue ID
        </ContextMenuItem>

        <ContextMenuSeparator />

        <ContextMenuSub>
          <ContextMenuSubTrigger>
            <ListTodo className="size-4" />
            Status
            <ContextMenuShortcut className={topLevelValueClass}>
              {statusLabel}
            </ContextMenuShortcut>
          </ContextMenuSubTrigger>
          <ContextMenuSubContent className="w-[14rem]">
            <ContextMenuRadioGroup value={issue.status}>
              {issueStatusOptions.map((option) => {
                const Icon = option.icon

                return (
                  <ContextMenuRadioItem
                    key={option.value}
                    value={option.value}
                    onSelect={() => {
                      void updateIssue({ status: option.value })
                    }}
                  >
                    <Icon className={`size-4 ${option.color}`} />
                    {option.label}
                  </ContextMenuRadioItem>
                )
              })}
            </ContextMenuRadioGroup>
          </ContextMenuSubContent>
        </ContextMenuSub>

        <ContextMenuSub>
          <ContextMenuSubTrigger>
            <UserRound className="size-4" />
            Assignee
            <ContextMenuShortcut className={topLevelValueClass}>
              {selectedAssignee?.name ?? `Unassigned`}
            </ContextMenuShortcut>
          </ContextMenuSubTrigger>
          <ContextMenuSubContent className="w-[15rem]">
            <ContextMenuRadioGroup value={issue.assigneeId ?? `__unassigned__`}>
              <ContextMenuRadioItem
                value="__unassigned__"
                onSelect={() => {
                  void updateIssue({ assigneeId: null })
                }}
              >
                <X className="size-4 text-muted-foreground" />
                Unassigned
              </ContextMenuRadioItem>

              {orderedUsers.length === 0 ? (
                <ContextMenuItem disabled inset>
                  No team members yet
                </ContextMenuItem>
              ) : (
                orderedUsers.map((user) => (
                  <ContextMenuRadioItem
                    key={user.id}
                    value={user.id}
                    onSelect={() => {
                      void updateIssue({ assigneeId: user.id })
                    }}
                  >
                    <Avatar className="size-5">
                      {user.image && (
                        <AvatarImage src={user.image} alt={user.name} />
                      )}
                      <AvatarFallback className="text-[0.5625rem]">
                        {getInitials(user.name)}
                      </AvatarFallback>
                    </Avatar>
                    <span className="truncate">{user.name}</span>
                  </ContextMenuRadioItem>
                ))
              )}
            </ContextMenuRadioGroup>
          </ContextMenuSubContent>
        </ContextMenuSub>

        <ContextMenuSub>
          <ContextMenuSubTrigger>
            <Flag className="size-4" />
            Priority
            <ContextMenuShortcut className={topLevelValueClass}>
              {priorityLabel}
            </ContextMenuShortcut>
          </ContextMenuSubTrigger>
          <ContextMenuSubContent className="w-[14rem]">
            <ContextMenuRadioGroup value={issue.priority}>
              {issuePriorityOptions.map((option) => {
                const Icon = option.icon

                return (
                  <ContextMenuRadioItem
                    key={option.value}
                    value={option.value}
                    onSelect={() => {
                      void updateIssue({ priority: option.value })
                    }}
                  >
                    <Icon className={`size-4 ${option.color}`} />
                    {option.label}
                  </ContextMenuRadioItem>
                )
              })}
            </ContextMenuRadioGroup>
          </ContextMenuSubContent>
        </ContextMenuSub>

        <ContextMenuSub>
          <ContextMenuSubTrigger>
            <Tag className="size-4" />
            Labels
            <ContextMenuShortcut className={topLevelValueClass}>
              {labelsLabel}
            </ContextMenuShortcut>
          </ContextMenuSubTrigger>
          <ContextMenuSubContent className="w-[15rem]">
            {labels.length === 0 ? (
              <ContextMenuItem disabled inset>
                No labels yet
              </ContextMenuItem>
            ) : (
              labels.map((label) => (
                <ContextMenuCheckboxItem
                  key={label.id}
                  checked={selectedLabelIds.has(label.id)}
                  onSelect={(event) => {
                    event.preventDefault()
                    void toggleLabel(label.id)
                  }}
                >
                  <div
                    className="size-2.5 rounded-full shrink-0"
                    style={{ backgroundColor: label.color }}
                  />
                  <span className="truncate">{label.name}</span>
                </ContextMenuCheckboxItem>
              ))
            )}
          </ContextMenuSubContent>
        </ContextMenuSub>

        <ContextMenuSub>
          <ContextMenuSubTrigger>
            <CalendarDays className="size-4" />
            Set due date
            <ContextMenuShortcut
              className={`${topLevelValueClass} tabular-nums`}
            >
              {dueDateLabel}
            </ContextMenuShortcut>
          </ContextMenuSubTrigger>
          <ContextMenuSubContent className="w-[15.5rem] p-1">
            {dueDatePresets.map((preset) => (
              <ContextMenuItem
                className="gap-3"
                key={preset.id}
                onSelect={() => {
                  void applyDueDate(preset.date)
                }}
              >
                <DueDatePresetIndicator
                  active={matchesDueDateValue(preset.date, issue.dueDate)}
                />
                <span>{preset.label}</span>
                <ContextMenuShortcut className="min-w-[5.125rem] text-right normal-case tracking-normal tabular-nums">
                  {formatDueDateMenuMeta(preset.date)}
                </ContextMenuShortcut>
              </ContextMenuItem>
            ))}

            {issue.dueDate && (
              <>
                <ContextMenuSeparator />
                <ContextMenuItem
                  className="gap-3"
                  onSelect={() => {
                    void applyDueDate(null)
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
      </ContextMenuContent>
    </ContextMenu>
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

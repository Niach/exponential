import { useState, type ReactNode } from "react"
import type { Issue, Label, User } from "@/db/schema"
import { formatDateForMutation } from "@/lib/domain"
import { trpc } from "@/lib/trpc-client"
import { IssuePickerDialog } from "@/components/issue-picker-dialog"
import {
  ContextMenu,
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuLabel,
  ContextMenuSeparator,
  ContextMenuSub,
  ContextMenuSubContent,
  ContextMenuSubTrigger,
  ContextMenuTrigger,
} from "@/components/ui/context-menu"
import {
  CheckCheck,
  Copy,
  Files,
  ListTodo,
  SquarePen,
  Trash2,
  Undo2,
} from "lucide-react"
import { DueDateSubmenu } from "./due-date-presets"
import {
  AssigneeSubmenu,
  LabelsSubmenu,
  PrioritySubmenu,
  StatusSubmenu,
} from "./submenus"

interface IssueRowContextMenuProps {
  children: ReactNode
  issue: Issue
  issueLabels: Label[]
  labels: Label[]
  onOpenIssue: () => void
  userMap: Map<string, User>
  users: User[]
}

const TOP_LEVEL_VALUE_CLASS = `w-[5.75rem] shrink-0 text-right normal-case tracking-normal truncate`

export function IssueRowContextMenu({
  children,
  issue,
  issueLabels,
  labels,
  onOpenIssue,
  userMap,
  users,
}: IssueRowContextMenuProps) {
  const [duplicatePickerOpen, setDuplicatePickerOpen] = useState(false)
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

  const updateIssue = async (updates: {
    assigneeId?: Issue[`assigneeId`]
    dueDate?: Issue[`dueDate`]
    duplicateOfId?: Issue[`duplicateOfId`]
    priority?: Issue[`priority`]
    status?: Issue[`status`]
  }) => {
    await trpc.issues.update.mutate({
      id: issue.id,
      ...updates,
    })
  }

  const applyDueDate = (date: Date | null | undefined) => {
    void updateIssue({
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

  const deleteIssue = async () => {
    await trpc.issues.delete.mutate({ id: issue.id })
  }

  const copyText = async (value: string) => {
    if (typeof navigator === `undefined` || !navigator.clipboard) {
      return
    }

    await navigator.clipboard.writeText(value)
  }

  return (
    <>
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

          {issue.duplicateOfId ? (
            <ContextMenuItem
              onSelect={() => {
                // Server restores 'backlog' and clears the link atomically.
                void updateIssue({ duplicateOfId: null })
              }}
            >
              <Undo2 className="size-4" />
              Unmark duplicate
            </ContextMenuItem>
          ) : (
            <ContextMenuItem
              onSelect={() => {
                // Defer past the menu's close/focus-restore so the dialog's
                // focus trap doesn't fight Radix.
                setTimeout(() => setDuplicatePickerOpen(true), 0)
              }}
            >
              <Files className="size-4" />
              Mark as duplicate…
            </ContextMenuItem>
          )}

          <ContextMenuSeparator />

          <StatusSubmenu
            status={issue.status}
            topLevelValueClass={TOP_LEVEL_VALUE_CLASS}
            onSelect={(status) => void updateIssue({ status })}
          />

          <AssigneeSubmenu
            assigneeId={issue.assigneeId}
            orderedUsers={orderedUsers}
            selectedAssignee={selectedAssignee}
            topLevelValueClass={TOP_LEVEL_VALUE_CLASS}
            onSelect={(userId) => void updateIssue({ assigneeId: userId })}
          />

          <PrioritySubmenu
            priority={issue.priority}
            topLevelValueClass={TOP_LEVEL_VALUE_CLASS}
            onSelect={(priority) => void updateIssue({ priority })}
          />

          <LabelsSubmenu
            labels={labels}
            selectedLabelIds={selectedLabelIds}
            topLevelValueClass={TOP_LEVEL_VALUE_CLASS}
            onToggle={(labelId) => void toggleLabel(labelId)}
          />

          <DueDateSubmenu
            dueDate={issue.dueDate}
            topLevelValueClass={TOP_LEVEL_VALUE_CLASS}
            onApplyDueDate={applyDueDate}
          />

          <ContextMenuSeparator />

          <ContextMenuSub>
            <ContextMenuSubTrigger className="text-destructive focus:bg-destructive/10 focus:text-destructive data-[state=open]:bg-destructive/10 data-[state=open]:text-destructive [&_svg]:text-destructive!">
              <Trash2 className="size-4" />
              Delete issue
            </ContextMenuSubTrigger>
            <ContextMenuSubContent className="w-[14rem]">
              <ContextMenuItem
                variant="destructive"
                onSelect={() => {
                  void deleteIssue()
                }}
              >
                <Trash2 className="size-4" />
                Confirm delete
              </ContextMenuItem>
            </ContextMenuSubContent>
          </ContextMenuSub>
        </ContextMenuContent>
      </ContextMenu>

      <IssuePickerDialog
        open={duplicatePickerOpen}
        onOpenChange={setDuplicatePickerOpen}
        excludeIssueIds={[issue.id]}
        title="Mark as duplicate"
        placeholder="Search the canonical issue…"
        onPick={(canonical) => {
          // The server sets status='duplicate' atomically with the link.
          void updateIssue({ duplicateOfId: canonical.id })
        }}
      />
    </>
  )
}

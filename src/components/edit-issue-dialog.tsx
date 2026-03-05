import { useState, useEffect } from "react"
import { Dialog, DialogContent } from "@/components/ui/dialog"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Textarea } from "@/components/ui/textarea"
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu"
import {
  Popover,
  PopoverContent,
  PopoverTrigger,
} from "@/components/ui/popover"
import { Calendar } from "@/components/ui/calendar"
import { trpc } from "@/lib/trpc-client"
import {
  statuses,
  StatusIcon,
  getStatusConfig,
} from "@/components/status-dropdown"
import {
  priorities,
  PriorityIcon,
  getPriorityConfig,
} from "@/components/priority-dropdown"
import { LabelPicker } from "@/components/label-picker"
import { ChevronRight, X, Paperclip, CalendarDays } from "lucide-react"
import type { Issue } from "@/db/schema"
import { formatDate } from "@/lib/utils"

interface EditIssueDialogProps {
  open: boolean
  onOpenChange: (open: boolean) => void
  issue: Issue
  projectPrefix: string
  projectColor: string
  workspaceId: string
  issueLabelIds: string[]
}

export function EditIssueDialog({
  open,
  onOpenChange,
  issue,
  projectPrefix,
  projectColor,
  workspaceId,
  issueLabelIds,
}: EditIssueDialogProps) {
  const [title, setTitle] = useState(issue.title)
  const [description, setDescription] = useState(
    issue.description &&
      typeof issue.description === `object` &&
      `text` in (issue.description as Record<string, unknown>)
      ? ((issue.description as Record<string, unknown>).text as string)
      : ``
  )

  // Reset local state when issue changes
  useEffect(() => {
    setTitle(issue.title)
    setDescription(
      issue.description &&
        typeof issue.description === `object` &&
        `text` in (issue.description as Record<string, unknown>)
        ? ((issue.description as Record<string, unknown>).text as string)
        : ``
    )
  }, [issue.id, issue.title, issue.description])

  const handleTitleBlur = async () => {
    const trimmed = title.trim()
    if (trimmed && trimmed !== issue.title) {
      await trpc.issues.update.mutate({ id: issue.id, title: trimmed })
    }
  }

  const handleDescriptionBlur = async () => {
    const trimmed = description.trim()
    const currentText =
      issue.description &&
      typeof issue.description === `object` &&
      `text` in (issue.description as Record<string, unknown>)
        ? ((issue.description as Record<string, unknown>).text as string)
        : ``
    if (trimmed !== currentText) {
      await trpc.issues.update.mutate({
        id: issue.id,
        description: trimmed ? { text: trimmed } : null,
      })
    }
  }

  const handleStatusChange = async (newStatus: string) => {
    await trpc.issues.update.mutate({
      id: issue.id,
      status: newStatus as
        | `backlog`
        | `todo`
        | `in_progress`
        | `done`
        | `cancelled`,
    })
  }

  const handlePriorityChange = async (newPriority: string) => {
    await trpc.issues.update.mutate({
      id: issue.id,
      priority: newPriority as `none` | `urgent` | `high` | `medium` | `low`,
    })
  }

  const handleToggleLabel = async (labelId: string) => {
    if (issueLabelIds.includes(labelId)) {
      await trpc.issueLabels.remove.mutate({ issueId: issue.id, labelId })
    } else {
      await trpc.issueLabels.add.mutate({ issueId: issue.id, labelId })
    }
  }

  const handleDueDateSelect = async (date: Date | undefined) => {
    await trpc.issues.update.mutate({
      id: issue.id,
      dueDate: date ? date.toISOString().split(`T`)[0] : null,
    })
  }

  const statusConfig = getStatusConfig(issue.status)
  const priorityConfig = getPriorityConfig(issue.priority)
  const dueDateValue = issue.dueDate
    ? new Date(issue.dueDate + `T00:00:00`)
    : undefined

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent
        showCloseButton={false}
        className="sm:max-w-[640px] p-0 gap-0"
      >
        {/* Header */}
        <div className="flex items-center justify-between px-5 pt-4 pb-2">
          <div className="flex items-center gap-1.5 text-sm text-muted-foreground">
            <div className="flex items-center gap-1.5 rounded-md bg-accent/50 px-2 py-0.5 text-xs font-medium text-foreground">
              <div
                className="h-2.5 w-2.5 rounded-full"
                style={{ backgroundColor: projectColor }}
              />
              {projectPrefix}
            </div>
            <ChevronRight className="h-3 w-3" />
            <span className="text-sm font-mono">{issue.identifier}</span>
          </div>
          <Button
            type="button"
            variant="ghost"
            size="icon-xs"
            className="text-muted-foreground"
            onClick={() => onOpenChange(false)}
          >
            <X className="size-3" />
          </Button>
        </div>

        {/* Title */}
        <Input
          value={title}
          onChange={(e) => setTitle(e.target.value)}
          onBlur={handleTitleBlur}
          placeholder="Issue title"
          className="bg-transparent border-none shadow-none text-lg font-medium px-5 py-1 focus-visible:ring-0 placeholder:text-muted-foreground/50"
        />

        {/* Description */}
        <Textarea
          value={description}
          onChange={(e) => setDescription(e.target.value)}
          onBlur={handleDescriptionBlur}
          placeholder="Add description..."
          className="bg-transparent border-none shadow-none resize-none min-h-[60px] px-5 py-1 focus-visible:ring-0 text-sm placeholder:text-muted-foreground/50"
        />

        {/* Toolbar */}
        <div className="flex items-center gap-1 px-4 py-2 border-t border-border">
          {/* Status picker */}
          <DropdownMenu>
            <DropdownMenuTrigger asChild>
              <Button
                variant="ghost"
                size="xs"
                className="text-muted-foreground"
              >
                <StatusIcon status={issue.status} className="!h-3 !w-3" />
                {statusConfig.label}
              </Button>
            </DropdownMenuTrigger>
            <DropdownMenuContent align="start">
              {statuses.map((s) => {
                const SIcon = s.icon
                return (
                  <DropdownMenuItem
                    key={s.value}
                    onClick={() => handleStatusChange(s.value)}
                  >
                    <SIcon className={`mr-2 h-4 w-4 ${s.color}`} />
                    {s.label}
                  </DropdownMenuItem>
                )
              })}
            </DropdownMenuContent>
          </DropdownMenu>

          {/* Priority picker */}
          <DropdownMenu>
            <DropdownMenuTrigger asChild>
              <Button
                variant="ghost"
                size="xs"
                className="text-muted-foreground"
              >
                <PriorityIcon priority={issue.priority} className="!h-3 !w-3" />
                {priorityConfig.label}
              </Button>
            </DropdownMenuTrigger>
            <DropdownMenuContent align="start">
              {priorities.map((p) => {
                const PIcon = p.icon
                return (
                  <DropdownMenuItem
                    key={p.value}
                    onClick={() => handlePriorityChange(p.value)}
                  >
                    <PIcon className={`mr-2 h-4 w-4 ${p.color}`} />
                    {p.label}
                  </DropdownMenuItem>
                )
              })}
            </DropdownMenuContent>
          </DropdownMenu>

          {/* Label picker */}
          <LabelPicker
            workspaceId={workspaceId}
            selectedLabelIds={issueLabelIds}
            onToggle={handleToggleLabel}
          />

          {/* Due date picker */}
          <Popover>
            <PopoverTrigger asChild>
              <Button
                variant="ghost"
                size="xs"
                className="text-muted-foreground"
              >
                <CalendarDays className="size-3" />
                {dueDateValue ? formatDate(dueDateValue) : `Due date`}
              </Button>
            </PopoverTrigger>
            <PopoverContent className="w-auto p-0" align="start">
              <Calendar
                mode="single"
                selected={dueDateValue}
                onSelect={handleDueDateSelect}
              />
            </PopoverContent>
          </Popover>
        </div>

        {/* Footer */}
        <div className="flex items-center px-4 py-3 border-t border-border">
          <Button
            variant="ghost"
            size="icon-xs"
            className="text-muted-foreground"
            type="button"
          >
            <Paperclip className="size-3" />
          </Button>
        </div>
      </DialogContent>
    </Dialog>
  )
}

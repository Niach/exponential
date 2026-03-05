import { useState, useRef, useEffect } from "react"
import { Dialog, DialogContent } from "@/components/ui/dialog"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { MarkdownEditor, type MarkdownEditorRef } from "@/components/markdown-editor"
import { Label } from "@/components/ui/label"
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu"
import { Switch } from "@/components/ui/switch"
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
import {
  Popover,
  PopoverContent,
  PopoverTrigger,
} from "@/components/ui/popover"
import { Calendar } from "@/components/ui/calendar"
import { AssigneePicker } from "@/components/assignee-picker"
import { ChevronRight, X, Paperclip, CalendarDays } from "lucide-react"
import type { User } from "@/db/schema"
import { formatDate } from "@/lib/utils"

interface CreateIssueDialogProps {
  open: boolean
  onOpenChange: (open: boolean) => void
  projectId: string
  projectPrefix: string
  projectColor: string
  workspaceId: string
  defaultStatus?: string
  users: User[]
}

export function CreateIssueDialog({
  open,
  onOpenChange,
  projectId,
  projectPrefix,
  projectColor,
  workspaceId,
  defaultStatus,
  users,
}: CreateIssueDialogProps) {
  const [title, setTitle] = useState(``)
  const [description, setDescription] = useState(``)
  const [status, setStatus] = useState(defaultStatus ?? `backlog`)
  const [priority, setPriority] = useState(`none`)
  const [selectedLabelIds, setSelectedLabelIds] = useState<string[]>([])
  const [assigneeId, setAssigneeId] = useState<string | null>(null)
  const [dueDate, setDueDate] = useState<Date | undefined>()
  const [createMore, setCreateMore] = useState(false)
  const [submitting, setSubmitting] = useState(false)
  const titleRef = useRef<HTMLInputElement>(null)
  const editorRef = useRef<MarkdownEditorRef>(null)

  useEffect(() => {
    if (open) {
      setStatus(defaultStatus ?? `backlog`)
    }
  }, [open, defaultStatus])

  const resetFields = () => {
    setTitle(``)
    setDescription(``)
    editorRef.current?.setMarkdown(``)
    setStatus(defaultStatus ?? `backlog`)
    setPriority(`none`)
    setAssigneeId(null)
    setSelectedLabelIds([])
    setDueDate(undefined)
  }

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault()
    if (!title.trim()) return

    setSubmitting(true)
    try {
      await trpc.issues.create.mutate({
        projectId,
        title: title.trim(),
        status: status as
          | `backlog`
          | `todo`
          | `in_progress`
          | `done`
          | `cancelled`,
        priority: priority as `none` | `urgent` | `high` | `medium` | `low`,
        assigneeId: assigneeId ?? undefined,
        description: description.trim()
          ? { text: description.trim() }
          : undefined,
        dueDate: dueDate ? dueDate.toISOString().split(`T`)[0] : undefined,
        labelIds: selectedLabelIds.length > 0 ? selectedLabelIds : undefined,
      })
      if (createMore) {
        resetFields()
        titleRef.current?.focus()
      } else {
        resetFields()
        onOpenChange(false)
      }
    } finally {
      setSubmitting(false)
    }
  }

  const handleToggleLabel = (labelId: string) => {
    setSelectedLabelIds((prev) =>
      prev.includes(labelId)
        ? prev.filter((id) => id !== labelId)
        : [...prev, labelId]
    )
  }

  const statusConfig = getStatusConfig(status)
  const priorityConfig = getPriorityConfig(priority)

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent
        showCloseButton={false}
        className="sm:max-w-[640px] p-0 gap-0"
      >
        <form onSubmit={handleSubmit}>
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
              <span className="text-sm">New issue</span>
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
            ref={titleRef}
            value={title}
            onChange={(e) => setTitle(e.target.value)}
            placeholder="Issue title"
            autoFocus
            className="bg-transparent dark:bg-transparent border-none shadow-none text-lg font-medium px-5 py-1 focus-visible:ring-0 placeholder:text-muted-foreground/50"
          />

          {/* Description */}
          <MarkdownEditor
            ref={editorRef}
            markdown={description}
            onChange={setDescription}
            placeholder="Add description..."
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
                  <StatusIcon status={status} className="!h-3 !w-3" />
                  {statusConfig.label}
                </Button>
              </DropdownMenuTrigger>
              <DropdownMenuContent align="start">
                {statuses.map((s) => {
                  const SIcon = s.icon
                  return (
                    <DropdownMenuItem
                      key={s.value}
                      onClick={() => setStatus(s.value)}
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
                  <PriorityIcon priority={priority} className="!h-3 !w-3" />
                  {priorityConfig.label}
                </Button>
              </DropdownMenuTrigger>
              <DropdownMenuContent align="start">
                {priorities.map((p) => {
                  const PIcon = p.icon
                  return (
                    <DropdownMenuItem
                      key={p.value}
                      onClick={() => setPriority(p.value)}
                    >
                      <PIcon className={`mr-2 h-4 w-4 ${p.color}`} />
                      {p.label}
                    </DropdownMenuItem>
                  )
                })}
              </DropdownMenuContent>
            </DropdownMenu>

            {/* Assignee picker */}
            <AssigneePicker
              users={users}
              selectedUserId={assigneeId}
              onSelect={setAssigneeId}
            />

            {/* Label picker */}
            <LabelPicker
              workspaceId={workspaceId}
              selectedLabelIds={selectedLabelIds}
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
                  {dueDate ? formatDate(dueDate) : `Due date`}
                </Button>
              </PopoverTrigger>
              <PopoverContent className="w-auto p-0" align="start">
                <Calendar
                  mode="single"
                  selected={dueDate}
                  onSelect={setDueDate}
                />
              </PopoverContent>
            </Popover>
          </div>

          {/* Footer */}
          <div className="flex items-center justify-between px-4 py-3 border-t border-border">
            <Button
              variant="ghost"
              size="icon-xs"
              className="text-muted-foreground"
              type="button"
            >
              <Paperclip className="size-3" />
            </Button>
            <div className="flex items-center gap-3">
              <div className="flex items-center gap-2">
                <Switch
                  id="create-more"
                  size="sm"
                  checked={createMore}
                  onCheckedChange={(checked) => setCreateMore(checked === true)}
                />
                <Label
                  htmlFor="create-more"
                  className="text-xs text-muted-foreground cursor-pointer select-none"
                >
                  Create more
                </Label>
              </div>
              <Button
                type="submit"
                disabled={!title.trim() || submitting}
                className="bg-indigo-600 hover:bg-indigo-700 text-white h-7 px-3 text-xs"
              >
                {submitting ? `Creating...` : `Create issue`}
              </Button>
            </div>
          </div>
        </form>
      </DialogContent>
    </Dialog>
  )
}

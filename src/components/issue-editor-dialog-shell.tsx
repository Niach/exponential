import { useRef } from "react"
import type {
  ComponentPropsWithoutRef,
  ReactNode,
  Ref,
} from "react"
import { CalendarDays, ChevronRight, LoaderCircle, Paperclip, X } from "lucide-react"
import type { User } from "@/db/schema"
import type {
  IssuePriority,
  IssueStatus,
} from "@/lib/domain"
import { acceptedImageContentTypes } from "@/lib/issue-attachments"
import { formatDate } from "@/lib/utils"
import { priorities, PriorityIcon } from "@/components/priority-dropdown"
import { statuses, StatusIcon } from "@/components/status-dropdown"
import { OptionDropdownMenu } from "@/components/option-dropdown-menu"
import { AssigneePicker } from "@/components/assignee-picker"
import { LabelPicker } from "@/components/label-picker"
import {
  MarkdownEditor,
  type MarkdownEditorImageUploadConfig,
  type MarkdownEditorRef,
} from "@/components/markdown-editor"
import { Button } from "@/components/ui/button"
import { Calendar } from "@/components/ui/calendar"
import { Dialog, DialogContent } from "@/components/ui/dialog"
import { Input } from "@/components/ui/input"
import {
  Popover,
  PopoverContent,
  PopoverTrigger,
} from "@/components/ui/popover"
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@/components/ui/tooltip"

interface IssueEditorDialogShellProps {
  assigneeId: string | null
  autoFocus?: boolean
  closeDisabled?: boolean
  description: string
  disabled?: boolean
  dialogTestId?: string
  dueDate: Date | undefined
  editorRef?: Ref<MarkdownEditorRef>
  footer: ReactNode
  formProps?: ComponentPropsWithoutRef<`form`>
  headerContent: ReactNode
  imageUpload?: MarkdownEditorImageUploadConfig
  onAssigneeChange: (userId: string | null) => void | Promise<void>
  onDescriptionBlur?: () => void
  onDescriptionChange: (markdown: string) => void
  onDueDateSelect: (date: Date | undefined) => void | Promise<void>
  onOpenChange: (open: boolean) => void
  onPriorityChange: (priority: IssuePriority) => void | Promise<void>
  onStatusChange: (status: IssueStatus) => void | Promise<void>
  onTitleBlur?: () => void
  onTitleChange: (value: string) => void
  onToggleLabel: (labelId: string) => void | Promise<void>
  open: boolean
  priority: IssuePriority
  projectColor: string
  projectPrefix: string
  selectedLabelIds: string[]
  status: IssueStatus
  title: string
  titleRef?: Ref<HTMLInputElement>
  users: User[]
  workspaceId: string
}

export function IssueEditorDialogShell({
  assigneeId,
  autoFocus,
  closeDisabled,
  description,
  disabled,
  dialogTestId,
  dueDate,
  editorRef,
  footer,
  formProps,
  headerContent,
  imageUpload,
  onAssigneeChange,
  onDescriptionBlur,
  onDescriptionChange,
  onDueDateSelect,
  onOpenChange,
  onPriorityChange,
  onStatusChange,
  onTitleBlur,
  onTitleChange,
  onToggleLabel,
  open,
  priority,
  projectColor,
  projectPrefix,
  selectedLabelIds,
  status,
  title,
  titleRef,
  users,
  workspaceId,
}: IssueEditorDialogShellProps) {
  const closeBlocked = closeDisabled === true

  const content = (
    <>
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
          {headerContent}
        </div>
        <Button
          type="button"
          variant="ghost"
          size="icon-xs"
          aria-label="Close dialog"
          className="text-muted-foreground"
          disabled={closeBlocked}
          onClick={() => onOpenChange(false)}
        >
          <X className="size-3" />
        </Button>
      </div>

      <Input
        ref={titleRef}
        value={title}
        onBlur={onTitleBlur}
        onChange={(event) => onTitleChange(event.target.value)}
        placeholder="Issue title"
        autoFocus={autoFocus}
        disabled={disabled}
        className="bg-transparent dark:bg-transparent border-none shadow-none text-lg font-medium px-5 py-1 focus-visible:ring-0 placeholder:text-muted-foreground/50"
      />

      <MarkdownEditor
        ref={editorRef}
        markdown={description}
        editable={!disabled}
        onChange={onDescriptionChange}
        onBlur={onDescriptionBlur}
        placeholder="Add description..."
        imageUpload={imageUpload}
      />

      <div className="flex items-center gap-1 px-4 py-2 border-t border-border">
        <OptionDropdownMenu
          value={status}
          disabled={disabled}
          options={statuses}
          onSelect={onStatusChange}
          renderTrigger={(selected) => (
            <Button
              variant="ghost"
              size="xs"
              className="text-muted-foreground"
              disabled={disabled}
            >
              <StatusIcon status={selected.value} className="!h-3 !w-3" />
              {selected.label}
            </Button>
          )}
        />

        <OptionDropdownMenu
          value={priority}
          disabled={disabled}
          options={priorities}
          onSelect={onPriorityChange}
          renderTrigger={(selected) => (
            <Button
              variant="ghost"
              size="xs"
              className="text-muted-foreground"
              disabled={disabled}
            >
              <PriorityIcon priority={selected.value} className="!h-3 !w-3" />
              {selected.label}
            </Button>
          )}
        />

        <AssigneePicker
          disabled={disabled}
          users={users}
          selectedUserId={assigneeId}
          onSelect={onAssigneeChange}
        />

        <LabelPicker
          disabled={disabled}
          workspaceId={workspaceId}
          selectedLabelIds={selectedLabelIds}
          onToggle={onToggleLabel}
        />

        <Popover>
          <PopoverTrigger asChild>
            <Button
              variant="ghost"
              size="xs"
              className="text-muted-foreground"
              disabled={disabled}
            >
              <CalendarDays className="size-3" />
              {dueDate ? formatDate(dueDate) : `Due date`}
            </Button>
          </PopoverTrigger>
          <PopoverContent className="w-auto p-0" align="start">
            <Calendar
              mode="single"
              selected={dueDate}
              onSelect={(date) => {
                void onDueDateSelect(date)
              }}
            />
          </PopoverContent>
        </Popover>
      </div>

      {footer}
    </>
  )

  return (
    <Dialog
      open={open}
      onOpenChange={(nextOpen) => {
        if (!nextOpen && closeBlocked) {
          return
        }

        onOpenChange(nextOpen)
      }}
    >
      <DialogContent
        showCloseButton={false}
        className="sm:max-w-[640px] p-0 gap-0"
        data-testid={dialogTestId}
        onEscapeKeyDown={(event) => {
          if (closeBlocked) {
            event.preventDefault()
          }
        }}
        onInteractOutside={(event) => {
          if (closeBlocked) {
            event.preventDefault()
          }
        }}
      >
        {formProps ? (
          <form {...formProps} className="contents">
            {content}
          </form>
        ) : (
          content
        )}
      </DialogContent>
    </Dialog>
  )
}

interface IssueEditorAttachmentButtonProps {
  disabled?: boolean
  disabledReason?: string
  onFiles?: (files: File[]) => void | Promise<void>
  uploading?: boolean
}

export function IssueEditorAttachmentButton({
  disabled,
  disabledReason,
  onFiles,
  uploading,
}: IssueEditorAttachmentButtonProps) {
  const inputRef = useRef<HTMLInputElement>(null)
  const isDisabled = disabled || !onFiles

  const button = (
    <>
      <Input
        ref={inputRef}
        type="file"
        accept={acceptedImageContentTypes.join(`,`)}
        className="hidden"
        disabled={isDisabled || uploading}
        onChange={(event) => {
          const files = Array.from(event.target.files ?? [])

          if (files.length > 0 && onFiles) {
            void onFiles(files)
          }

          event.target.value = ``
        }}
      />
      <Button
        variant="ghost"
        size="icon-xs"
        className="text-muted-foreground"
        type="button"
        aria-label="Add image"
        disabled={isDisabled || uploading}
        onClick={() => {
          inputRef.current?.click()
        }}
      >
        {uploading ? (
          <LoaderCircle className="size-3 animate-spin" />
        ) : (
          <Paperclip className="size-3" />
        )}
      </Button>
    </>
  )

  if (!disabledReason) {
    return button
  }

  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <span>{button}</span>
      </TooltipTrigger>
      <TooltipContent>{disabledReason}</TooltipContent>
    </Tooltip>
  )
}

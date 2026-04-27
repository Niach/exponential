import { useRef } from "react"
import type { ComponentPropsWithoutRef, ReactNode, Ref } from "react"
import {
  ArrowUp,
  CalendarDays,
  ChevronRight,
  LoaderCircle,
  MoreHorizontal,
  Paperclip,
  X,
} from "lucide-react"
import type { User } from "@/db/schema"
import type { IssuePriority, IssueStatus } from "@/lib/domain"
import { acceptedImageContentTypes } from "@/lib/issue-attachments"
import { formatDate } from "@/lib/utils"
import { useIsMobile } from "@/hooks/use-mobile"
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
import { Dialog, DialogContent, DialogTitle } from "@/components/ui/dialog"
import {
  Sheet,
  SheetContent,
  SheetTitle,
} from "@/components/ui/sheet"
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu"
import { Input } from "@/components/ui/input"
import { TimeInput } from "@/components/time-input"
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

interface PrimaryAction {
  disabled?: boolean
  onClick?: () => void
  type?: `button` | `submit`
  loading?: boolean
}

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
  chipRowExtras?: ReactNode
  hideDueDateChip?: boolean
  imageUpload?: MarkdownEditorImageUploadConfig
  overflowMenuItems?: ReactNode
  onAssigneeChange: (userId: string | null) => void | Promise<void>
  onDescriptionBlur?: () => void
  onDescriptionChange: (markdown: string) => void
  onDueDateSelect: (date: Date | undefined) => void | Promise<void>
  dueTime: string | null
  endTime: string | null
  onDueTimeChange: (time: string | null) => void | Promise<void>
  onEndTimeChange: (time: string | null) => void | Promise<void>
  onOpenChange: (open: boolean) => void
  onPriorityChange: (priority: IssuePriority) => void | Promise<void>
  onStatusChange: (status: IssueStatus) => void | Promise<void>
  onTitleBlur?: () => void
  onTitleChange: (value: string) => void
  onToggleLabel: (labelId: string) => void | Promise<void>
  open: boolean
  primaryAction?: PrimaryAction
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
  chipRowExtras,
  footer,
  formProps,
  headerContent,
  hideDueDateChip,
  imageUpload,
  overflowMenuItems,
  onAssigneeChange,
  onDescriptionBlur,
  onDescriptionChange,
  onDueDateSelect,
  dueTime,
  endTime,
  onDueTimeChange,
  onEndTimeChange,
  onOpenChange,
  onPriorityChange,
  onStatusChange,
  onTitleBlur,
  onTitleChange,
  onToggleLabel,
  open,
  primaryAction,
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
  const isMobile = useIsMobile()
  const closeBlocked = closeDisabled === true

  const projectPill = (
    <div className="flex items-center gap-1.5 rounded-md bg-accent/50 px-2 py-0.5 text-xs font-medium text-foreground">
      <div
        className="h-2.5 w-2.5 rounded-full"
        style={{ backgroundColor: projectColor }}
      />
      {projectPrefix}
    </div>
  )

  const titleInput = (
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
  )

  const editor = (
    <MarkdownEditor
      ref={editorRef}
      markdown={description}
      editable={!disabled}
      onChange={onDescriptionChange}
      onBlur={onDescriptionBlur}
      placeholder="Add description..."
      imageUpload={imageUpload}
    />
  )

  const chipNodes = (
    <>
      <OptionDropdownMenu
        value={status}
        disabled={disabled}
        options={statuses}
        onSelect={onStatusChange}
        renderTrigger={(selected) => (
          <Button
            variant="ghost"
            size="xs"
            className="text-muted-foreground shrink-0"
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
            className="text-muted-foreground shrink-0"
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

      {!hideDueDateChip && (
        <Popover>
          <PopoverTrigger asChild>
            <Button
              variant="ghost"
              size="xs"
              className="text-muted-foreground shrink-0"
              disabled={disabled}
            >
              <CalendarDays className="size-3" />
              {dueDate
                ? `${formatDate(dueDate)}${dueTime ? ` · ${dueTime.slice(0, 5)}` : ``}`
                : `Due date`}
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
            {dueDate && (
              <div className="flex items-center gap-2 border-t border-border px-3 py-2 text-xs text-muted-foreground">
                <span>Time</span>
                <TimeInput
                  value={dueTime}
                  onChange={(t) => void onDueTimeChange(t)}
                  className="h-7 w-20 text-xs tabular-nums"
                  ariaLabel="Start time"
                />
                <span>–</span>
                <TimeInput
                  value={endTime}
                  onChange={(t) => void onEndTimeChange(t)}
                  disabled={!dueTime}
                  className="h-7 w-20 text-xs tabular-nums"
                  ariaLabel="End time"
                />
                {(dueTime || endTime) && (
                  <Button
                    type="button"
                    variant="ghost"
                    size="xs"
                    className="ml-auto h-6 text-xs"
                    onClick={() => {
                      void onDueTimeChange(null)
                      void onEndTimeChange(null)
                    }}
                  >
                    All day
                  </Button>
                )}
              </div>
            )}
          </PopoverContent>
        </Popover>
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
              <MoreHorizontal className="size-3" />
            </Button>
          </DropdownMenuTrigger>
          <DropdownMenuContent align="start">
            {overflowMenuItems}
          </DropdownMenuContent>
        </DropdownMenu>
      )}
    </>
  )

  const guardedOpenChange = (nextOpen: boolean) => {
    if (!nextOpen && closeBlocked) {
      return
    }

    onOpenChange(nextOpen)
  }

  if (isMobile) {
    const mobileBody = (
      <>
        <SheetTitle className="sr-only">
          {title || `Issue ${projectPrefix}`}
        </SheetTitle>
        <div className="flex items-center justify-between gap-2 px-3 pt-3 pb-2 border-b border-border/50">
          <Button
            type="button"
            variant="ghost"
            size="icon-xs"
            aria-label="Close"
            disabled={closeBlocked}
            onClick={() => onOpenChange(false)}
            className="text-muted-foreground shrink-0"
          >
            <X className="size-4" />
          </Button>
          <div className="flex flex-1 items-center justify-center gap-1.5 min-w-0 text-sm text-muted-foreground">
            {projectPill}
            <span className="truncate">{headerContent}</span>
          </div>
          {primaryAction ? (
            <Button
              type={primaryAction.type ?? `button`}
              variant="ghost"
              size="icon-xs"
              aria-label="Submit"
              disabled={primaryAction.disabled}
              onClick={primaryAction.onClick}
              className="shrink-0 rounded-full bg-accent/50 disabled:opacity-40"
            >
              {primaryAction.loading ? (
                <LoaderCircle className="size-4 animate-spin" />
              ) : (
                <ArrowUp className="size-4" />
              )}
            </Button>
          ) : (
            <span className="w-7 shrink-0" aria-hidden />
          )}
        </div>

        {titleInput}

        <div className="flex-1 min-h-0 overflow-y-auto">{editor}</div>

        <div className="flex items-center gap-1.5 overflow-x-auto px-3 py-2 border-t border-border">
          {chipNodes}
        </div>

        {footer}
      </>
    )

    return (
      <Sheet open={open} onOpenChange={guardedOpenChange}>
        <SheetContent
          side="bottom"
          showCloseButton={false}
          data-testid={dialogTestId}
          aria-describedby={undefined}
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
          className="top-0 h-[100dvh] p-0 gap-0 flex flex-col pb-[env(safe-area-inset-bottom)]"
        >
          {formProps ? (
            <form {...formProps} className="contents">
              {mobileBody}
            </form>
          ) : (
            mobileBody
          )}
        </SheetContent>
      </Sheet>
    )
  }

  const desktopBody = (
    <>
      <DialogTitle className="sr-only">
        {title || `Issue ${projectPrefix}`}
      </DialogTitle>
      <div className="flex items-center justify-between px-5 pt-4 pb-2">
        <div className="flex items-center gap-1.5 text-sm text-muted-foreground">
          {projectPill}
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

      {titleInput}
      {editor}

      <div className="flex items-center gap-1 px-4 py-2 border-t border-border">
        {chipNodes}
      </div>

      {footer}
    </>
  )

  return (
    <Dialog open={open} onOpenChange={guardedOpenChange}>
      <DialogContent
        showCloseButton={false}
        className="sm:max-w-[40rem] p-0 gap-0"
        data-testid={dialogTestId}
        aria-describedby={undefined}
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
            {desktopBody}
          </form>
        ) : (
          desktopBody
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

import { useCallback, useRef } from "react"
import type { ComponentPropsWithoutRef, ReactNode, Ref } from "react"
import { ArrowUp, ChevronRight, LoaderCircle, X } from "lucide-react"
import type { User } from "@/db/schema"
import type { IssuePriority, IssueStatus } from "@/lib/domain"
import { useIsMobile } from "@/hooks/use-mobile"
import {
  MarkdownEditor,
  type MarkdownEditorImageUploadConfig,
  type MarkdownEditorRef,
} from "@/components/issue-editor/markdown-editor"
import { IssueEditorChips } from "@/components/issue-editor/chips"
import { Button } from "@/components/ui/button"
import { Dialog, DialogContent, DialogTitle } from "@/components/ui/dialog"
import { Sheet, SheetContent, SheetTitle } from "@/components/ui/sheet"
import { Input } from "@/components/ui/input"

export { IssueEditorAttachmentButton } from "@/components/issue-editor/attachment-button"

// The editor's @/# autocomplete popup portals to document.body (EXP-54 — the
// dialog's scroll region would clip it), so Radix sees interactions with it as
// OUTSIDE the modal content and would close the dialog. Whitelist them.
function isEditorAutocompleteInteraction(event: {
  target: EventTarget | null
}): boolean {
  return (
    event.target instanceof Element &&
    event.target.closest(`[data-editor-autocomplete]`) !== null
  )
}

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
  // When true, disables status / priority / assignee / due-date / overflow
  // controls while keeping title, description, labels, and image upload
  // available. Used for non-moderator contributors in public workspaces.
  restrictModeration?: boolean
  dialogTestId?: string
  dueDate: Date | undefined
  editorRef?: Ref<MarkdownEditorRef>
  footer: ReactNode
  formProps?: ComponentPropsWithoutRef<`form`>
  headerContent: ReactNode
  chipRowExtras?: ReactNode
  hideAssignee?: boolean
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
  restrictModeration,
  dialogTestId,
  dueDate,
  editorRef,
  chipRowExtras,
  footer,
  formProps,
  headerContent,
  hideAssignee,
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

  // Local handle on the markdown editor (merged with the caller's optional
  // `editorRef`) so Tab in the title can move the caret into the description.
  const internalEditorRef = useRef<MarkdownEditorRef | null>(null)
  const assignEditorRef = useCallback(
    (instance: MarkdownEditorRef | null) => {
      internalEditorRef.current = instance
      if (typeof editorRef === `function`) {
        editorRef(instance)
      } else if (editorRef) {
        editorRef.current = instance
      }
    },
    [editorRef]
  )

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
      onKeyDown={(event) => {
        // Tab jumps straight into the description editor instead of cycling
        // the formatting-toolbar buttons (which are tabIndex={-1}); handling
        // it here means TipTap never sees the Tab, so it can't be swallowed
        // by indent/format keymaps. Shift+Tab keeps its default (backward)
        // behavior. (EXP-10)
        if (event.key === `Tab` && !event.shiftKey && !disabled) {
          event.preventDefault()
          internalEditorRef.current?.focus()
        }
      }}
      placeholder="Issue title"
      autoFocus={autoFocus}
      disabled={disabled}
      className="bg-transparent dark:bg-transparent border-none shadow-none text-lg font-medium px-5 py-1 focus-visible:ring-0 placeholder:text-muted-foreground/50"
    />
  )

  const editor = (
    <MarkdownEditor
      ref={assignEditorRef}
      markdown={description}
      editable={!disabled}
      onChange={onDescriptionChange}
      onBlur={onDescriptionBlur}
      placeholder="Add description..."
      imageUpload={imageUpload}
    />
  )

  const moderationDisabled = disabled || restrictModeration
  const chipNodes = (
    <IssueEditorChips
      status={status}
      priority={priority}
      assigneeId={assigneeId}
      selectedLabelIds={selectedLabelIds}
      workspaceId={workspaceId}
      users={users}
      dueDate={dueDate}
      dueTime={dueTime}
      endTime={endTime}
      hideAssignee={hideAssignee}
      hideDueDateChip={hideDueDateChip}
      disabled={disabled}
      moderationDisabled={moderationDisabled}
      chipRowExtras={chipRowExtras}
      overflowMenuItems={overflowMenuItems}
      onStatusChange={onStatusChange}
      onPriorityChange={onPriorityChange}
      onAssigneeChange={onAssigneeChange}
      onToggleLabel={onToggleLabel}
      onDueDateSelect={onDueDateSelect}
      onDueTimeChange={onDueTimeChange}
      onEndTimeChange={onEndTimeChange}
    />
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

        <div className="editor-scroll-region flex-1 min-h-0 min-w-0 overflow-y-auto">
          {editor}
        </div>

        <div className="flex flex-wrap items-center gap-1.5 px-3 py-2 border-t border-border shrink-0">
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
            if (closeBlocked || isEditorAutocompleteInteraction(event)) {
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
      <div className="editor-scroll-region flex-1 min-h-0 min-w-0 overflow-y-auto">
        {editor}
      </div>

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
        className="sm:max-w-[40rem] p-0 gap-0 flex max-h-[85vh] flex-col"
        data-testid={dialogTestId}
        aria-describedby={undefined}
        onEscapeKeyDown={(event) => {
          if (closeBlocked) {
            event.preventDefault()
          }
        }}
        onInteractOutside={(event) => {
          if (closeBlocked || isEditorAutocompleteInteraction(event)) {
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

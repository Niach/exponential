import { useCallback, useRef } from "react"
import type { ComponentPropsWithoutRef, ReactNode, Ref } from "react"
import { ChevronRight, LoaderCircle, X } from "lucide-react"
import { conceptIcon } from "@/lib/icons.generated"
import type { User } from "@/db/schema"
import type { IssuePriority } from "@/lib/domain"
import type { StatusRowOption } from "@/lib/team-statuses"
import { useIsMobile } from "@/hooks/use-mobile"
import {
  MarkdownEditor,
  type MarkdownEditorImageUploadConfig,
  type MarkdownEditorRef,
} from "@/components/issue-editor/markdown-editor"
import { BoardGlyph } from "@/components/board-glyph"
import { IssueEditorChips } from "@/components/issue-editor/chips"
import { IssueEditorMobileProperties } from "@/components/issue-editor/mobile-properties"
import { Button } from "@/components/ui/button"
import { Dialog, DialogContent, DialogTitle } from "@/components/ui/dialog"
import { Sheet, SheetContent, SheetTitle } from "@/components/ui/sheet"
import { Input } from "@/components/ui/input"

const UiBackIcon = conceptIcon(`ui-back`)

// The editor's @/# autocomplete popup portals to document.body (EXP-54 — the
// dialog's scroll region would clip it), and EXP-568's formatting rail does
// the same, so Radix sees interactions with either as OUTSIDE the modal
// content and would close the dialog. Whitelist them.
function isEditorAutocompleteInteraction(event: {
  target: EventTarget | null
}): boolean {
  return (
    event.target instanceof Element &&
    (event.target.closest(`[data-editor-autocomplete]`) !== null ||
      event.target.closest(`[data-editor-rail]`) !== null)
  )
}

// Escape aimed at an inner editor layer — the @/# autocomplete popup or the
// toolbar's inline link input — must close only that layer. Radix listens for
// Escape on document with capture, so it fires BEFORE the editor's own
// handler can consume the key; swallow it here (the layer's handler still
// runs and closes it) instead of routing it into the dismiss confirm.
function isEditorInnerLayerEscape(event: {
  target: EventTarget | null
}): boolean {
  // The autocomplete portals to document.body, so the Escape's target is the
  // editor itself — a mounted portal is the "menu is open" signal.
  if (document.querySelector(`[data-editor-autocomplete]`) !== null) {
    return true
  }
  return (
    event.target instanceof Element &&
    (event.target.closest(`[data-editor-link-edit]`) !== null ||
      event.target.closest(`[data-editor-rail]`) !== null)
  )
}

interface PrimaryAction {
  disabled?: boolean
  onClick?: () => void
  type?: `button` | `submit`
  loading?: boolean
  // The phone header's submit is a labelled pill, not an icon (EXP-687).
  label?: string
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
  footer?: ReactNode
  formProps?: ComponentPropsWithoutRef<`form`>
  headerContent: ReactNode
  chipRowExtras?: ReactNode
  // Right-aligned slot in the desktop chip row (EXP-586: the create dialog's
  // submit button lives here instead of a footer).
  chipRowAction?: ReactNode
  hideAssignee?: boolean
  hideDueDateChip?: boolean
  disableStatus?: boolean
  imageUpload?: MarkdownEditorImageUploadConfig
  overflowMenuItems?: ReactNode
  onAssigneeChange: (userId: string | null) => void | Promise<void>
  onDescriptionBlur?: () => void
  onDescriptionChange: (markdown: string) => void
  // Called on an accidental dismissal (Escape / backdrop) so the caller can
  // take one over — return `true` to keep the shell open because the caller
  // handled it (REV2-60: the create dialog confirms before discarding a
  // typed draft). The explicit Close button never routes through here.
  onDismissAttempt?: () => boolean
  onDueDateSelect: (date: Date | undefined) => void | Promise<void>
  onOpenChange: (open: boolean) => void
  onPriorityChange: (priority: IssuePriority) => void | Promise<void>
  onStatusChange: (status: StatusRowOption) => void | Promise<void>
  onTitleBlur?: () => void
  onTitleChange: (value: string) => void
  onToggleLabel: (labelId: string) => void | Promise<void>
  open: boolean
  primaryAction?: PrimaryAction
  // Mobile renders properties as a native-style card of full-width rows
  // inside the scroll region (EXP-247); callers may pass a slimmer
  // `mobileFooter`.
  mobileFooter?: ReactNode
  priority: IssuePriority
  boardColor: string
  boardPrefix: string
  // Board glyph inputs for the static pill (EXP-449: icon+color instead of
  // the anonymous dot).
  boardIcon?: string | null
  boardRepositoryId?: string | null
  // When set, replaces the static board pill with an interactive control
  // (the create dialog's board select).
  boardPicker?: ReactNode
  selectedLabelIds: string[]
  status: StatusRowOption
  title: string
  titleRef?: Ref<HTMLInputElement>
  users: User[]
  teamId: string
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
  chipRowAction,
  footer,
  formProps,
  headerContent,
  hideAssignee,
  hideDueDateChip,
  disableStatus,
  imageUpload,
  overflowMenuItems,
  onAssigneeChange,
  onDescriptionBlur,
  onDescriptionChange,
  onDismissAttempt,
  onDueDateSelect,
  onOpenChange,
  onPriorityChange,
  onStatusChange,
  onTitleBlur,
  onTitleChange,
  onToggleLabel,
  open,
  primaryAction,
  mobileFooter,
  priority,
  boardColor,
  boardPrefix,
  boardIcon,
  boardRepositoryId,
  boardPicker,
  selectedLabelIds,
  status,
  title,
  titleRef,
  users,
  teamId,
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

  const boardPill = boardPicker ?? (
    <div className="flex items-center gap-1.5 rounded-md bg-accent/50 px-2 py-0.5 text-xs font-medium text-foreground">
      <BoardGlyph
        board={{
          icon: boardIcon,
          repositoryId: boardRepositoryId,
          color: boardColor,
        }}
        className="size-3.5"
      />
      {boardPrefix}
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

  const chipNodes = (
    <IssueEditorChips
      status={status}
      priority={priority}
      assigneeId={assigneeId}
      selectedLabelIds={selectedLabelIds}
      teamId={teamId}
      users={users}
      dueDate={dueDate}
      hideAssignee={hideAssignee}
      hideDueDateChip={hideDueDateChip}
      disableStatus={disableStatus}
      disabled={disabled}
      chipRowExtras={chipRowExtras}
      overflowMenuItems={overflowMenuItems}
      onStatusChange={onStatusChange}
      onPriorityChange={onPriorityChange}
      onAssigneeChange={onAssigneeChange}
      onToggleLabel={onToggleLabel}
      onDueDateSelect={onDueDateSelect}
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
          {title || `Issue ${boardPrefix}`}
        </SheetTitle>
        {/* The phone editor is a PAGE, not a sheet (EXP-687): back arrow
            top-left, the title left-aligned beside it, and the primary action
            as a labelled pill top-right — the same header the iOS and Android
            New-issue pages draw. Back is wired exactly like the desktop ✕
            (onOpenChange(false)), so create-issue-dialog's discard veto at the
            Root still runs. */}
        <div className="flex items-center gap-2 border-b border-border/50 px-3 pt-3 pb-2">
          <Button
            type="button"
            variant="ghost"
            size="icon-xs"
            aria-label="Back"
            disabled={closeBlocked}
            onClick={() => onOpenChange(false)}
            className="shrink-0 text-muted-foreground"
          >
            <UiBackIcon className="size-4" />
          </Button>
          <span className="min-w-0 flex-1 truncate text-lg font-semibold text-foreground">
            {headerContent}
          </span>
          <div className="shrink-0">{boardPill}</div>
          {primaryAction ? (
            <Button
              type={primaryAction.type ?? `button`}
              size="sm"
              disabled={primaryAction.disabled}
              onClick={primaryAction.onClick}
              className="shrink-0"
            >
              {primaryAction.loading && (
                <LoaderCircle className="size-4 animate-spin" />
              )}
              {primaryAction.label ?? `Create`}
            </Button>
          ) : null}
        </div>

        {titleInput}

        <div className="editor-scroll-region flex-1 min-h-0 min-w-0 overflow-y-auto">
          {editor}
          <IssueEditorMobileProperties
            status={status}
            priority={priority}
            assigneeId={assigneeId}
            selectedLabelIds={selectedLabelIds}
            teamId={teamId}
            users={users}
            dueDate={dueDate}
            hideAssignee={hideAssignee}
            hideDueDateChip={hideDueDateChip}
            disableStatus={disableStatus}
            disabled={disabled}
            onStatusChange={onStatusChange}
            onPriorityChange={onPriorityChange}
            onAssigneeChange={onAssigneeChange}
            onToggleLabel={onToggleLabel}
            onDueDateSelect={onDueDateSelect}
          />
        </div>

        {mobileFooter ?? footer}
      </>
    )

    return (
      <Sheet open={open} onOpenChange={guardedOpenChange}>
        <SheetContent
          side="bottom"
          showGrabber={false}
          data-testid={dialogTestId}
          aria-describedby={undefined}
          onEscapeKeyDown={(event) => {
            if (isEditorInnerLayerEscape(event)) {
              event.preventDefault()
              return
            }
            if (closeBlocked || onDismissAttempt?.() === true) {
              event.preventDefault()
            }
          }}
          onInteractOutside={(event) => {
            if (
              closeBlocked ||
              isEditorAutocompleteInteraction(event) ||
              onDismissAttempt?.() === true
            ) {
              event.preventDefault()
            }
          }}
          className="top-0 flex h-[100dvh] max-h-none flex-col gap-0 rounded-none p-0 pb-[env(safe-area-inset-bottom)]"
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
        {title || `Issue ${boardPrefix}`}
      </DialogTitle>
      <div className="flex items-center justify-between px-5 pt-4 pb-2">
        <div className="flex items-center gap-1.5 text-sm text-muted-foreground">
          {boardPill}
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
        {chipRowAction ? (
          <div className="ml-auto shrink-0 pl-2">{chipRowAction}</div>
        ) : null}
      </div>

      {footer}
    </>
  )

  return (
    <Dialog open={open} onOpenChange={guardedOpenChange}>
      <DialogContent
        showCloseButton={false}
        className="sm:max-w-3xl p-0 gap-0 flex sm:max-h-[85vh] flex-col"
        data-testid={dialogTestId}
        aria-describedby={undefined}
        onEscapeKeyDown={(event) => {
          if (isEditorInnerLayerEscape(event)) {
            event.preventDefault()
            return
          }
          if (closeBlocked || onDismissAttempt?.() === true) {
            event.preventDefault()
          }
        }}
        onInteractOutside={(event) => {
          if (
            closeBlocked ||
            isEditorAutocompleteInteraction(event) ||
            onDismissAttempt?.() === true
          ) {
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

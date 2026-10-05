import { useCallback, useEffect, useRef, useState } from "react"
import { useBlocker, useNavigate } from "@tanstack/react-router"
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
  Button,
  CollapsedTitle,
  conceptIcon,
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
  Pill,
  toast,
  Tooltip,
  TooltipContent,
  TooltipTrigger,
  useIsMobile,
  WORK_BAR_HEIGHT,
  WORK_COLUMN_CLASS,
  WorkHeader,
  WorkStickyTray,
} from "@exp/ui"
import type { Board, IssueDraft, User } from "@/db/schema"
import { useMeasuredSize, useTitleCollapsed } from "@/hooks/use-detail-chrome"
import { useIssueDraftEditor } from "@/hooks/use-issue-draft-editor"
import { originListNavigation, parseOrigin } from "@/lib/detail-origin"
import { draftExitPrompt, ISSUE_DRAFT_COPY } from "@/lib/issue-draft-page"
import {
  isFileAttachment,
  isInlineImageAttachment,
  isInlineMediaAttachment,
} from "@/lib/attachment-files"
import {
  uploadDraftFile,
  uploadDraftImageFile,
} from "@/lib/storage/issue-image-upload"
import {
  mediaPlayabilityHint,
  prepareMediaUpload,
  uploadDraftMediaFile,
} from "@/lib/storage/media-upload"
import { cn } from "@/lib/utils"
import {
  MarkdownEditor,
  type MarkdownEditorRef,
} from "@/components/issue-editor/markdown-editor"
import { FilesSectionView } from "@/components/issue-files-section"
import { IssuePropertiesPanel } from "@/components/issue-properties-panel"
import { PropertiesTrayCard } from "@/components/issue-properties-tray"
import { IssueTitleInput } from "@/components/issue-title-field"
import {
  HEADER_BUTTON_CLASS,
  MOBILE_DETAIL_SCREEN_CLASS,
  MobileDetailHeader,
} from "@/components/team/mobile-detail-header"

const UiCloseIcon = conceptIcon(`ui-close`)

// EXP-1170: the New issue PAGE — the issue detail (`issue-detail-view.tsx`)
// in DRAFT mode, ×4. Same chrome, same title row, same properties tray, same
// editor and Files section; what differs is only what a not-yet-filed issue
// cannot have: no identifier (the header says "New issue"), no pin, PR,
// faces, coding, timeline, relations or bottom bar, and the trailing cluster
// is Create plus an `×` whose tooltip says "Discard draft" (EXP-1191).
// Everything typed autosaves to the draft row (`use-issue-draft-editor.ts`).
//
// EXP-1212: a draft WITH content never goes silently (`draftExitPrompt`). The
// `×` first asks the destructive discard confirm; every other in-app
// navigation (Back, a sidebar or tab bar entry, another screen) is HELD by
// the router blocker and asks Create · Keep as draft · Discard, then
// continues to where the person was going. The page's own exits (a filed
// Create, a confirmed Discard) bypass it; closing the browser tab does not
// ask (the autosave keeps the draft).

export interface IssueDraftPageProps {
  draftId: string
  teamId: string
  teamSlug: string
  boards: Board[]
  users: User[]
  labels: readonly { id: string }[]
  /** The synced row present on open (a reopened draft), else undefined. */
  draft?: IssueDraft
  initialBoardId: string
  initialStatusId?: string
  /** The `?from=` origin — Back returns there, Create keeps it. */
  from?: string
}

export function IssueDraftPage({
  draftId,
  teamId,
  teamSlug,
  boards,
  users,
  labels,
  draft,
  initialBoardId,
  initialStatusId,
  from,
}: IssueDraftPageProps) {
  const isMobile = useIsMobile()
  const navigate = useNavigate()
  const editorRef = useRef<MarkdownEditorRef>(null)
  const editor = useIssueDraftEditor({
    draftId,
    teamId,
    initialBoardId,
    initialStatusId,
    draft,
    boards,
    labels,
    users,
  })
  const board =
    boards.find((row) => row.id === editor.boardId) ??
    boards.find((row) => row.id === initialBoardId) ??
    boards[0]

  const [uploadStatusText, setUploadStatusText] = useState<string | null>(null)
  const [attachmentStatus, setAttachmentStatus] = useState<string | null>(null)
  const [discardConfirmOpen, setDiscardConfirmOpen] = useState(false)

  // EXP-1162: the detail chrome — the title row scrolls away under the
  // header, which then breaks into the collapsed title.
  const [mobileHeaderRef, mobileHeaderSize] = useMeasuredSize()
  const [clusterRef, clusterSize] = useMeasuredSize()
  const [trayRef, traySize] = useMeasuredSize()
  const {
    scrollRef,
    titleRef,
    collapsed: titleCollapsed,
  } = useTitleCollapsed(isMobile ? mobileHeaderSize.height : WORK_BAR_HEIGHT)

  // Back = the list this draft was opened from, else its board.
  // `ignoreBlocker`: only the page's OWN exits (a confirmed Discard) pass
  // the leave blocker unasked; Back goes through it.
  const goBack = useCallback(
    (ignoreBlocker = false) => {
      void navigate({
        ...((originListNavigation(teamSlug, parseOrigin(from)) ?? {
          to: `/t/$teamSlug/boards/$boardSlug`,
          params: { teamSlug, boardSlug: board?.slug ?? `` },
          search: {},
        }) as object),
        ignoreBlocker,
      } as never)
    },
    [navigate, teamSlug, from, board?.slug]
  )

  const handleBack = () => {
    void editor.leave()
    goBack()
  }

  const handleDiscard = async () => {
    setDiscardConfirmOpen(false)
    // A Create in flight wins (R2): nothing is thrown away, nothing leaves.
    if (!(await editor.discard())) return
    goBack(true)
  }

  // The `×`: a draft with content asks first.
  const requestDiscard = () => {
    if (draftExitPrompt(`discard`, editor.hasContent) === `discardConfirm`) {
      setDiscardConfirmOpen(true)
    } else {
      void handleDiscard()
    }
  }

  const handleCreate = async () => {
    if (!editor.canCreate) return
    const created = await editor.create()
    if (!created) return
    void navigate({
      to: `/t/$teamSlug/boards/$boardSlug/issues/$issueIdentifier`,
      params: {
        teamSlug,
        boardSlug: created.boardSlug ?? board?.slug ?? ``,
        issueIdentifier: created.identifier,
      },
      search: from ? { from } : {},
      replace: true,
      ignoreBlocker: true,
    })
  }

  // ── Leaving (EXP-1212): any in-app navigation to another path is HELD
  // while the draft has content and no Create is in flight (R2). Read
  // through refs so the blocker registers once; the browser's own unload is
  // never held.
  // Known limit (not fixed): a held browser Forward or multi-step history
  // pop answered with "stay" can leave the URL one step off — that is
  // @tanstack/history's own blocker behaviour.
  const hasContentRef = useRef(editor.hasContent)
  hasContentRef.current = editor.hasContent
  const isCreatingRef = useRef(editor.isCreating)
  isCreatingRef.current = editor.isCreating
  const shouldBlockLeave = useCallback(
    ({
      current,
      next,
    }: {
      current: { pathname: string }
      next: { pathname: string }
    }) =>
      next.pathname !== current.pathname &&
      !isCreatingRef.current() &&
      draftExitPrompt(`leave`, hasContentRef.current) === `leave`,
    []
  )
  const blocker = useBlocker({
    shouldBlockFn: shouldBlockLeave,
    enableBeforeUnload: false,
    withResolver: true,
  })
  const [leaveBusy, setLeaveBusy] = useState(false)
  // One answer per held navigation (R6): a second click before the
  // disabled state renders never replays it.
  const leaveBusyRef = useRef(false)
  // Run the choice, then continue to the HELD destination; a choice that
  // failed (a Create the server refused, a "Keep" whose save failed) stays
  // on the page — its toast already said why — and drops the held
  // navigation (R3).
  const resolveLeave = async (choice: () => Promise<boolean>) => {
    if (blocker.status !== `blocked` || leaveBusyRef.current) return
    const { proceed, reset } = blocker
    leaveBusyRef.current = true
    setLeaveBusy(true)
    const ok = await choice()
    leaveBusyRef.current = false
    setLeaveBusy(false)
    if (ok) proceed()
    else reset()
  }
  const leaveCreate = () =>
    resolveLeave(async () => (await editor.create()) !== null)
  const leaveKeep = () => resolveLeave(() => editor.leave())
  const leaveDiscard = () => resolveLeave(() => editor.discard())
  // R5: the dialog opens on "Keep as draft", never on the destructive
  // Discard (Radix would focus the first button, and Enter would delete).
  const leaveKeepRef = useRef<HTMLButtonElement | null>(null)

  // Cmd/Ctrl+Enter anywhere on the page files it. Capture phase, so the
  // description editor never sees it as its own hard break. Only for keys
  // aimed INSIDE the page — never a dialog's or a picker's (portalled) own.
  const rootRef = useRef<HTMLDivElement>(null)
  const createRef = useRef(handleCreate)
  createRef.current = handleCreate
  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key !== `Enter` || !(event.metaKey || event.ctrlKey)) return
      const target = event.target
      if (!(target instanceof Node) || !rootRef.current?.contains(target)) {
        return
      }
      if (
        target instanceof Element &&
        target.closest(
          `[role="dialog"],[role="menu"],[role="listbox"],[data-radix-popper-content-wrapper]`
        )
      ) {
        return
      }
      event.preventDefault()
      event.stopPropagation()
      void createRef.current()
    }
    document.addEventListener(`keydown`, onKeyDown, true)
    return () => document.removeEventListener(`keydown`, onKeyDown, true)
  }, [])

  // ── Eager uploads (EXP-878): the row exists before anything uploads
  // against it, so the description only ever carries final URLs. ─────────
  const trackUpload = async (task: () => Promise<void>, fallback: string) => {
    setAttachmentStatus(null)
    editor.beginUpload()
    try {
      await task()
    } catch (error) {
      setAttachmentStatus(error instanceof Error ? error.message : fallback)
    } finally {
      editor.endUpload()
    }
  }

  const syncDescription = () => {
    const markdown = editorRef.current?.getMarkdown()
    if (markdown != null) editor.setDescription(markdown)
  }

  const handleImageFiles = (files: File[], placement: `insert` | `append`) =>
    trackUpload(async () => {
      const id = await editor.ensureDraft()
      for (const file of files) {
        const uploaded = await uploadDraftImageFile(id, file)
        const image = { alt: file.name, src: uploaded.url }
        if (placement === `append`) editorRef.current?.appendImage(image)
        else editorRef.current?.insertImage(image)
        syncDescription()
      }
    }, `Failed to upload image`)

  const handleMediaFiles = (files: File[], placement: `insert` | `append`) =>
    trackUpload(async () => {
      const id = await editor.ensureDraft()
      for (const file of files) {
        try {
          setUploadStatusText(`Preparing ${file.name}…`)
          const prepared = await prepareMediaUpload(file, (stage) =>
            setUploadStatusText(
              stage === `remuxing`
                ? `Converting ${file.name} to MP4…`
                : `Preparing ${file.name}…`
            )
          )
          setUploadStatusText(`Uploading ${prepared.file.name}…`)
          const uploaded = await uploadDraftMediaFile(id, prepared, {
            onProgress: (percent) =>
              setUploadStatusText(
                `Uploading ${prepared.file.name}… ${percent}%`
              ),
          })
          const block = { label: uploaded.filename, src: uploaded.url }
          if (placement === `append`) editorRef.current?.appendMedia(block)
          else editorRef.current?.insertMedia(block)
          syncDescription()
          const hint = mediaPlayabilityHint(uploaded)
          if (hint) toast.message(hint)
        } finally {
          setUploadStatusText(null)
        }
      }
    }, `Failed to upload media`)

  const handlePlainFiles = (files: File[]) =>
    trackUpload(async () => {
      const id = await editor.ensureDraft()
      for (const file of files) {
        const uploaded = await uploadDraftFile(id, file)
        editor.addFile({
          id: uploaded.id,
          filename: uploaded.filename,
          contentType: uploaded.contentType,
          sizeBytes: uploaded.sizeBytes,
          url: uploaded.url,
        })
      }
    }, `Failed to upload file`)

  // The Files section's attach button: inline picks (images, clips) append to
  // the description, the rest become Files rows.
  const handleAttachFiles = async (files: File[]) => {
    const images = files.filter((file) => isInlineImageAttachment(file.type))
    const media = files.filter((file) => isInlineMediaAttachment(file.type))
    const others = files.filter((file) => isFileAttachment(file.type))
    if (images.length > 0) await handleImageFiles(images, `append`)
    if (media.length > 0) await handleMediaFiles(media, `append`)
    if (others.length > 0) await handlePlainFiles(others)
  }

  const disabled = editor.creating

  const createButton = (
    <Button
      size="sm"
      onClick={() => void handleCreate()}
      disabled={!editor.canCreate}
      data-testid="issue-draft-create"
    >
      {ISSUE_DRAFT_COPY.create}
    </Button>
  )

  // EXP-1191: Discard is the draft's only action, so it is a bare `×` with
  // the copy as its tooltip, not a one-item `…` menu.
  const discardButton = (phone: boolean) => (
    <Tooltip>
      <TooltipTrigger asChild>
        <Button
          variant="ghost"
          size={phone ? `icon` : `icon-sm`}
          className={phone ? HEADER_BUTTON_CLASS : undefined}
          aria-label={ISSUE_DRAFT_COPY.discard}
          disabled={disabled}
          onClick={requestDiscard}
          data-testid="issue-draft-discard"
        >
          <UiCloseIcon className="size-4" />
        </Button>
      </TooltipTrigger>
      <TooltipContent>{ISSUE_DRAFT_COPY.discard}</TooltipContent>
    </Tooltip>
  )

  const collapsedTitle = (align: `start` | `center`) => (
    <CollapsedTitle
      align={align}
      identifier={ISSUE_DRAFT_COPY.header}
      title={editor.title.trim() || ISSUE_DRAFT_COPY.untitled}
    />
  )

  const titleInput = (
    <IssueTitleInput
      value={editor.title}
      onChange={editor.setTitle}
      onBlur={editor.onTitleBlur}
      onEnter={() => editorRef.current?.focus()}
      placeholder={ISSUE_DRAFT_COPY.titlePlaceholder}
      readOnly={disabled}
      autoFocus
    />
  )

  const tray = (
    <PropertiesTrayCard>
      <div className="min-w-0 flex-1">
        <IssuePropertiesPanel
          className="md:px-0"
          status={editor.status}
          onStatusChange={editor.setStatus}
          priority={editor.priority}
          onPriorityChange={editor.setPriority}
          assigneeId={editor.assigneeId}
          onAssigneeChange={editor.setAssigneeId}
          users={users}
          teamId={teamId}
          selectedLabelIds={editor.labelIds}
          onToggleLabel={editor.toggleLabel}
          dueDate={editor.dueDate}
          onDueDateSelect={editor.setDueDate}
          boardColor={board?.color ?? `#71717a`}
          boardPrefix={board?.prefix ?? ``}
          boardIcon={board?.icon}
          boardRepositoryId={board?.repositoryId}
          boardId={editor.boardId}
          onBoardChange={editor.setBoardId}
          boardConfirm={false}
          hideBoard={boards.length <= 1}
          creatableOnly
          disabled={disabled}
        />
      </div>
    </PropertiesTrayCard>
  )

  const descriptionEditor = (
    <div className="max-md:px-1">
      <MarkdownEditor
        ref={editorRef}
        markdown={editor.description}
        editable={!disabled}
        // EXP-1191: the caret scrolls clear of the sticky bar + tray.
        topScrollInset={
          isMobile ? undefined : WORK_BAR_HEIGHT + traySize.height
        }
        onChange={editor.setDescription}
        onBlur={editor.onDescriptionBlur}
        placeholder={ISSUE_DRAFT_COPY.descriptionPlaceholder}
        imageUpload={{
          enabled: !disabled,
          uploading: editor.uploading,
          statusText: uploadStatusText,
          onFiles: (files) => handleImageFiles(files, `insert`),
          onMediaFiles: (files) => handleMediaFiles(files, `insert`),
          onOtherFiles: handlePlainFiles,
        }}
      />
    </div>
  )

  const attachmentError = attachmentStatus ? (
    <p className="px-5 py-2 text-xs text-destructive">{attachmentStatus}</p>
  ) : null

  const filesSection = (
    <FilesSectionView
      files={editor.files}
      readOnly={disabled}
      onAttach={handleAttachFiles}
      onDelete={editor.removeFile}
      uploading={editor.uploading}
    />
  )

  // EXP-1212: the destructive confirm the `×` raises on a draft with content.
  const discardConfirm = (
    <AlertDialog
      open={discardConfirmOpen}
      onOpenChange={(open) => {
        if (!open) setDiscardConfirmOpen(false)
      }}
    >
      <AlertDialogContent data-testid="issue-draft-discard-confirm">
        <AlertDialogHeader>
          <AlertDialogTitle>
            {ISSUE_DRAFT_COPY.discardConfirm.title}
          </AlertDialogTitle>
          <AlertDialogDescription>
            {ISSUE_DRAFT_COPY.discardConfirm.body}
          </AlertDialogDescription>
        </AlertDialogHeader>
        <AlertDialogFooter>
          <AlertDialogCancel>Cancel</AlertDialogCancel>
          <AlertDialogAction
            className="bg-destructive text-white hover:bg-destructive/90"
            onClick={(event) => {
              event.preventDefault()
              void handleDiscard()
            }}
          >
            {ISSUE_DRAFT_COPY.discardConfirm.confirm}
          </AlertDialogAction>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  )

  // EXP-1212: the held navigation's three choices (the blocked-start
  // dialog's shape). Dismissing it (Esc, scrim) stays on the page.
  const leaveDialog = (
    <Dialog
      open={blocker.status === `blocked`}
      onOpenChange={(open) => {
        if (!open && !leaveBusy) blocker.reset?.()
      }}
    >
      <DialogContent
        mobile="alert"
        data-testid="issue-draft-leave-dialog"
        onOpenAutoFocus={(event) => {
          event.preventDefault()
          leaveKeepRef.current?.focus()
        }}
      >
        <DialogHeader>
          <DialogTitle>{ISSUE_DRAFT_COPY.leave.title}</DialogTitle>
          <DialogDescription>{ISSUE_DRAFT_COPY.leave.body}</DialogDescription>
        </DialogHeader>
        <DialogFooter>
          <Button
            variant="destructive"
            disabled={leaveBusy}
            onClick={() => void leaveDiscard()}
          >
            {ISSUE_DRAFT_COPY.leave.discard}
          </Button>
          <Button
            ref={leaveKeepRef}
            variant="outline"
            disabled={leaveBusy}
            onClick={() => void leaveKeep()}
          >
            {ISSUE_DRAFT_COPY.leave.keep}
          </Button>
          <Button
            disabled={leaveBusy || !editor.canCreate}
            onClick={() => void leaveCreate()}
          >
            {ISSUE_DRAFT_COPY.leave.create}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )

  if (isMobile) {
    return (
      <div
        ref={rootRef}
        className={MOBILE_DETAIL_SCREEN_CLASS}
        data-testid="issue-draft-page"
      >
        <MobileDetailHeader
          overlay
          ref={mobileHeaderRef}
          title={
            titleCollapsed ? (
              collapsedTitle(`center`)
            ) : (
              <span>{ISSUE_DRAFT_COPY.header}</span>
            )
          }
          backLabel="Back"
          onBack={handleBack}
          menu={
            <div className="flex shrink-0 items-center gap-1">
              <Pill
                size="md"
                mode="action"
                primary
                onClick={() => void handleCreate()}
                disabled={!editor.canCreate}
                data-testid="issue-draft-create"
              >
                {ISSUE_DRAFT_COPY.create}
              </Pill>
              {discardButton(true)}
            </div>
          }
        />
        <div
          ref={scrollRef}
          className="flex-1 overflow-y-auto pb-[max(2rem,env(safe-area-inset-bottom))]"
          style={{ paddingTop: mobileHeaderSize.height }}
        >
          <div ref={titleRef}>{titleInput}</div>
          {tray}
          <div className="pt-2">{descriptionEditor}</div>
          {attachmentError}
          {filesSection}
        </div>
        {discardConfirm}
        {leaveDialog}
      </div>
    )
  }

  return (
    <div
      ref={rootRef}
      className="flex h-full min-h-0 flex-col"
      data-testid="issue-draft-page"
    >
      <div className="flex min-h-0 flex-1 overflow-hidden">
        <div className="flex min-w-0 flex-1 flex-col overflow-hidden">
          <div
            ref={scrollRef}
            className="min-h-0 flex-1 overflow-y-auto"
            data-detail-scroll=""
          >
            <WorkHeader
              floating
              edge={false}
              collapsed={titleCollapsed}
              title={collapsedTitle(`start`)}
              trailingRef={clusterRef}
              trailing={
                <>
                  {createButton}
                  {discardButton(false)}
                </>
              }
            />
            <div className={WORK_COLUMN_CLASS}>
              {/* EXP-1191: a little air above the title — with no parent
                  line or identifier above it, it hugged the card's top. */}
              <div
                ref={titleRef}
                className="pt-4"
                style={{ paddingRight: clusterSize.width }}
              >
                {titleInput}
              </div>
            </div>
            {/* EXP-1191: the tray stays in view, like the issue's. */}
            <WorkStickyTray ref={trayRef} collapsed={titleCollapsed}>
              {tray}
            </WorkStickyTray>
            <div className={cn(WORK_COLUMN_CLASS, `pb-8`)}>
              {descriptionEditor}
              {attachmentError}
              {filesSection}
            </div>
          </div>
        </div>
      </div>
      {discardConfirm}
      {leaveDialog}
    </div>
  )
}

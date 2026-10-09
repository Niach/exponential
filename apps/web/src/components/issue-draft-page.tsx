import { useCallback, useEffect, useRef, useState } from "react"
import { useBlocker, useNavigate } from "@tanstack/react-router"
import {
  Button,
  CollapsedTitle,
  Pill,
  Prompt,
  toast,
  useIsMobile,
  WORK_BAR_HEIGHT,
  WORK_COLUMN_CLASS,
  WorkHeader,
  WorkStickyTray,
} from "@exp/ui"
import type { Board, Issue, IssueDraft, User } from "@/db/schema"
import { useMeasuredSize, useTitleCollapsed } from "@/hooks/use-detail-chrome"
import {
  useIssueDraftEditor,
  type DraftConsumedElsewhere,
} from "@/hooks/use-issue-draft-editor"
import { originListNavigation, parseOrigin } from "@/lib/detail-origin"
import { draftExitPrompt, ISSUE_DRAFT_COPY } from "@/lib/issue-draft-page"
import { uploadDraftAttachment } from "@/lib/storage/issue-image-upload"
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
  MOBILE_DETAIL_SCREEN_CLASS,
  MobileDetailHeader,
} from "@/components/team/mobile-detail-header"

// EXP-1170: the New issue PAGE — the issue detail (`issue-detail-view.tsx`)
// in DRAFT mode, ×4. Same chrome, same title row, same properties tray, same
// editor and Files section; what differs is only what a not-yet-filed issue
// cannot have: no identifier (the header says "New issue"), no pin, PR,
// faces, coding, timeline, relations or bottom bar, and the trailing cluster
// is Create alone (EXP-1247: Back + Create ×4, the `×` is gone; discarding
// is the leave dialog's answer). Everything typed autosaves to the draft row
// (`use-issue-draft-editor.ts`).
//
// EXP-1212: a draft WITH content never goes silently (`draftExitPrompt`).
// Every in-app navigation (Back, a sidebar or tab bar entry, another screen)
// is HELD by the router blocker and asks Discard · Create issue · Save
// draft, then continues to where the person was going. The page's own exits
// (a filed Create) bypass it; closing the browser tab does not ask (the
// autosave keeps the draft).
//
// EXP-1231: the draft may be consumed ELSEWHERE (another tab or device
// created the issue from it, or discarded it). The controller notices
// through the shape; created = this page becomes the issue's detail exactly
// as its own Create would (replace, no prompt), discarded = a toast and
// Back. Both are the page's own exits: the blocker never asks.

// EXP-1212/EXP-1215: the two prompts are the shared `Prompt` (one question,
// no ✕, no body, one row of the 32px `Pill` capsules) on phone AND desktop.

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
  // Land on an issue — the page's own Create and a create elsewhere
  // (EXP-1231) both REPLACE the draft entry, keep `?from=`, and pass the
  // leave blocker (the draft is consumed; there is nothing to ask about).
  const openIssue = (issue: Pick<Issue, `identifier` | `boardId`>) => {
    void navigate({
      to: `/t/$teamSlug/boards/$boardSlug/issues/$issueIdentifier`,
      params: {
        teamSlug,
        boardSlug:
          boards.find((row) => row.id === issue.boardId)?.slug ??
          boards[0]?.slug ??
          ``,
        issueIdentifier: issue.identifier,
      },
      search: from ? { from } : {},
      replace: true,
      ignoreBlocker: true,
    })
  }
  const onConsumedElsewhere = (consumed: DraftConsumedElsewhere) => {
    if (consumed.kind === `created`) {
      openIssue(consumed.issue)
      return
    }
    toast.message(ISSUE_DRAFT_COPY.discardedElsewhere)
    goBackRef.current(true)
  }
  const editor = useIssueDraftEditor({
    draftId,
    teamId,
    initialBoardId,
    initialStatusId,
    draft,
    boards,
    labels,
    users,
    onConsumedElsewhere,
  })
  const board =
    boards.find((row) => row.id === editor.boardId) ??
    boards.find((row) => row.id === initialBoardId) ??
    boards[0]

  const [uploadStatusText, setUploadStatusText] = useState<string | null>(null)
  const [attachmentStatus, setAttachmentStatus] = useState<string | null>(null)

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
  // `ignoreBlocker`: only the page's OWN exits (a discarded-elsewhere toast)
  // pass the leave blocker unasked; Back goes through it.
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

  const goBackRef = useRef(goBack)
  goBackRef.current = goBack

  const handleBack = () => {
    void editor.leave()
    goBack()
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
  // navigation (R3). A choice that CONSUMED the draft (Create, Discard)
  // never replays the held push (`proceed`): that keeps this draft's entry
  // in history and lands Back on an empty New issue page at the consumed
  // id. It resets the blocker and REPLACES the entry with the destination
  // instead. Save draft keeps the entry (Back reopens the draft); a held
  // Back/Forward has no push to replace and goes on as held.
  const resolveLeave = async (
    choice: () => Promise<boolean>,
    consumed = false
  ) => {
    if (blocker.status !== `blocked` || leaveBusyRef.current) return
    const { proceed, reset, next, action } = blocker
    leaveBusyRef.current = true
    setLeaveBusy(true)
    const ok = await choice()
    leaveBusyRef.current = false
    setLeaveBusy(false)
    if (!ok) {
      reset()
      return
    }
    if (consumed && action === `PUSH`) {
      reset()
      void navigate({
        to: next.pathname,
        search: next.search,
        replace: true,
        ignoreBlocker: true,
      } as never)
      return
    }
    proceed()
  }
  const leaveCreate = () =>
    resolveLeave(async () => (await editor.create()) !== null, true)
  const leaveKeep = () => resolveLeave(() => editor.leave())
  const leaveDiscard = () => resolveLeave(() => editor.discard(), true)
  // R5: the dialog opens on its default "Create issue" (or "Save draft"
  // while Create is disabled, no title), never on the destructive Discard
  // (`Prompt` never focuses a destructive answer).

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

  const handleImageFiles = (files: File[]) =>
    trackUpload(async () => {
      const id = await editor.ensureDraft()
      for (const file of files) {
        const uploaded = await uploadDraftAttachment(id, file)
        editorRef.current?.insertImage({ alt: file.name, src: uploaded.url })
        syncDescription()
      }
    }, `Failed to upload image`)

  const handleMediaFiles = (files: File[]) =>
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
          editorRef.current?.insertMedia({
            label: uploaded.filename,
            src: uploaded.url,
          })
          syncDescription()
          const hint = mediaPlayabilityHint(uploaded)
          if (hint) toast.message(hint)
        } finally {
          setUploadStatusText(null)
        }
      }
    }, `Failed to upload media`)

  // Non-inline pastes/drops, and EVERY pick of a FILE path (the rail's
  // "Attach file", the Files paperclip: EXP-1247 `asFile`, images too)
  // become Files rows; nothing here enters the description.
  const handlePlainFiles = (files: File[], options?: { asFile?: boolean }) =>
    trackUpload(async () => {
      const id = await editor.ensureDraft()
      for (const file of files) {
        const uploaded = await uploadDraftAttachment(id, file, options)
        editor.addFile({
          id: uploaded.id,
          filename: uploaded.filename,
          contentType: uploaded.contentType,
          sizeBytes: uploaded.sizeBytes,
          url: uploaded.url,
          asFile: uploaded.asFile ?? options?.asFile ?? false,
        })
      }
    }, `Failed to upload file`)

  const handleAttachFiles = (files: File[]) =>
    handlePlainFiles(files, { asFile: true })

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
          onFiles: handleImageFiles,
          onMediaFiles: handleMediaFiles,
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

  // EXP-1212: the held navigation's three answers, Thunderbird's save
  // prompt: a quiet destructive Discard set apart on the leading edge, then
  // Save draft (the plain pill) and the DEFAULT Create issue (the primary
  // pill, trailing, initial focus, so Enter creates; disabled without a
  // title, then Save draft takes the focus). No ✕: dismissing (Esc, scrim)
  // stays on the page.
  const leaveDialog = (
    <Prompt
      open={blocker.status === `blocked`}
      onOpenChange={() => {}}
      onDismiss={() => blocker.reset?.()}
      busy={leaveBusy}
      data-testid="issue-draft-leave-dialog"
      title={ISSUE_DRAFT_COPY.leave.title}
      actions={[
        {
          label: ISSUE_DRAFT_COPY.leave.discard,
          role: `quietDestructive`,
          onSelect: () => void leaveDiscard(),
        },
        {
          label: ISSUE_DRAFT_COPY.leave.keep,
          autoFocus: !editor.canCreate,
          onSelect: () => void leaveKeep(),
        },
        {
          label: ISSUE_DRAFT_COPY.leave.create,
          role: `primary`,
          disabled: !editor.canCreate,
          onSelect: () => void leaveCreate(),
        },
      ]}
    />
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
              trailing={createButton}
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
      {leaveDialog}
    </div>
  )
}

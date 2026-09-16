import { useEffect, useRef, useState } from "react"
import type * as React from "react"
import { Files } from "lucide-react"
import { toast } from "sonner"
import { conceptIcon, useIsMobile, Pill, type SessionDotTone } from "@exp/ui"
import { eq, useLiveQuery } from "@tanstack/react-db"
import type { Issue, User, Board } from "@/db/schema"
import { issueCollection } from "@/lib/collections"
import { trpc } from "@/lib/trpc-client"
import {
  getIssueDescriptionText,
  normalizeIssueDescriptionText,
} from "@/lib/domain"
import {
  uploadIssueFile,
  uploadIssueImageFile,
} from "@/lib/storage/issue-image-upload"
import {
  mediaPlayabilityHint,
  prepareMediaUpload,
  uploadIssueMediaFile,
} from "@/lib/storage/media-upload"
import { isInlineMediaAttachment } from "@/lib/attachment-files"
import { useSession } from "@/hooks/use-session"
import { cn, parseLocalDate } from "@/lib/utils"
import { useIssueRefs } from "@/components/issue-ref-provider"
import { useTeamStatusesContext } from "@/hooks/use-team-statuses"
import { useIssuePropertyHandlers } from "@/hooks/use-issue-property-handlers"
import {
  MarkdownEditor,
  type MarkdownEditorRef,
} from "@/components/issue-editor/markdown-editor"
import { IssueTimeline } from "@/components/issue-timeline"
import { IssueCodingControl, IssuePrRow } from "@/components/issue-coding-rows"
import { IssueDetailMobileBar } from "@/components/issue-detail-mobile-bar"
import { IssueMobileHeader } from "@/components/issue-mobile-header"
import { MOBILE_WORK_BAR_CLEARANCE } from "@/components/mobile-work-bar"
import { IssueEditorMobileProperties } from "@/components/issue-editor/mobile-properties"
import { IssueFilesSection } from "@/components/issue-files-section"
import { IssueRelationsSection } from "@/components/issue-relations-card"
import { IssueChip } from "@/components/issue-chip"
import { SubIssueComposer } from "@/components/sub-issue-composer"
import { PinToggleButton } from "@/components/pin-toggle-button"
import { WidgetSubmissionCard } from "@/components/widget-submission-card"
import { IssueActionsMenu } from "@/components/issue-actions-menu"
import { IssuePropertiesTray } from "@/components/issue-properties-tray"
import { IssueTitleField } from "@/components/issue-title-field"
import { WORK_COLUMN_CLASS, WorkHeader } from "@/components/work-header"
import { PrGraphBadge } from "@/components/pr-graph-badge"

const UiUndoIcon = conceptIcon(`ui-undo`)

interface IssueDetailViewProps {
  issue: Issue
  users: User[]
  board: Board
  teamSlug: string
  teamId: string
  readOnly?: boolean
  /** EXP-851: the `?from=` token this issue was opened with
   *  (`lib/detail-origin.ts`) — the phone header's back button returns THERE
   *  (the inbox, the board, a review queue) instead of always to the board. */
  origin?: string
  /** The session→issue hop draws its own back-to-run header, so the issue's
   *  phone header would be a second bar on the same line. */
  showMobileHeader?: boolean
  /** EXP-870/877: the md+ work header's face toggle (`WorkFaceToggle`) — the
   *  issue and its run are one work tab with faces. */
  faceToggle?: React.ReactNode
  /** EXP-893: the phone's Work screen parts — the face switcher for the
   *  bar's right circle (absent = the Start coding circle) and the shown
   *  session's state dot for the header title. */
  mobileWork?: {
    switcher?: React.ReactNode
    dot?: { tone: SessionDotTone; connecting?: boolean } | null
  }
}

// Canonical-issue banner shown on a duplicate's detail view: "Duplicate of
// #IDENT — {title}", clickable through to the canonical issue, with an Unmark
// action (clears the link; the server restores status atomically).
export function DuplicateOfBanner({
  duplicateOfId,
  onUnmark,
  readOnly,
}: {
  duplicateOfId: string
  onUnmark: () => void
  readOnly: boolean
}) {
  const issueRefs = useIssueRefs()
  const { data } = useLiveQuery(
    (query) =>
      query
        .from({ issues: issueCollection })
        .where(({ issues }) => eq(issues.id, duplicateOfId)),
    [duplicateOfId]
  )
  const canonical = (data?.[0] ?? null) as Issue | null
  if (!canonical) return null

  return (
    <div className="flex items-center gap-2 border-b border-border bg-accent/30 px-4 py-2 text-sm min-w-0">
      <Files className="size-4 shrink-0 text-muted-foreground" />
      <span className="shrink-0 text-muted-foreground">Duplicate of</span>
      {/* EXP-887: a duplicate-of reference names an ISSUE, so it draws the
          issue chip (glyph · identifier · title, hover preview) — the capsule
          this used to be was the wrong shape and repeated the title beside
          itself. */}
      <IssueChip
        issue={canonical}
        testId="duplicate-of-chip"
        onClick={
          issueRefs ? () => issueRefs.open(canonical.identifier) : undefined
        }
        // The banner is the chip's whole row: no 18rem cap on the title here.
        className="max-w-none"
      />
      {!readOnly && (
        <Pill mode="action" className="ml-auto" onClick={onUnmark}>
          <UiUndoIcon className="size-3" />
          Unmark
        </Pill>
      )}
    </div>
  )
}

export function IssueDetailView({
  issue,
  users,
  board,
  teamSlug,
  teamId,
  readOnly = false,
  origin,
  showMobileHeader = true,
  faceToggle,
  mobileWork,
}: IssueDetailViewProps) {
  const { data: session } = useSession()
  const currentUserId = session?.user?.id ?? null
  const isMobile = useIsMobile()

  const editorRef = useRef<MarkdownEditorRef>(null)
  const descriptionRef = useRef(getIssueDescriptionText(issue.description))
  // Two baselines in two coordinate systems, both always normalized. The
  // editor re-serializes whatever it parses, and markdown authored on other
  // clients (native apps, MCP, the widget) need not round-trip
  // byte-identically through TipTap — mixing the spaces made one applied
  // non-canonical description look like unsaved local edits forever,
  // deferring every later remote update and letting a mere focus+blur save
  // stale re-serialized text over newer remote saves.
  // - lastSavedDescriptionRef: EDITOR-serialized text at the last
  //   apply/save/settle — compared against the editor's local text to detect
  //   unsaved edits.
  // - syncedDescriptionRef: RAW synced text this view has accounted for —
  //   compared against the incoming value to detect new remote content.
  const lastSavedDescriptionRef = useRef(
    normalizeIssueDescriptionText(getIssueDescriptionText(issue.description))
  )
  const syncedDescriptionRef = useRef(
    normalizeIssueDescriptionText(getIssueDescriptionText(issue.description))
  )
  const saveQueueRef = useRef<Promise<void>>(Promise.resolve())
  const uploadQueueRef = useRef<Promise<void>>(Promise.resolve())

  const [description, setDescription] = useState(
    getIssueDescriptionText(issue.description)
  )
  const [attachmentStatus, setAttachmentStatus] = useState<string | null>(null)
  const [activeUploadCount, setActiveUploadCount] = useState(0)
  // EXP-824: the narrated progress of a media upload ("Uploading clip.mp4… 42%").
  const [uploadStatusText, setUploadStatusText] = useState<string | null>(null)
  // EXP-568: the floating phone bar steps aside while the description is being
  // written — the keyboard formatting rail owns the bottom edge then.
  const [descriptionFocused, setDescriptionFocused] = useState(false)
  const { resolve: resolveStatus } = useTeamStatusesContext()
  const statusOption = resolveStatus(issue)

  // EXP-877: ONE definition per property mutation, shared with the session
  // route's issue face (`use-issue-property-handlers.ts`); the hook also owns
  // the issue's label ids and the duplicate-status picker.
  const handlers = useIssuePropertyHandlers({ issue, teamSlug, readOnly })
  const { issueLabelIds, duplicatePicker } = handlers

  const incomingDescription = getIssueDescriptionText(issue.description)
  const normalizedIncoming = normalizeIssueDescriptionText(incomingDescription)

  // Destructive replace of the local editor content with a synced value —
  // setMarkdown resets the caret, so callers must ensure there are no unsaved
  // local edits worth keeping.
  const applyIncomingDescription = (nextDescription: string) => {
    setDescription(nextDescription)
    descriptionRef.current = nextDescription
    syncedDescriptionRef.current =
      normalizeIssueDescriptionText(nextDescription)
    editorRef.current?.setMarkdown(nextDescription)
    // Settle the local text and the unsaved-edits baseline from the editor's
    // OWN serialization of what it just parsed (setMarkdown also re-enters
    // onChange with it), never from the raw incoming string — the two need
    // not match byte-for-byte. While the editor instance does not exist yet
    // the raw value stands in for both, and the editor is then created from
    // that same value, so the pair stays consistent either way.
    const editorMarkdown = editorRef.current?.getMarkdown()
    if (editorMarkdown != null) {
      setDescription(editorMarkdown)
      descriptionRef.current = editorMarkdown
    }
    lastSavedDescriptionRef.current = normalizeIssueDescriptionText(
      descriptionRef.current
    )
  }

  // Full reset when navigating to a different issue.
  useEffect(() => {
    applyIncomingDescription(incomingDescription)
    setAttachmentStatus(null)
  }, [issue.id])

  // Opening an issue clears its inbox notifications (EXP-92) — the safety net
  // for push taps and email deep links that never pass through the inbox (whose
  // row click keeps its own eager markRead). Fire-and-forget: failure just
  // leaves the row unread.
  useEffect(() => {
    trpc.notifications.markReadByIssue
      .mutate({ issueId: issue.id })
      .catch(() => {})
  }, [issue.id])

  // Sync description from Electric when another client changes it — without
  // clobbering typing in progress. An incoming value the editor already shows
  // (the Electric echo of a save can beat the tRPC response) only settles the
  // bookkeeping; with unsaved local edits the replace is deferred to the next
  // blur instead of wiping the user's text and resetting the caret.
  useEffect(() => {
    if (normalizedIncoming === syncedDescriptionRef.current) return
    const normalizedLocal = normalizeIssueDescriptionText(
      descriptionRef.current
    )
    if (normalizedIncoming === normalizedLocal) {
      syncedDescriptionRef.current = normalizedIncoming
      lastSavedDescriptionRef.current = normalizedLocal
      return
    }
    if (normalizedLocal !== lastSavedDescriptionRef.current) return
    applyIncomingDescription(incomingDescription)
  }, [normalizedIncoming])

  const queueDescriptionSave = async (nextDescription: string) => {
    if (readOnly) return
    const normalizedDescription = normalizeIssueDescriptionText(nextDescription)
    if (normalizedDescription === lastSavedDescriptionRef.current) {
      await saveQueueRef.current
      return
    }
    const saveTask = async () => {
      const baselineAtSaveStart = lastSavedDescriptionRef.current
      await trpc.issues.update.mutate({
        id: issue.id,
        description: normalizedDescription ? normalizedDescription : null,
      })
      // A remote apply, an echo settle, or an issue switch may have moved the
      // baselines while the mutate was in flight — rewinding them to this
      // save would mark the newer editor content as unsaved local edits.
      if (lastSavedDescriptionRef.current === baselineAtSaveStart) {
        lastSavedDescriptionRef.current = normalizedDescription
        syncedDescriptionRef.current = normalizedDescription
      }
    }
    const queuedSave = saveQueueRef.current.then(saveTask, saveTask)
    saveQueueRef.current = queuedSave.catch(() => undefined)
    try {
      await queuedSave
      setAttachmentStatus(null)
    } catch (error) {
      setAttachmentStatus(
        error instanceof Error ? error.message : `Failed to save description`
      )
      throw error
    }
  }

  const handleDescriptionBlur = async () => {
    const hadLocalEdits =
      normalizeIssueDescriptionText(descriptionRef.current) !==
      lastSavedDescriptionRef.current
    try {
      await queueDescriptionSave(descriptionRef.current)
    } catch {
      return
    }
    // A remote change that arrived mid-edit was deferred by the sync effect;
    // when this blur had nothing of ours to write over it, show it now. After
    // a real save the Electric echo of our own write reconciles instead.
    if (!hadLocalEdits && normalizedIncoming !== syncedDescriptionRef.current) {
      applyIncomingDescription(incomingDescription)
    }
  }

  const setDescriptionValue = (nextDescription: string) => {
    descriptionRef.current = nextDescription
    setDescription(nextDescription)
  }

  const enqueueUploadTask = async (task: () => Promise<void>) => {
    setActiveUploadCount((c) => c + 1)
    const queuedTask = uploadQueueRef.current.then(task, task)
    uploadQueueRef.current = queuedTask.catch(() => undefined)
    try {
      await queuedTask
    } finally {
      setActiveUploadCount((c) => c - 1)
    }
  }

  const handleImageFiles = async (files: File[]) => {
    setAttachmentStatus(null)
    try {
      await enqueueUploadTask(async () => {
        for (const file of files) {
          const { url } = await uploadIssueImageFile(issue.id, file)
          editorRef.current?.insertImage({ alt: file.name, src: url })
          const nextDescription =
            editorRef.current?.getMarkdown() ?? descriptionRef.current
          setDescriptionValue(nextDescription)
          await queueDescriptionSave(nextDescription)
        }
      })
    } catch (error) {
      setAttachmentStatus(
        error instanceof Error ? error.message : `Failed to upload image`
      )
    }
  }

  // Images picked via the Files section's attach button (EXP-316): they belong
  // in the description, appended at the bottom rather than at the caret.
  const handleAppendImageFiles = async (files: File[]) => {
    setAttachmentStatus(null)
    try {
      await enqueueUploadTask(async () => {
        for (const file of files) {
          const { url } = await uploadIssueImageFile(issue.id, file)
          editorRef.current?.appendImage({ alt: file.name, src: url })
          const nextDescription =
            editorRef.current?.getMarkdown() ?? descriptionRef.current
          setDescriptionValue(nextDescription)
          await queueDescriptionSave(nextDescription)
        }
      })
    } catch (error) {
      setAttachmentStatus(
        error instanceof Error ? error.message : `Failed to upload image`
      )
    }
  }

  // EXP-824: video/audio → probe + poster + `.mov` remux in the browser, a
  // progress-narrated upload, then the media block `[name](url)` at the caret
  // (paste/drop/rail) or the bottom (Files rail attach button). A clip that
  // is not H.264 MP4 gets a non-blocking "may not play everywhere" toast.
  const handleMediaFiles = async (
    files: File[],
    placement: `insert` | `append`
  ) => {
    setAttachmentStatus(null)
    try {
      await enqueueUploadTask(async () => {
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
            const uploaded = await uploadIssueMediaFile(issue.id, prepared, {
              onProgress: (percent) =>
                setUploadStatusText(
                  `Uploading ${prepared.file.name}… ${percent}%`
                ),
            })
            const block = { label: uploaded.filename, src: uploaded.url }
            if (placement === `append`) {
              editorRef.current?.appendMedia(block)
            } else {
              editorRef.current?.insertMedia(block)
            }
            const nextDescription =
              editorRef.current?.getMarkdown() ?? descriptionRef.current
            setDescriptionValue(nextDescription)
            await queueDescriptionSave(nextDescription)
            const hint = mediaPlayabilityHint(uploaded)
            if (hint) toast.message(hint)
          } finally {
            setUploadStatusText(null)
          }
        }
      })
    } catch (error) {
      setAttachmentStatus(
        error instanceof Error ? error.message : `Failed to upload media`
      )
    }
  }

  // The Files rail's attach button hands over every INLINE pick (images and
  // media both leave the rail): images append as image nodes, clips as media
  // blocks.
  const handleAppendInlineFiles = async (files: File[]) => {
    const media = files.filter((file) => isInlineMediaAttachment(file.type))
    const images = files.filter((file) => !isInlineMediaAttachment(file.type))
    if (images.length > 0) await handleAppendImageFiles(images)
    if (media.length > 0) await handleMediaFiles(media, `append`)
  }

  // Pasted/dropped files that are NOT inline-embeddable images (EXP-297): they
  // upload to the Files section instead of entering the markdown.
  const handleOtherFiles = async (files: File[]) => {
    setAttachmentStatus(null)
    try {
      await enqueueUploadTask(async () => {
        for (const file of files) {
          await uploadIssueFile(issue.id, file)
        }
      })
    } catch (error) {
      setAttachmentStatus(
        error instanceof Error ? error.message : `Failed to upload file`
      )
    }
  }

  // EXP-893: the bar's right circle is Start coding only while the issue has
  // nothing to switch to; the moment a run (or a PR) of mine exists the route
  // hands down the face switcher instead.
  const codingFab =
    currentUserId && isMobile && !mobileWork?.switcher ? (
      <IssueCodingControl
        issue={issue}
        board={board}
        teamId={teamId}
        currentUserId={currentUserId}
        variant="fab"
      />
    ) : null

  const dueDate = issue.dueDate ? parseLocalDate(issue.dueDate) : undefined

  // EXP-698 r5 — the phone's properties SHEET is the create form's row list,
  // not the desktop chip band: Status / Priority / Assignee / Due date /
  // Board as full-width rows, then the label chips. Same mutations as the
  // band above; Board rides the shared move-confirm dialog.
  const mobilePropertiesPanel = (
    <IssueEditorMobileProperties
      status={statusOption}
      priority={issue.priority}
      assigneeId={issue.assigneeId}
      selectedLabelIds={issueLabelIds}
      teamId={teamId}
      users={users}
      dueDate={dueDate}
      disabled={readOnly}
      board={{
        boardId: issue.boardId,
        teamId,
        issueIdentifier: issue.identifier,
        onBoardChange: handlers.handleBoardChange,
      }}
      onStatusChange={handlers.handleStatusChange}
      onPriorityChange={handlers.handlePriorityChange}
      onAssigneeChange={handlers.handleAssigneeChange}
      onToggleLabel={handlers.handleToggleLabel}
      onDueDateSelect={handlers.handleDueDateSelect}
      relations={{ issueId: issue.id, readOnly }}
    />
  )

  // EXP-877: row 2 of the work header — the properties tray with Merge and
  // the ONE coding action inside it (`issue-properties-tray.tsx`). The phone
  // renders the same node at the top of its scroll column, minus the coding
  // action: its floating bar's circle owns the start there.
  const propsTray = (showCodingAction: boolean) => (
    <IssuePropertiesTray
      issue={issue}
      board={board}
      users={users}
      teamId={teamId}
      currentUserId={currentUserId}
      readOnly={readOnly}
      handlers={handlers}
      showCodingAction={showCodingAction}
    />
  )

  // EXP-778: the small pin toggle beside the title — pinned issues land in
  // the sidebar's Pinned group on every client.
  const pinToggle = (
    // EXP-850 §10: ghost everywhere a pin toggle renders — no circle stroke,
    // no fill (the session header and the action dialog already read this way).
    <PinToggleButton
      teamId={teamId}
      kind="issue"
      targetId={issue.id}
      variant="ghost"
    />
  )

  // EXP-893 phone header: the ONE header every face of the Work screen
  // wears (`issue-mobile-header.tsx`) — round back to the list this issue
  // was opened from, the state dot + IDENTIFIER centred, the `…` on the
  // right. Nothing jumps when the face flips.
  const mobileHeader = (
    <IssueMobileHeader
      issue={issue}
      board={board}
      teamSlug={teamSlug}
      teamId={teamId}
      readOnly={readOnly}
      origin={origin}
      handlers={handlers}
      dot={mobileWork?.dot ?? null}
      graphBadge={
        /* EXP-897: the same pill the md+ header wears, opening the same
           overlay as a sheet. */
        <PrGraphBadge
          teamId={issue.teamId}
          teamSlug={teamSlug}
          face="issue"
          issue={issue}
        />
      }
    />
  )

  const duplicateBanner = issue.duplicateOfId ? (
    <DuplicateOfBanner
      duplicateOfId={issue.duplicateOfId}
      readOnly={readOnly}
      onUnmark={handlers.handleUnmarkDuplicate}
    />
  ) : null

  const titleField = <IssueTitleField issue={issue} readOnly={readOnly} />

  const editor = (
    <div className="px-1">
      <MarkdownEditor
        ref={editorRef}
        markdown={description}
        editable={!readOnly}
        onChange={setDescriptionValue}
        onBlur={() => void handleDescriptionBlur()}
        placeholder="Add description..."
        onFocusChange={setDescriptionFocused}
        imageUpload={{
          enabled: !readOnly,
          uploading: activeUploadCount > 0,
          statusText: uploadStatusText,
          onFiles: handleImageFiles,
          onMediaFiles: (files) => handleMediaFiles(files, `insert`),
          onOtherFiles: handleOtherFiles,
        }}
      />
    </div>
  )

  // The attachments strip is gone (EXP-256) — the editor's rail/paste/drop and
  // the image node menu cover add/remove; only upload errors still need a
  // surface.
  const attachmentError = attachmentStatus ? (
    <p className="px-5 py-2 text-xs text-destructive">{attachmentStatus}</p>
  ) : null

  // EXP-297 Files rail: non-inline-image attachments straight from the synced
  // shape, plus the "Attach file" affordance for members.
  const filesSection = (
    <IssueFilesSection
      issueId={issue.id}
      readOnly={readOnly}
      onInlineFiles={handleAppendInlineFiles}
    />
  )

  // PR / pushed-branch link to the review-detail route (EXP-106) — stays in
  // the main column on every layout.
  const prRow = currentUserId ? (
    <IssuePrRow
      issue={issue}
      board={board}
      teamId={teamId}
      teamSlug={teamSlug}
      currentUserId={currentUserId}
    />
  ) : null

  // Keyed on issue.id: prev/next navigation swaps issues in place, and the
  // composer draft (and comment edit state) must not carry over — an unsent
  // reply typed on one issue would otherwise post to the next (REV-47).
  const timeline = currentUserId ? (
    <IssueTimeline
      key={issue.id}
      issue={issue}
      currentUserId={currentUserId}
      users={users}
      hideComposer={isMobile}
    />
  ) : null

  // Same mutation the timeline's own composer runs (EXP-568: on phones the
  // composer moved into the floating bar, which sits outside the timeline).
  const handleCommentSubmit = async (
    body: string,
    attachmentIds: string[]
  ) => {
    await trpc.comments.create.mutate({
      issueId: issue.id,
      body,
      attachmentIds,
    })
  }

  // EXP-760: the inline "Add sub-issues" composer. Desktop + members only —
  // a phone has no room for a second editor under the description, and a
  // read-only viewer has nothing to file. Keyed on the issue so prev/next
  // navigation never carries a half-typed child over (REV-47's rule).
  const subIssueComposer =
    !readOnly && !isMobile ? (
      <SubIssueComposer
        // Namespaced: the editor sibling is keyed on the bare issue id, and
        // React duplicates children that share a key.
        key={`sub-issues:${issue.id}`}
        parent={issue}
        teamId={teamId}
        users={users}
      />
    ) : null

  // EXP-42b: reporter/page/env metadata of widget-filed issues, members-only
  // (the server gates it; anonymous viewers never even fetch).
  const widgetCard = currentUserId ? (
    <WidgetSubmissionCard issueId={issue.id} source={issue.source} />
  ) : null

  if (isMobile) {
    return (
      <div className="flex flex-col h-full min-h-0">
        {showMobileHeader && mobileHeader}
        {duplicateBanner}
        {/* EXP-698: clearance for the floating bar below, so the last comment
            scrolls clear of it instead of ending under the glass
            (`MOBILE_WORK_BAR_CLEARANCE`); the tab bar itself is hidden on this
            route, so nothing else is reserved here. */}
        <div className={cn(`flex-1 overflow-y-auto`, MOBILE_WORK_BAR_CLEARANCE)}>
          {propsTray(false)}
          {titleField}
          {editor}
          {attachmentError}
          {filesSection}
          {prRow}
          {widgetCard}
          {timeline}
        </div>
        {currentUserId && (
          <IssueDetailMobileBar
            issueId={issue.id}
            users={users}
            propertiesNode={mobilePropertiesPanel}
            trailingNode={mobileWork?.switcher ?? codingFab}
            onSubmitComment={handleCommentSubmit}
            hidden={descriptionFocused}
          />
        )}
        {/* No `addRelation.dialog` here: the phone reaches relations through
            the properties sheet's own "Add relation" chip, which carries its
            own picker (issue-editor/mobile-properties.tsx). */}
        {duplicatePicker}
      </div>
    )
  }

  return (
    <div className="flex flex-col h-full min-h-0">
      {/* EXP-818: no breadcrumb row — the sidebar's board row already says
          where you are. EXP-877: the header is the ONE work header the
          session route renders too, FIXED above the scrolling body (the
          IDE's `work_header.rs`): title | face toggle · pin · `…`, then the
          properties tray with Merge and the coding action inside it. */}
      {duplicateBanner}
      <WorkHeader
        title={titleField}
        trailing={
          <>
            {/* EXP-897: what this issue is part of — its stack, its batch. */}
            <PrGraphBadge
              teamId={issue.teamId}
              teamSlug={teamSlug}
              face="issue"
              issue={issue}
            />
            {faceToggle}
            {pinToggle}
            <IssueActionsMenu
              issue={issue}
              board={board}
              teamSlug={teamSlug}
              readOnly={readOnly}
            />
          </>
        }
        tray={propsTray(true)}
      />
      <div className="flex flex-1 min-h-0 overflow-hidden">
        <div className="flex flex-1 min-w-0 flex-col overflow-hidden">
          <div className="flex-1 min-h-0 overflow-y-auto">
            <div className={WORK_COLUMN_CLASS}>
              {editor}
              {attachmentError}
              {filesSection}
              {/* EXP-760: relations moved BELOW the description (Linear's
                  order) and lost their card + "Relations" title — the group
                  headings are the labels, and the whole block is absent when
                  the issue has no relations. The phone carries them inside the
                  properties sheet instead. */}
              <IssueRelationsSection issueId={issue.id} readOnly={readOnly} />
              {subIssueComposer}
              {prRow}
              {widgetCard}
              {timeline}
            </div>
          </div>
        </div>
      </div>
      {duplicatePicker}
    </div>
  )
}

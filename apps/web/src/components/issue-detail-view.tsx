import { useCallback, useEffect, useRef, useState } from "react"
import type * as React from "react"
import { Files } from "lucide-react"
import {
  CollapsedTitle,
  conceptIcon,
  useIsMobile,
  MOBILE_WORK_BAR_CLEARANCE,
  Pill,
  WORK_BAR_HEIGHT,
  WORK_COLUMN_CLASS,
  WORK_STICKY_TOP_VAR,
  WorkHeader,
  WorkStickyTray,
  toast,
} from "@exp/ui"
import { eq, useLiveQuery } from "@tanstack/react-db"
import type { CommentAudience } from "@exp/db-schema/domain"
import type { Issue, User, Board } from "@/db/schema"
import { issueCollection } from "@/lib/collections"
import { issueMemoryOwner } from "@/lib/work-tab-memory"
import { useRememberedScroll } from "@/hooks/use-remembered-scroll"
import { useMeasuredSize, useTitleCollapsed } from "@/hooks/use-detail-chrome"
import { MOBILE_DETAIL_SCREEN_CLASS } from "@/components/team/mobile-detail-header"
import { TRPCClientError } from "@trpc/client"
import { trpc } from "@/lib/trpc-client"
import {
  getIssueDescriptionText,
  normalizeIssueDescriptionText,
} from "@/lib/domain"
import {
  descriptionSaves,
  resolveIncomingDescription,
} from "@/lib/unechoed-saves"
import { uploadIssueAttachment } from "@/lib/storage/issue-image-upload"
import {
  mediaPlayabilityHint,
  prepareMediaUpload,
  uploadIssueMediaFile,
} from "@/lib/storage/media-upload"
import { useSession } from "@/hooks/use-session"
import { cn } from "@/lib/utils"
import { useIssueRefs } from "@/components/issue-ref-provider"
import { useTeamStatusesContext } from "@/hooks/use-team-statuses"
import { useTeamById } from "@/hooks/use-team-data"
import type { IssueEstimation } from "@/lib/domain"
import { useIssuePropertyHandlers } from "@/hooks/use-issue-property-handlers"
import {
  MarkdownEditor,
  type MarkdownEditorRef,
} from "@/components/issue-editor/markdown-editor"
import { IssueTimeline } from "@/components/issue-timeline"
import { IssueStartButton } from "@/components/issue-coding-rows"
import { IssueDetailMobileBar } from "@/components/issue-detail-mobile-bar"
import { IssueMobileHeader } from "@/components/issue-mobile-header"
import { IssueEditorMobileProperties } from "@/components/issue-editor/mobile-properties"
import { IssueFilesSection } from "@/components/issue-files-section"
import {
  IssueParentLine,
  IssueRelationsSection,
} from "@/components/issue-relations-card"
import { IssueChip } from "@/components/issue-chip"
import { PinToggleButton } from "@/components/pin-toggle-button"
import { WidgetSubmissionCard } from "@/components/widget-submission-card"
import { useWidgetSubmission } from "@/hooks/use-widget-submission"
import {
  reporterDisplayName,
  toastReporterReply,
} from "@/components/reporter-reply-copy"
import { IssueActionsMenu } from "@/components/issue-actions-menu"
import { IssuePropertiesTray } from "@/components/issue-properties-tray"
import { IssueTitleField } from "@/components/issue-title-field"
import { PrGraphBadge } from "@/components/pr-graph-badge"
import { IssueThreadsRow } from "@/components/thread/issue-threads-row"
import { failedAction } from "@/lib/action-error"

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
   *  (the inbox, the board) instead of always to the board. */
  origin?: string
  /** EXP-1316: the thread's issue tab — the issue BODY without its own
   *  chrome: no work header bar (Start, pin, PR graph badge, `…`), no phone
   *  header, no Threads row; the phone bar keeps properties + Comment. */
  embedded?: boolean
  /** Embedded on a phone under a host's overlay header band: the scroller
   *  pads its top by the band's height. */
  topInset?: number
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
  embedded = false,
  topInset = 0,
}: IssueDetailViewProps) {
  const { data: session } = useSession()
  const currentUserId = session?.user?.id ?? null
  const isMobile = useIsMobile()

  const editorRef = useRef<MarkdownEditorRef>(null)
  // EXP-894: the body comes back where it was left after a work-tab switch.
  const rememberedScrollRef = useRememberedScroll(
    issueMemoryOwner(issue.id),
    `scroll`
  )
  // EXP-1162: the detail chrome. The title is a ROW of the scrolling body;
  // the header above it (the phone's overlay band, the md+ floating bar)
  // breaks into the collapsed title once that row scrolled away under it.
  const [mobileHeaderRef, mobileHeaderSize] = useMeasuredSize()
  const [clusterRef, clusterSize] = useMeasuredSize()
  const [trayRef, traySize] = useMeasuredSize()
  const {
    scrollRef: collapseScrollRef,
    titleRef,
    collapsed: titleCollapsed,
  } = useTitleCollapsed(isMobile ? mobileHeaderSize.height : WORK_BAR_HEIGHT)
  const bodyScrollRef = useCallback(
    (node: HTMLElement | null) => {
      rememberedScrollRef(node)
      collapseScrollRef(node)
    },
    [rememberedScrollRef, collapseScrollRef]
  )

  // EXP-928: the synced row lags its own save by a round trip, and this view
  // is REUSED across issue switches (only the `[issue.id]` effect reseeds it)
  // — leaving an issue right after a blur save and coming straight back read
  // the pre-save row and reverted the save on screen, and typing on top of
  // that made the next blur write the old text plus the new typing over it.
  // `descriptionSaves` answers what the issue really holds until the echo (or
  // a newer remote write) lands, so EVERY reader below — the issue-switch
  // reset, the sync effect, the blur handler — sees the saved text instead of
  // the stale row.
  const incomingDescription = resolveIncomingDescription(
    descriptionSaves,
    issue.id,
    getIssueDescriptionText(issue.description)
  )
  const normalizedIncoming = normalizeIssueDescriptionText(incomingDescription)

  const descriptionRef = useRef(incomingDescription)
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
  const lastSavedDescriptionRef = useRef(normalizedIncoming)
  const syncedDescriptionRef = useRef(normalizedIncoming)
  const saveQueueRef = useRef<Promise<void>>(Promise.resolve())
  const uploadQueueRef = useRef<Promise<void>>(Promise.resolve())

  const [description, setDescription] = useState(incomingDescription)
  const [activeUploadCount, setActiveUploadCount] = useState(0)
  // EXP-824: the narrated progress of a media upload ("Uploading clip.mp4… 42%").
  const [uploadStatusText, setUploadStatusText] = useState<string | null>(null)
  // EXP-568: the floating phone bar steps aside while the description is being
  // written — the keyboard formatting rail owns the bottom edge then.
  const [descriptionFocused, setDescriptionFocused] = useState(false)
  const { resolve: resolveStatus } = useTeamStatusesContext()
  // EXP-630: the estimate scale rides the synced team row.
  const team = useTeamById(teamId)
  const statusOption = resolveStatus(issue)
  // SLOP-4: ONE fetch of the widget submission behind this issue, shared by
  // the metadata card, the timeline (reporter comment names) and both
  // composers (the "Reply to reporter" toggle). Members only — anonymous
  // viewers never fetch.
  const submission = useWidgetSubmission(issue.id, currentUserId !== null)
  const reporter = submission?.reporterEmail
    ? { name: reporterDisplayName(submission.reporterName) }
    : null

  // EXP-877: ONE definition per property mutation, shared with the session
  // route's issue face (`use-issue-property-handlers.ts`); the hook also owns
  // the issue's label ids and the duplicate-status picker.
  const handlers = useIssuePropertyHandlers({ issue, teamSlug, readOnly })
  const { issueLabelIds, duplicatePicker } = handlers

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
      // EXP-928: the row keeps reading what this save REPLACED until the echo
      // lands — record both texts it may still show (the synced value and the
      // baseline this editor was sitting on) so every read of the issue in
      // between resolves to the saved text instead.
      descriptionSaves.record(issue.id, normalizedDescription, [
        syncedDescriptionRef.current,
        baselineAtSaveStart,
      ])
      try {
        await trpc.issues.update.mutate({
          id: issue.id,
          description: normalizedDescription ? normalizedDescription : null,
        })
      } catch (error) {
        // A failed write has no echo to retire its entry, and leaving it
        // would keep showing text the server never took.
        descriptionSaves.retire(issue.id, normalizedDescription)
        throw error
      }
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
    // FEED-79: a failed save is the tRPC link's toast; the caller decides.
    await queuedSave
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

  // FEED-79: an upload's failure toast. A tRPC error here is the description
  // save that follows the upload, which the mutation link already toasted.
  const failedUpload = (thing: string, error: unknown) => {
    if (error instanceof TRPCClientError) return
    failedAction(`upload`, thing, error)
  }

  const handleImageFiles = async (files: File[]) => {
    try {
      await enqueueUploadTask(async () => {
        for (const file of files) {
          const { url } = await uploadIssueAttachment(issue.id, file)
          editorRef.current?.insertImage({ alt: file.name, src: url })
          const nextDescription =
            editorRef.current?.getMarkdown() ?? descriptionRef.current
          setDescriptionValue(nextDescription)
          await queueDescriptionSave(nextDescription)
        }
      })
    } catch (error) {
      failedUpload(`the image`, error)
    }
  }

  // EXP-824: video/audio → probe + poster + `.mov` remux in the browser, a
  // progress-narrated upload, then the media block `[name](url)` at the caret
  // (paste/drop/the image button). A clip that is not H.264 MP4 gets a
  // non-blocking "may not play everywhere" toast.
  const handleMediaFiles = async (files: File[]) => {
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
            editorRef.current?.insertMedia({
              label: uploaded.filename,
              src: uploaded.url,
            })
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
      failedUpload(`the file`, error)
    }
  }

  // Pasted/dropped files that are NOT inline-embeddable images (EXP-297), and
  // EVERY pick of the rail's "Attach file" (EXP-1247 `asFile`, images too):
  // they upload to the Files section instead of entering the markdown.
  const handleOtherFiles = async (
    files: File[],
    options?: { asFile?: boolean }
  ) => {
    try {
      await enqueueUploadTask(async () => {
        for (const file of files) {
          await uploadIssueAttachment(issue.id, file, options)
        }
      })
    } catch (error) {
      failedUpload(`the file`, error)
    }
  }

  // EXP-1293: THE coding control is Start (readiness-gated, it opens the
  // home composer seeded with this issue): the white pill in the md+
  // header's trailing slot, the right circle of the phone bar. It hides
  // itself for a non-member or an instance without remote start.
  const startControl = currentUserId ? (
    <IssueStartButton
      issue={issue}
      board={board}
      teamId={teamId}
      currentUserId={currentUserId}
      variant={isMobile ? `fab` : `pill`}
    />
  ) : null

  const dueDate = issue.dueDate ?? null

  // EXP-698 r5 — the phone's properties SHEET is the create form's row list,
  // not the desktop chip band: Status / Priority / Assignee / Due date /
  // Board as full-width rows, then the label chips. Same mutations as the
  // band above; Board rides the shared move-confirm dialog.
  const mobilePropertiesPanel = (
    <IssueEditorMobileProperties
      status={statusOption}
      priority={issue.priority}
      assigneeId={issue.assigneeId}
      assigneeAgentId={issue.assigneeAgentId}
      selectedLabelIds={issueLabelIds}
      teamId={teamId}
      users={users}
      dueDate={dueDate}
      estimate={issue.estimate ?? null}
      estimation={(team?.estimationType as IssueEstimation | undefined) ?? `none`}
      onEstimateChange={handlers.handleEstimateChange}
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

  // EXP-877: row 2 of the work header — the properties tray
  // (`issue-properties-tray.tsx`). The phone renders the same node at the top
  // of its scroll column.
  const propsTray = (
    <IssuePropertiesTray
      issue={issue}
      board={board}
      users={users}
      teamId={teamId}
      readOnly={readOnly}
      handlers={handlers}
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

  // EXP-893 phone header (`issue-mobile-header.tsx`): round back to the list
  // this issue was opened from, the IDENTIFIER centred, the `…` on the right.
  const mobileHeader = (
    <IssueMobileHeader
      issue={issue}
      board={board}
      teamSlug={teamSlug}
      teamId={teamId}
      readOnly={readOnly}
      origin={origin}
      handlers={handlers}
      collapsed={titleCollapsed}
      overlay
      headerRef={mobileHeaderRef}
      graphBadge={
        /* EXP-897: the same badge the md+ header wears (SLOP-16: an icon
           button + `+N` beside the `…`), opening the overlay as a sheet. */
        <PrGraphBadge
          teamId={teamId}
          teamSlug={teamSlug}
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
  // EXP-1284/1293: the issue NAMES its threads (one chip each, opening it).
  const threadsRow = embedded ? null : (
    <IssueThreadsRow
      issueId={issue.id}
      teamId={teamId}
      teamSlug={teamSlug}
      className="px-5 pt-1 pb-2"
    />
  )
  // EXP-1097: "Sub-issue of [parent]" rides ABOVE the title, on every layout.
  const parentLine = <IssueParentLine issueId={issue.id} phone={isMobile} />

  const editor = (
    <div className="max-md:px-1">
      <MarkdownEditor
        ref={editorRef}
        markdown={description}
        editable={!readOnly}
        // EXP-1191: the caret scrolls clear of the sticky bar + tray.
        topScrollInset={
          isMobile ? undefined : embedded ? 0 : WORK_BAR_HEIGHT + traySize.height
        }
        onChange={setDescriptionValue}
        onBlur={() => void handleDescriptionBlur()}
        placeholder="Add description..."
        onFocusChange={setDescriptionFocused}
        imageUpload={{
          enabled: !readOnly,
          uploading: activeUploadCount > 0,
          statusText: uploadStatusText,
          onFiles: handleImageFiles,
          onMediaFiles: handleMediaFiles,
          onOtherFiles: handleOtherFiles,
        }}
      />
    </div>
  )

  // EXP-297 Files rail: non-inline-image attachments straight from the synced
  // shape, plus the "Attach file" affordance for members.
  const filesSection = (
    <IssueFilesSection
      issueId={issue.id}
      description={issue.description}
      readOnly={readOnly}
    />
  )

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
      submission={submission ?? null}
    />
  ) : null

  // Same mutation the timeline's own composer runs (EXP-568: on phones the
  // composer moved into the floating bar, which sits outside the timeline).
  const handleCommentSubmit = async (
    body: string,
    attachmentIds: string[],
    audience: CommentAudience
  ) => {
    const result = await trpc.comments.create.mutate({
      issueId: issue.id,
      body,
      attachmentIds,
      audience,
    })
    toastReporterReply(result.reporterEmailed)
  }

  // EXP-1097: the relations block under the description — the Sub-issues
  // band (the `+` opens the inline composer, EXP-760) and, md+ only, one
  // foldable band per remaining relation side (`lib/issue-relations-view.ts`).
  // The phone draws those bands in its properties sheet instead.
  const relationsSection = (
    <IssueRelationsSection
      issue={issue}
      teamId={teamId}
      users={users}
      readOnly={readOnly}
      phone={isMobile}
    />
  )

  // EXP-42b: reporter/page/env metadata of widget-filed issues, members-only
  // (the server gates it; anonymous viewers never even fetch).
  const widgetCard =
    currentUserId && submission ? (
      <WidgetSubmissionCard submission={submission} source={issue.source} />
    ) : null

  if (isMobile) {
    return (
      <div className={MOBILE_DETAIL_SCREEN_CLASS}>
        {!embedded && mobileHeader}
        {/* EXP-698: clearance for the floating bar below, so the last comment
            scrolls clear of it instead of ending under the glass
            (`MOBILE_WORK_BAR_CLEARANCE`); the tab bar itself is hidden on this
            route, so nothing else is reserved here. */}
        <div
          ref={bodyScrollRef}
          className={cn(`flex-1 overflow-y-auto`, !embedded && MOBILE_WORK_BAR_CLEARANCE)}
          /* EXP-1162: the header band floats over this scroller — the
             column starts below it and scrolls under it. */
          style={{ paddingTop: embedded ? topInset : mobileHeaderSize.height }}
        >
          {duplicateBanner}
          {parentLine}
          <div ref={titleRef}>{titleField}</div>
          {/* The title first, then the tray, as on the natives and the New
              issue page. */}
          {propsTray}
          {threadsRow}
          {editor}
          {filesSection}
          {relationsSection}
          {widgetCard}
          {timeline}
          {/* EXP-1316: embedded under a thread the scroller lives inside the
              host's screen; an explicit spacer (the bar's clearance) keeps
              the last rows clear of the floating Comment bar there too. */}
          {embedded && (
            <div
              aria-hidden
              data-testid="issue-bar-clearance"
              className="h-[calc(5.5rem+env(safe-area-inset-bottom))] shrink-0"
            />
          )}
        </div>
        {currentUserId && (
          <IssueDetailMobileBar
            key={issue.id}
            issueId={issue.id}
            users={users}
            propertiesNode={mobilePropertiesPanel}
            trailingNode={embedded ? undefined : startControl}
            onSubmitComment={handleCommentSubmit}
            reporter={reporter}
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
          where you are. */}
      {duplicateBanner}
      <div className="flex flex-1 min-h-0 overflow-hidden">
        <div className="flex flex-1 min-w-0 flex-col overflow-hidden">
          <div
            ref={bodyScrollRef}
            className="flex-1 min-h-0 overflow-y-auto"
            data-detail-scroll=""
            /* EXP-1191: sticky rows inside (a diff's file headers) stick
               below the bar and the tray. */
            style={
              {
                [WORK_STICKY_TOP_VAR]: `${embedded ? 0 : WORK_BAR_HEIGHT + traySize.height}px`,
              } as React.CSSProperties
            }
          >
            {/* EXP-877: the work header (the IDE's `work_header.rs`).
                EXP-1162: a compact bar FLOATING over this scroller — Start ·
                pin · `…` on the title's own line at rest; the collapsed
                title beside them once the title row scrolled away. */}
            {!embedded && (
            <WorkHeader
              floating
              edge={false}
              collapsed={titleCollapsed}
              title={
                <CollapsedTitle
                  identifier={issue.identifier}
                  title={issue.title}
                />
              }
              trailingRef={clusterRef}
              trailing={
                <>
                  {/* EXP-897: what this issue is part of: its stack, its
                      batch (SLOP-16: an icon button per shape) or its open
                      blockers (EXP-1097). */}
                  <PrGraphBadge
                    teamId={teamId}
                    teamSlug={teamSlug}
                    issue={issue}
                  />
                  {startControl}
                  {pinToggle}
                  <IssueActionsMenu
                    issue={issue}
                    board={board}
                    teamSlug={teamSlug}
                    readOnly={readOnly}
                  />
                </>
              }
            />
            )}
            {/* The title row: the large editable title, clear of the bar's
                cluster on its right. The tray follows as the next row;
                EXP-1191: it sticks under the bar once the title scrolled
                away. */}
            <div className={WORK_COLUMN_CLASS}>
              <div
                ref={titleRef}
                style={{ paddingRight: clusterSize.width }}
              >
                {parentLine}
                {titleField}
              </div>
              {threadsRow}
            </div>
            {embedded ? (
              // No bar above it: the tray stays a plain row (a sticky one
              // would stop a bar's height below the top).
              <div ref={trayRef} className="pb-3">
                {propsTray}
              </div>
            ) : (
              <WorkStickyTray ref={trayRef} collapsed={titleCollapsed}>
                {propsTray}
              </WorkStickyTray>
            )}
            <div className={WORK_COLUMN_CLASS}>
              {editor}
              {filesSection}
              {/* EXP-760: relations sit BELOW the description (Linear's
                  order); EXP-1097: as grouped bands. */}
              {relationsSection}
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

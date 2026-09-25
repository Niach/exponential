import { useEffect, useMemo, useRef, useState } from "react"
import { eq, useLiveQuery } from "@tanstack/react-db"
import { boardCollection } from "@/lib/collections"
import { BoardPicker, conceptIcon, Pill, BoardGlyph } from "@exp/ui"
import { trpc } from "@/lib/trpc-client"
import { toIssueDescription, type IssuePriority } from "@/lib/domain"
import { useTeamLabels } from "@/hooks/use-team-data"
import { useTeamStatusesContext } from "@/hooks/use-team-statuses"
import {
  isFallbackStatusOption,
  statusUpdatePayload,
  type StatusRowOption,
} from "@/lib/team-statuses"
import {
  hasDraftContent,
  toDialogSeed,
  toUpsertInput,
  type DraftSnapshot,
} from "@/lib/issue-drafts"
import {
  uploadDraftFile,
  uploadDraftImageFile,
} from "@/lib/storage/issue-image-upload"
import {
  mediaPlayabilityHint,
  prepareMediaUpload,
  uploadDraftMediaFile,
} from "@/lib/storage/media-upload"
import { toast } from "sonner"
import {
  isInlineImageAttachment,
  isInlineMediaAttachment,
} from "@/lib/attachment-files"
import type { Board, IssueDraft, User } from "@/db/schema"
import { IssueEditorDialogShell } from "@/components/issue-editor/dialog-shell"
import {
  IssueEditorAttachmentRail,
  type RailFile,
} from "@/components/issue-editor/attachment-rail"
import type { MarkdownEditorRef } from "@/components/issue-editor/markdown-editor"

const ChevronGlyph = conceptIcon(`ui-chevron-down`)

type CreateIssueSubmitPhase = `idle` | `creating`

export interface CreateIssueResult {
  // The wire row, not the collection row: tRPC hands back serialized dates,
  // and the only thing a caller needs from it is where to navigate.
  issue: { id: string; identifier: string }
  txId: number
  /** The slug of the board the issue actually landed on (the header picker
   *  may have moved it off the caller's board). */
  boardSlug: string
}

interface CreateIssueDialogProps {
  // The team status row the "+" in a group header seeds; absent = the team's
  // Backlog builtin.
  defaultStatus?: StatusRowOption
  onOpenChange: (open: boolean) => void
  open: boolean
  prefill?: { title?: string; description?: string }
  boardColor: string
  boardId: string
  boardPrefix: string
  users: User[]
  teamId: string
  teamSlug: string
  /** EXP-878: reopening THIS draft rather than composing a fresh one. */
  draftId?: string
  /** The synced row for `draftId`, once it lands. */
  draft?: IssueDraft
  onCreated?: (result: CreateIssueResult) => void
}

export function CreateIssueDialog({
  defaultStatus,
  onOpenChange,
  open,
  prefill,
  boardColor,
  boardId,
  boardPrefix,
  users,
  teamId,
  draftId,
  draft,
  onCreated,
}: CreateIssueDialogProps) {
  // EXP-449: the header pill is a board select — the dialog opens on the
  // caller's board but the issue can be filed onto any same-team board.
  // Statuses/labels/assignees are team-scoped, so a pick changes only the
  // create target. `null` = "no explicit pick", following the caller's board.
  const [pickedBoardId, setPickedBoardId] = useState<string | null>(null)
  const { data: boardRows } = useLiveQuery(
    (q) =>
      teamId
        ? q
            .from({ boards: boardCollection })
            .where(({ boards }) => eq(boards.teamId, teamId))
        : undefined,
    [teamId]
  )
  const boards = useMemo(
    () =>
      [...((boardRows ?? []) as Board[])].sort((left, right) =>
        left.name.localeCompare(right.name)
      ),
    [boardRows]
  )
  const selectedBoardId = pickedBoardId ?? boardId
  const selectedBoard = boards.find((board) => board.id === selectedBoardId)
  const labels = useTeamLabels(teamId)

  const [title, setTitle] = useState(prefill?.title ?? ``)
  const [description, setDescription] = useState(prefill?.description ?? ``)
  const { resolve: resolveStatus } = useTeamStatusesContext()
  // `null` = "no explicit pick yet", which resolves live to the group seed or
  // the team's Backlog builtin — so a status_statuses snapshot landing while
  // the dialog is open upgrades the constructed fallback in place without
  // clobbering a pick the user already made.
  const [pickedStatus, setPickedStatus] = useState<StatusRowOption | null>(null)
  const status =
    pickedStatus ??
    defaultStatus ??
    resolveStatus({ status: `backlog`, statusId: null })
  const [priority, setPriority] = useState<IssuePriority>(`none`)
  const [selectedLabelIds, setSelectedLabelIds] = useState<string[]>([])
  const [assigneeId, setAssigneeId] = useState<string | null>(null)
  const [dueDate, setDueDate] = useState<string | null>(null)
  const [attachmentStatus, setAttachmentStatus] = useState<string | null>(null)
  // EXP-878: the draft's NON-inline attachments, already uploaded. Images and
  // clips never live here — they are in the description as final URLs.
  const [draftFiles, setDraftFiles] = useState<RailFile[]>([])
  const [uploading, setUploading] = useState(false)
  const [submitPhase, setSubmitPhase] = useState<CreateIssueSubmitPhase>(`idle`)
  const editorRef = useRef<MarkdownEditorRef>(null)
  const titleRef = useRef<HTMLInputElement>(null)
  const descriptionRef = useRef(``)

  // ── EXP-878: the draft this dialog session owns ─────────────────────────
  // The id is minted CLIENT-side when the dialog opens (or adopted from the
  // row it opened from), so every write is the same idempotent upsert and
  // `issues.create({ draftId })` can consume the row in its own transaction.
  const sessionDraftIdRef = useRef<string>(draftId ?? ``)
  // Does a row for that id exist server-side? True when opened FROM a draft,
  // or once an eager upload had to create one.
  const draftRowExistsRef = useRef(false)
  // The once-per-dialog-session "make sure the row exists" promise: several
  // pasted images must not race three inserts of the same id.
  const ensureDraftRef = useRef<Promise<string> | null>(null)
  // The close write happens exactly once per session — a close is one upsert
  // (or one delete), never a write per keystroke.
  const wroteRef = useRef(true)
  // A successful create already deleted the draft server-side; the close that
  // follows must not resurrect it.
  const skipDraftSaveRef = useRef(false)
  const draftSnapshotRef = useRef<DraftSnapshot>({
    id: ``,
    teamId,
    boardId,
    title: ``,
    description: ``,
    statusId: null,
    priority: `none`,
    assigneeId: null,
    labelIds: [],
    dueDate: null,
    attachmentCount: 0,
  })

  // Re-read on every render: the close path and the unmount cleanup both read
  // this ref, and neither can afford a stale closure.
  draftSnapshotRef.current = {
    id: sessionDraftIdRef.current,
    teamId,
    boardId: selectedBoardId,
    title,
    description,
    // A constructed fallback row has no real id — the draft then carries NULL
    // and resolves to the team's Backlog, which is what it means anyway.
    statusId: isFallbackStatusOption(status) ? null : status.id,
    priority,
    assigneeId,
    labelIds: selectedLabelIds,
    dueDate,
    attachmentCount: draftFiles.length,
  }

  useEffect(() => {
    if (!open) return
    sessionDraftIdRef.current = draftId ?? crypto.randomUUID()
    draftRowExistsRef.current = draftId != null
    ensureDraftRef.current = null
    wroteRef.current = false
    skipDraftSaveRef.current = false
  }, [open, draftId])

  useEffect(() => {
    if (open) {
      setPickedStatus(null)
      if (prefill?.title) {
        setTitle(prefill.title)
      }
      if (prefill?.description) {
        descriptionRef.current = prefill.description
        setDescription(prefill.description)
        editorRef.current?.setMarkdown(prefill.description)
      }
    }
  }, [defaultStatus, open, prefill?.title, prefill?.description])

  // Seeding is keyed on the ROW, not on `open`: the dialog opens the moment
  // `?draft=` is read, and the synced row may land a tick later.
  useEffect(() => {
    if (!open || !draft) return
    const seed = toDialogSeed(draft, { boards, labels, users })
    if (!seed) return
    setTitle(seed.title)
    descriptionRef.current = seed.description
    setDescription(seed.description)
    editorRef.current?.setMarkdown(seed.description)
    setPickedBoardId(seed.boardId)
    setPickedStatus(
      seed.statusId
        ? resolveStatus({ status: `backlog`, statusId: seed.statusId })
        : null
    )
    setPriority(seed.priority)
    setSelectedLabelIds(seed.labelIds)
    setAssigneeId(seed.assigneeId)
    setDueDate(seed.dueDate ?? null)
    // Intentionally keyed on the row identity alone: re-running this on every
    // `boards`/`labels` snapshot would overwrite what the person is typing.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [draft?.id, open])

  // Draft attachments are server-only (the attachments shape drops them), so
  // the reopened Files rail is a fetch, not a live query.
  useEffect(() => {
    if (!open || !draftId) return
    let cancelled = false
    void trpc.issueDrafts.listAttachments
      .query({ id: draftId })
      .then((rows) => {
        if (cancelled) return
        setDraftFiles(
          rows
            .filter(
              (row) =>
                !isInlineImageAttachment(row.contentType) &&
                !isInlineMediaAttachment(row.contentType)
            )
            .map((row) => ({
              id: row.id,
              name: row.filename,
              contentType: row.contentType,
              sizeBytes: row.sizeBytes,
            }))
        )
      })
      .catch(() => undefined)
    return () => {
      cancelled = true
    }
  }, [open, draftId])

  // In a solo team (exactly one human member) the assignee control is
  // hidden and new issues default to the sole member — mirrors the server's
  // default assignment so optimistic UI + field resets stay correct.
  // `users` is the bot-excluded team member list; length 0 means the list
  // is still loading (never a genuine empty), so it never flags multi-member
  // teams as solo.
  const isSolo = users.length === 1
  const soleMemberId = isSolo ? users[0].id : null

  useEffect(() => {
    if (soleMemberId) setAssigneeId(soleMemberId)
  }, [soleMemberId])

  const setDescriptionValue = (nextDescription: string) => {
    descriptionRef.current = nextDescription
    setDescription(nextDescription)
  }

  const resetFields = () => {
    setDraftFiles([])
    setTitle(``)
    setDescriptionValue(``)
    setAttachmentStatus(null)
    setUploading(false)
    setSubmitPhase(`idle`)
    editorRef.current?.setMarkdown(``)
    setPickedStatus(null)
    setPriority(`none`)
    setAssigneeId(soleMemberId)
    setSelectedLabelIds([])
    setDueDate(null)
  }

  const handleToggleLabel = (labelId: string) => {
    setSelectedLabelIds((previous) =>
      previous.includes(labelId)
        ? previous.filter((id) => id !== labelId)
        : [...previous, labelId]
    )
  }

  /**
   * EXP-878: make sure the draft ROW exists before anything is uploaded
   * against it. Uploads are eager — the create path never uploads, so the
   * description only ever carries final `/api/attachments/{id}` URLs — which
   * means a pasted image is what brings a draft into existence mid-compose.
   * Cached per dialog session: pasting five images inserts one row.
   */
  const ensureDraft = (): Promise<string> => {
    if (!ensureDraftRef.current) {
      const snapshot = draftSnapshotRef.current
      ensureDraftRef.current = trpc.issueDrafts.upsert
        .mutate(toUpsertInput(snapshot))
        .then((result) => {
          draftRowExistsRef.current = true
          return result.draft.id
        })
        .catch((error: unknown) => {
          // A failed ensure must not poison the session: the next paste
          // retries instead of rejecting forever.
          ensureDraftRef.current = null
          throw error
        })
    }
    return ensureDraftRef.current
  }

  const handleImageFiles = async (files: File[]) => {
    if (submitPhase !== `idle`) return
    setAttachmentStatus(null)
    setUploading(true)
    try {
      const id = await ensureDraft()
      for (const file of files) {
        const uploaded = await uploadDraftImageFile(id, file)
        editorRef.current?.insertImage({ alt: file.name, src: uploaded.url })
        setDescriptionValue(
          editorRef.current?.getMarkdown() ?? descriptionRef.current
        )
      }
    } catch (error) {
      setAttachmentStatus(
        error instanceof Error ? error.message : `Failed to upload image`
      )
    } finally {
      setUploading(false)
    }
  }

  const handleMediaFiles = async (files: File[]) => {
    if (submitPhase !== `idle`) return
    setAttachmentStatus(null)
    setUploading(true)
    try {
      const id = await ensureDraft()
      for (const file of files) {
        const prepared = await prepareMediaUpload(file)
        const uploaded = await uploadDraftMediaFile(id, prepared)
        editorRef.current?.insertMedia({
          label: uploaded.filename,
          src: uploaded.url,
        })
        setDescriptionValue(
          editorRef.current?.getMarkdown() ?? descriptionRef.current
        )
        const hint = mediaPlayabilityHint(uploaded)
        if (hint) toast.message(hint)
      }
    } catch (error) {
      setAttachmentStatus(
        error instanceof Error ? error.message : `Failed to upload media`
      )
    } finally {
      setUploading(false)
    }
  }

  const handlePlainFiles = async (files: File[]) => {
    if (submitPhase !== `idle`) return
    setAttachmentStatus(null)
    setUploading(true)
    try {
      const id = await ensureDraft()
      for (const file of files) {
        const uploaded = await uploadDraftFile(id, file)
        setDraftFiles((previous) => [
          ...previous,
          {
            id: uploaded.id,
            name: uploaded.filename,
            contentType: uploaded.contentType,
            sizeBytes: uploaded.sizeBytes,
          },
        ])
      }
    } catch (error) {
      setAttachmentStatus(
        error instanceof Error ? error.message : `Failed to upload file`
      )
    } finally {
      setUploading(false)
    }
  }

  const handleAttachFiles = (files: File[]) => {
    if (submitPhase !== `idle`) return

    // Inline-image picks embed into the description like any other image add;
    // clips (EXP-824) embed as media blocks. Only the rest become rail rows.
    const images = files.filter((file) => isInlineImageAttachment(file.type))
    const media = files.filter((file) => isInlineMediaAttachment(file.type))
    const others = files.filter(
      (file) =>
        !isInlineImageAttachment(file.type) &&
        !isInlineMediaAttachment(file.type)
    )

    if (images.length > 0) void handleImageFiles(images)
    if (media.length > 0) void handleMediaFiles(media)
    if (others.length > 0) void handlePlainFiles(others)
  }

  const handleRemoveDraftFile = (attachmentId: string) => {
    setDraftFiles((previous) =>
      previous.filter((file) => file.id !== attachmentId)
    )
    void trpc.attachments.delete
      .mutate({ id: attachmentId })
      .catch(() => toast.error(`Could not remove the attachment`))
  }

  /**
   * EXP-878: the close write. Closing with content KEEPS the draft, silently —
   * there is no "Discard?" on any path (Escape, backdrop, ✕, unmount), which
   * is the whole point: nothing typed is ever destroyed by a stray keystroke.
   * Exactly one write: an upsert when there is content, a delete when an
   * existing draft was emptied out, nothing at all for an untouched dialog.
   */
  const persistDraft = () => {
    if (wroteRef.current) return
    wroteRef.current = true
    if (skipDraftSaveRef.current) return

    const snapshot = draftSnapshotRef.current
    const existed = draftRowExistsRef.current
    // An eager upload still in flight owns the row's creation — queue behind
    // it so the close write can never land before the insert.
    const pending = ensureDraftRef.current ?? Promise.resolve(null)

    if (hasDraftContent(snapshot)) {
      void pending
        .then(() => trpc.issueDrafts.upsert.mutate(toUpsertInput(snapshot)))
        .catch(() => undefined)
      return
    }
    if (existed) {
      void pending
        .then(() => trpc.issueDrafts.delete.mutate({ id: snapshot.id }))
        .catch(() => undefined)
    }
  }

  // Unmounting IS a close (route change, team switch). Refs only, so the
  // mount-time closure stays correct for the life of the component.
  useEffect(() => {
    return () => {
      persistDraft()
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [])

  const handleClose = () => {
    persistDraft()
    resetFields()
    // Follow the caller's board again on the next open.
    setPickedBoardId(null)
    onOpenChange(false)
  }

  const handleOpenChange = (nextOpen: boolean) => {
    if (nextOpen) {
      onOpenChange(true)
      return
    }

    if (submitPhase === `creating`) {
      return
    }

    handleClose()
  }

  const handleSubmit = async (event: React.FormEvent) => {
    event.preventDefault()

    if (!title.trim() || submitPhase !== `idle` || uploading) {
      return
    }

    setAttachmentStatus(null)
    setSubmitPhase(`creating`)

    const boardSlug = selectedBoard?.slug

    try {
      const { issue, txId } = await trpc.issues.create.mutate({
        boardId: selectedBoardId,
        title: title.trim(),
        // EXP-314: real rows write `statusId`; a constructed fallback row
        // (issue_statuses not synced yet) writes the anchor enum instead.
        ...statusUpdatePayload(status),
        priority,
        assigneeId: assigneeId ?? undefined,
        // EXP-878: already final `/api/attachments/{id}` URLs — nothing is
        // uploaded after the create any more.
        description: toIssueDescription(descriptionRef.current) ?? undefined,
        dueDate: dueDate ?? undefined,
        labelIds: selectedLabelIds.length > 0 ? selectedLabelIds : undefined,
        draftId: draftRowExistsRef.current
          ? sessionDraftIdRef.current
          : undefined,
      })

      // The create already consumed the draft row in its own transaction.
      skipDraftSaveRef.current = true
      handleClose()
      if (boardSlug) {
        onCreated?.({ issue, txId, boardSlug })
      }
    } catch (error) {
      setAttachmentStatus(
        error instanceof Error ? error.message : `Failed to create issue`
      )
      setSubmitPhase(`idle`)
    }
  }

  const dialogDisabled = submitPhase !== `idle`
  const closeDisabled = submitPhase === `creating`

  const displayPrefix = selectedBoard?.prefix ?? boardPrefix
  // EXP-1021: the board chip is the shared `BoardPicker` — the hand-rolled
  // dropdown it replaced drew its own trailing `Check` and its own board row.
  const boardPicker =
    boards.length > 1 ? (
      <BoardPicker
        boards={boards}
        value={selectedBoardId}
        onChange={setPickedBoardId}
        disabled={dialogDisabled}
        mobileTitle="Board"
        searchPlaceholder="Search boards…"
        emptyText="No boards found."
        width="sm"
        trigger={
          <Pill
            mode="action"
            disabled={dialogDisabled}
            leading={
              <BoardGlyph
                board={selectedBoard ?? { color: boardColor }}
                className="size-3"
              />
            }
          >
            {displayPrefix}
            <ChevronGlyph className="size-3 text-muted-foreground" />
          </Pill>
        }
      />
    ) : undefined

  return (
    <IssueEditorDialogShell
      open={open}
      onOpenChange={handleOpenChange}
      boardPrefix={displayPrefix}
      boardColor={selectedBoard?.color ?? boardColor}
      boardIcon={selectedBoard?.icon}
      boardRepositoryId={selectedBoard?.repositoryId}
      boardPicker={boardPicker}
      dialogTestId="issue-editor-create"
      formProps={{ onSubmit: handleSubmit }}
      primaryAction={{
        type: `submit`,
        disabled: !title.trim() || closeDisabled,
        loading: submitPhase === `creating`,
        label: `Create`,
      }}
      headerContent="New issue"
      title={title}
      titleRef={titleRef}
      autoFocus
      disabled={dialogDisabled}
      closeDisabled={closeDisabled}
      onTitleChange={setTitle}
      description={description}
      editorRef={editorRef}
      onDescriptionChange={setDescriptionValue}
      imageUpload={{
        enabled: true,
        uploading,
        onFiles: handleImageFiles,
        onOtherFiles: handleAttachFiles,
      }}
      status={status}
      onStatusChange={setPickedStatus}
      priority={priority}
      onPriorityChange={setPriority}
      teamId={teamId}
      selectedLabelIds={selectedLabelIds}
      onToggleLabel={handleToggleLabel}
      users={users}
      assigneeId={assigneeId}
      onAssigneeChange={setAssigneeId}
      hideAssignee={isSolo}
      dueDate={dueDate}
      onDueDateSelect={setDueDate}
      chipRowAction={
        <Pill
          size="md"
          mode="action"
          primary
          type="submit"
          disabled={!title.trim() || closeDisabled}
        >
          {submitPhase === `creating` ? `Creating...` : `Create issue`}
        </Pill>
      }
      footer={
        draftFiles.length > 0 || attachmentStatus ? (
          // EXP-586: images live inline in the description only; the footer
          // row exists solely for uploaded non-image files and errors, and
          // disappears when there are none. Submit sits in the chip row
          // (desktop) or the header FAB (mobile).
          <div className="px-4 py-3 border-t border-border">
            <IssueEditorAttachmentRail
              attachmentStatus={attachmentStatus}
              files={draftFiles}
              onRemoveFile={handleRemoveDraftFile}
              uploading={uploading}
              disabled={closeDisabled}
            />
          </div>
        ) : null
      }
    />
  )
}

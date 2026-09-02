import { useEffect, useMemo, useRef, useState } from "react"
import { eq, useLiveQuery } from "@tanstack/react-db"
import { Check, ChevronDown } from "lucide-react"
import { boardCollection } from "@/lib/collections"
import { BoardGlyph } from "@/components/board-glyph"
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu"
import { Button } from "@/components/ui/button"
import { Pill } from "@/components/ui/pill"
import {
  Dialog,
  DialogCancel,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import { trpc } from "@/lib/trpc-client"
import {
  formatDateForMutation,
  normalizeIssueDescriptionText,
  toIssueDescription,
  type IssuePriority,
} from "@/lib/domain"
import { useTeamStatusesContext } from "@/hooks/use-team-statuses"
import {
  statusUpdatePayload,
  type StatusRowOption,
} from "@/lib/team-statuses"
import {
  collectMarkdownImageUrls,
  removeMarkdownImagesByUrl,
  replaceMarkdownImageUrls,
} from "@/lib/storage/issue-attachments"
import {
  uploadIssueFile,
  uploadIssueImageFile,
} from "@/lib/storage/issue-image-upload"
import {
  buildPostCreateFileErrorMessage,
  buildPostCreateImageErrorMessage,
  revokeDraftImages,
  type DraftFile,
  type DraftImage,
} from "@/lib/create-issue-helpers"
import { isInlineImageAttachment } from "@/lib/attachment-files"
import type { Board, User } from "@/db/schema"
import { IssueEditorDialogShell } from "@/components/issue-editor/dialog-shell"
import { IssueEditorAttachmentRail } from "@/components/issue-editor/attachment-rail"
import type { MarkdownEditorRef } from "@/components/issue-editor/markdown-editor"

type CreateIssueSubmitPhase =
  | `idle`
  | `creating`
  | `uploading`
  | `created_with_image_errors`

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
  const [dueDate, setDueDate] = useState<Date | undefined>()
  const [attachmentStatus, setAttachmentStatus] = useState<string | null>(null)
  const [draftImages, setDraftImages] = useState<DraftImage[]>([])
  // EXP-297: non-image attachments queued locally and uploaded (sequentially)
  // once the issue exists — they never enter the description markdown.
  const [draftFiles, setDraftFiles] = useState<DraftFile[]>([])
  const [submitPhase, setSubmitPhase] = useState<CreateIssueSubmitPhase>(`idle`)
  const [discardConfirmOpen, setDiscardConfirmOpen] = useState(false)
  const editorRef = useRef<MarkdownEditorRef>(null)
  const titleRef = useRef<HTMLInputElement>(null)
  const descriptionRef = useRef(``)
  const draftImagesRef = useRef<DraftImage[]>([])

  useEffect(() => {
    draftImagesRef.current = draftImages
  }, [draftImages])

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

  useEffect(() => {
    return () => {
      revokeDraftImages(draftImagesRef.current)
    }
  }, [])

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
    setDraftImages((previous) => {
      const referencedUrls = new Set(collectMarkdownImageUrls(nextDescription))
      const nextDraftImages: DraftImage[] = []

      for (const draftImage of previous) {
        if (referencedUrls.has(draftImage.objectUrl)) {
          nextDraftImages.push(draftImage)
          continue
        }

        URL.revokeObjectURL(draftImage.objectUrl)
      }

      draftImagesRef.current = nextDraftImages
      return nextDraftImages.length === previous.length
        ? previous
        : nextDraftImages
    })
  }

  const clearDraftImages = () => {
    revokeDraftImages(draftImagesRef.current)
    draftImagesRef.current = []
    setDraftImages([])
  }

  const resetFields = () => {
    clearDraftImages()
    setDraftFiles([])
    setTitle(``)
    setDescriptionValue(``)
    setAttachmentStatus(null)
    setSubmitPhase(`idle`)
    editorRef.current?.setMarkdown(``)
    setPickedStatus(null)
    setPriority(`none`)
    setAssigneeId(soleMemberId)
    setSelectedLabelIds([])
    setDueDate(undefined)
  }

  const handleToggleLabel = (labelId: string) => {
    setSelectedLabelIds((previous) =>
      previous.includes(labelId)
        ? previous.filter((id) => id !== labelId)
        : [...previous, labelId]
    )
  }

  const getReferencedDraftImages = (markdown: string) => {
    const draftImagesByUrl = new Map(
      draftImagesRef.current.map((draftImage) => [
        draftImage.objectUrl,
        draftImage,
      ])
    )

    return collectMarkdownImageUrls(markdown)
      .map((url) => draftImagesByUrl.get(url))
      .filter(
        (draftImage): draftImage is DraftImage => draftImage !== undefined
      )
  }

  const handleImageFiles = async (files: File[]) => {
    if (submitPhase !== `idle`) {
      return
    }

    setAttachmentStatus(null)

    const nextDraftImages = files.map((file) => ({
      id: crypto.randomUUID(),
      file,
      objectUrl: URL.createObjectURL(file),
      alt: file.name,
    }))

    setDraftImages((previous) => {
      const updatedDraftImages = [...previous, ...nextDraftImages]
      draftImagesRef.current = updatedDraftImages
      return updatedDraftImages
    })

    for (const draftImage of nextDraftImages) {
      editorRef.current?.insertImage({
        alt: draftImage.alt,
        src: draftImage.objectUrl,
      })
    }

    const nextDescription =
      editorRef.current?.getMarkdown() ?? descriptionRef.current
    setDescriptionValue(nextDescription)
  }

  const handleAttachFiles = (files: File[]) => {
    if (submitPhase !== `idle`) {
      return
    }

    setAttachmentStatus(null)

    // Inline-image picks embed into the description like any other image add.
    // A draft FILE row of an inline type would upload to an attachment every
    // client's Files section filters out — invisible and unreferenced, exactly
    // what the sweep deletes (EXP-297 classification contract).
    const images = files.filter((file) => isInlineImageAttachment(file.type))
    const others = files.filter((file) => !isInlineImageAttachment(file.type))

    if (images.length > 0) {
      void handleImageFiles(images)
    }
    if (others.length > 0) {
      setDraftFiles((previous) => [
        ...previous,
        ...others.map((file) => ({ id: crypto.randomUUID(), file })),
      ])
    }
  }

  const handleRemoveDraftFile = (draftFileId: string) => {
    setDraftFiles((previous) =>
      previous.filter((draftFile) => draftFile.id !== draftFileId)
    )
  }

  const handleClose = () => {
    setDiscardConfirmOpen(false)
    resetFields()
    // Follow the caller's board again on the next open.
    setPickedBoardId(null)
    onOpenChange(false)
  }

  const hasDraftContent =
    title.trim().length > 0 ||
    toIssueDescription(description) !== null ||
    draftImages.length > 0 ||
    draftFiles.length > 0

  // Escape or a backdrop click is one stray keystroke away from destroying an
  // arbitrarily long draft — resetFields() also revokes the pasted images'
  // object URLs, so the draft is unrecoverable. Confirm first (REV2-60); the
  // explicit Close button and the post-create error footer stay immediate.
  const handleDismissAttempt = () => {
    if (submitPhase !== `idle` || !hasDraftContent) {
      return false
    }

    setDiscardConfirmOpen(true)
    return true
  }

  const handleOpenChange = (nextOpen: boolean) => {
    if (nextOpen) {
      onOpenChange(true)
      return
    }

    if (submitPhase === `creating` || submitPhase === `uploading`) {
      return
    }

    handleClose()
  }

  const handleSubmit = async (event: React.FormEvent) => {
    event.preventDefault()

    if (!title.trim() || submitPhase !== `idle`) {
      return
    }

    setAttachmentStatus(null)
    setSubmitPhase(`creating`)

    try {
      const currentDescription = descriptionRef.current
      const referencedDraftImages = getReferencedDraftImages(currentDescription)
      const referencedDraftUrls = referencedDraftImages.map(
        (draftImage) => draftImage.objectUrl
      )
      const strippedDescription = removeMarkdownImagesByUrl(
        currentDescription,
        referencedDraftUrls
      )

      const { issue } = await trpc.issues.create.mutate({
        boardId: selectedBoardId,
        title: title.trim(),
        // EXP-314: real rows write `statusId`; a constructed fallback row
        // (issue_statuses not synced yet) writes the anchor enum instead.
        ...statusUpdatePayload(status),
        priority,
        assigneeId: assigneeId ?? undefined,
        description: toIssueDescription(strippedDescription) ?? undefined,
        dueDate: formatDateForMutation(dueDate) ?? undefined,
        labelIds: selectedLabelIds.length > 0 ? selectedLabelIds : undefined,
      })

      const uploadedImageUrls = new Map<string, string>()
      const failedDraftUrls = new Set<string>()
      setSubmitPhase(`uploading`)

      for (const draftImage of referencedDraftImages) {
        try {
          const uploadedImage = await uploadIssueImageFile(
            issue.id,
            draftImage.file
          )
          uploadedImageUrls.set(draftImage.objectUrl, uploadedImage.url)
        } catch {
          failedDraftUrls.add(draftImage.objectUrl)
        }
      }

      // EXP-297: plain file attachments ride the same post-create window but
      // never touch the markdown — a failure only costs that one attachment.
      let failedFileCount = 0

      for (const draftFile of draftFiles) {
        try {
          await uploadIssueFile(issue.id, draftFile.file)
        } catch {
          failedFileCount += 1
        }
      }

      const finalDescription = replaceMarkdownImageUrls(
        removeMarkdownImagesByUrl(currentDescription, failedDraftUrls),
        uploadedImageUrls
      )

      editorRef.current?.setMarkdown(finalDescription)
      setDescriptionValue(finalDescription)

      const normalizedStrippedDescription =
        normalizeIssueDescriptionText(strippedDescription)
      const normalizedFinalDescription =
        normalizeIssueDescriptionText(finalDescription)

      try {
        if (normalizedFinalDescription !== normalizedStrippedDescription) {
          await trpc.issues.update.mutate({
            id: issue.id,
            description: toIssueDescription(finalDescription) ?? null,
          })
        }
      } catch {
        setAttachmentStatus(buildPostCreateImageErrorMessage(issue.identifier))
        setSubmitPhase(`created_with_image_errors`)
        return
      }

      const uploadFailureMessages = [
        failedDraftUrls.size > 0
          ? buildPostCreateImageErrorMessage(
              issue.identifier,
              failedDraftUrls.size
            )
          : null,
        failedFileCount > 0
          ? buildPostCreateFileErrorMessage(issue.identifier, failedFileCount)
          : null,
      ].filter((message): message is string => message !== null)

      if (uploadFailureMessages.length > 0) {
        setAttachmentStatus(uploadFailureMessages.join(` `))
        setSubmitPhase(`created_with_image_errors`)
        return
      }

      handleClose()
    } catch (error) {
      setAttachmentStatus(
        error instanceof Error ? error.message : `Failed to create issue`
      )
      setSubmitPhase(`idle`)
    }
  }

  const dialogDisabled = submitPhase !== `idle`
  const closeDisabled =
    submitPhase === `creating` || submitPhase === `uploading`

  const displayPrefix = selectedBoard?.prefix ?? boardPrefix
  const boardPicker =
    boards.length > 1 ? (
      <DropdownMenu>
        <DropdownMenuTrigger asChild>
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
            <ChevronDown className="size-3 text-muted-foreground" />
          </Pill>
        </DropdownMenuTrigger>
        <DropdownMenuContent align="start" className="w-56">
          {boards.map((board) => (
            <DropdownMenuItem
              key={board.id}
              onClick={() => setPickedBoardId(board.id)}
            >
              <BoardGlyph board={board} className="size-3.5" />
              <span className="truncate">{board.name}</span>
              {board.id === selectedBoardId && (
                <Check className="ml-auto size-3.5 shrink-0" />
              )}
            </DropdownMenuItem>
          ))}
        </DropdownMenuContent>
      </DropdownMenu>
    ) : undefined

  return (
    <>
      <IssueEditorDialogShell
        open={open}
        onOpenChange={handleOpenChange}
        onDismissAttempt={handleDismissAttempt}
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
          loading: submitPhase === `creating` || submitPhase === `uploading`,
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
          uploading: submitPhase === `uploading`,
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
          <Button
            type="submit"
            disabled={!title.trim() || closeDisabled}
            className="inline-flex items-center justify-center rounded-full bg-primary px-3 text-xs font-medium text-primary-foreground transition-colors hover:bg-primary/90 disabled:pointer-events-none disabled:opacity-50 h-7"
          >
            {submitPhase === `uploading`
              ? `Uploading images...`
              : submitPhase === `creating`
                ? `Creating...`
                : `Create issue`}
          </Button>
        }
        footer={
          submitPhase === `created_with_image_errors` ? (
            <div className="flex items-center justify-between gap-3 px-4 py-3 border-t border-border">
              <span className="text-xs text-destructive">
                {attachmentStatus}
              </span>
              <Pill mode="action" onClick={handleClose}>
                Close
              </Pill>
            </div>
          ) : draftFiles.length > 0 || attachmentStatus ? (
            // EXP-586: images live inline in the description only; the footer
            // row exists solely for queued non-image files and errors, and
            // disappears when there are none. Submit sits in the chip row
            // (desktop) or the header FAB (mobile).
            <div className="px-4 py-3 border-t border-border">
              <IssueEditorAttachmentRail
                attachmentStatus={attachmentStatus}
                files={draftFiles}
                onRemoveFile={handleRemoveDraftFile}
                uploading={submitPhase === `uploading`}
                disabled={closeDisabled}
              />
            </div>
          ) : null
        }
      />

      <Dialog open={discardConfirmOpen} onOpenChange={setDiscardConfirmOpen}>
        <DialogContent
          mobile="alert"
          className="sm:max-w-sm"
          data-testid="discard-draft-confirm"
        >
          <DialogHeader>
            <DialogTitle>Discard draft?</DialogTitle>
            <DialogDescription>
              This issue hasn&apos;t been created yet. Discarding clears the
              title, description and any attached images.
            </DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <DialogCancel onClick={() => setDiscardConfirmOpen(false)}>
              Keep editing
            </DialogCancel>
            <Button type="button" variant="destructive" onClick={handleClose}>
              Discard draft
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </>
  )
}

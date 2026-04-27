import { useEffect, useRef, useState } from "react"
import { Switch } from "@/components/ui/switch"
import { Label } from "@/components/ui/label"
import { Button } from "@/components/ui/button"
import { DropdownMenuItem } from "@/components/ui/dropdown-menu"
import { Repeat } from "lucide-react"
import { trpc } from "@/lib/trpc-client"
import {
  formatDateForMutation,
  normalizeIssueDescriptionText,
  toIssueDescription,
  type IssuePriority,
  type IssueStatus,
} from "@/lib/domain"
import {
  RecurrenceEditor,
  type RecurrenceValue,
} from "@/components/recurrence-editor"
import {
  extractMarkdownImageOccurrences,
  collectMarkdownImageUrls,
  removeMarkdownImageByOccurrence,
  removeMarkdownImagesByUrl,
  replaceMarkdownImageUrls,
} from "@/lib/issue-attachments"
import { uploadIssueImageFile } from "@/lib/issue-image-upload"
import type { User } from "@/db/schema"
import { IssueEditorDialogShell } from "@/components/issue-editor-dialog-shell"
import { IssueEditorAttachmentRail } from "@/components/issue-editor-attachment-rail"
import type { MarkdownEditorRef } from "@/components/markdown-editor"

type CreateIssueSubmitPhase =
  | `idle`
  | `creating`
  | `uploading`
  | `created_with_image_errors`

interface DraftImage {
  alt: string
  file: File
  id: string
  objectUrl: string
}

interface CreateIssueDialogProps {
  defaultStatus?: IssueStatus
  onOpenChange: (open: boolean) => void
  open: boolean
  projectColor: string
  projectId: string
  projectPrefix: string
  users: User[]
  workspaceId: string
}

function revokeDraftImages(images: DraftImage[]) {
  for (const image of images) {
    URL.revokeObjectURL(image.objectUrl)
  }
}

function buildPostCreateImageErrorMessage(
  issueIdentifier: string,
  failedImageCount?: number
) {
  if (typeof failedImageCount === `number` && failedImageCount > 0) {
    return `Created ${issueIdentifier}, but ${failedImageCount} ${
      failedImageCount === 1 ? `image` : `images`
    } failed to upload. Reopen the issue later to retry failed images.`
  }

  return `Created ${issueIdentifier}, but the image uploads could not be finalized cleanly. Reopen the issue later to retry them.`
}

export function CreateIssueDialog({
  defaultStatus,
  onOpenChange,
  open,
  projectColor,
  projectId,
  projectPrefix,
  users,
  workspaceId,
}: CreateIssueDialogProps) {
  const [title, setTitle] = useState(``)
  const [description, setDescription] = useState(``)
  const [status, setStatus] = useState<IssueStatus>(defaultStatus ?? `backlog`)
  const [priority, setPriority] = useState<IssuePriority>(`none`)
  const [selectedLabelIds, setSelectedLabelIds] = useState<string[]>([])
  const [assigneeId, setAssigneeId] = useState<string | null>(null)
  const [dueDate, setDueDate] = useState<Date | undefined>()
  const [dueTime, setDueTime] = useState<string | null>(null)
  const [endTime, setEndTime] = useState<string | null>(null)
  const [recurrence, setRecurrence] = useState<RecurrenceValue | null>(null)
  const [createMore, setCreateMore] = useState(false)
  const [attachmentStatus, setAttachmentStatus] = useState<string | null>(null)
  const [draftImages, setDraftImages] = useState<DraftImage[]>([])
  const [submitPhase, setSubmitPhase] = useState<CreateIssueSubmitPhase>(`idle`)
  const editorRef = useRef<MarkdownEditorRef>(null)
  const titleRef = useRef<HTMLInputElement>(null)
  const descriptionRef = useRef(``)
  const draftImagesRef = useRef<DraftImage[]>([])

  useEffect(() => {
    draftImagesRef.current = draftImages
  }, [draftImages])

  useEffect(() => {
    if (open) {
      setStatus(defaultStatus ?? `backlog`)
    }
  }, [defaultStatus, open])

  useEffect(() => {
    return () => {
      revokeDraftImages(draftImagesRef.current)
    }
  }, [])

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
    setTitle(``)
    setDescriptionValue(``)
    setAttachmentStatus(null)
    setSubmitPhase(`idle`)
    editorRef.current?.setMarkdown(``)
    setStatus(defaultStatus ?? `backlog`)
    setPriority(`none`)
    setAssigneeId(null)
    setSelectedLabelIds([])
    setDueDate(undefined)
    setDueTime(null)
    setEndTime(null)
    setRecurrence((previous) =>
      previous ? { ...previous, firstDue: new Date() } : null
    )
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

  const handleClose = () => {
    resetFields()
    onOpenChange(false)
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
        projectId,
        title: title.trim(),
        status: recurrence ? `todo` : status,
        priority,
        assigneeId: assigneeId ?? undefined,
        description: toIssueDescription(strippedDescription) ?? undefined,
        dueDate: recurrence
          ? (formatDateForMutation(recurrence.firstDue) ?? undefined)
          : (formatDateForMutation(dueDate) ?? undefined),
        dueTime: dueTime ?? undefined,
        endTime: endTime ?? undefined,
        labelIds: selectedLabelIds.length > 0 ? selectedLabelIds : undefined,
        recurrenceInterval: recurrence?.interval,
        recurrenceUnit: recurrence?.unit,
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

      if (failedDraftUrls.size > 0) {
        setAttachmentStatus(
          buildPostCreateImageErrorMessage(
            issue.identifier,
            failedDraftUrls.size
          )
        )
        setSubmitPhase(`created_with_image_errors`)
        return
      }

      if (createMore) {
        resetFields()
        titleRef.current?.focus()
        return
      }

      resetFields()
      onOpenChange(false)
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
  const imageOccurrences = extractMarkdownImageOccurrences(description)

  const enableRecurrence = () => {
    setRecurrence({
      firstDue: dueDate ?? new Date(),
      interval: 1,
      unit: `week`,
    })
  }

  const overflowMenuItems = (
    <DropdownMenuItem
      disabled={recurrence !== null}
      onSelect={(event) => {
        event.preventDefault()
        enableRecurrence()
      }}
    >
      <Repeat className="mr-2 h-4 w-4 text-muted-foreground" />
      Make recurring…
    </DropdownMenuItem>
  )

  const recurringFooter = recurrence ? (
    <div className="flex items-center justify-between gap-3 px-4 py-3 border-t border-border">
      <RecurrenceEditor
        value={recurrence}
        disabled={closeDisabled}
        onChange={setRecurrence}
      />
      <div className="flex items-center gap-3">
        <div className="flex items-center gap-2">
          <Switch
            id="create-more"
            size="sm"
            checked={createMore}
            disabled={closeDisabled}
            onCheckedChange={(checked) => setCreateMore(checked === true)}
          />
          <Label
            htmlFor="create-more"
            className="text-xs text-muted-foreground cursor-pointer select-none"
          >
            Create more
          </Label>
        </div>
        <Button
          type="submit"
          disabled={!title.trim() || !recurrence.firstDue || closeDisabled}
          className="inline-flex items-center justify-center rounded-md bg-indigo-600 px-3 text-xs font-medium text-white transition-colors hover:bg-indigo-700 disabled:pointer-events-none disabled:opacity-50 h-7"
        >
          {submitPhase === `creating`
            ? `Creating...`
            : `Create recurring issue`}
        </Button>
      </div>
    </div>
  ) : null

  const handleRemoveImageOccurrence = (occurrenceIndex: number) => {
    const nextDescription = removeMarkdownImageByOccurrence(
      descriptionRef.current,
      occurrenceIndex
    )

    editorRef.current?.setMarkdown(nextDescription)
    setDescriptionValue(nextDescription)
    setAttachmentStatus(null)
  }

  return (
    <IssueEditorDialogShell
      open={open}
      onOpenChange={handleOpenChange}
      projectPrefix={projectPrefix}
      projectColor={projectColor}
      dialogTestId="issue-editor-create"
      formProps={{ onSubmit: handleSubmit }}
      headerContent={<span className="text-sm">New issue</span>}
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
      }}
      status={status}
      onStatusChange={setStatus}
      priority={priority}
      onPriorityChange={setPriority}
      workspaceId={workspaceId}
      selectedLabelIds={selectedLabelIds}
      onToggleLabel={handleToggleLabel}
      users={users}
      assigneeId={assigneeId}
      onAssigneeChange={setAssigneeId}
      dueDate={dueDate}
      onDueDateSelect={setDueDate}
      dueTime={dueTime}
      endTime={endTime}
      onDueTimeChange={setDueTime}
      onEndTimeChange={setEndTime}
      hideDueDateChip={recurrence !== null}
      overflowMenuItems={overflowMenuItems}
      footer={
        recurringFooter ? (
          recurringFooter
        ) : submitPhase === `created_with_image_errors` ? (
          <div className="flex items-center justify-between gap-3 px-4 py-3 border-t border-border">
            <span className="text-xs text-destructive">{attachmentStatus}</span>
            <Button
              type="button"
              variant="outline"
              size="xs"
              onClick={handleClose}
            >
              Close
            </Button>
          </div>
        ) : (
          <div className="flex items-center justify-between px-4 py-3 border-t border-border">
            <div className="min-w-0 flex-1">
              <IssueEditorAttachmentRail
                attachmentStatus={attachmentStatus}
                images={imageOccurrences}
                onFiles={handleImageFiles}
                onRemove={handleRemoveImageOccurrence}
                uploading={submitPhase === `uploading`}
                disabled={closeDisabled}
              />
            </div>
            <div className="flex items-center gap-3">
              <div className="flex items-center gap-2">
                <Switch
                  id="create-more"
                  size="sm"
                  checked={createMore}
                  disabled={closeDisabled}
                  onCheckedChange={(checked) => setCreateMore(checked === true)}
                />
                <Label
                  htmlFor="create-more"
                  className="text-xs text-muted-foreground cursor-pointer select-none"
                >
                  Create more
                </Label>
              </div>
              <Button
                type="submit"
                disabled={!title.trim() || closeDisabled}
                className="inline-flex items-center justify-center rounded-md bg-indigo-600 px-3 text-xs font-medium text-white transition-colors hover:bg-indigo-700 disabled:pointer-events-none disabled:opacity-50 h-7"
              >
                {submitPhase === `uploading`
                  ? `Uploading images...`
                  : submitPhase === `creating`
                    ? `Creating...`
                    : `Create issue`}
              </Button>
            </div>
          </div>
        )
      }
    />
  )
}

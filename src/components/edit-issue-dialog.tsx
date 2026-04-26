import { useEffect, useRef, useState } from "react"
import { Repeat } from "lucide-react"
import type { Issue, User } from "@/db/schema"
import { trpc } from "@/lib/trpc-client"
import {
  formatDateForMutation,
  formatRecurrence,
  getIssueDescriptionText,
  normalizeIssueDescriptionText,
} from "@/lib/domain"
import {
  extractMarkdownImageOccurrences,
  removeMarkdownImageByOccurrence,
} from "@/lib/issue-attachments"
import { uploadIssueImageFile } from "@/lib/issue-image-upload"
import { Button } from "@/components/ui/button"
import { DropdownMenuItem } from "@/components/ui/dropdown-menu"
import {
  Popover,
  PopoverContent,
  PopoverTrigger,
} from "@/components/ui/popover"
import { IssueEditorDialogShell } from "@/components/issue-editor-dialog-shell"
import { IssueEditorAttachmentRail } from "@/components/issue-editor-attachment-rail"
import {
  RecurrenceEditor,
  type RecurrenceValue,
} from "@/components/recurrence-editor"
import type { MarkdownEditorRef } from "@/components/markdown-editor"

interface EditIssueDialogProps {
  issue: Issue
  issueLabelIds: string[]
  onOpenChange: (open: boolean) => void
  open: boolean
  projectColor: string
  projectPrefix: string
  users: User[]
  workspaceId: string
}

export function EditIssueDialog({
  issue,
  issueLabelIds,
  onOpenChange,
  open,
  projectColor,
  projectPrefix,
  users,
  workspaceId,
}: EditIssueDialogProps) {
  const editorRef = useRef<MarkdownEditorRef>(null)
  const descriptionRef = useRef(getIssueDescriptionText(issue.description))
  const lastSavedDescriptionRef = useRef(
    getIssueDescriptionText(issue.description)
  )
  const saveQueueRef = useRef<Promise<void>>(Promise.resolve())
  const uploadQueueRef = useRef<Promise<void>>(Promise.resolve())
  const [title, setTitle] = useState(issue.title)
  const [description, setDescription] = useState(
    getIssueDescriptionText(issue.description)
  )
  const [attachmentStatus, setAttachmentStatus] = useState<string | null>(null)
  const [activeUploadCount, setActiveUploadCount] = useState(0)

  useEffect(() => {
    const nextDescription = getIssueDescriptionText(issue.description)
    setTitle(issue.title)
    setDescription(nextDescription)
    descriptionRef.current = nextDescription
    lastSavedDescriptionRef.current =
      normalizeIssueDescriptionText(nextDescription)
    setAttachmentStatus(null)
    editorRef.current?.setMarkdown(nextDescription)
  }, [issue.description, issue.id, issue.title])

  const handleTitleBlur = async () => {
    const trimmed = title.trim()

    if (trimmed && trimmed !== issue.title) {
      await trpc.issues.update.mutate({ id: issue.id, title: trimmed })
    }
  }

  const handleDescriptionBlur = async () => {
    try {
      await queueDescriptionSave(descriptionRef.current)
    } catch {
      return
    }
  }

  const queueDescriptionSave = async (nextDescription: string) => {
    const normalizedDescription = normalizeIssueDescriptionText(nextDescription)

    if (normalizedDescription === lastSavedDescriptionRef.current) {
      await saveQueueRef.current
      return
    }

    const saveTask = async () => {
      await trpc.issues.update.mutate({
        id: issue.id,
        description: normalizedDescription
          ? { text: normalizedDescription }
          : null,
      })

      lastSavedDescriptionRef.current = normalizedDescription
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

  const setDescriptionValue = (nextDescription: string) => {
    descriptionRef.current = nextDescription
    setDescription(nextDescription)
  }

  const enqueueUploadTask = async (task: () => Promise<void>) => {
    setActiveUploadCount((current) => current + 1)

    const queuedTask = uploadQueueRef.current.then(task, task)
    uploadQueueRef.current = queuedTask.catch(() => undefined)

    try {
      await queuedTask
    } finally {
      setActiveUploadCount((current) => current - 1)
    }
  }

  const handleImageFiles = async (files: File[]) => {
    setAttachmentStatus(null)

    try {
      await enqueueUploadTask(async () => {
        for (const file of files) {
          const { url } = await uploadIssueImageFile(issue.id, file)
          editorRef.current?.insertImage({
            alt: file.name,
            src: url,
          })

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

  const dueDate = issue.dueDate
    ? new Date(issue.dueDate + `T00:00:00`)
    : undefined
  const imageOccurrences = extractMarkdownImageOccurrences(description)

  const isRecurring =
    issue.recurrenceInterval !== null && issue.recurrenceUnit !== null

  const handleRecurrenceChange = async (next: RecurrenceValue | null) => {
    if (!next) {
      await trpc.issues.update.mutate({
        id: issue.id,
        recurrenceInterval: null,
        recurrenceUnit: null,
      })
      return
    }

    await trpc.issues.update.mutate({
      id: issue.id,
      recurrenceInterval: next.interval,
      recurrenceUnit: next.unit,
      dueDate: formatDateForMutation(next.firstDue),
    })
  }

  const recurrenceChip = isRecurring ? (
    <Popover>
      <PopoverTrigger asChild>
        <Button
          type="button"
          variant="ghost"
          size="xs"
          className="text-muted-foreground"
        >
          <Repeat className="size-3" />
          {formatRecurrence(issue.recurrenceInterval!, issue.recurrenceUnit!)}
        </Button>
      </PopoverTrigger>
      <PopoverContent align="start" className="w-auto p-3">
        <RecurrenceEditor
          value={{
            firstDue: dueDate,
            interval: issue.recurrenceInterval!,
            unit: issue.recurrenceUnit!,
          }}
          onChange={(next) => {
            void handleRecurrenceChange(next)
          }}
        />
      </PopoverContent>
    </Popover>
  ) : null

  const overflowMenuItems = (
    <>
      {!isRecurring && (
        <DropdownMenuItem
          onSelect={(event) => {
            event.preventDefault()
            void handleRecurrenceChange({
              firstDue: dueDate ?? new Date(),
              interval: 1,
              unit: `week`,
            })
          }}
        >
          <Repeat className="mr-2 h-4 w-4 text-muted-foreground" />
          Make recurring…
        </DropdownMenuItem>
      )}
      {isRecurring && (
        <DropdownMenuItem
          onSelect={(event) => {
            event.preventDefault()
            void handleRecurrenceChange(null)
          }}
        >
          <Repeat className="mr-2 h-4 w-4 text-muted-foreground" />
          Stop recurring
        </DropdownMenuItem>
      )}
    </>
  )

  const handleRemoveImageOccurrence = (occurrenceIndex: number) => {
    const nextDescription = removeMarkdownImageByOccurrence(
      descriptionRef.current,
      occurrenceIndex
    )

    editorRef.current?.setMarkdown(nextDescription)
    setDescriptionValue(nextDescription)
    setAttachmentStatus(null)
  }

  const handleOpenChange = (nextOpen: boolean) => {
    if (nextOpen) {
      onOpenChange(true)
      return
    }

    void (async () => {
      try {
        await uploadQueueRef.current
        await queueDescriptionSave(descriptionRef.current)
        await saveQueueRef.current
        onOpenChange(false)
      } catch {
        editorRef.current?.focus()
      }
    })()
  }

  return (
    <IssueEditorDialogShell
      open={open}
      onOpenChange={handleOpenChange}
      projectPrefix={projectPrefix}
      projectColor={projectColor}
      dialogTestId="issue-editor-edit"
      headerContent={
        <span className="text-sm font-mono">{issue.identifier}</span>
      }
      title={title}
      onTitleChange={setTitle}
      onTitleBlur={() => {
        void handleTitleBlur()
      }}
      description={description}
      editorRef={editorRef}
      onDescriptionChange={setDescriptionValue}
      onDescriptionBlur={() => {
        void handleDescriptionBlur()
      }}
      imageUpload={{
        enabled: true,
        uploading: activeUploadCount > 0,
        onFiles: handleImageFiles,
      }}
      status={issue.status}
      onStatusChange={async (status) => {
        await trpc.issues.update.mutate({
          id: issue.id,
          status,
        })
      }}
      priority={issue.priority}
      onPriorityChange={async (priority) => {
        await trpc.issues.update.mutate({
          id: issue.id,
          priority,
        })
      }}
      workspaceId={workspaceId}
      selectedLabelIds={issueLabelIds}
      onToggleLabel={async (labelId) => {
        if (issueLabelIds.includes(labelId)) {
          await trpc.issueLabels.remove.mutate({
            issueId: issue.id,
            labelId,
          })
          return
        }

        await trpc.issueLabels.add.mutate({
          issueId: issue.id,
          labelId,
        })
      }}
      users={users}
      assigneeId={issue.assigneeId}
      onAssigneeChange={async (assigneeId) => {
        await trpc.issues.update.mutate({
          id: issue.id,
          assigneeId,
        })
      }}
      dueDate={dueDate}
      onDueDateSelect={async (date) => {
        await trpc.issues.update.mutate({
          id: issue.id,
          dueDate: formatDateForMutation(date),
        })
      }}
      chipRowExtras={recurrenceChip}
      overflowMenuItems={overflowMenuItems}
      footer={
        <div className="flex items-center px-4 py-3 border-t border-border">
          <IssueEditorAttachmentRail
            attachmentStatus={attachmentStatus}
            images={imageOccurrences}
            onFiles={handleImageFiles}
            onRemove={handleRemoveImageOccurrence}
            uploading={activeUploadCount > 0}
            disabled={activeUploadCount > 0}
          />
        </div>
      }
    />
  )
}

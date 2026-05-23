import { useEffect, useRef, useState } from "react"
import { ChevronRight } from "lucide-react"
import { Link } from "@tanstack/react-router"
import type { Issue, User, Project } from "@/db/schema"
import { trpc } from "@/lib/trpc-client"
import {
  formatDateForMutation,
  getIssueDescriptionText,
  normalizeIssueDescriptionText,
} from "@/lib/domain"
import {
  extractMarkdownImageOccurrences,
  removeMarkdownImageByOccurrence,
} from "@/lib/issue-attachments"
import { uploadIssueImageFile } from "@/lib/issue-image-upload"
import { authClient } from "@/lib/auth-client"
import { useIsMobile } from "@/hooks/use-mobile"
import { Input } from "@/components/ui/input"
import {
  MarkdownEditor,
  type MarkdownEditorRef,
} from "@/components/markdown-editor"
import { IssueEditorAttachmentRail } from "@/components/issue-editor-attachment-rail"
import { IssuePropertiesPanel } from "@/components/issue-properties-panel"
import { IssueTimeline } from "@/components/issue-timeline"
import { type RecurrenceValue } from "@/components/recurrence-editor"

export interface IssueDetailViewProps {
  issue: Issue
  issueLabelIds: string[]
  users: User[]
  project: Project
  workspaceSlug: string
  workspaceId: string
  readOnly?: boolean
  restrictModeration?: boolean
}

export function IssueDetailView({
  issue,
  issueLabelIds,
  users,
  project,
  workspaceSlug,
  workspaceId,
  readOnly = false,
  restrictModeration = false,
}: IssueDetailViewProps) {
  const { data: session } = authClient.useSession()
  const currentUserId = session?.user?.id ?? null
  const isAdmin = Boolean(
    (session?.user as { isAdmin?: boolean } | undefined)?.isAdmin
  )
  const isMobile = useIsMobile()

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

  // Reset local form state only when the visible issue changes.
  useEffect(() => {
    const nextDescription = getIssueDescriptionText(issue.description)
    setTitle(issue.title)
    setDescription(nextDescription)
    descriptionRef.current = nextDescription
    lastSavedDescriptionRef.current = normalizeIssueDescriptionText(nextDescription)
    setAttachmentStatus(null)
    editorRef.current?.setMarkdown(nextDescription)
  }, [issue.id])

  const handleTitleBlur = async () => {
    if (readOnly) return
    const trimmed = title.trim()
    if (trimmed && trimmed !== issue.title) {
      await trpc.issues.update.mutate({ id: issue.id, title: trimmed })
    }
  }

  const queueDescriptionSave = async (nextDescription: string) => {
    if (readOnly) return
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

  const handleDescriptionBlur = async () => {
    try {
      await queueDescriptionSave(descriptionRef.current)
    } catch {
      return
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

  const handleRemoveImageOccurrence = (occurrenceIndex: number) => {
    const nextDescription = removeMarkdownImageByOccurrence(
      descriptionRef.current,
      occurrenceIndex
    )
    editorRef.current?.setMarkdown(nextDescription)
    setDescriptionValue(nextDescription)
    setAttachmentStatus(null)
  }

  const dueDate = issue.dueDate ? new Date(issue.dueDate + `T00:00:00`) : undefined
  const imageOccurrences = extractMarkdownImageOccurrences(description)

  const handleRecurrenceChange = async (next: RecurrenceValue | null) => {
    if (readOnly) return
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

  const canApprovePlan = !readOnly && !restrictModeration

  const propsPanel = (
    <IssuePropertiesPanel
      layout={isMobile ? `chiprow` : `sidebar`}
      status={issue.status}
      onStatusChange={async (status) => {
        if (readOnly) return
        await trpc.issues.update.mutate({ id: issue.id, status })
      }}
      priority={issue.priority}
      onPriorityChange={async (priority) => {
        if (readOnly) return
        await trpc.issues.update.mutate({ id: issue.id, priority })
      }}
      assigneeId={issue.assigneeId}
      onAssigneeChange={async (assigneeId) => {
        if (readOnly) return
        await trpc.issues.update.mutate({ id: issue.id, assigneeId })
      }}
      users={users}
      workspaceId={workspaceId}
      selectedLabelIds={issueLabelIds}
      onToggleLabel={async (labelId) => {
        if (readOnly) return
        if (issueLabelIds.includes(labelId)) {
          await trpc.issueLabels.remove.mutate({ issueId: issue.id, labelId })
          return
        }
        await trpc.issueLabels.add.mutate({ issueId: issue.id, labelId })
      }}
      dueDate={dueDate}
      dueTime={issue.dueTime ?? null}
      endTime={issue.endTime ?? null}
      onDueDateSelect={async (date) => {
        if (readOnly) return
        await trpc.issues.update.mutate({
          id: issue.id,
          dueDate: formatDateForMutation(date),
          ...(date ? {} : { dueTime: null, endTime: null }),
        })
      }}
      onDueTimeChange={async (time) => {
        if (readOnly) return
        await trpc.issues.update.mutate({
          id: issue.id,
          dueTime: time,
          ...(time ? {} : { endTime: null }),
        })
      }}
      onEndTimeChange={async (time) => {
        if (readOnly) return
        await trpc.issues.update.mutate({ id: issue.id, endTime: time })
      }}
      recurrenceInterval={issue.recurrenceInterval}
      recurrenceUnit={issue.recurrenceUnit}
      onRecurrenceChange={handleRecurrenceChange}
      projectName={project.name}
      projectColor={project.color}
      projectPrefix={project.prefix}
      disabled={readOnly}
      restrictModeration={restrictModeration}
    />
  )

  const breadcrumb = (
    <div className="flex items-center gap-1.5 text-xs text-muted-foreground px-4 py-2 border-b border-border min-w-0">
      <Link
        to="/w/$workspaceSlug/projects/$projectSlug"
        params={{ workspaceSlug, projectSlug: project.slug }}
        className="inline-flex items-center gap-1.5 hover:text-foreground"
      >
        <div
          className="h-2.5 w-2.5 rounded-full"
          style={{ backgroundColor: project.color }}
        />
        {project.name}
      </Link>
      <ChevronRight className="size-3" />
      <span className="font-mono">{issue.identifier}</span>
      <ChevronRight className="size-3" />
      <span className="truncate">{title}</span>
    </div>
  )

  const titleField = (
    <Input
      value={title}
      onBlur={() => void handleTitleBlur()}
      onChange={(e) => setTitle(e.target.value)}
      placeholder="Issue title"
      disabled={readOnly}
      className="bg-transparent dark:bg-transparent border-none shadow-none text-2xl font-semibold px-5 pt-4 pb-1 focus-visible:ring-0 placeholder:text-muted-foreground/50"
    />
  )

  const editor = (
    <div className="px-1">
      <MarkdownEditor
        ref={editorRef}
        markdown={description}
        editable={!readOnly}
        onChange={setDescriptionValue}
        onBlur={() => void handleDescriptionBlur()}
        placeholder="Add description..."
        imageUpload={{
          enabled: !readOnly,
          uploading: activeUploadCount > 0,
          onFiles: handleImageFiles,
        }}
      />
    </div>
  )

  const attachmentRail = (
    <div className="flex items-center px-4 py-3 border-t border-border">
      <IssueEditorAttachmentRail
        attachmentStatus={attachmentStatus}
        images={imageOccurrences}
        onFiles={readOnly ? undefined : handleImageFiles}
        onRemove={readOnly ? undefined : handleRemoveImageOccurrence}
        uploading={activeUploadCount > 0}
        disabled={readOnly || activeUploadCount > 0}
      />
    </div>
  )

  const timeline = currentUserId ? (
    <IssueTimeline
      issue={issue}
      currentUserId={currentUserId}
      isAdmin={isAdmin}
      canApprovePlan={canApprovePlan}
      users={users}
    />
  ) : null

  if (isMobile) {
    return (
      <div className="flex flex-col h-full min-h-0">
        {breadcrumb}
        {propsPanel}
        <div className="flex-1 overflow-y-auto">
          {titleField}
          {editor}
          {attachmentRail}
          {timeline}
        </div>
      </div>
    )
  }

  return (
    <div className="flex flex-col h-full min-h-0">
      {breadcrumb}
      <div className="flex flex-1 min-h-0 overflow-hidden">
        <div className="flex-1 min-w-0 overflow-y-auto">
          <div className="mx-auto max-w-3xl">
            {titleField}
            {editor}
            {attachmentRail}
            {timeline}
          </div>
        </div>
        {propsPanel}
      </div>
    </div>
  )
}

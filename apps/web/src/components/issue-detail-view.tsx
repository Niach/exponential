import { useEffect, useRef, useState } from "react"
import { ChevronRight, Files, MoreHorizontal, Undo2 } from "lucide-react"
import { Link } from "@tanstack/react-router"
import { eq, useLiveQuery } from "@tanstack/react-db"
import type { Issue, User, Project } from "@/db/schema"
import { issueCollection } from "@/lib/collections"
import { trpc } from "@/lib/trpc-client"
import {
  formatDateForMutation,
  getIssueDescriptionText,
  normalizeIssueDescriptionText,
} from "@/lib/domain"
import {
  extractMarkdownImageOccurrences,
  removeMarkdownImageByOccurrence,
} from "@/lib/storage/issue-attachments"
import { uploadIssueImageFile } from "@/lib/storage/issue-image-upload"
import { useSession } from "@/hooks/use-session"
import { isAdminUser } from "@/lib/auth/app-user"
import { parseLocalDate } from "@/lib/utils"
import { useIsMobile } from "@/hooks/use-mobile"
import { Button } from "@/components/ui/button"
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu"
import { Input } from "@/components/ui/input"
import { IssuePickerDialog } from "@/components/issue-picker-dialog"
import { useIssueRefs } from "@/components/issue-ref-provider"
import {
  MarkdownEditor,
  type MarkdownEditorRef,
} from "@/components/issue-editor/markdown-editor"
import { IssueEditorAttachmentRail } from "@/components/issue-editor/attachment-rail"
import { IssuePropertiesPanel } from "@/components/issue-properties-panel"
import { IssueTimeline } from "@/components/issue-timeline"
import { SteerTerminal } from "@/components/steer-terminal"
import { SubscribeToggle } from "@/components/subscribe-toggle"
import { type RecurrenceValue } from "@/components/recurrence-editor"

interface IssueDetailViewProps {
  issue: Issue
  issueLabelIds: string[]
  users: User[]
  project: Project
  workspaceSlug: string
  workspaceId: string
  readOnly?: boolean
  restrictModeration?: boolean
}

// Canonical-issue banner shown on a duplicate's detail view: "Duplicate of
// #IDENT — {title}", clickable through to the canonical issue, with an Unmark
// action (clears the link; the server restores status atomically).
function DuplicateOfBanner({
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
      <Button
        variant="outline"
        size="xs"
        className="h-5 shrink-0 rounded-full px-2 font-mono text-xs"
        onClick={() => issueRefs?.open(canonical.identifier)}
      >
        #{canonical.identifier}
      </Button>
      <span className="truncate text-muted-foreground">{canonical.title}</span>
      {!readOnly && (
        <Button
          variant="ghost"
          size="xs"
          className="ml-auto shrink-0 text-muted-foreground"
          onClick={onUnmark}
        >
          <Undo2 className="size-3.5" />
          Unmark
        </Button>
      )}
    </div>
  )
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
  const { data: session } = useSession()
  const currentUserId = session?.user?.id ?? null
  const isAdmin = isAdminUser(session?.user)
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
  const [duplicatePickerOpen, setDuplicatePickerOpen] = useState(false)

  const incomingDescription = getIssueDescriptionText(issue.description)
  const normalizedIncoming = normalizeIssueDescriptionText(incomingDescription)

  // Full reset when navigating to a different issue.
  useEffect(() => {
    setTitle(issue.title)
    setDescription(incomingDescription)
    descriptionRef.current = incomingDescription
    lastSavedDescriptionRef.current = normalizedIncoming
    setAttachmentStatus(null)
    editorRef.current?.setMarkdown(incomingDescription)
  }, [issue.id])

  // Sync title from Electric when another client changes it,
  // but skip if the local value matches what we'd save (user is editing).
  useEffect(() => {
    if (issue.title !== title && issue.title !== title.trim()) {
      setTitle(issue.title)
    }
  }, [issue.title])

  // Sync description from Electric when another client changes it.
  useEffect(() => {
    if (normalizedIncoming !== lastSavedDescriptionRef.current) {
      setDescription(incomingDescription)
      descriptionRef.current = incomingDescription
      lastSavedDescriptionRef.current = normalizedIncoming
      editorRef.current?.setMarkdown(incomingDescription)
    }
  }, [normalizedIncoming])

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
        description: normalizedDescription ? normalizedDescription : null,
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

  const dueDate = issue.dueDate ? parseLocalDate(issue.dueDate) : undefined
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
      <div className="ml-auto flex items-center gap-1 shrink-0">
        {currentUserId && (
          <SubscribeToggle issueId={issue.id} currentUserId={currentUserId} />
        )}
        {!readOnly && !restrictModeration && (
          <DropdownMenu>
            <DropdownMenuTrigger asChild>
              <Button
                variant="ghost"
                size="icon-xs"
                className="text-muted-foreground"
                aria-label="Issue actions"
              >
                <MoreHorizontal className="size-4" />
              </Button>
            </DropdownMenuTrigger>
            <DropdownMenuContent align="end">
              {issue.duplicateOfId ? (
                <DropdownMenuItem
                  onSelect={() => {
                    void trpc.issues.update.mutate({
                      id: issue.id,
                      duplicateOfId: null,
                    })
                  }}
                >
                  <Undo2 className="size-4" />
                  Unmark duplicate
                </DropdownMenuItem>
              ) : (
                <DropdownMenuItem
                  onSelect={() => {
                    setTimeout(() => setDuplicatePickerOpen(true), 0)
                  }}
                >
                  <Files className="size-4" />
                  Mark as duplicate…
                </DropdownMenuItem>
              )}
            </DropdownMenuContent>
          </DropdownMenu>
        )}
      </div>
    </div>
  )

  const duplicateBanner = issue.duplicateOfId ? (
    <DuplicateOfBanner
      duplicateOfId={issue.duplicateOfId}
      readOnly={readOnly}
      onUnmark={() => {
        void trpc.issues.update.mutate({ id: issue.id, duplicateOfId: null })
      }}
    />
  ) : null

  const duplicatePicker = (
    <IssuePickerDialog
      open={duplicatePickerOpen}
      onOpenChange={setDuplicatePickerOpen}
      excludeIssueIds={[issue.id]}
      title="Mark as duplicate"
      placeholder="Search the canonical issue…"
      onPick={(canonical) => {
        // The server sets status='duplicate' atomically with the link.
        void trpc.issues.update.mutate({
          id: issue.id,
          duplicateOfId: canonical.id,
        })
      }}
    />
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
      users={users}
    />
  ) : null

  // Live "coding now" badge + remote terminal watch/steer + "Start on my
  // desktop" (masterplan §3.7), sitting where the PR/diff review lives.
  const steerPanel = currentUserId ? (
    <SteerTerminal
      issueId={issue.id}
      workspaceId={workspaceId}
      currentUserId={currentUserId}
      users={users}
    />
  ) : null

  if (isMobile) {
    return (
      <div className="flex flex-col h-full min-h-0">
        {breadcrumb}
        {duplicateBanner}
        {propsPanel}
        <div className="flex-1 overflow-y-auto">
          {titleField}
          {editor}
          {attachmentRail}
          {steerPanel}
          {timeline}
        </div>
        {duplicatePicker}
      </div>
    )
  }

  return (
    <div className="flex flex-col h-full min-h-0">
      {breadcrumb}
      {duplicateBanner}
      <div className="flex flex-1 min-h-0 overflow-hidden">
        <div className="flex-1 min-w-0 overflow-y-auto">
          <div className="mx-auto max-w-3xl">
            {titleField}
            {editor}
            {attachmentRail}
            {steerPanel}
            {timeline}
          </div>
        </div>
        {propsPanel}
      </div>
      {duplicatePicker}
    </div>
  )
}

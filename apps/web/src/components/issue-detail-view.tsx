import { useEffect, useRef, useState } from "react"
import {
  Check,
  ChevronRight,
  Files,
  Link2,
  MoreHorizontal,
  Undo2,
} from "lucide-react"
import { Link } from "@tanstack/react-router"
import { and, eq, useLiveQuery } from "@tanstack/react-db"
import type { CodingSession, Issue, User, Project } from "@/db/schema"
import { codingSessionCollection, issueCollection } from "@/lib/collections"
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
import { useDuplicateInterception } from "@/hooks/use-duplicate-interception"
import { useIssueRefs } from "@/components/issue-ref-provider"
import {
  MarkdownEditor,
  type MarkdownEditorRef,
} from "@/components/issue-editor/markdown-editor"
import { IssueEditorAttachmentRail } from "@/components/issue-editor/attachment-rail"
import { IssuePropertiesPanel } from "@/components/issue-properties-panel"
import { IssueTimeline } from "@/components/issue-timeline"
import { IssueChangesTab } from "@/components/issue-changes-tab"
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
  const [activeTab, setActiveTab] = useState<`details` | `changes`>(`details`)
  const [linkCopied, setLinkCopied] = useState(false)

  const { handleStatusChange, duplicatePicker } = useDuplicateInterception({
    issueId: issue.id,
    onStatusChange: async (status) => {
      if (readOnly) return
      await trpc.issues.update.mutate({ id: issue.id, status })
    },
  })

  // Live "coding now" dot on the Changes tab: a running session (or an open PR /
  // pushed branch) means there is something to see in Changes.
  const { data: runningSessionRows } = useLiveQuery(
    (query) =>
      query
        .from({ s: codingSessionCollection })
        .where(({ s }) =>
          and(eq(s.issueId, issue.id), eq(s.status, `running`))
        ),
    [issue.id]
  )
  const isCodingNow = ((runningSessionRows ?? []) as CodingSession[]).length > 0
  const hasChanges = isCodingNow || issue.prNumber != null

  const incomingDescription = getIssueDescriptionText(issue.description)
  const normalizedIncoming = normalizeIssueDescriptionText(incomingDescription)

  // Full reset when navigating to a different issue.
  useEffect(() => {
    setTitle(issue.title)
    setDescription(incomingDescription)
    descriptionRef.current = incomingDescription
    lastSavedDescriptionRef.current = normalizedIncoming
    setAttachmentStatus(null)
    setActiveTab(`details`)
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
      onStatusChange={handleStatusChange}
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
        <Button
          variant="ghost"
          size="icon-xs"
          className="text-muted-foreground"
          aria-label="Copy link to issue"
          onClick={() => {
            if (typeof navigator === `undefined` || !navigator.clipboard) {
              return
            }
            const url = `${window.location.origin}/w/${workspaceSlug}/projects/${project.slug}/issues/${issue.identifier}`
            navigator.clipboard.writeText(url).then(
              () => {
                setLinkCopied(true)
                setTimeout(() => setLinkCopied(false), 1500)
              },
              () => {
                // Clipboard denied (permissions/insecure context) — no success state.
              }
            )
          }}
        >
          {linkCopied ? (
            <Check className="size-4 text-primary" />
          ) : (
            <Link2 className="size-4" />
          )}
        </Button>
        {currentUserId && (
          <SubscribeToggle issueId={issue.id} currentUserId={currentUserId} />
        )}
        {!readOnly && !restrictModeration && issue.duplicateOfId && (
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

  // Details · Changes segmented control (masterplan §4.8 / §5.4). Changes is
  // the single home of PR/branch diffs + the live steer viewer.
  const tabsBar = (
    <div className="flex items-center gap-1 px-4 py-2 border-b border-border">
      <div className="inline-flex items-center gap-0.5 rounded-lg bg-muted/50 p-0.5">
        {(
          [
            { id: `details`, label: `Details` },
            { id: `changes`, label: `Changes` },
          ] as const
        ).map((tab) => (
          <Button
            key={tab.id}
            variant="ghost"
            size="sm"
            onClick={() => setActiveTab(tab.id)}
            className={`h-6 rounded-md px-3 text-xs ${
              activeTab === tab.id
                ? `bg-background text-foreground shadow-sm`
                : `text-muted-foreground hover:text-foreground`
            }`}
          >
            {tab.label}
            {tab.id === `changes` && hasChanges && (
              <span className="relative ml-1.5 flex size-1.5">
                {isCodingNow && (
                  <span className="absolute inline-flex h-full w-full animate-ping rounded-full bg-emerald-400 opacity-60" />
                )}
                <span className="relative inline-flex size-1.5 rounded-full bg-emerald-500" />
              </span>
            )}
          </Button>
        ))}
      </div>
    </div>
  )

  // Changes tab body: PR diff → branchDiff → "Being coded on <device>" steer
  // viewer (masterplan §5.4). Mounted only while active so opening the tab
  // triggers a fresh branch-diff fetch (§4.8 freshness).
  const changesContent = currentUserId ? (
    <IssueChangesTab
      issue={issue}
      workspaceId={workspaceId}
      currentUserId={currentUserId}
      users={users}
    />
  ) : (
    <div className="px-4 py-6 text-xs text-muted-foreground">
      Sign in to view changes.
    </div>
  )

  if (isMobile) {
    return (
      <div className="flex flex-col h-full min-h-0">
        {breadcrumb}
        {duplicateBanner}
        {propsPanel}
        {tabsBar}
        <div className="flex-1 overflow-y-auto">
          {activeTab === `details` ? (
            <>
              {titleField}
              {editor}
              {attachmentRail}
              {timeline}
            </>
          ) : (
            changesContent
          )}
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
        <div className="flex flex-1 min-w-0 flex-col overflow-hidden">
          {tabsBar}
          <div className="flex-1 min-h-0 overflow-y-auto">
            {activeTab === `details` ? (
              <div className="mx-auto max-w-3xl">
                {titleField}
                {editor}
                {attachmentRail}
                {timeline}
              </div>
            ) : (
              // Wider than Details: diff lines need the room (desktop-IDE parity).
              <div className="mx-auto max-w-5xl">{changesContent}</div>
            )}
          </div>
        </div>
        {propsPanel}
      </div>
      {duplicatePicker}
    </div>
  )
}

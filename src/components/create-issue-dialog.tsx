import { useEffect, useRef, useState } from "react"
import { Switch } from "@/components/ui/switch"
import { Label } from "@/components/ui/label"
import { Button } from "@/components/ui/button"
import { trpc } from "@/lib/trpc-client"
import {
  formatDateForMutation,
  toIssueDescription,
  type IssuePriority,
  type IssueStatus,
} from "@/lib/domain"
import type { User } from "@/db/schema"
import {
  IssueEditorAttachmentButton,
  IssueEditorDialogShell,
} from "@/components/issue-editor-dialog-shell"
import type { MarkdownEditorRef } from "@/components/markdown-editor"

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
  const [createMore, setCreateMore] = useState(false)
  const [attachmentStatus, setAttachmentStatus] = useState<string | null>(null)
  const [submitting, setSubmitting] = useState(false)
  const editorRef = useRef<MarkdownEditorRef>(null)
  const titleRef = useRef<HTMLInputElement>(null)
  const createDisabledReason = `Create the issue first to add images`

  useEffect(() => {
    if (open) {
      setStatus(defaultStatus ?? `backlog`)
    }
  }, [defaultStatus, open])

  const resetFields = () => {
    setTitle(``)
    setDescription(``)
    setAttachmentStatus(null)
    editorRef.current?.setMarkdown(``)
    setStatus(defaultStatus ?? `backlog`)
    setPriority(`none`)
    setAssigneeId(null)
    setSelectedLabelIds([])
    setDueDate(undefined)
  }

  const handleSubmit = async (event: React.FormEvent) => {
    event.preventDefault()

    if (!title.trim()) {
      return
    }

    setSubmitting(true)

    try {
      await trpc.issues.create.mutate({
        projectId,
        title: title.trim(),
        status,
        priority,
        assigneeId: assigneeId ?? undefined,
        description: toIssueDescription(description) ?? undefined,
        dueDate: formatDateForMutation(dueDate) ?? undefined,
        labelIds: selectedLabelIds.length > 0 ? selectedLabelIds : undefined,
      })

      if (createMore) {
        resetFields()
        titleRef.current?.focus()
        return
      }

      resetFields()
      onOpenChange(false)
    } finally {
      setSubmitting(false)
    }
  }

  const handleToggleLabel = (labelId: string) => {
    setSelectedLabelIds((previous) =>
      previous.includes(labelId)
        ? previous.filter((id) => id !== labelId)
        : [...previous, labelId]
    )
  }

  return (
<<<<<<< ours
    <IssueEditorDialogShell
      open={open}
      onOpenChange={onOpenChange}
      projectPrefix={projectPrefix}
      projectColor={projectColor}
      dialogTestId="issue-editor-create"
      formProps={{ onSubmit: handleSubmit }}
      headerContent={<span className="text-sm">New issue</span>}
      title={title}
      titleRef={titleRef}
      autoFocus
      onTitleChange={setTitle}
      description={description}
      editorRef={editorRef}
      onDescriptionChange={setDescription}
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
      footer={
        <div className="flex items-center justify-between px-4 py-3 border-t border-border">
          <IssueEditorAttachmentButton />
          <div className="flex items-center gap-3">
            <div className="flex items-center gap-2">
              <Switch
                id="create-more"
                size="sm"
                checked={createMore}
                onCheckedChange={(checked) => setCreateMore(checked === true)}
              />
              <Label
                htmlFor="create-more"
                className="text-xs text-muted-foreground cursor-pointer select-none"
=======
    <form onSubmit={handleSubmit}>
      <IssueEditorDialogShell
        open={open}
        onOpenChange={onOpenChange}
        projectPrefix={projectPrefix}
        projectColor={projectColor}
        headerContent={<span className="text-sm">New issue</span>}
        title={title}
        titleRef={titleRef}
        autoFocus
        onTitleChange={setTitle}
        description={description}
        editorRef={editorRef}
        onDescriptionChange={setDescription}
        imageUpload={{
          enabled: false,
          disabledReason: createDisabledReason,
          onFiles: async () => {
            setAttachmentStatus(createDisabledReason)
          },
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
        footer={
          <div className="flex items-center justify-between px-4 py-3 border-t border-border">
            <div className="flex items-center gap-3">
              <IssueEditorAttachmentButton
                disabled
                disabledReason={createDisabledReason}
              />
              {attachmentStatus ? (
                <span className="text-xs text-destructive">{attachmentStatus}</span>
              ) : null}
            </div>
            <div className="flex items-center gap-3">
              <div className="flex items-center gap-2">
                <Switch
                  id="create-more"
                  size="sm"
                  checked={createMore}
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
                disabled={!title.trim() || submitting}
                className="inline-flex items-center justify-center rounded-md bg-indigo-600 px-3 text-xs font-medium text-white transition-colors hover:bg-indigo-700 disabled:pointer-events-none disabled:opacity-50 h-7"
>>>>>>> theirs
              >
                Create more
              </Label>
            </div>
            <Button
              type="submit"
              disabled={!title.trim() || submitting}
              className="inline-flex items-center justify-center rounded-md bg-indigo-600 px-3 text-xs font-medium text-white transition-colors hover:bg-indigo-700 disabled:pointer-events-none disabled:opacity-50 h-7"
            >
              {submitting ? `Creating...` : `Create issue`}
            </Button>
          </div>
        </div>
      }
    />
  )
}

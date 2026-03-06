import { useEffect, useRef, useState } from "react"
import type { Issue, User } from "@/db/schema"
import { trpc } from "@/lib/trpc-client"
import {
  formatDateForMutation,
  getIssueDescriptionText,
} from "@/lib/domain"
import {
  IssueEditorAttachmentButton,
  IssueEditorDialogShell,
} from "@/components/issue-editor-dialog-shell"
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
  const [title, setTitle] = useState(issue.title)
  const [description, setDescription] = useState(
    getIssueDescriptionText(issue.description)
  )

  useEffect(() => {
    const nextDescription = getIssueDescriptionText(issue.description)
    setTitle(issue.title)
    setDescription(nextDescription)
    editorRef.current?.setMarkdown(nextDescription)
  }, [issue.description, issue.id, issue.title])

  const handleTitleBlur = async () => {
    const trimmed = title.trim()

    if (trimmed && trimmed !== issue.title) {
      await trpc.issues.update.mutate({ id: issue.id, title: trimmed })
    }
  }

  const handleDescriptionBlur = async () => {
    const trimmed = description.trim()
    const currentText = getIssueDescriptionText(issue.description)

    if (trimmed !== currentText) {
      await trpc.issues.update.mutate({
        id: issue.id,
        description: trimmed ? { text: trimmed } : null,
      })
    }
  }

  const dueDate = issue.dueDate ? new Date(issue.dueDate + `T00:00:00`) : undefined

  return (
    <IssueEditorDialogShell
      open={open}
      onOpenChange={onOpenChange}
      projectPrefix={projectPrefix}
      projectColor={projectColor}
      headerContent={<span className="text-sm font-mono">{issue.identifier}</span>}
      title={title}
      onTitleChange={setTitle}
      onTitleBlur={() => {
        void handleTitleBlur()
      }}
      description={description}
      editorRef={editorRef}
      onDescriptionChange={setDescription}
      onDescriptionBlur={() => {
        void handleDescriptionBlur()
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
      footer={
        <div className="flex items-center px-4 py-3 border-t border-border">
          <IssueEditorAttachmentButton />
        </div>
      }
    />
  )
}

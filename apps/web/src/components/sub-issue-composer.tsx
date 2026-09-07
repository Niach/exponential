import { useRef, useState } from "react"
import type { Issue, User } from "@/db/schema"
import { issueCollection } from "@/lib/collections"
import type { IssuePriority } from "@/lib/domain"
import { conceptIcon } from "@/lib/icons.generated"
import { statusUpdatePayload, type StatusRowOption } from "@/lib/team-statuses"
import { trpc } from "@/lib/trpc-client"
import { useTeamStatusesContext } from "@/hooks/use-team-statuses"
import { IssueEditorChips } from "@/components/issue-editor/chips"
import {
  MarkdownEditor,
  type MarkdownEditorRef,
} from "@/components/issue-editor/markdown-editor"
import { StatusIcon } from "@/components/issue-properties/status-dropdown"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Pill } from "@/components/ui/pill"

const UiAddIcon = conceptIcon(`ui-add`)

// EXP-760 — the inline sub-issue composer under an issue's description
// (Linear's "Add sub-issues"). It is the create form in miniature: the same
// `IssueEditorChips` row every other editor uses, the same markdown editor,
// and ONE `issues.create` call that carries `parentId` so the `parent`
// relation lands in the create transaction rather than as a second write the
// user can see arrive late.
//
// Desktop only and members only (the caller gates both). Deliberately STAYS
// OPEN after a create with the chips intact: filing sub-issues is a run, not a
// single act — only the title and the description clear, and focus returns to
// the title so the next one can be typed straight away. The `#IDENT` autocomplete,
// image paste and everything else stay out: a sub-issue is filed in a line or
// two, and images can be added on its own page afterwards.

export function SubIssueComposer({
  parent,
  teamId,
  users,
}: {
  parent: Issue
  teamId: string
  users: User[]
}) {
  const { resolve: resolveStatus } = useTeamStatusesContext()
  const [open, setOpen] = useState(false)
  const [title, setTitle] = useState(``)
  const [description, setDescription] = useState(``)
  const [creating, setCreating] = useState(false)
  // `null` = "no explicit pick yet", so a late issue_statuses snapshot
  // upgrades the constructed fallback in place (create-issue-dialog's rule).
  const [pickedStatus, setPickedStatus] = useState<StatusRowOption | null>(null)
  const [priority, setPriority] = useState<IssuePriority>(`none`)
  const [assigneeId, setAssigneeId] = useState<string | null>(null)
  const [selectedLabelIds, setSelectedLabelIds] = useState<string[]>([])
  const titleRef = useRef<HTMLInputElement>(null)
  const editorRef = useRef<MarkdownEditorRef>(null)
  const descriptionRef = useRef(``)

  const status =
    pickedStatus ?? resolveStatus({ status: `backlog`, statusId: null })
  // `users` is the bot-excluded member list; 0 means still loading, so only
  // an actual 1 hides the control (create-issue-dialog's rule).
  const isSolo = users.length === 1

  const close = () => {
    setOpen(false)
    setTitle(``)
    setDescription(``)
    descriptionRef.current = ``
    setPickedStatus(null)
    setPriority(`none`)
    setAssigneeId(null)
    setSelectedLabelIds([])
  }

  const submit = async () => {
    const trimmed = title.trim()
    if (!trimmed || creating) return
    setCreating(true)
    try {
      const body = descriptionRef.current.trim()
      const { txId } = await trpc.issues.create.mutate({
        boardId: parent.boardId,
        parentId: parent.id,
        title: trimmed,
        // EXP-314: real rows write `statusId`; a constructed fallback row
        // writes the anchor enum instead.
        ...statusUpdatePayload(status),
        priority,
        assigneeId: assigneeId ?? undefined,
        description: body ? body : undefined,
        labelIds:
          selectedLabelIds.length > 0 ? selectedLabelIds : undefined,
      })
      // Wait for the row (and its `parent` relation) to reach this client, so
      // the new sub-issue is already under the "Sub-issues" heading when the
      // form clears.
      await issueCollection.utils.awaitTxId(txId)
      setTitle(``)
      setDescription(``)
      descriptionRef.current = ``
      editorRef.current?.setMarkdown(``)
      titleRef.current?.focus()
    } finally {
      setCreating(false)
    }
  }

  if (!open) {
    return (
      <div className="mx-auto w-full max-w-3xl px-4 pt-3">
        <Button
          variant="ghost"
          size="sm"
          className="text-muted-foreground hover:text-foreground"
          onClick={() => setOpen(true)}
        >
          <UiAddIcon className="size-3.5" />
          Add sub-issues
        </Button>
      </div>
    )
  }

  return (
    <div className="mx-auto w-full max-w-3xl px-4 pt-3">
      <div className="flex flex-col gap-2 rounded-xl border border-glass-stroke-card bg-popover/40 p-3">
        <div className="flex min-w-0 items-center gap-2">
          <StatusIcon option={status} className="size-4 shrink-0" />
          <Input
            ref={titleRef}
            autoFocus
            value={title}
            placeholder="Sub-issue title"
            disabled={creating}
            onChange={(event) => setTitle(event.target.value)}
            onKeyDown={(event) => {
              if (event.key === `Enter`) {
                event.preventDefault()
                void submit()
                return
              }
              if (event.key === `Escape`) {
                event.preventDefault()
                close()
              }
            }}
            className="h-8 border-none bg-transparent px-0 shadow-none focus-visible:ring-0 dark:bg-transparent"
          />
        </div>

        <MarkdownEditor
          ref={editorRef}
          markdown={description}
          onChange={(next) => {
            descriptionRef.current = next
            setDescription(next)
          }}
          placeholder="Add description…"
          appearance="chat"
          // Images belong to an issue that exists; a draft-image pipeline for
          // a two-line composer is not worth its weight (the created issue's
          // own page has the full editor).
          imageUpload={{ enabled: false, onFiles: async () => {} }}
        />

        <div className="flex flex-wrap items-center gap-1.5">
          <IssueEditorChips
            status={status}
            onStatusChange={setPickedStatus}
            priority={priority}
            onPriorityChange={setPriority}
            assigneeId={assigneeId}
            onAssigneeChange={setAssigneeId}
            hideAssignee={isSolo}
            users={users}
            teamId={teamId}
            selectedLabelIds={selectedLabelIds}
            onToggleLabel={(labelId) =>
              setSelectedLabelIds((current) =>
                current.includes(labelId)
                  ? current.filter((id) => id !== labelId)
                  : [...current, labelId]
              )
            }
            // A sub-issue inherits its parent's schedule in practice; the due
            // date is one chip too many for a two-line form.
            dueDate={undefined}
            hideDueDateChip
            onDueDateSelect={() => {}}
            disabled={creating}
          />
          <div className="ml-auto flex items-center gap-1.5">
            <Button
              variant="ghost"
              size="sm"
              disabled={creating}
              onClick={close}
            >
              Cancel
            </Button>
            <Pill
              size="md"
              mode="action"
              primary
              disabled={!title.trim() || creating}
              onClick={() => void submit()}
            >
              {creating ? `Creating…` : `Create`}
            </Pill>
          </div>
        </div>
      </div>
    </div>
  )
}

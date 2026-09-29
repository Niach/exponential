import { useCallback, useEffect, useState } from "react"
import type { Issue } from "@/db/schema"
import { trpc } from "@/lib/trpc-client"
import {
  conceptIcon,
  Button,
  Dialog,
  DialogBody,
  DialogCancel,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
  GlassCard,
  GlassGroup,
  Input,
  Pill,
  Textarea,
} from "@exp/ui"
import { MarkdownEditor } from "@/components/issue-editor/markdown-editor"
import { GROUPED_FIELD_ROW } from "@/components/action-editor-dialog"

// EXP-1139: the pull request's DESCRIPTION on the review page — the title and
// body as GitHub holds them (`issues.prDescription`, never a synced column),
// over the diff, with Edit for members while the PR is open. A run's
// description goes stale the moment a later commit changes the scope; before
// this card nothing in the product could show or fix it. Edit is the same
// procedure the MCP `exponential_pr_update` tool uses (`issues.updatePr`), so a
// person and an agent rewrite the same PR the same way. Mirrors the desktop
// `pr_diff` description card + `pr_description_dialog`.

const UiEditIcon = conceptIcon(`ui-edit`)
const UiLoadingIcon = conceptIcon(`ui-loading`)
const ChevronDownIcon = conceptIcon(`ui-chevron-down`)
const ChevronRightIcon = conceptIcon(`ui-chevron-right`)

type DescriptionState =
  | { kind: `loading` }
  | { kind: `ready`; title: string; body: string; state: string | null }
  | { kind: `error`; message: string }

export const PR_TITLE_MAX = 255
export const PR_BODY_MAX = 60_000

export function PrDescriptionCard({
  issue,
  canEdit,
  className,
}: {
  issue: Pick<Issue, `id` | `prNumber` | `prState`>
  /** A member looking at an OPEN pull request. */
  canEdit: boolean
  className?: string
}) {
  const [state, setState] = useState<DescriptionState>({ kind: `loading` })
  const [expanded, setExpanded] = useState(true)
  const [editing, setEditing] = useState(false)
  const issueId = issue.id
  const prNumber = issue.prNumber

  const load = useCallback(() => {
    if (prNumber == null) return
    let cancelled = false
    setState({ kind: `loading` })
    trpc.issues.prDescription
      .query({ issueId })
      .then((res) => {
        if (cancelled) return
        if (res.title === null) {
          setState({ kind: `error`, message: `No pull request is linked.` })
          return
        }
        setState({
          kind: `ready`,
          title: res.title,
          body: res.body ?? ``,
          state: res.state,
        })
      })
      .catch((err: unknown) => {
        if (cancelled) return
        setState({
          kind: `error`,
          message:
            err instanceof Error
              ? err.message
              : `Failed to load the pull request`,
        })
      })
    return () => {
      cancelled = true
    }
  }, [issueId, prNumber])

  useEffect(() => load(), [load])

  if (prNumber == null) return null

  const editable = canEdit && issue.prState === `open`

  return (
    <GlassCard
      className={className}
      data-testid="pr-description-card"
    >
      <div className="flex items-center gap-2 px-3 py-2">
        <button
          type="button"
          className="flex min-w-0 flex-1 items-center gap-2 text-left"
          aria-expanded={expanded}
          aria-label={expanded ? `Hide description` : `Show description`}
          onClick={() => setExpanded((open) => !open)}
        >
          {expanded ? (
            <ChevronDownIcon className="size-3.5 shrink-0 text-muted-foreground" />
          ) : (
            <ChevronRightIcon className="size-3.5 shrink-0 text-muted-foreground" />
          )}
          <span className="min-w-0 truncate text-sm font-medium">
            {state.kind === `ready` ? state.title : `Pull request`}
          </span>
          <span className="shrink-0 font-mono text-xs text-muted-foreground">
            #{prNumber}
          </span>
        </button>
        {state.kind === `loading` && (
          <UiLoadingIcon className="size-3.5 animate-spin text-muted-foreground" />
        )}
        {editable && state.kind === `ready` && (
          <Pill
            mode="action"
            size="sm"
            data-testid="pr-description-edit"
            onClick={() => setEditing(true)}
          >
            <UiEditIcon className="size-3" />
            Edit
          </Pill>
        )}
      </div>
      {expanded && state.kind === `error` && (
        <div className="flex flex-wrap items-center gap-2 border-t border-border px-3 py-2 text-xs text-destructive">
          {state.message}
          <Pill mode="action" onClick={() => load()}>
            Retry
          </Pill>
        </div>
      )}
      {expanded && state.kind === `ready` && (
        <div className="border-t border-border px-3 py-2 text-sm">
          {state.body.trim() ? (
            <MarkdownEditor
              editable={false}
              markdown={state.body}
              onChange={() => {}}
              ariaLabel="Pull request description"
            />
          ) : (
            <span className="text-xs text-muted-foreground">
              No description.
            </span>
          )}
        </div>
      )}
      {state.kind === `ready` && (
        <PrDescriptionDialog
          open={editing}
          onOpenChange={setEditing}
          issueId={issueId}
          prNumber={prNumber}
          title={state.title}
          body={state.body}
          onSaved={(next) =>
            setState({ kind: `ready`, ...next, state: state.state })
          }
        />
      )}
    </GlassCard>
  )
}

function PrDescriptionDialog({
  open,
  onOpenChange,
  issueId,
  prNumber,
  title: initialTitle,
  body: initialBody,
  onSaved,
}: {
  open: boolean
  onOpenChange: (open: boolean) => void
  issueId: string
  prNumber: number
  title: string
  body: string
  onSaved: (next: { title: string; body: string }) => void
}) {
  const [title, setTitle] = useState(initialTitle)
  const [body, setBody] = useState(initialBody)
  const [submitting, setSubmitting] = useState(false)
  const [error, setError] = useState<string | null>(null)

  // Re-seed from GitHub's values every time the dialog opens — a cancelled
  // edit must not leak into the next one.
  useEffect(() => {
    if (!open) return
    setTitle(initialTitle)
    setBody(initialBody)
    setError(null)
  }, [open, initialTitle, initialBody])

  const trimmedTitle = title.trim()
  const changed = trimmedTitle !== initialTitle || body !== initialBody
  const canSubmit = trimmedTitle.length > 0 && changed && !submitting

  const submit = (event: React.FormEvent) => {
    event.preventDefault()
    if (!canSubmit) return
    setSubmitting(true)
    setError(null)
    trpc.issues.updatePr
      .mutate(
        {
          issueId,
          ...(trimmedTitle !== initialTitle ? { title: trimmedTitle } : {}),
          ...(body !== initialBody ? { body } : {}),
        },
        { context: { skipErrorToast: true } }
      )
      .then(() => {
        onSaved({ title: trimmedTitle, body })
        setSubmitting(false)
        onOpenChange(false)
      })
      .catch((err: unknown) => {
        setError(
          err instanceof Error
            ? err.message
            : `The pull request could not be updated`
        )
        setSubmitting(false)
      })
  }

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent
        mobile="sheet-full"
        className="sm:max-h-[85dvh] sm:max-w-3xl"
        data-testid="pr-description-dialog"
      >
        <DialogHeader>
          <DialogTitle>{`Edit pull request #${prNumber}`}</DialogTitle>
          <DialogDescription>
            The title and description on GitHub. Name the issues as #IDENT.
          </DialogDescription>
        </DialogHeader>
        <form onSubmit={submit} className="flex min-h-0 flex-1 flex-col gap-4">
          <DialogBody className="flex min-h-0 flex-col gap-2 sm:overflow-y-visible">
            <GlassGroup className="flex min-h-0 flex-1 flex-col">
              <Input
                id="pr-description-title"
                value={title}
                onChange={(e) => setTitle(e.target.value)}
                placeholder="Title"
                maxLength={PR_TITLE_MAX}
                className={GROUPED_FIELD_ROW}
                autoFocus
              />
              <Textarea
                id="pr-description-body"
                value={body}
                onChange={(e) => setBody(e.target.value)}
                placeholder="Description (GFM)"
                maxLength={PR_BODY_MAX}
                rows={14}
                className={`${GROUPED_FIELD_ROW} min-h-48 flex-1 resize-none field-sizing-fixed font-mono text-xs`}
              />
            </GlassGroup>
            {error && (
              <div className="rounded-md border border-destructive/50 bg-destructive/10 px-3 py-2 text-sm text-destructive">
                {error}
              </div>
            )}
          </DialogBody>
          <DialogFooter>
            <DialogCancel
              onClick={() => onOpenChange(false)}
              disabled={submitting}
            />
            <Button type="submit" disabled={!canSubmit}>
              {submitting ? `Saving…` : `Save changes`}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  )
}

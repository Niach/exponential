import { useEffect, useState } from "react"
import { TRPCClientError } from "@trpc/client"
import { trpc } from "@/lib/trpc-client"
import { ACTION_TEMPLATES } from "@/lib/action-templates"
import { Button } from "@/components/ui/button"
import {
  Dialog,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select"
import { Textarea } from "@/components/ui/textarea"

// Create/edit dialog for team actions (EXP-253) — owner-only (the server
// enforces it). Creating offers the starter templates; the body is the GFM
// prompt an interactive claude session executes on a member's desktop.

/** One action as the actions router returns it. */
export type TeamAction = Awaited<
  ReturnType<typeof trpc.actions.list.query>
>[`actions`][number]

export interface ActionRepoOption {
  id: string
  fullName: string
}

// Radix Select forbids empty-string item values — sentinels for the
// "no repository" choice and the blank template.
const NO_REPO = `none`
const BLANK_TEMPLATE = `blank`

export function ActionEditorDialog({
  open,
  onOpenChange,
  teamId,
  repos,
  action,
  onSaved,
}: {
  open: boolean
  onOpenChange: (open: boolean) => void
  teamId: string
  /** The team's connected repos, for the optional clone-target select. */
  repos: ActionRepoOption[]
  /** Action being edited; null = create a new one. */
  action: TeamAction | null
  onSaved: () => void
}) {
  const [templateId, setTemplateId] = useState(BLANK_TEMPLATE)
  const [name, setName] = useState(``)
  const [description, setDescription] = useState(``)
  const [repoValue, setRepoValue] = useState(NO_REPO)
  const [body, setBody] = useState(``)
  const [submitting, setSubmitting] = useState(false)
  // Duplicate-name CONFLICTs render next to the name field; everything else
  // in the generic box above the footer.
  const [nameError, setNameError] = useState<string | null>(null)
  const [error, setError] = useState<string | null>(null)

  // Seed on OPEN: the edited action's fields, or a blank create form.
  useEffect(() => {
    if (!open) return
    setTemplateId(BLANK_TEMPLATE)
    setName(action?.name ?? ``)
    setDescription(action?.description ?? ``)
    setRepoValue(action?.repositoryId ?? NO_REPO)
    setBody(action?.body ?? ``)
    setSubmitting(false)
    setNameError(null)
    setError(null)
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open])

  const applyTemplate = (id: string) => {
    setTemplateId(id)
    setNameError(null)
    const template = ACTION_TEMPLATES.find((t) => t.id === id)
    if (!template) {
      setName(``)
      setDescription(``)
      setBody(``)
      setRepoValue(NO_REPO)
      return
    }
    setName(template.name)
    setDescription(template.description)
    setBody(template.body)
    // A repo-wanting template preselects the team's only repo; with several
    // (or none) the owner picks explicitly.
    setRepoValue(
      template.wantsRepo && repos.length === 1 ? repos[0].id : NO_REPO
    )
  }

  const canSubmit = Boolean(name.trim()) && Boolean(body.trim())

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault()
    if (!canSubmit || submitting) return
    setSubmitting(true)
    setNameError(null)
    setError(null)
    const payload = {
      name: name.trim(),
      description: description.trim() === `` ? null : description.trim(),
      repositoryId: repoValue === NO_REPO ? null : repoValue,
      body,
    }
    try {
      if (action) {
        await trpc.actions.update.mutate(
          { id: action.id, ...payload },
          { context: { skipErrorToast: true } }
        )
      } else {
        await trpc.actions.create.mutate(
          { teamId, ...payload },
          { context: { skipErrorToast: true } }
        )
      }
      onSaved()
      onOpenChange(false)
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err)
      if (
        err instanceof TRPCClientError &&
        (err.data as { code?: string } | undefined)?.code === `CONFLICT`
      ) {
        setNameError(message)
      } else {
        setError(message)
      }
    } finally {
      setSubmitting(false)
    }
  }

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-h-[calc(100dvh-2rem)] overflow-y-auto sm:max-w-lg">
        <DialogHeader>
          <DialogTitle>{action ? `Edit action` : `New action`}</DialogTitle>
        </DialogHeader>

        <form onSubmit={handleSubmit} className="space-y-4">
          {!action && (
            <div className="space-y-2">
              <Label htmlFor="action-template">Template</Label>
              <Select value={templateId} onValueChange={applyTemplate}>
                <SelectTrigger id="action-template" className="w-full">
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value={BLANK_TEMPLATE}>Blank</SelectItem>
                  {ACTION_TEMPLATES.map((template) => (
                    <SelectItem key={template.id} value={template.id}>
                      {template.name}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </div>
          )}

          <div className="space-y-2">
            <Label htmlFor="action-name">Name</Label>
            <Input
              id="action-name"
              value={name}
              onChange={(e) => {
                setName(e.target.value)
                setNameError(null)
              }}
              placeholder="Code review sweep"
              autoFocus
            />
            {nameError && (
              <p className="text-xs text-destructive">{nameError}</p>
            )}
          </div>

          <div className="space-y-2">
            <Label htmlFor="action-description">Description (optional)</Label>
            <Input
              id="action-description"
              value={description}
              onChange={(e) => setDescription(e.target.value)}
              placeholder="What this action does, for the list"
            />
          </div>

          <div className="space-y-2">
            <Label htmlFor="action-repository">Repository (optional)</Label>
            <Select value={repoValue} onValueChange={setRepoValue}>
              <SelectTrigger id="action-repository" className="w-full">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value={NO_REPO}>None</SelectItem>
                {repos.map((repo) => (
                  <SelectItem key={repo.id} value={repo.id}>
                    {repo.fullName}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
            <p className="text-xs text-muted-foreground">
              With a repository the run clones it first; without one the agent
              works in a scratch directory.
            </p>
          </div>

          <div className="space-y-2">
            <Label htmlFor="action-body">Prompt</Label>
            <Textarea
              id="action-body"
              value={body}
              onChange={(e) => setBody(e.target.value)}
              placeholder="The markdown prompt the agent runs with…"
              rows={12}
              className="min-h-48 font-mono text-xs"
            />
          </div>

          {error && (
            <div className="rounded-md border border-destructive/50 bg-destructive/10 px-3 py-2 text-sm text-destructive">
              {error}
            </div>
          )}

          <DialogFooter>
            <Button
              type="button"
              variant="ghost"
              onClick={() => onOpenChange(false)}
              disabled={submitting}
            >
              Cancel
            </Button>
            <Button type="submit" disabled={!canSubmit || submitting}>
              {submitting
                ? action
                  ? `Saving…`
                  : `Creating…`
                : action
                  ? `Save changes`
                  : `Create action`}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  )
}

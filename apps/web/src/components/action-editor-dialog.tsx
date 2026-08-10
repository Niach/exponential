import { useEffect, useState } from "react"
import { TRPCClientError } from "@trpc/client"
import type { BoardIcon } from "@exp/db-schema/domain"
import type { SyncedAction } from "@/db/schema"
import { BOARD_ICON_OPTIONS } from "@/lib/board-icons"
import { IconSwatchGrid } from "@/components/ui/icon-swatch-grid"
import type { BuiltinAction } from "@/lib/builtin-actions"
import { trpc } from "@/lib/trpc-client"
import { Button } from "@/components/ui/button"
import {
  Dialog,
  DialogBody,
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

// Edit dialog for team actions (EXP-253) — owner-only (the server enforces
// it), EDIT-ONLY since EXP-257: new actions are authored by the builtin
// "Create action" run, so the manual create path (and its templates) is gone.
// The body is the GFM prompt an interactive agent session executes on a
// member's desktop — synced rows exclude it (EXP-268), so the dialog fetches
// it via tRPC `actions.get` on open.

/** One action as the clients list them: a synced (body-less) row or the
 * client-constructed builtin. */
export type TeamAction = (SyncedAction & { builtin: false }) | BuiltinAction

export interface ActionRepoOption {
  id: string
  fullName: string
}

// Radix Select forbids empty-string item values — sentinel for the
// "no repository" choice.
const NO_REPO = `none`

export function ActionEditorDialog({
  open,
  onOpenChange,
  repos,
  action,
}: {
  open: boolean
  onOpenChange: (open: boolean) => void
  /** The team's connected repos, for the optional clone-target select. */
  repos: ActionRepoOption[]
  /** The action being edited (never the builtin — it has no editable body). */
  action: TeamAction
}) {
  const [name, setName] = useState(``)
  const [description, setDescription] = useState(``)
  const [repoValue, setRepoValue] = useState(NO_REPO)
  // EXP-273: the action's display glyph, from the same curated set as boards.
  const [icon, setIcon] = useState<BoardIcon>(BOARD_ICON_OPTIONS[0].name)
  const [body, setBody] = useState(``)
  // Synced rows carry no body (EXP-268) — fetched on open; the prompt field
  // stays disabled until it lands so a save can never blank it.
  const [bodyLoading, setBodyLoading] = useState(false)
  const [submitting, setSubmitting] = useState(false)
  // Duplicate-name CONFLICTs render next to the name field; everything else
  // in the generic box above the footer.
  const [nameError, setNameError] = useState<string | null>(null)
  const [error, setError] = useState<string | null>(null)

  // Seed the edited action's fields on OPEN; the body comes from tRPC.
  useEffect(() => {
    if (!open) return
    setName(action.name)
    setDescription(action.description ?? ``)
    setRepoValue(action.repositoryId ?? NO_REPO)
    setIcon((action.icon as BoardIcon | null) ?? BOARD_ICON_OPTIONS[0].name)
    setBody(``)
    setBodyLoading(true)
    setSubmitting(false)
    setNameError(null)
    setError(null)
    let active = true
    trpc.actions.get
      .query({ id: action.id })
      .then((res) => {
        if (!active) return
        setBody(res.action.body)
        setBodyLoading(false)
      })
      .catch((err) => {
        if (!active) return
        setError(err instanceof Error ? err.message : String(err))
        setBodyLoading(false)
      })
    return () => {
      active = false
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open])

  const canSubmit = Boolean(name.trim()) && Boolean(body.trim()) && !bodyLoading

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault()
    if (!canSubmit || submitting) return
    setSubmitting(true)
    setNameError(null)
    setError(null)
    try {
      await trpc.actions.update.mutate(
        {
          id: action.id,
          name: name.trim(),
          description: description.trim() === `` ? null : description.trim(),
          icon,
          repositoryId: repoValue === NO_REPO ? null : repoValue,
          body,
        },
        { context: { skipErrorToast: true } }
      )
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
      {/* Wide on desktop (EXP-267): metadata left, prompt right — the prompt
          is the tall field, so splitting columns keeps the dialog 16:9-ish
          instead of a narrow tower. On mobile the base dialog is a
          full-screen page and the grid stacks to one column. */}
      <DialogContent className="sm:max-h-[85dvh] sm:max-w-4xl">
        <DialogHeader>
          <DialogTitle>Edit action</DialogTitle>
        </DialogHeader>

        <form
          onSubmit={handleSubmit}
          className="flex min-h-0 flex-1 flex-col gap-4"
        >
          {/* EXP-467: the BODY never scrolls on desktop — the prompt textarea
              is the scrollable element (field-sizing-fixed below), so the
              dialog's height tracks the short metadata column instead of
              growing to the viewport cap with whitespace beside it. Mobile
              keeps the base single-column body scroll. */}
          <DialogBody className="grid gap-4 sm:min-h-0 sm:grid-cols-[minmax(0,2fr)_minmax(0,3fr)] sm:gap-x-6 sm:overflow-y-visible">
            <div className="space-y-4">
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
                <Label htmlFor="action-description">
                  Description (optional)
                </Label>
                <Input
                  id="action-description"
                  value={description}
                  onChange={(e) => setDescription(e.target.value)}
                  placeholder="What this action does, for the list"
                />
              </div>

              <div className="space-y-2">
                <Label>Icon</Label>
                <IconSwatchGrid value={icon} onChange={setIcon} />
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
                  With a repository the run clones it first; without one the
                  agent works in a scratch directory.
                </p>
              </div>
            </div>

            <div className="flex min-h-0 flex-col gap-2">
              <Label htmlFor="action-body">Prompt</Label>
              <Textarea
                id="action-body"
                value={body}
                onChange={(e) => setBody(e.target.value)}
                placeholder={
                  bodyLoading
                    ? `Loading prompt…`
                    : `The markdown prompt the agent runs with…`
                }
                disabled={bodyLoading}
                rows={12}
                className="min-h-48 flex-1 resize-none field-sizing-fixed font-mono text-xs"
              />
            </div>

            {error && (
              <div className="rounded-md border border-destructive/50 bg-destructive/10 px-3 py-2 text-sm text-destructive sm:col-span-2">
                {error}
              </div>
            )}
          </DialogBody>

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
              {submitting ? `Saving…` : `Save changes`}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  )
}

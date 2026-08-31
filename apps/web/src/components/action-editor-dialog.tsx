import { useEffect, useState } from "react"
import { TRPCClientError } from "@trpc/client"
import type { BoardIcon } from "@exp/db-schema/domain"
import type { SyncedAction } from "@/db/schema"
import { BOARD_ICON_OPTIONS } from "@/lib/board-icons"
import { IconPicker } from "@/components/ui/icon-picker"
import type { BuiltinAction } from "@/lib/builtin-actions"
import { trpc } from "@/lib/trpc-client"
import { Button } from "@/components/ui/button"
import {
  Dialog,
  DialogBody,
  DialogCancel,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import { GlassGroup, GlassPickerRow } from "@/components/ui/glass-rows"
import { Input } from "@/components/ui/input"
import { Textarea } from "@/components/ui/textarea"

// Edit dialog for team actions (EXP-253) — owner-only writes (the server
// enforces it; a non-owner opens the same dialog `readOnly`, exactly as the
// native sheets do). EDIT-ONLY since EXP-257: new actions are authored by the
// builtin "Create action" run, so the manual create path (and its templates)
// is gone.
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

// EXP-694 — the editor controls are the SAME on every client: fields sit as
// rows inside the grouped card stack, with no label above them (the
// placeholder carries the title) and no chrome of their own (the group's fill
// and hairlines ARE the field). Mirrors the desktop `action_editor_dialog` and
// the native Create/Edit action sheets. Exported because the New-action dialog
// (`create-action-dialog.tsx`) is the same form and must not re-derive them.
export const GROUPED_FIELD = `rounded-none border-0 bg-transparent text-sm shadow-none focus-visible:border-0 focus-visible:ring-0 md:text-sm`
// 16h/12v is the row padding of the whole ladder (GlassPickerRow/ToggleRow).
export const GROUPED_FIELD_ROW = `${GROUPED_FIELD} px-4 py-3`

export function ActionEditorDialog({
  open,
  onOpenChange,
  repos,
  action,
  readOnly = false,
}: {
  open: boolean
  onOpenChange: (open: boolean) => void
  /** The team's connected repos, for the optional clone-target select. */
  repos: ActionRepoOption[]
  /** The action being edited (never the builtin — it has no editable body). */
  action: TeamAction
  /** EXP-694: writes are owner-only (the server enforces it), so a NON-owner
   * who reaches this dialog from a session row reads the action instead of
   * filling in a form whose save would be refused — the same read-only sheet
   * iOS (`EditActionSheet`) and Android (`ActionEditSheet`) fall back to. */
  readOnly?: boolean
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
    if (readOnly || !canSubmit || submitting) return
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
          instead of a narrow tower. EXP-694: on mobile it is the tall sheet
          every other big form uses, and the grid stacks to one column. */}
      <DialogContent
        mobile="sheet-full"
        className="sm:max-h-[85dvh] sm:max-w-4xl"
      >
        <DialogHeader>
          <DialogTitle>{readOnly ? `Action` : `Edit action`}</DialogTitle>
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
            <div className="flex flex-col gap-2">
              <GlassGroup>
                {/* Icon and name are ONE row on every client — the glyph
                    picker leads, the name types straight into the row. */}
                <div className="flex items-center gap-3 px-4 py-3">
                  <IconPicker
                    id="action-icon"
                    value={icon}
                    onChange={(next) => setIcon(next as BoardIcon)}
                    disabled={readOnly}
                  />
                  <Input
                    id="action-name"
                    value={name}
                    onChange={(e) => {
                      setName(e.target.value)
                      setNameError(null)
                    }}
                    placeholder="Name"
                    className={`${GROUPED_FIELD} h-auto min-w-0 flex-1 p-0`}
                    autoFocus={!readOnly}
                    readOnly={readOnly}
                  />
                </div>
                <Textarea
                  id="action-description"
                  value={description}
                  onChange={(e) => setDescription(e.target.value)}
                  placeholder="Description"
                  className={`${GROUPED_FIELD_ROW} min-h-16`}
                  readOnly={readOnly}
                />
              </GlassGroup>
              {nameError && (
                <p className="px-1 text-xs text-destructive">{nameError}</p>
              )}

              <GlassGroup>
                <GlassPickerRow
                  label="Repository"
                  value={repoValue}
                  onValueChange={setRepoValue}
                  disabled={readOnly}
                  options={[
                    { value: NO_REPO, label: `None` },
                    ...repos.map((repo) => ({
                      value: repo.id,
                      label: repo.fullName,
                    })),
                  ]}
                />
              </GlassGroup>
              <p className="px-1 text-xs text-muted-foreground">
                With a repository the run clones it first; without one the
                agent works in a scratch directory.
              </p>
            </div>

            <div className="flex min-h-0 flex-col gap-2">
              <GlassGroup className="min-h-0 flex-1">
                <Textarea
                  id="action-body"
                  value={body}
                  onChange={(e) => setBody(e.target.value)}
                  placeholder={bodyLoading ? `Loading prompt…` : `Prompt`}
                  disabled={bodyLoading}
                  readOnly={readOnly}
                  rows={12}
                  className={`${GROUPED_FIELD_ROW} min-h-48 flex-1 resize-none field-sizing-fixed font-mono text-xs`}
                />
              </GlassGroup>
            </div>

            {error && (
              <div className="rounded-md border border-destructive/50 bg-destructive/10 px-3 py-2 text-sm text-destructive sm:col-span-2">
                {error}
              </div>
            )}
          </DialogBody>

          {/* Read-only draws no footer at all — the same "no bottom strip"
              the native read-only sheets resolve to (iOS `EditActionSheet`);
              the dialog's own close control dismisses it. */}
          {!readOnly && (
            <DialogFooter>
              <DialogCancel
                onClick={() => onOpenChange(false)}
                disabled={submitting}
              />
              <Button type="submit" disabled={!canSubmit || submitting}>
                {submitting ? `Saving…` : `Save changes`}
              </Button>
            </DialogFooter>
          )}
        </form>
      </DialogContent>
    </Dialog>
  )
}

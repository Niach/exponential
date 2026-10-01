import { useEffect, useState } from "react"
import { TRPCClientError } from "@trpc/client"
import { MAX_ACTION_PROMPT_PLACEHOLDER, type BoardIcon } from "@exp/db-schema/domain"
import type { SyncedAction } from "@/db/schema"
import {
  BARE_FIELD_CLASS,
  BOARD_ICON_OPTIONS,
  IconPicker,
  Button,
  Combobox,
  GlassGroup,
  Input,
  Textarea,
} from "@exp/ui"
import type { BuiltinAction } from "@/lib/builtin-actions"
import { trpc } from "@/lib/trpc-client"

// The Prompt part of an action's page (EXP-253; a dialog until SLOP-2) —
// owner-only writes (the server enforces it; a non-owner gets the same form
// `readOnly`, exactly as the native screens do). EDIT-ONLY since EXP-257: new
// actions are authored by the builtin "Create action" run.
// The body is the GFM prompt an agent session executes on a member's device —
// synced rows exclude it (EXP-268), so the form fetches it via tRPC
// `actions.get` when the action changes.

/** One action as the clients list them: a synced (body-less) row or the
 * client-constructed builtin. */
export type TeamAction = (SyncedAction & { builtin: false }) | BuiltinAction

export interface ActionRepoOption {
  id: string
  fullName: string
}

// An empty string is no usable option identity — sentinel for the
// "no repository" choice.
const NO_REPO = `none`

// EXP-694 — the editor controls are the SAME on every client: fields sit as
// rows inside the grouped card stack, with no label above them (the
// placeholder carries the title) and no chrome of their own (the group's fill
// and hairlines ARE the field). Mirrors the desktop `action_editor_dialog` and
// the native action screens. Exported for the other grouped forms.
export const GROUPED_FIELD = `rounded-none border-0 bg-transparent text-sm shadow-none focus-visible:border-0 focus-visible:ring-0 md:text-sm`
// 16h/12v is the row padding of the whole ladder (the Combobox `row` trigger
// and GlassToggleRow).
export const GROUPED_FIELD_ROW = `${GROUPED_FIELD} px-4 py-3`

export function ActionPromptForm({
  repos,
  action,
  readOnly = false,
}: {
  /** The team's connected repos, for the optional clone-target select. */
  repos: ActionRepoOption[]
  /** The action being edited (never the builtin — it has no editable body). */
  action: TeamAction
  /** EXP-694: writes are owner-only (the server enforces it), so a NON-owner
   * reads the action instead of filling in a form whose save would be
   * refused — the same read-only form the natives fall back to. */
  readOnly?: boolean
}) {
  const [name, setName] = useState(``)
  const [description, setDescription] = useState(``)
  // EXP-825: the composer's field hint while this action is picked.
  const [promptPlaceholder, setPromptPlaceholder] = useState(``)
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
  // The body as last loaded or saved — Save lights up once a field differs.
  const [savedBody, setSavedBody] = useState(``)

  // Seed the fields when the ACTION changes (never on a resync of the same
  // one: that would wipe what is being typed); the body comes from tRPC.
  useEffect(() => {
    setName(action.name)
    setDescription(action.description ?? ``)
    setPromptPlaceholder(action.promptPlaceholder ?? ``)
    setRepoValue(action.repositoryId ?? NO_REPO)
    setIcon((action.icon as BoardIcon | null) ?? BOARD_ICON_OPTIONS[0].name)
    setBody(``)
    setSavedBody(``)
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
        setSavedBody(res.action.body)
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
  }, [action.id])

  const dirty =
    name.trim() !== action.name ||
    description.trim() !== (action.description ?? ``) ||
    promptPlaceholder.trim() !== (action.promptPlaceholder ?? ``) ||
    repoValue !== (action.repositoryId ?? NO_REPO) ||
    icon !== ((action.icon as BoardIcon | null) ?? BOARD_ICON_OPTIONS[0].name) ||
    body !== savedBody
  const canSubmit =
    dirty && Boolean(name.trim()) && Boolean(body.trim()) && !bodyLoading

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
          promptPlaceholder:
            promptPlaceholder.trim() === `` ? null : promptPlaceholder.trim(),
        },
        { context: { skipErrorToast: true } }
      )
      setSavedBody(body)
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
    <form onSubmit={handleSubmit} className="flex flex-col gap-3">
      {/* Wide on desktop (EXP-267): metadata left, prompt right — the prompt
          is the tall field. On a phone the grid stacks to one column. */}
      <div className="grid gap-4 md:grid-cols-[minmax(0,2fr)_minmax(0,3fr)] md:gap-x-6">
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
                className={BARE_FIELD_CLASS}
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
            {/* EXP-825: what the requester should type beside this action
                — shown as the composer's field hint. */}
            <Input
              id="action-prompt-placeholder"
              value={promptPlaceholder}
              onChange={(e) => setPromptPlaceholder(e.target.value)}
              placeholder="Composer hint, e.g. Scope: which platforms, which version"
              maxLength={MAX_ACTION_PROMPT_PLACEHOLDER}
              className={GROUPED_FIELD_ROW}
              readOnly={readOnly}
            />
          </GlassGroup>
          {nameError && (
            <p className="px-1 text-xs text-destructive">{nameError}</p>
          )}

          <GlassGroup>
            <Combobox
              triggerVariant="row"
              searchable={false}
              mobileTitle="Repository"
              value={repoValue}
              onChange={(value) => {
                if (value !== null) setRepoValue(value)
              }}
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
            With a repository the run clones it first; without one the agent
            works in a scratch directory.
          </p>
        </div>

        <GlassGroup>
          <Textarea
            id="action-body"
            value={body}
            onChange={(e) => setBody(e.target.value)}
            placeholder={bodyLoading ? `Loading prompt…` : `Prompt`}
            disabled={bodyLoading}
            readOnly={readOnly}
            rows={12}
            className={`${GROUPED_FIELD_ROW} h-full min-h-64 resize-none field-sizing-fixed font-mono text-xs`}
          />
        </GlassGroup>
      </div>

      {error && (
        <div className="rounded-md border border-destructive/50 bg-destructive/10 px-3 py-2 text-sm text-destructive">
          {error}
        </div>
      )}

      {/* Read-only draws no save strip at all, like the native screens. */}
      {!readOnly && (
        <div className="flex justify-end">
          <Button type="submit" disabled={!canSubmit || submitting}>
            {submitting ? `Saving…` : `Save changes`}
          </Button>
        </div>
      )}
    </form>
  )
}

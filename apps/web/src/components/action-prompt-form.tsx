import { useCallback, useEffect, useRef, useState } from "react"
import { TRPCClientError } from "@trpc/client"
import {
  MAX_ACTION_INPUT_LABEL,
  MAX_ACTION_INPUTS,
  MAX_ACTION_PROMPT_PLACEHOLDER,
  type ActionInputDef,
  type ActionInputType,
  type BoardIcon,
} from "@exp/db-schema/domain"
import type { SyncedAction } from "@/db/schema"
import {
  BARE_FIELD_CLASS,
  BOARD_ICON_OPTIONS,
  conceptIcon,
  Dialog,
  DialogBody,
  DialogContent,
  DialogHeader,
  DialogTitle,
  FEEDBACK_ERRORS,
  FEEDBACK_WORDS,
  FieldError,
  IconPicker,
  Button,
  GlassGroup,
  GlassSectionHeader,
  Input,
  Picker,
  Pill,
  SkeletonRows,
  Switch,
  Textarea,
} from "@exp/ui"
import { failedAction, SKIP_ERROR_TOAST } from "@/lib/action-error"
import { trpc } from "@/lib/trpc-client"
import { actionCollection } from "@/lib/collections"

// An action = a SAVED PROMPT (EXP-1293 Threads v2): a name, an icon, the
// markdown body a session runs, the typed picks the requester fills in
// (`inputs`: repo | board | pr | issue) and the composer hint
// (`promptPlaceholder`). Running one seeds the home composer; there are no
// triggers (they moved onto team agents), no runs list, no repository and
// no MCP picks any more. Owner-only writes (the server enforces it; a
// non-owner reads the same form `readOnly`, as the native screens do). The
// body is excluded from the synced rows (EXP-268), so an existing action's
// form fetches it via `actions.get`; a NEW action is authored here too
// (`CreateActionDialog`), since the builtin "Create action" run is gone.

// EXP-694 — the editor controls are the SAME on every client: fields sit as
// rows inside the grouped card stack, with no label above them (the
// placeholder carries the title) and no chrome of their own (the group's fill
// and hairlines ARE the field). Exported for the other grouped forms.
export const GROUPED_FIELD = `rounded-none border-0 bg-transparent text-sm shadow-none focus-visible:border-0 focus-visible:ring-0 md:text-sm`
// 16h/12v is the row padding of the whole ladder (the Picker `row` trigger
// and GlassToggleRow).
export const GROUPED_FIELD_ROW = `${GROUPED_FIELD} px-4 py-3`

/** The server's `MAX_ACTION_BODY` (`lib/trpc/actions.ts`, a zod `.max`, so
 *  characters): over it is an input error under the prompt, never a save. */
export const ACTION_BODY_MAX_CHARS = 64 * 1024

const UiAddIcon = conceptIcon(`ui-add`)
const UiCloseIcon = conceptIcon(`ui-close`)

/** The pick kinds an input may ask for, in picker order. */
export const ACTION_INPUT_TYPE_LABELS: Record<ActionInputType, string> = {
  repo: `Repository`,
  board: `Board`,
  pr: `Pull request`,
  issue: `Issue`,
}

/** One input row as edited: `key` is kept for an existing input (runs and
 *  the prompt refer to it), minted from the label for a new one. */
export interface ActionInputDraft {
  key?: string
  label: string
  type: ActionInputType
  required: boolean
  placeholder?: string
}

/** A label → a snake_case key (`[a-z][a-z0-9_]{0,31}`), never empty. */
export function inputKeyFromLabel(label: string): string {
  const slug = label
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, `_`)
    .replace(/^_+|_+$/g, ``)
    .replace(/^[^a-z]+/, ``)
    .slice(0, 32)
    .replace(/_+$/, ``)
  return slug === `` ? `input` : slug
}

/** The drafts as the strict `inputs` array: blank rows dropped, every key
 *  unique (a clash gets `_2`, `_3`…), existing keys kept verbatim. */
export function draftsToInputs(drafts: readonly ActionInputDraft[]): ActionInputDef[] {
  const kept = drafts.filter((draft) => draft.label.trim() !== ``)
  const taken = new Set(kept.flatMap((draft) => (draft.key ? [draft.key] : [])))
  return kept.map((draft) => {
    let key = draft.key
    if (!key) {
      const base = inputKeyFromLabel(draft.label)
      key = base
      for (let n = 2; taken.has(key); n++) {
        const suffix = `_${n}`
        key = `${base.slice(0, 32 - suffix.length)}${suffix}`
      }
      taken.add(key)
    }
    return {
      key,
      label: draft.label.trim(),
      type: draft.type,
      required: draft.required,
      ...(draft.placeholder ? { placeholder: draft.placeholder } : {}),
    }
  })
}

function draftsOf(inputs: readonly ActionInputDef[] | null | undefined): ActionInputDraft[] {
  return (inputs ?? []).map((input) => ({
    key: input.key,
    label: input.label,
    type: input.type,
    required: input.required,
    placeholder: input.placeholder,
  }))
}

const sameInputs = (a: readonly ActionInputDef[], b: readonly ActionInputDef[]) =>
  JSON.stringify(a) === JSON.stringify(b)

/**
 * The action's ONE form. `action` = the synced row to edit; null = create
 * a new action in `teamId` (`onCreated` gets its id).
 */
export function ActionPromptForm({
  action,
  teamId,
  readOnly = false,
  onCreated,
  onCancel,
}: {
  action: SyncedAction | null
  teamId: string
  /** EXP-694: writes are owner-only; a NON-owner reads the action. */
  readOnly?: boolean
  onCreated?: (actionId: string) => void
  /** Create mode inside a dialog: the Cancel beside Create. */
  onCancel?: () => void
}) {
  const creating = action === null
  const [name, setName] = useState(``)
  const [promptPlaceholder, setPromptPlaceholder] = useState(``)
  const [icon, setIcon] = useState<BoardIcon>(BOARD_ICON_OPTIONS[0].name)
  const [inputs, setInputs] = useState<ActionInputDraft[]>([])
  const [body, setBody] = useState(``)
  const [savedBody, setSavedBody] = useState(``)
  const [bodyLoading, setBodyLoading] = useState(false)
  const [bodyFailed, setBodyFailed] = useState(false)
  const [submitting, setSubmitting] = useState(false)
  // FEED-79: a duplicate-name CONFLICT renders under the name field (input);
  // every other failed load/save is a toast.
  const [nameError, setNameError] = useState<string | null>(null)

  // The body fetch; a newer load or another action drops a stale answer.
  const loadSeq = useRef(0)
  const actionId = action?.id ?? null
  const loadBody = useCallback(
    (retried = false) => {
      if (!actionId) return
      const seq = ++loadSeq.current
      setBodyLoading(true)
      trpc.actions.get
        .query({ id: actionId })
        .then((res) => {
          if (seq !== loadSeq.current) return
          setBody(res.action.body)
          setSavedBody(res.action.body)
          setBodyFailed(false)
          setBodyLoading(false)
        })
        .catch((err) => {
          if (seq !== loadSeq.current) return
          setBodyFailed(true)
          setBodyLoading(false)
          // The load on open fails quietly (the field's empty state + Retry);
          // only a failed Retry tap, the person's own action, toasts.
          if (retried) failedAction(`load`, `the prompt`, err, { retry: () => loadBody(true) })
        })
    },
    [actionId]
  )

  // Seed when the ACTION changes (never on a resync of the same one: that
  // would wipe what is being typed); the body comes from tRPC.
  useEffect(() => {
    setName(action?.name ?? ``)
    setPromptPlaceholder(action?.promptPlaceholder ?? ``)
    setIcon((action?.icon as BoardIcon | null) ?? BOARD_ICON_OPTIONS[0].name)
    setInputs(draftsOf(action?.inputs))
    setBody(``)
    setSavedBody(``)
    setBodyFailed(false)
    setSubmitting(false)
    setNameError(null)
    loadBody()
    return () => {
      loadSeq.current++
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [actionId])

  const nextInputs = draftsToInputs(inputs)
  const inputsDirty = !sameInputs(nextInputs, action?.inputs ?? [])
  const dirty =
    creating ||
    name.trim() !== (action?.name ?? ``) ||
    promptPlaceholder.trim() !== (action?.promptPlaceholder ?? ``) ||
    icon !== ((action?.icon as BoardIcon | null) ?? BOARD_ICON_OPTIONS[0].name) ||
    body !== savedBody ||
    inputsDirty
  // Input errors under their fields (FEED-79).
  const [touched, setTouched] = useState(false)
  const nameRequired =
    !readOnly && (touched || !creating) && name.trim() === `` ? `Enter a name` : null
  const bodyTooLong =
    body.length > ACTION_BODY_MAX_CHARS
      ? `Too long: ${body.length} of ${ACTION_BODY_MAX_CHARS} characters`
      : null
  const canSubmit =
    dirty &&
    Boolean(name.trim()) &&
    Boolean(body.trim()) &&
    !bodyTooLong &&
    !bodyLoading &&
    !bodyFailed

  const isConflict = (err: unknown) =>
    err instanceof TRPCClientError &&
    (err.data as { code?: string } | undefined)?.code === `CONFLICT`

  const fields = () => ({
    name: name.trim(),
    icon,
    body,
    inputs: nextInputs,
    promptPlaceholder: promptPlaceholder.trim() === `` ? null : promptPlaceholder.trim(),
  })

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault()
    setTouched(true)
    if (readOnly || !canSubmit || submitting) return
    if (creating) await create(fields())
    else if (action) await save({ id: action.id, ...fields() })
  }

  type ActionUpdate = Parameters<typeof trpc.actions.update.mutate>[0]
  const save = async (input: ActionUpdate) => {
    setSubmitting(true)
    setNameError(null)
    try {
      await trpc.actions.update.mutate(input, SKIP_ERROR_TOAST)
      setSavedBody(input.body ?? body)
    } catch (err) {
      if (isConflict(err)) setNameError((err as Error).message)
      // An update is idempotent: Retry sends the same fields again.
      else failedAction(`save`, `the action`, err, { retry: () => void save(input) })
    } finally {
      setSubmitting(false)
    }
  }

  type ActionCreate = Omit<Parameters<typeof trpc.actions.create.mutate>[0], `teamId`>
  const create = async (input: ActionCreate) => {
    setSubmitting(true)
    setNameError(null)
    try {
      const { action: created, txId } = await trpc.actions.create.mutate(
        { teamId, ...input },
        SKIP_ERROR_TOAST
      )
      // Open the page once the row has synced (it reads the live shape).
      await actionCollection.utils.awaitTxId(txId).catch(() => {})
      onCreated?.(created.id)
    } catch (err) {
      if (isConflict(err)) setNameError((err as Error).message)
      else failedAction(`create`, `the action`, err)
    } finally {
      setSubmitting(false)
    }
  }

  const patchInput = (index: number, patch: Partial<ActionInputDraft>) =>
    setInputs((current) =>
      current.map((input, at) => (at === index ? { ...input, ...patch } : input))
    )

  return (
    <form onSubmit={handleSubmit} className="flex flex-col gap-3" data-testid="action-prompt-form">
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
                aria-invalid={Boolean(nameError ?? nameRequired) || undefined}
                aria-describedby={nameError ?? nameRequired ? `action-name-error` : undefined}
                className={BARE_FIELD_CLASS}
                readOnly={readOnly}
                autoFocus={creating}
              />
            </div>
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
          <FieldError id="action-name-error" className="px-1">
            {nameError ?? nameRequired}
          </FieldError>

          <div>
            <GlassSectionHeader
              label="Inputs"
              trailing={
                !readOnly && inputs.length < MAX_ACTION_INPUTS ? (
                  <Pill
                    mode="action"
                    onClick={() =>
                      setInputs((current) => [
                        ...current,
                        { label: ``, type: `issue`, required: false },
                      ])
                    }
                    data-testid="action-add-input"
                  >
                    <UiAddIcon className="size-3" />
                    Add input
                  </Pill>
                ) : undefined
              }
            />
            {inputs.length === 0 ? (
              <p className="px-1 py-2 text-xs text-muted-foreground">
                No inputs. The prompt runs as written.
              </p>
            ) : (
              <GlassGroup>
                {inputs.map((input, index) => (
                  <div
                    key={input.key ?? `new-${index}`}
                    className="flex items-center gap-2 py-1 pr-2"
                    data-testid="action-input-row"
                  >
                    <Input
                      aria-label="Input label"
                      value={input.label}
                      onChange={(e) => patchInput(index, { label: e.target.value })}
                      placeholder="Label"
                      maxLength={MAX_ACTION_INPUT_LABEL}
                      className={`${GROUPED_FIELD_ROW} min-w-0 flex-1`}
                      readOnly={readOnly}
                    />
                    <Picker
                      mode="single"
                      triggerVariant="inline"
                      mobileTitle="Picks"
                      width="sm"
                      value={input.type}
                      disabled={readOnly}
                      items={(Object.keys(ACTION_INPUT_TYPE_LABELS) as ActionInputType[]).map(
                        (type) => ({ value: type, label: ACTION_INPUT_TYPE_LABELS[type] })
                      )}
                      onChange={(value) => {
                        if (value !== null) patchInput(index, { type: value as ActionInputType })
                      }}
                    />
                    <span className="flex shrink-0 items-center gap-1.5 text-xs text-muted-foreground">
                      Required
                      <Switch
                        checked={input.required}
                        disabled={readOnly}
                        onCheckedChange={(required) => patchInput(index, { required })}
                        aria-label={`Required: ${input.label || `input`}`}
                      />
                    </span>
                    {!readOnly && (
                      <Button
                        type="button"
                        variant="ghost"
                        size="icon-sm"
                        aria-label={`Remove ${input.label || `input`}`}
                        onClick={() =>
                          setInputs((current) => current.filter((_, at) => at !== index))
                        }
                      >
                        <UiCloseIcon />
                      </Button>
                    )}
                  </div>
                ))}
              </GlassGroup>
            )}
          </div>
        </div>

        <div className="flex flex-col gap-2">
          <GlassGroup className="flex-1">
            {bodyLoading ? (
              <SkeletonRows dot={false} className="min-h-64 py-1" />
            ) : bodyFailed ? (
              <div className="flex min-h-64 flex-col items-center justify-center gap-2 text-sm text-muted-foreground">
                Prompt unavailable
                <Button type="button" variant="outline" size="sm" onClick={() => loadBody(true)}>
                  {FEEDBACK_ERRORS.retry}
                </Button>
              </div>
            ) : (
              <Textarea
                id="action-body"
                value={body}
                onChange={(e) => setBody(e.target.value)}
                placeholder="Prompt"
                readOnly={readOnly}
                rows={12}
                aria-invalid={Boolean(bodyTooLong) || undefined}
                aria-describedby={bodyTooLong ? `action-body-error` : undefined}
                className={`${GROUPED_FIELD_ROW} h-full min-h-64 resize-none field-sizing-fixed font-mono text-xs`}
              />
            )}
          </GlassGroup>
          <FieldError id="action-body-error" className="px-1">
            {bodyTooLong}
          </FieldError>
        </div>
      </div>

      {/* Read-only draws no save strip at all, like the native screens. */}
      {!readOnly && (
        <div className="flex justify-end gap-2">
          {onCancel && (
            <Button type="button" variant="outline" onClick={onCancel} disabled={submitting}>
              Cancel
            </Button>
          )}
          <Button
            type="submit"
            disabled={!canSubmit}
            pending={submitting}
            pendingLabel={FEEDBACK_WORDS.pending.save}
          >
            {creating ? `Create action` : `Save changes`}
          </Button>
        </div>
      )}
    </form>
  )
}

/** "New action": the same form in create mode, in a dialog (a sheet on a
 *  phone); `onCreated` opens the new action's page. */
export function CreateActionDialog({
  open,
  onOpenChange,
  teamId,
  onCreated,
}: {
  open: boolean
  onOpenChange: (open: boolean) => void
  teamId: string
  onCreated: (actionId: string) => void
}) {
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent mobile="sheet-full" className="sm:max-h-[85dvh] sm:max-w-3xl">
        <DialogHeader>
          <DialogTitle>New action</DialogTitle>
        </DialogHeader>
        <DialogBody>
          {open && (
            <ActionPromptForm
              action={null}
              teamId={teamId}
              onCancel={() => onOpenChange(false)}
              onCreated={(id) => {
                onOpenChange(false)
                onCreated(id)
              }}
            />
          )}
        </DialogBody>
      </DialogContent>
    </Dialog>
  )
}

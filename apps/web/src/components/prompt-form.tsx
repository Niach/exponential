import { useCallback, useEffect, useRef, useState } from "react"
import { TRPCClientError } from "@trpc/client"
import type { BoardIcon } from "@exp/db-schema/domain"
import type { SyncedPrompt } from "@/db/schema"
import {
  BARE_FIELD_CLASS,
  BOARD_ICON_OPTIONS,
  FEEDBACK_ERRORS,
  FEEDBACK_WORDS,
  FieldError,
  IconPicker,
  Button,
  GlassGroup,
  Input,
  SkeletonRows,
  Textarea,
} from "@exp/ui"
import { failedAction, SKIP_ERROR_TOAST } from "@/lib/action-error"
import { trpc } from "@/lib/trpc-client"
import { promptCollection } from "@/lib/collections"

// NEW-2: a PROMPT = a saved first message (actions became these): a name, a
// curated icon and the markdown body, nothing else (no inputs, no composer
// hint, no triggers, no repository). Running one is the composer's `/` menu.
// Owner-only writes (the server enforces it; a non-owner reads the same form
// `readOnly`). The body is excluded from the synced rows, so an existing
// prompt's form fetches it via `prompts.get`.

// EXP-694 — the editor controls are the SAME on every client: fields sit as
// rows inside the grouped card stack, with no label above them (the
// placeholder carries the title) and no chrome of their own (the group's fill
// and hairlines ARE the field). Exported for the other grouped forms.
export const GROUPED_FIELD = `rounded-none border-0 bg-transparent text-sm shadow-none focus-visible:border-0 focus-visible:ring-0 md:text-sm`
// 16h/12v is the row padding of the whole ladder (the Picker `row` trigger
// and GlassToggleRow).
export const GROUPED_FIELD_ROW = `${GROUPED_FIELD} px-4 py-3`

/** The server's `MAX_PROMPT_BODY` (`lib/trpc/prompts.ts`, a zod `.max`, so
 *  characters): over it is an input error under the body, never a save. */
export const PROMPT_BODY_MAX_CHARS = 64 * 1024

const DEFAULT_ICON = BOARD_ICON_OPTIONS[0].name

/**
 * The prompt's ONE form. `prompt` = the synced row to edit; null = create a
 * new prompt in `teamId` (`onCreated` gets its id).
 */
export function PromptForm({
  prompt,
  teamId,
  readOnly = false,
  onCreated,
}: {
  prompt: SyncedPrompt | null
  teamId: string
  /** Writes are owner-only; a NON-owner reads the prompt. */
  readOnly?: boolean
  onCreated?: (promptId: string) => void
}) {
  const creating = prompt === null
  const [name, setName] = useState(``)
  const [icon, setIcon] = useState<BoardIcon>(DEFAULT_ICON)
  const [body, setBody] = useState(``)
  const [savedBody, setSavedBody] = useState(``)
  const [bodyLoading, setBodyLoading] = useState(false)
  const [bodyFailed, setBodyFailed] = useState(false)
  const [submitting, setSubmitting] = useState(false)
  // FEED-79: a duplicate-name CONFLICT renders under the name field (input);
  // every other failed load/save is a toast.
  const [nameError, setNameError] = useState<string | null>(null)

  // The body fetch; a newer load or another prompt drops a stale answer.
  const loadSeq = useRef(0)
  const promptId = prompt?.id ?? null
  const loadBody = useCallback(
    (retried = false) => {
      if (!promptId) return
      const seq = ++loadSeq.current
      setBodyLoading(true)
      trpc.prompts.get
        .query({ id: promptId })
        .then((res) => {
          if (seq !== loadSeq.current) return
          setBody(res.prompt.body)
          setSavedBody(res.prompt.body)
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
    [promptId]
  )

  // Seed when the PROMPT changes (never on a resync of the same one: that
  // would wipe what is being typed); the body comes from tRPC.
  useEffect(() => {
    setName(prompt?.name ?? ``)
    setIcon((prompt?.icon as BoardIcon | null) ?? DEFAULT_ICON)
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
  }, [promptId])

  const dirty =
    creating ||
    name.trim() !== (prompt?.name ?? ``) ||
    icon !== ((prompt?.icon as BoardIcon | null) ?? DEFAULT_ICON) ||
    body !== savedBody
  // Input errors under their fields (FEED-79).
  const [touched, setTouched] = useState(false)
  const nameRequired =
    !readOnly && (touched || !creating) && name.trim() === `` ? `Enter a name` : null
  const bodyTooLong =
    body.length > PROMPT_BODY_MAX_CHARS
      ? `Too long: ${body.length} of ${PROMPT_BODY_MAX_CHARS} characters`
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

  const fields = () => ({ name: name.trim(), icon, body })

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault()
    setTouched(true)
    if (readOnly || !canSubmit || submitting) return
    if (creating) await create(fields())
    else if (prompt) await save({ id: prompt.id, ...fields() })
  }

  type PromptUpdate = Parameters<typeof trpc.prompts.update.mutate>[0]
  const save = async (input: PromptUpdate) => {
    setSubmitting(true)
    setNameError(null)
    try {
      await trpc.prompts.update.mutate(input, SKIP_ERROR_TOAST)
      setSavedBody(input.body ?? body)
    } catch (err) {
      if (isConflict(err)) setNameError((err as Error).message)
      // An update is idempotent: Retry sends the same fields again.
      else failedAction(`save`, `the prompt`, err, { retry: () => void save(input) })
    } finally {
      setSubmitting(false)
    }
  }

  type PromptCreate = Omit<Parameters<typeof trpc.prompts.create.mutate>[0], `teamId`>
  const create = async (input: PromptCreate) => {
    setSubmitting(true)
    setNameError(null)
    try {
      const { prompt: created, txId } = await trpc.prompts.create.mutate(
        { teamId, ...input },
        SKIP_ERROR_TOAST
      )
      // Open the page once the row has synced (it reads the live shape).
      await promptCollection.utils.awaitTxId(txId).catch(() => {})
      onCreated?.(created.id)
    } catch (err) {
      if (isConflict(err)) setNameError((err as Error).message)
      else failedAction(`create`, `the prompt`, err)
    } finally {
      setSubmitting(false)
    }
  }

  return (
    <form onSubmit={handleSubmit} className="flex flex-col gap-3" data-testid="prompt-form">
      <div className="flex flex-col gap-2">
        <GlassGroup>
          {/* Icon and name are ONE row on every client — the glyph picker
              leads, the name types straight into the row. */}
          <div className="flex items-center gap-3 px-4 py-3">
            <IconPicker
              id="prompt-icon"
              value={icon}
              onChange={(next) => setIcon(next as BoardIcon)}
              disabled={readOnly}
            />
            <Input
              id="prompt-name"
              value={name}
              onChange={(e) => {
                setName(e.target.value)
                setNameError(null)
              }}
              placeholder="Name"
              aria-invalid={Boolean(nameError ?? nameRequired) || undefined}
              aria-describedby={nameError ?? nameRequired ? `prompt-name-error` : undefined}
              className={BARE_FIELD_CLASS}
              readOnly={readOnly}
              autoFocus={creating}
            />
          </div>
        </GlassGroup>
        <FieldError id="prompt-name-error" className="px-1">
          {nameError ?? nameRequired}
        </FieldError>
      </div>

      <div className="flex flex-col gap-2">
        <GlassGroup>
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
              id="prompt-body"
              value={body}
              onChange={(e) => setBody(e.target.value)}
              placeholder="Prompt"
              readOnly={readOnly}
              rows={14}
              aria-invalid={Boolean(bodyTooLong) || undefined}
              aria-describedby={bodyTooLong ? `prompt-body-error` : undefined}
              className={`${GROUPED_FIELD_ROW} min-h-64 resize-y font-mono text-xs`}
              data-testid="prompt-body"
            />
          )}
        </GlassGroup>
        <FieldError id="prompt-body-error" className="px-1">
          {bodyTooLong}
        </FieldError>
      </div>

      {/* Read-only draws no save strip at all, like the native screens. */}
      {!readOnly && (
        <div className="flex justify-end gap-2">
          <Button
            type="submit"
            disabled={!canSubmit}
            pending={submitting}
            pendingLabel={FEEDBACK_WORDS.pending.save}
            data-testid="prompt-submit"
          >
            {creating ? `Create prompt` : `Save changes`}
          </Button>
        </div>
      )}
    </form>
  )
}

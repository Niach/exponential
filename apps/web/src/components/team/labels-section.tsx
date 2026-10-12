import { useState } from "react"
import { deleteLabelPrompt, promptActions } from "@/lib/prompts"
import { useLiveQuery, eq } from "@tanstack/react-db"
import { Plus, Trash2 } from "lucide-react"
import { labelCollection } from "@/lib/collections"
import { trpc } from "@/lib/trpc-client"
import { failedAction, SKIP_ERROR_TOAST } from "@/lib/action-error"
import { trpcErrorCode, trpcErrorMessage } from "@/lib/trpc-error"
import { Empty } from "@/components/empty"
import type { Label as LabelType } from "@/db/schema"
import {
  Button,
  FEEDBACK_WORDS,
  FieldError,
  Pill,
  ListRow,
  SETTINGS_LIST_CLASS,
  GlassSectionHeader,
  Input,
  ColorPicker,
  LABEL_COLORS,
  ColorSwatchGrid,
  Prompt,
} from "@exp/ui"

const DUPLICATE_LABEL = `A label with this name already exists.`

function LabelRow({
  label,
  teamId,
  isDuplicateName,
}: {
  label: LabelType
  teamId: string
  isDuplicateName: (name: string) => boolean
}) {
  const [name, setName] = useState(label.name)
  const [confirmingDelete, setConfirmingDelete] = useState(false)
  const [busy, setBusy] = useState(false)
  // Invalid input only (a name already taken); a failed rename is a toast.
  const [error, setError] = useState<string | null>(null)

  const persistName = async () => {
    const trimmed = name.trim()
    if (!trimmed || trimmed === label.name) {
      setName(label.name)
      setError(null)
      return
    }
    if (isDuplicateName(trimmed)) {
      setError(DUPLICATE_LABEL)
      return
    }
    setBusy(true)
    try {
      const { txId } = await trpc.labels.update.mutate(
        { teamId, labelId: label.id, name: trimmed },
        SKIP_ERROR_TOAST
      )
      await labelCollection.utils.awaitTxId(txId)
      setError(null)
    } catch (err) {
      // A name taken in the meantime is input (under the field), the rest a toast.
      if (trpcErrorCode(err) === `CONFLICT`) setError(trpcErrorMessage(err, DUPLICATE_LABEL))
      else failedAction(`rename`, `the label`, err, { retry: () => void persistName() })
    } finally {
      setBusy(false)
    }
  }

  const persistColor = async (color: string) => {
    if (color === label.color) return
    setBusy(true)
    try {
      const { txId } = await trpc.labels.update.mutate({
        teamId,
        labelId: label.id,
        color,
      })
      await labelCollection.utils.awaitTxId(txId)
    } finally {
      setBusy(false)
    }
  }

  const handleDelete = async () => {
    setBusy(true)
    try {
      const { txId } = await trpc.labels.delete.mutate({
        teamId,
        labelId: label.id,
      })
      await labelCollection.utils.awaitTxId(txId)
    } finally {
      setBusy(false)
      setConfirmingDelete(false)
    }
  }

  const deleteCopy = deleteLabelPrompt(label.name)

  return (
    <ListRow className="flex-col items-stretch gap-0 px-3 py-2">
      <div className="flex items-center gap-3">
        {/* EXP-941: the shared picker, wearing this row's 16px dot as its
            trigger — the popover and the swatch grid inside it are the ONE
            copy now. */}
        <ColorPicker
          value={label.color}
          onChange={persistColor}
          align="start"
          renderTrigger={(color) => (
            <button
              type="button"
              aria-label="Change label color"
              className="h-4 w-4 rounded-full ring-1 ring-border shrink-0"
              style={{ backgroundColor: color }}
              disabled={busy}
            />
          )}
        />
        <Input
          value={name}
          onChange={(e) => {
            setName(e.target.value)
            setError(null)
          }}
          onBlur={persistName}
          onKeyDown={(e) => {
            if (e.key === `Enter`) {
              e.preventDefault()
              ;(e.target as HTMLInputElement).blur()
            }
            if (e.key === `Escape`) {
              setName(label.name)
              setError(null)
              ;(e.target as HTMLInputElement).blur()
            }
          }}
          disabled={busy}
          aria-invalid={error ? true : undefined}
          // EXP-719: the row card IS the field's box. The stock Input brings
          // its own glass fill + radius (EXP-616), which nested a second box
          // inside every row; strip the fill too, not just the stroke, so
          // the name reads as row text that happens to be editable.
          className="h-8 flex-1 rounded-none border-none bg-transparent px-1 shadow-none focus-visible:ring-0 dark:bg-transparent"
        />
        <Button
          variant="ghost"
          size="icon-sm"
          className="hover:text-destructive"
          onClick={() => setConfirmingDelete(true)}
          disabled={busy}
          aria-label={`Delete label ${label.name}`}
        >
          <Trash2 />
        </Button>
      </div>
      <FieldError className="mt-1 px-1">{error}</FieldError>
      <Prompt
        open={confirmingDelete}
        onOpenChange={setConfirmingDelete}
        busy={busy}
        title={deleteCopy.title}
        body={deleteCopy.body}
        actions={promptActions(deleteCopy, {
          delete: { busy, onSelect: handleDelete },
        })}
      />
    </ListRow>
  )
}

export function TeamLabelsSection({ teamId }: { teamId: string }) {
  const [creating, setCreating] = useState(false)
  const [newName, setNewName] = useState(``)
  const [newColor, setNewColor] = useState(LABEL_COLORS[6])
  const [submitting, setSubmitting] = useState(false)
  // A name the server reports as taken (a race past the local check).
  const [createConflict, setCreateConflict] = useState<string | null>(null)

  const { data: labels } = useLiveQuery(
    (q) =>
      q
        .from({ labels: labelCollection })
        .where(({ labels }) => eq(labels.teamId, teamId))
        .orderBy(({ labels }) => labels.sortOrder),
    [teamId]
  )

  const labelList = labels ?? []

  const isDuplicateName = (name: string, excludeId?: string) =>
    labelList.some(
      (l) =>
        l.id !== excludeId &&
        l.name.trim().toLowerCase() === name.trim().toLowerCase()
    )

  const resetForm = () => {
    setNewName(``)
    setNewColor(LABEL_COLORS[Math.floor(Math.random() * LABEL_COLORS.length)])
    setCreateConflict(null)
  }

  const newNameIsDuplicate =
    newName.trim().length > 0 && isDuplicateName(newName)

  const handleCreate = async () => {
    const trimmed = newName.trim()
    if (!trimmed || submitting || newNameIsDuplicate) return
    setSubmitting(true)
    try {
      const { txId } = await trpc.labels.create.mutate(
        { teamId, name: trimmed, color: newColor },
        SKIP_ERROR_TOAST
      )
      await labelCollection.utils.awaitTxId(txId)
      resetForm()
      setCreating(false)
    } catch (err) {
      if (trpcErrorCode(err) === `CONFLICT`) setCreateConflict(trpcErrorMessage(err, DUPLICATE_LABEL))
      else failedAction(`create`, `the label`, err)
    } finally {
      setSubmitting(false)
    }
  }

  return (
    <div>
      <GlassSectionHeader
        label="Labels"
        trailing={
          !creating && (
            <Pill mode="action" onClick={() => setCreating(true)}>
              <Plus />
              New label
            </Pill>
          )
        }
      />
      {/* EXP-1076: ONE settings ladder — the rows and the inline create form
          are flat rows of the SAME divided list under the band, never a card
          stack with a card below it. */}
      <div className={SETTINGS_LIST_CLASS}>
        {labelList.map((label) => (
          <LabelRow
            key={label.id}
            label={label}
            teamId={teamId}
            isDuplicateName={(name) => isDuplicateName(name, label.id)}
          />
        ))}
        {labelList.length === 0 && !creating && (
          <Empty id="labels" />
        )}
        {creating && (
          <ListRow className="flex-col items-stretch gap-3 px-3 py-2">
            <Input
              value={newName}
              onChange={(e) => {
                setNewName(e.target.value)
                setCreateConflict(null)
              }}
              aria-invalid={newNameIsDuplicate || createConflict ? true : undefined}
              placeholder="Label name"
              autoFocus
              className="h-8 text-sm"
              onKeyDown={(e) => {
                if (e.key === `Enter`) {
                  e.preventDefault()
                  handleCreate()
                }
                if (e.key === `Escape`) {
                  setCreating(false)
                  resetForm()
                }
              }}
            />
            <FieldError>{newNameIsDuplicate ? DUPLICATE_LABEL : createConflict}</FieldError>
            <div>
              <span className="text-xs text-muted-foreground mb-1.5 block">
                Color
              </span>
              <ColorSwatchGrid value={newColor} onChange={setNewColor} />
            </div>
            <div className="flex items-center gap-2">
              <Button
                size="xs"
                variant="default"
                disabled={!newName.trim() || newNameIsDuplicate}
                pending={submitting}
                pendingLabel={FEEDBACK_WORDS.pending.save}
                onClick={handleCreate}
              >
                Create label
              </Button>
              <Pill
                mode="action"
                disabled={submitting}
                onClick={() => {
                  setCreating(false)
                  resetForm()
                }}
              >
                Cancel
              </Pill>
            </div>
          </ListRow>
        )}
      </div>
    </div>
  )
}

import { useState } from "react"
import { useLiveQuery, eq } from "@tanstack/react-db"
import { Plus, Trash2, X, Check } from "lucide-react"
import { labelCollection } from "@/lib/collections"
import { trpc } from "@/lib/trpc-client"
import type { Label as LabelType } from "@/db/schema"
import {
  Button,
  Pill,
  ListRow,
  SETTINGS_LIST_CLASS,
  GlassSectionHeader,
  Input,
  ColorPicker,
  LABEL_COLORS,
  ColorSwatchGrid,
} from "@exp/ui"

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
  const [error, setError] = useState<string | null>(null)

  const persistName = async () => {
    const trimmed = name.trim()
    if (!trimmed || trimmed === label.name) {
      setName(label.name)
      setError(null)
      return
    }
    if (isDuplicateName(trimmed)) {
      setError(`A label with this name already exists.`)
      return
    }
    setBusy(true)
    try {
      const { txId } = await trpc.labels.update.mutate({
        teamId,
        labelId: label.id,
        name: trimmed,
      })
      await labelCollection.utils.awaitTxId(txId)
      setError(null)
    } catch (err) {
      setError(err instanceof Error ? err.message : `Failed to rename label.`)
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
          // EXP-719: the row card IS the field's box. The stock Input brings
          // its own glass fill + radius (EXP-616), which nested a second box
          // inside every row; strip the fill too, not just the stroke, so
          // the name reads as row text that happens to be editable.
          className="h-8 flex-1 rounded-none border-none bg-transparent px-1 shadow-none focus-visible:ring-0 dark:bg-transparent"
        />
        {confirmingDelete ? (
          <div className="flex items-center gap-1">
            <span className="text-xs text-muted-foreground">Delete?</span>
            <Button
              variant="ghost"
              size="icon-sm"
              className="text-destructive"
              onClick={handleDelete}
              disabled={busy}
              aria-label="Confirm delete"
            >
              <Check />
            </Button>
            <Button
              variant="ghost"
              size="icon-sm"
              onClick={() => setConfirmingDelete(false)}
              disabled={busy}
              aria-label="Cancel delete"
            >
              <X />
            </Button>
          </div>
        ) : (
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
        )}
      </div>
      {error && <p className="text-xs text-destructive mt-1 px-1">{error}</p>}
    </ListRow>
  )
}

export function TeamLabelsSection({ teamId }: { teamId: string }) {
  const [creating, setCreating] = useState(false)
  const [newName, setNewName] = useState(``)
  const [newColor, setNewColor] = useState(LABEL_COLORS[6])
  const [submitting, setSubmitting] = useState(false)
  const [createError, setCreateError] = useState<string | null>(null)

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
    setCreateError(null)
  }

  const newNameIsDuplicate =
    newName.trim().length > 0 && isDuplicateName(newName)

  const handleCreate = async () => {
    const trimmed = newName.trim()
    if (!trimmed || submitting || newNameIsDuplicate) return
    setSubmitting(true)
    try {
      const { txId } = await trpc.labels.create.mutate({
        teamId,
        name: trimmed,
        color: newColor,
      })
      await labelCollection.utils.awaitTxId(txId)
      resetForm()
      setCreating(false)
    } catch (err) {
      setCreateError(
        err instanceof Error ? err.message : `Failed to create label.`
      )
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
          <p className="text-sm text-muted-foreground px-3 py-2">
            No labels yet.
          </p>
        )}
        {creating && (
          <ListRow className="flex-col items-stretch gap-3 px-3 py-2">
            <Input
              value={newName}
              onChange={(e) => {
                setNewName(e.target.value)
                setCreateError(null)
              }}
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
            {(newNameIsDuplicate || createError) && (
              <p className="text-xs text-destructive">
                {newNameIsDuplicate
                  ? `A label with this name already exists.`
                  : createError}
              </p>
            )}
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
                disabled={!newName.trim() || submitting || newNameIsDuplicate}
                onClick={handleCreate}
              >
                {submitting ? `Creating...` : `Create label`}
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

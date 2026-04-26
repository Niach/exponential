import { useState } from "react"
import { useLiveQuery, eq } from "@tanstack/react-db"
import { Plus, Trash2, X, Check } from "lucide-react"
import { labelCollection } from "@/lib/collections"
import { trpc } from "@/lib/trpc-client"
import type { Label as LabelType } from "@/db/schema"
import { Button } from "@/components/ui/button"
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card"
import { Input } from "@/components/ui/input"
import {
  Popover,
  PopoverContent,
  PopoverTrigger,
} from "@/components/ui/popover"

const LABEL_COLORS = [
  `#ef4444`,
  `#f97316`,
  `#eab308`,
  `#22c55e`,
  `#06b6d4`,
  `#3b82f6`,
  `#6366f1`,
  `#a855f7`,
  `#ec4899`,
  `#78716c`,
]

function ColorSwatchGrid({
  value,
  onChange,
}: {
  value: string
  onChange: (color: string) => void
}) {
  return (
    <div className="flex flex-wrap gap-1.5">
      {LABEL_COLORS.map((color) => (
        <button
          key={color}
          type="button"
          aria-label={`Select label color ${color}`}
          className={`h-5 w-5 rounded-full transition-all ${
            value === color
              ? `ring-2 ring-offset-2 ring-offset-background ring-foreground`
              : `hover:scale-110`
          }`}
          style={{ backgroundColor: color }}
          onClick={() => onChange(color)}
        />
      ))}
    </div>
  )
}

function LabelRow({
  label,
  workspaceId,
}: {
  label: LabelType
  workspaceId: string
}) {
  const [name, setName] = useState(label.name)
  const [confirmingDelete, setConfirmingDelete] = useState(false)
  const [busy, setBusy] = useState(false)

  const persistName = async () => {
    const trimmed = name.trim()
    if (!trimmed || trimmed === label.name) {
      setName(label.name)
      return
    }
    setBusy(true)
    try {
      const { txId } = await trpc.labels.update.mutate({
        workspaceId,
        labelId: label.id,
        name: trimmed,
      })
      await labelCollection.utils.awaitTxId(txId)
    } finally {
      setBusy(false)
    }
  }

  const persistColor = async (color: string) => {
    if (color === label.color) return
    setBusy(true)
    try {
      const { txId } = await trpc.labels.update.mutate({
        workspaceId,
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
        workspaceId,
        labelId: label.id,
      })
      await labelCollection.utils.awaitTxId(txId)
    } finally {
      setBusy(false)
      setConfirmingDelete(false)
    }
  }

  return (
    <div className="flex items-center gap-3 rounded-md border px-3 py-2">
      <Popover>
        <PopoverTrigger asChild>
          <button
            type="button"
            aria-label="Change label color"
            className="h-4 w-4 rounded-full ring-1 ring-border shrink-0"
            style={{ backgroundColor: label.color }}
            disabled={busy}
          />
        </PopoverTrigger>
        <PopoverContent className="w-auto p-2" align="start">
          <ColorSwatchGrid value={label.color} onChange={persistColor} />
        </PopoverContent>
      </Popover>
      <Input
        value={name}
        onChange={(e) => setName(e.target.value)}
        onBlur={persistName}
        onKeyDown={(e) => {
          if (e.key === `Enter`) {
            e.preventDefault()
            ;(e.target as HTMLInputElement).blur()
          }
          if (e.key === `Escape`) {
            setName(label.name)
            ;(e.target as HTMLInputElement).blur()
          }
        }}
        disabled={busy}
        className="h-8 flex-1 border-none shadow-none focus-visible:ring-0 px-1"
      />
      {confirmingDelete ? (
        <div className="flex items-center gap-1">
          <span className="text-xs text-muted-foreground">Delete?</span>
          <Button
            variant="ghost"
            size="icon"
            className="h-7 w-7 text-destructive"
            onClick={handleDelete}
            disabled={busy}
            aria-label="Confirm delete"
          >
            <Check className="h-4 w-4" />
          </Button>
          <Button
            variant="ghost"
            size="icon"
            className="h-7 w-7"
            onClick={() => setConfirmingDelete(false)}
            disabled={busy}
            aria-label="Cancel delete"
          >
            <X className="h-4 w-4" />
          </Button>
        </div>
      ) : (
        <Button
          variant="ghost"
          size="icon"
          className="h-7 w-7 text-muted-foreground hover:text-destructive"
          onClick={() => setConfirmingDelete(true)}
          disabled={busy}
          aria-label={`Delete label ${label.name}`}
        >
          <Trash2 className="h-4 w-4" />
        </Button>
      )}
    </div>
  )
}

export function WorkspaceLabelsSection({
  workspaceId,
}: {
  workspaceId: string
}) {
  const [creating, setCreating] = useState(false)
  const [newName, setNewName] = useState(``)
  const [newColor, setNewColor] = useState(LABEL_COLORS[6])
  const [submitting, setSubmitting] = useState(false)

  const { data: labels } = useLiveQuery(
    (q) =>
      q
        .from({ labels: labelCollection })
        .where(({ labels }) => eq(labels.workspaceId, workspaceId))
        .orderBy(({ labels }) => labels.sortOrder),
    [workspaceId]
  )

  const labelList = labels ?? []

  const resetForm = () => {
    setNewName(``)
    setNewColor(LABEL_COLORS[Math.floor(Math.random() * LABEL_COLORS.length)])
  }

  const handleCreate = async () => {
    const trimmed = newName.trim()
    if (!trimmed || submitting) return
    setSubmitting(true)
    try {
      const { txId } = await trpc.labels.create.mutate({
        workspaceId,
        name: trimmed,
        color: newColor,
      })
      await labelCollection.utils.awaitTxId(txId)
      resetForm()
      setCreating(false)
    } finally {
      setSubmitting(false)
    }
  }

  return (
    <Card>
      <CardHeader>
        <CardTitle className="text-base">Labels</CardTitle>
        <CardDescription>
          {labelList.length} label{labelList.length !== 1 ? `s` : ``} in this
          workspace. Deleting a label removes it from all issues.
        </CardDescription>
      </CardHeader>
      <CardContent>
        <div className="space-y-2">
          {labelList.map((label) => (
            <LabelRow
              key={label.id}
              label={label}
              workspaceId={workspaceId}
            />
          ))}
          {labelList.length === 0 && !creating && (
            <p className="text-sm text-muted-foreground py-2">
              No labels yet.
            </p>
          )}
        </div>

        {creating ? (
          <div className="mt-3 space-y-3 rounded-md border p-3">
            <Input
              value={newName}
              onChange={(e) => setNewName(e.target.value)}
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
            <div>
              <span className="text-xs text-muted-foreground mb-1.5 block">
                Color
              </span>
              <ColorSwatchGrid value={newColor} onChange={setNewColor} />
            </div>
            <div className="flex items-center gap-2">
              <Button
                size="xs"
                className="bg-indigo-600 hover:bg-indigo-700 text-white"
                disabled={!newName.trim() || submitting}
                onClick={handleCreate}
              >
                {submitting ? `Creating...` : `Create label`}
              </Button>
              <Button
                size="xs"
                variant="ghost"
                disabled={submitting}
                onClick={() => {
                  setCreating(false)
                  resetForm()
                }}
              >
                Cancel
              </Button>
            </div>
          </div>
        ) : (
          <Button
            variant="outline"
            size="sm"
            className="mt-3"
            onClick={() => setCreating(true)}
          >
            <Plus className="h-4 w-4 mr-1" />
            New label
          </Button>
        )}
      </CardContent>
    </Card>
  )
}

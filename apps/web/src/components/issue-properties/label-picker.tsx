import { useState } from "react"
import { useLiveQuery, eq } from "@tanstack/react-db"
import { labelCollection } from "@/lib/collections"
import {
  Combobox,
  Button,
  Pill,
  Input,
  LABEL_COLORS,
  ColorSwatchGrid,
  type PickerOption,
} from "@exp/ui"
import { Tag, Plus, ArrowLeft } from "lucide-react"
import { trpc } from "@/lib/trpc-client"
import type { Label } from "@/db/schema"

interface LabelPickerProps {
  disabled?: boolean
  teamId: string
  selectedLabelIds: string[]
  onToggle: (labelId: string) => void
  // Replaces the default chip button (the mobile create form renders the
  // picker as a full-width property row). Receives the selected labels so
  // the row can show its own value.
  renderTrigger?: (selectedLabels: Label[]) => React.ReactNode
}

// EXP-941: the shared `Combobox` — the rows are the primitive's multi-select
// ones (the leading circle pair instead of this file's old Checkbox), the
// "Create label" row is its `footer`, and the create form is its `panel`,
// which REPLACES the search field and the list while `view === "create"`.
// Every bit of create state stays here.
export function LabelPicker({
  disabled,
  teamId,
  selectedLabelIds,
  onToggle,
  renderTrigger,
}: LabelPickerProps) {
  const [open, setOpen] = useState(false)
  const [view, setView] = useState<`list` | `create`>(`list`)
  const [newName, setNewName] = useState(``)
  const [newColor, setNewColor] = useState(LABEL_COLORS[6])
  const [creating, setCreating] = useState(false)
  const [createError, setCreateError] = useState<string | null>(null)

  const { data: labels } = useLiveQuery(
    (q) =>
      teamId
        ? q
            .from({ labels: labelCollection })
            .where(({ labels }) => eq(labels.teamId, teamId))
            .orderBy(({ labels }) => labels.sortOrder)
        : undefined,
    [teamId]
  )

  const selectedLabels = (labels ?? []).filter((l: Label) =>
    selectedLabelIds.includes(l.id)
  )
  const options: PickerOption[] = (labels ?? []).map((label: Label) => ({
    value: label.id,
    label: label.name,
    dot: label.color,
  }))

  const newNameIsDuplicate =
    newName.trim().length > 0 &&
    (labels ?? []).some(
      (l: Label) => l.name.trim().toLowerCase() === newName.trim().toLowerCase()
    )

  const handleCreate = async () => {
    if (!newName.trim() || creating || newNameIsDuplicate) return
    setCreating(true)
    try {
      const { txId, label } = await trpc.labels.create.mutate({
        teamId,
        name: newName.trim(),
        color: newColor,
      })
      await labelCollection.utils.awaitTxId(txId)
      onToggle(label.id)
      setNewName(``)
      setNewColor(LABEL_COLORS[Math.floor(Math.random() * LABEL_COLORS.length)])
      setCreateError(null)
      setView(`list`)
    } catch (err) {
      setCreateError(
        err instanceof Error ? err.message : `Failed to create label.`
      )
    } finally {
      setCreating(false)
    }
  }

  return (
    <Combobox
      multiple
      options={options}
      value={selectedLabelIds}
      // The hosts own one label at a time, so the primitive's whole-selection
      // change is reported back as the toggled id.
      onChange={(next) => {
        const added = next.find((id) => !selectedLabelIds.includes(id))
        const removed = selectedLabelIds.find((id) => !next.includes(id))
        const changed = added ?? removed
        if (changed) onToggle(changed)
      }}
      disabled={disabled}
      open={open}
      onOpenChange={(o) => {
        setOpen(o)
        if (!o) {
          setView(`list`)
          setNewName(``)
          setCreateError(null)
        }
      }}
      width="sm"
      mobileTitle="Labels"
      placeholder="Filter labels..."
      emptyText="No labels found."
      footer={
        <Button
          variant="ghost"
          size="sm"
          className="w-full justify-start rounded-none font-normal"
          onClick={() => setView(`create`)}
        >
          <Plus className="size-3.5" />
          Create label
        </Button>
      }
      panel={
        view === `create` ? (
          <div className="p-2 space-y-3">
            <div className="flex items-center gap-2">
              <Button
                variant="ghost"
                size="icon-xs"
                onClick={() => setView(`list`)}
              >
                <ArrowLeft className="size-3.5" />
              </Button>
              <span className="text-sm font-medium">Create label</span>
            </div>
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
                  void handleCreate()
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
            <Button
              size="xs"
              variant="default"
              className="w-full"
              disabled={!newName.trim() || creating || newNameIsDuplicate}
              onClick={() => void handleCreate()}
            >
              {creating ? `Creating...` : `Create label`}
            </Button>
          </div>
        ) : undefined
      }
      renderTrigger={() =>
        renderTrigger ? (
          renderTrigger(selectedLabels)
        ) : (
          <Pill mode="action" disabled={disabled}>
            <Tag className="size-3" />
            {selectedLabels.length > 0 ? (
              <>
                <span className="flex items-center -space-x-0.5">
                  {selectedLabels.slice(0, 3).map((l: Label) => (
                    <span
                      key={l.id}
                      className="h-2 w-2 shrink-0 rounded-full ring-1 ring-background"
                      style={{ backgroundColor: l.color }}
                    />
                  ))}
                </span>
                <span className="max-w-[7.5rem] truncate">
                  {selectedLabels.map((l: Label) => l.name).join(`, `)}
                </span>
              </>
            ) : (
              `Label`
            )}
          </Pill>
        )
      }
    />
  )
}

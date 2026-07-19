import { useState } from "react"
import { useLiveQuery, eq } from "@tanstack/react-db"
import { labelCollection } from "@/lib/collections"
import {
  MobilePopover,
  MobilePopoverContent,
  MobilePopoverTrigger,
} from "@/components/mobile-popover"
import {
  Command,
  CommandEmpty,
  CommandGroup,
  CommandInput,
  CommandItem,
  CommandList,
  CommandSeparator,
} from "@/components/ui/command"
import { Checkbox } from "@/components/ui/checkbox"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Tag, Plus, ArrowLeft } from "lucide-react"
import { trpc } from "@/lib/trpc-client"
import { LABEL_COLORS } from "@/lib/label-colors"
import { ColorSwatchGrid } from "@/components/ui/color-swatch-grid"
import type { Label } from "@/db/schema"

interface LabelPickerProps {
  disabled?: boolean
  teamId: string
  selectedLabelIds: string[]
  onToggle: (labelId: string) => void
}

export function LabelPicker({
  disabled,
  teamId,
  selectedLabelIds,
  onToggle,
}: LabelPickerProps) {
  const [open, setOpen] = useState(false)
  const [view, setView] = useState<`list` | `create`>(`list`)
  const [newName, setNewName] = useState(``)
  const [newColor, setNewColor] = useState(LABEL_COLORS[6])
  const [creating, setCreating] = useState(false)

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

  const handleCreate = async () => {
    if (!newName.trim() || creating) return
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
      setView(`list`)
    } finally {
      setCreating(false)
    }
  }

  return (
    <MobilePopover
      open={disabled ? false : open}
      onOpenChange={(o) => {
        if (disabled) {
          return
        }

        setOpen(o)
        if (!o) {
          setView(`list`)
          setNewName(``)
        }
      }}
    >
      <MobilePopoverTrigger asChild>
        <Button
          variant="ghost"
          size="xs"
          className="text-muted-foreground"
          disabled={disabled}
        >
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
        </Button>
      </MobilePopoverTrigger>
      <MobilePopoverContent
        className="w-[14rem] p-0"
        align="start"
        mobileTitle="Labels"
      >
        {view === `list` ? (
          <Command>
            <CommandInput placeholder="Filter labels..." />
            <CommandList>
              <CommandEmpty>No labels found.</CommandEmpty>
              <CommandGroup>
                {(labels ?? []).map((label: Label) => {
                  const isSelected = selectedLabelIds.includes(label.id)
                  return (
                    <CommandItem
                      key={label.id}
                      value={label.id}
                      keywords={[label.name]}
                      onSelect={() => onToggle(label.id)}
                      className="flex items-center gap-2"
                    >
                      <Checkbox
                        checked={isSelected}
                        className="pointer-events-none"
                      />
                      <div
                        className="h-2.5 w-2.5 rounded-full shrink-0"
                        style={{ backgroundColor: label.color }}
                      />
                      <span className="truncate text-sm">{label.name}</span>
                    </CommandItem>
                  )
                })}
              </CommandGroup>
              <CommandSeparator />
              <CommandGroup>
                <CommandItem
                  onSelect={() => setView(`create`)}
                  className="flex items-center gap-2"
                >
                  <Plus className="size-3.5" />
                  <span className="text-sm">Create label</span>
                </CommandItem>
              </CommandGroup>
            </CommandList>
          </Command>
        ) : (
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
              onChange={(e) => setNewName(e.target.value)}
              placeholder="Label name"
              autoFocus
              className="h-8 text-sm"
              onKeyDown={(e) => {
                if (e.key === `Enter`) {
                  e.preventDefault()
                  handleCreate()
                }
              }}
            />
            <div>
              <span className="text-xs text-muted-foreground mb-1.5 block">
                Color
              </span>
              <ColorSwatchGrid value={newColor} onChange={setNewColor} />
            </div>
            <Button
              size="xs"
              className="w-full bg-indigo-600 hover:bg-indigo-700 text-white"
              disabled={!newName.trim() || creating}
              onClick={handleCreate}
            >
              {creating ? `Creating...` : `Create label`}
            </Button>
          </div>
        )}
      </MobilePopoverContent>
    </MobilePopover>
  )
}

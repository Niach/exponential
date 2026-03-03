import { useState } from "react"
import { useLiveQuery, eq } from "@tanstack/react-db"
import { labelCollection } from "@/lib/collections"
import { Popover, PopoverContent, PopoverTrigger } from "@/components/ui/popover"
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
import type { Label } from "@/db/schema"

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

interface LabelPickerProps {
  workspaceId: string
  selectedLabelIds: string[]
  onToggle: (labelId: string) => void
}

export function LabelPicker({
  workspaceId,
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
      workspaceId
        ? q
            .from({ labels: labelCollection })
            .where(({ labels }) => eq(labels.workspaceId, workspaceId))
            .orderBy(({ labels }) => labels.sortOrder)
        : undefined,
    [workspaceId]
  )

  const selectedLabels = (labels ?? []).filter((l: Label) =>
    selectedLabelIds.includes(l.id)
  )

  const handleCreate = async () => {
    if (!newName.trim() || creating) return
    setCreating(true)
    try {
      const { label } = await trpc.labels.create.mutate({
        workspaceId,
        name: newName.trim(),
        color: newColor,
      })
      onToggle(label.id)
      setNewName(``)
      setNewColor(LABEL_COLORS[Math.floor(Math.random() * LABEL_COLORS.length)])
      setView(`list`)
    } finally {
      setCreating(false)
    }
  }

  return (
    <Popover
      open={open}
      onOpenChange={(o) => {
        setOpen(o)
        if (!o) {
          setView(`list`)
          setNewName(``)
        }
      }}
    >
      <PopoverTrigger asChild>
        <Button variant="ghost" size="xs" className="text-muted-foreground">
          <Tag className="size-3" />
          {selectedLabels.length > 0 ? (
            <span className="max-w-[120px] truncate">
              {selectedLabels.map((l: Label) => l.name).join(`, `)}
            </span>
          ) : (
            `Label`
          )}
        </Button>
      </PopoverTrigger>
      <PopoverContent className="w-[220px] p-0" align="start">
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
                      value={label.name}
                      onSelect={() => onToggle(label.id)}
                      className="flex items-center gap-2"
                    >
                      <Checkbox checked={isSelected} className="pointer-events-none" />
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
              <span className="text-xs text-muted-foreground mb-1.5 block">Color</span>
              <div className="flex flex-wrap gap-1.5">
                {LABEL_COLORS.map((color) => (
                  <button
                    key={color}
                    type="button"
                    className={`h-5 w-5 rounded-full transition-all ${
                      newColor === color
                        ? `ring-2 ring-offset-2 ring-offset-background ring-foreground`
                        : `hover:scale-110`
                    }`}
                    style={{ backgroundColor: color }}
                    onClick={() => setNewColor(color)}
                  />
                ))}
              </div>
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
      </PopoverContent>
    </Popover>
  )
}

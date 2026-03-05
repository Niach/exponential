import { useState } from "react"
import {
  Popover,
  PopoverContent,
  PopoverTrigger,
} from "@/components/ui/popover"
import {
  Command,
  CommandEmpty,
  CommandGroup,
  CommandInput,
  CommandItem,
  CommandList,
} from "@/components/ui/command"
import { Checkbox } from "@/components/ui/checkbox"
import { Button } from "@/components/ui/button"
import { ListFilter, ChevronRight, ArrowLeft } from "lucide-react"
import { statuses, StatusIcon } from "@/components/status-dropdown"
import { priorities, PriorityIcon } from "@/components/priority-dropdown"
import type { IssueFilters } from "@/lib/filters"
import { activeFilterCount } from "@/lib/filters"
import type { Label } from "@/db/schema"

type View = `categories` | `status` | `priority` | `labels`

interface IssueFilterPopoverProps {
  filters: IssueFilters
  onFiltersChange: (filters: IssueFilters) => void
  labels: Label[]
}

export function IssueFilterPopover({
  filters,
  onFiltersChange,
  labels,
}: IssueFilterPopoverProps) {
  const [open, setOpen] = useState(false)
  const [view, setView] = useState<View>(`categories`)

  const count = activeFilterCount(filters)

  const toggleStatus = (value: string) => {
    const next = filters.statuses.includes(value)
      ? filters.statuses.filter((s) => s !== value)
      : [...filters.statuses, value]
    onFiltersChange({ ...filters, statuses: next })
  }

  const togglePriority = (value: string) => {
    const next = filters.priorities.includes(value)
      ? filters.priorities.filter((p) => p !== value)
      : [...filters.priorities, value]
    onFiltersChange({ ...filters, priorities: next })
  }

  const toggleLabel = (id: string) => {
    const next = filters.labelIds.includes(id)
      ? filters.labelIds.filter((l) => l !== id)
      : [...filters.labelIds, id]
    onFiltersChange({ ...filters, labelIds: next })
  }

  return (
    <Popover
      open={open}
      onOpenChange={(o) => {
        setOpen(o)
        if (!o) setView(`categories`)
      }}
    >
      <PopoverTrigger asChild>
        <Button variant="ghost" size="xs" className="text-muted-foreground">
          <ListFilter className="size-3" />
          Filter
          {count > 0 && (
            <span className="ml-1 rounded-full bg-indigo-500/20 text-indigo-400 px-1.5 text-[10px] font-medium">
              {count}
            </span>
          )}
        </Button>
      </PopoverTrigger>
      <PopoverContent className="w-[220px] p-0" align="start">
        {view === `categories` && (
          <CategoriesView filters={filters} onNavigate={setView} />
        )}
        {view === `status` && (
          <StatusView
            selected={filters.statuses}
            onToggle={toggleStatus}
            onBack={() => setView(`categories`)}
          />
        )}
        {view === `priority` && (
          <PriorityView
            selected={filters.priorities}
            onToggle={togglePriority}
            onBack={() => setView(`categories`)}
          />
        )}
        {view === `labels` && (
          <LabelsView
            labels={labels}
            selected={filters.labelIds}
            onToggle={toggleLabel}
            onBack={() => setView(`categories`)}
          />
        )}
      </PopoverContent>
    </Popover>
  )
}

function CategoriesView({
  filters,
  onNavigate,
}: {
  filters: IssueFilters
  onNavigate: (view: View) => void
}) {
  const categories = [
    { key: `status` as View, label: `Status`, count: filters.statuses.length },
    {
      key: `priority` as View,
      label: `Priority`,
      count: filters.priorities.length,
    },
    { key: `labels` as View, label: `Labels`, count: filters.labelIds.length },
  ]

  return (
    <Command>
      <CommandList>
        <CommandGroup>
          {categories.map((cat) => (
            <CommandItem
              key={cat.key}
              onSelect={() => onNavigate(cat.key)}
              className="flex items-center justify-between"
            >
              <span>{cat.label}</span>
              <span className="flex items-center gap-1">
                {cat.count > 0 && (
                  <span className="rounded-full bg-indigo-500/20 text-indigo-400 px-1.5 text-[10px] font-medium">
                    {cat.count}
                  </span>
                )}
                <ChevronRight className="size-3.5 text-muted-foreground" />
              </span>
            </CommandItem>
          ))}
        </CommandGroup>
      </CommandList>
    </Command>
  )
}

function StatusView({
  selected,
  onToggle,
  onBack,
}: {
  selected: string[]
  onToggle: (value: string) => void
  onBack: () => void
}) {
  return (
    <Command>
      <CommandList>
        <CommandGroup>
          <CommandItem onSelect={onBack} className="flex items-center gap-2">
            <ArrowLeft className="size-3.5" />
            <span className="font-medium">Status</span>
          </CommandItem>
          {statuses.map((s) => (
            <CommandItem
              key={s.value}
              onSelect={() => onToggle(s.value)}
              className="flex items-center gap-2"
            >
              <Checkbox
                checked={selected.includes(s.value)}
                className="pointer-events-none"
              />
              <StatusIcon status={s.value} className="!h-3.5 !w-3.5" />
              <span className="text-sm">{s.label}</span>
            </CommandItem>
          ))}
        </CommandGroup>
      </CommandList>
    </Command>
  )
}

function PriorityView({
  selected,
  onToggle,
  onBack,
}: {
  selected: string[]
  onToggle: (value: string) => void
  onBack: () => void
}) {
  return (
    <Command>
      <CommandList>
        <CommandGroup>
          <CommandItem onSelect={onBack} className="flex items-center gap-2">
            <ArrowLeft className="size-3.5" />
            <span className="font-medium">Priority</span>
          </CommandItem>
          {priorities.map((p) => (
            <CommandItem
              key={p.value}
              onSelect={() => onToggle(p.value)}
              className="flex items-center gap-2"
            >
              <Checkbox
                checked={selected.includes(p.value)}
                className="pointer-events-none"
              />
              <PriorityIcon priority={p.value} className="!h-3.5 !w-3.5" />
              <span className="text-sm">{p.label}</span>
            </CommandItem>
          ))}
        </CommandGroup>
      </CommandList>
    </Command>
  )
}

function LabelsView({
  labels,
  selected,
  onToggle,
  onBack,
}: {
  labels: Label[]
  selected: string[]
  onToggle: (id: string) => void
  onBack: () => void
}) {
  return (
    <Command>
      <div className="flex items-center gap-2 px-2 py-1.5">
        <Button
          variant="ghost"
          size="icon-xs"
          onClick={onBack}
          className="shrink-0"
        >
          <ArrowLeft className="size-3.5" />
        </Button>
        <CommandInput placeholder="Filter labels..." className="h-8" />
      </div>
      <CommandList>
        <CommandEmpty>No labels found.</CommandEmpty>
        <CommandGroup>
          {labels.map((label) => (
            <CommandItem
              key={label.id}
              value={label.name}
              onSelect={() => onToggle(label.id)}
              className="flex items-center gap-2"
            >
              <Checkbox
                checked={selected.includes(label.id)}
                className="pointer-events-none"
              />
              <div
                className="h-2.5 w-2.5 rounded-full shrink-0"
                style={{ backgroundColor: label.color }}
              />
              <span className="truncate text-sm">{label.name}</span>
            </CommandItem>
          ))}
        </CommandGroup>
      </CommandList>
    </Command>
  )
}

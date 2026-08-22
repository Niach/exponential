import { useState } from "react"
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
} from "@/components/ui/command"
import { Checkbox } from "@/components/ui/checkbox"
import { Button } from "@/components/ui/button"
import { ListFilter, ChevronRight, ArrowLeft } from "lucide-react"
import { toStatusMenuOptions } from "@/components/issue-properties/status-dropdown"
import { priorities } from "@/components/issue-properties/priority-dropdown"
import { useTeamStatusesContext } from "@/hooks/use-team-statuses"
import { statusFilterToken } from "@/lib/team-statuses"
import { IssueOptionFilterView } from "@/components/issue-option-filter-view"
import type { IssueFilters } from "@/lib/filters"
import { activeFilterCount } from "@/lib/filters"
import type { Label } from "@/db/schema"
import type { IssuePriority } from "@/lib/domain"

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
  const { options: statusOptions } = useTeamStatusesContext()

  const count = activeFilterCount(filters)

  // EXP-314 dual-token selection: a legacy enum token from an older URL shows
  // the matching builtin row ticked, and ANY toggle normalizes the whole set
  // to row ids (dropping enum tokens whose row is on screen).
  const selectedStatusIds = statusOptions
    .filter((option) =>
      filters.statusTokens.some(
        (token) => token === option.id || token === option.builtinKey
      )
    )
    .map((option) => option.id)

  const toggleStatus = (value: string) => {
    const nextIds = selectedStatusIds.includes(value)
      ? selectedStatusIds.filter((s) => s !== value)
      : [...selectedStatusIds, value]
    // Tokens the on-screen rows can't represent (a status deleted while the
    // URL was open) are dropped by the normalization — that is the point.
    // Constructed fallback rows (shape unsynced) emit their anchor ENUM: a
    // synthetic `builtin:<key>` id would fail the URL allowlist.
    const next = nextIds.map((id) => {
      const option = statusOptions.find((o) => o.id === id)!
      return statusFilterToken(option)
    })
    onFiltersChange({ ...filters, statusTokens: next })
  }

  const togglePriority = (value: IssuePriority) => {
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
    <MobilePopover
      open={open}
      onOpenChange={(o) => {
        setOpen(o)
        if (!o) setView(`categories`)
      }}
    >
      <MobilePopoverTrigger asChild>
        <Button variant="ghost" size="xs" className="text-muted-foreground">
          <ListFilter className="size-3" />
          Filter
          {count > 0 && (
            <span className="ml-1 rounded-full bg-glass-active text-foreground px-1.5 text-[0.625rem] font-medium">
              {count}
            </span>
          )}
        </Button>
      </MobilePopoverTrigger>
      <MobilePopoverContent
        className="w-[14rem] p-0"
        align="start"
        mobileTitle="Filters"
      >
        {view === `categories` && (
          <CategoriesView filters={filters} onNavigate={setView} />
        )}
        {view === `status` && (
          <IssueOptionFilterView
            title="Status"
            options={toStatusMenuOptions(statusOptions)}
            selected={selectedStatusIds}
            onToggle={toggleStatus}
            onBack={() => setView(`categories`)}
          />
        )}
        {view === `priority` && (
          <IssueOptionFilterView
            title="Priority"
            options={priorities}
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
      </MobilePopoverContent>
    </MobilePopover>
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
    {
      key: `status` as View,
      label: `Status`,
      count: filters.statusTokens.length,
    },
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
                  <span className="rounded-full bg-glass-active text-foreground px-1.5 text-[0.625rem] font-medium">
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
              value={label.id}
              keywords={[label.name]}
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

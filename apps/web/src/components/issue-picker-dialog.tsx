import { useState } from "react"
import { Search } from "lucide-react"
import {
  Command,
  CommandEmpty,
  CommandInput,
  CommandItem,
  CommandList,
  Dialog,
  DialogContent,
  DialogTitle,
} from "@exp/ui"
import { IssueStatusIcon } from "@/components/issue-properties/status-dropdown"
import {
  useIssueRefs,
  type ResolvedIssueRef,
} from "@/components/issue-ref-provider"
import { useIssueSearchResults } from "@/hooks/use-issue-search-results"

interface IssuePickerDialogProps {
  open: boolean
  onOpenChange: (open: boolean) => void
  onPick: (issue: ResolvedIssueRef) => void
  /** Issues to hide from results (e.g. the issue being marked). */
  excludeIssueIds?: string[]
  title?: string
  placeholder?: string
}

const NO_ROWS: ResolvedIssueRef[] = []

// A small centered issue picker: search the team's issues and pick one.
// EXP-892: the shared engine (`useIssueSearchResults` — local ranking over
// the synced rows plus the server's full-text pass) and the shared list
// contract: cmdk keeps the top row selected while typing, ↑/↓ step, Enter
// picks, hovering moves the selection. Used by the mark-as-duplicate flow
// and the relations card.
export function IssuePickerDialog({
  open,
  onOpenChange,
  onPick,
  excludeIssueIds,
  title = `Select issue`,
  placeholder = `Search issues…`,
}: IssuePickerDialogProps) {
  const issueRefs = useIssueRefs()
  const [query, setQuery] = useState(``)

  const { results } = useIssueSearchResults({
    teamId: issueRefs?.teamId,
    query,
    rows: open ? (issueRefs?.rows ?? NO_ROWS) : NO_ROWS,
    limit: 30,
    exclude: excludeIssueIds,
    resolveHit: (hit) => issueRefs?.fromHit(hit) ?? null,
    server: open,
  })

  if (!open) return null

  const handleOpenChange = (next: boolean) => {
    onOpenChange(next)
    if (!next) setQuery(``)
  }

  const handlePick = (issue: ResolvedIssueRef) => {
    handleOpenChange(false)
    onPick(issue)
  }

  return (
    <Dialog open={open} onOpenChange={handleOpenChange}>
      <DialogContent
        mobile="sheet-full"
        showCloseButton={false}
        className="flex flex-col gap-0 overflow-hidden p-0 sm:p-0 max-sm:px-0 sm:top-[15%] sm:max-h-[60vh] sm:max-w-lg sm:translate-y-0"
      >
        <DialogTitle className="sr-only">{title}</DialogTitle>
        <Command
          shouldFilter={false}
          className="min-h-0 bg-transparent **:data-[slot=command-input-wrapper]:border-border/50"
        >
          <CommandInput
            value={query}
            onValueChange={setQuery}
            placeholder={placeholder}
            autoFocus
            className="text-base md:text-sm"
          />
          <CommandList className="max-h-none flex-1 overflow-y-auto">
            <CommandEmpty className="p-0">
              <div className="flex flex-col items-center justify-center p-12 text-muted-foreground">
                <Search className="size-8 mb-3 opacity-50" />
                <p className="text-sm">
                  {query.trim()
                    ? `No issues match "${query}"`
                    : `No issues to pick from`}
                </p>
              </div>
            </CommandEmpty>
            {results.map((issue) => (
              <CommandItem
                key={issue.id}
                value={issue.id}
                onSelect={() => handlePick(issue)}
                className="gap-3 rounded-none px-4 py-3 cursor-pointer border-b border-border/30"
              >
                <IssueStatusIcon issue={issue} className="size-4 shrink-0" />
                <span className="shrink-0 font-mono text-xs text-muted-foreground">
                  {issue.identifier}
                </span>
                <span className="flex-1 truncate text-sm">{issue.title}</span>
              </CommandItem>
            ))}
          </CommandList>
        </Command>
      </DialogContent>
    </Dialog>
  )
}

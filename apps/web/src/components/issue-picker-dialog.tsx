import { useState } from "react"
import { Search } from "lucide-react"
import { Dialog, DialogContent, DialogTitle } from "@/components/ui/dialog"
import { Input } from "@/components/ui/input"
import { Button } from "@/components/ui/button"
import { StatusIcon } from "@/components/issue-properties/status-dropdown"
import {
  useIssueRefs,
  type ResolvedIssueRef,
} from "@/components/issue-ref-provider"

interface IssuePickerDialogProps {
  open: boolean
  onOpenChange: (open: boolean) => void
  onPick: (issue: ResolvedIssueRef) => void
  /** Issues to hide from results (e.g. the issue being marked). */
  excludeIssueIds?: string[]
  title?: string
  placeholder?: string
}

// A small centered issue picker: search the workspace's issues by identifier
// or title and pick one. Backed by the IssueRefProvider (already-synced issues
// shape — no server round-trips); shares the search-sheet visual language.
// Used by the mark-as-duplicate flow.
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

  if (!open) return null

  const results = issueRefs?.search(query, { excludeIssueIds, limit: 30 }) ?? []

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
        showCloseButton={false}
        className="top-[15%] translate-y-0 p-0 gap-0 flex max-h-[60vh] flex-col overflow-hidden sm:max-w-lg"
      >
        <DialogTitle className="sr-only">{title}</DialogTitle>
        <div className="flex items-center gap-2 px-3 py-3 border-b border-border/50">
          <Search className="size-4 text-muted-foreground shrink-0" />
          <Input
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            placeholder={placeholder}
            autoFocus
            className="border-none shadow-none focus-visible:ring-0 h-9 text-base md:text-sm"
          />
        </div>
        <div className="flex-1 overflow-y-auto">
          {results.length === 0 && (
            <div className="flex flex-col items-center justify-center p-12 text-muted-foreground">
              <p className="text-sm">
                {query.trim()
                  ? `No issues match "${query}"`
                  : `No issues to pick from`}
              </p>
            </div>
          )}
          {results.map((issue) => (
            <Button
              key={issue.id}
              type="button"
              variant="ghost"
              onClick={() => handlePick(issue)}
              className="flex h-auto w-full items-center justify-start gap-3 rounded-none px-4 py-3 text-left font-normal hover:bg-accent active:bg-accent/70 border-b border-border/30"
            >
              <StatusIcon status={issue.status} className="size-4 shrink-0" />
              <span className="shrink-0 font-mono text-xs text-muted-foreground">
                {issue.identifier}
              </span>
              <span className="flex-1 truncate text-sm">{issue.title}</span>
            </Button>
          ))}
        </div>
      </DialogContent>
    </Dialog>
  )
}

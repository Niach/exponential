import { useState } from "react"
import { Dialog, DialogContent, DialogTitle, PickerList, issuePickerItems } from "@exp/ui"
import { toStatusPickerStatus } from "@/components/issue-properties/status-dropdown"
import { useTeamStatusesContext } from "@/hooks/use-team-statuses"
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
//
// EXP-1021: the rows ARE the issue picker's rows (`issuePickerItems` through
// `PickerList` — the primitive's body without a surface): the status glyph in
// its colour, then the one-line `IDENT Title` label that is the ×4 contract.
// Only the SHELL stays a dialog rather than a popover, because both callers
// open it from a MENU ITEM: there is no trigger for a popover to hang off,
// and a phone gets the full sheet either way.
export function IssuePickerDialog({
  open,
  onOpenChange,
  onPick,
  excludeIssueIds,
  title = `Select issue`,
  placeholder = `Search issues…`,
}: IssuePickerDialogProps) {
  const issueRefs = useIssueRefs()
  const { resolve } = useTeamStatusesContext()
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

  const resultsById = new Map(results.map((issue) => [issue.id, issue]))

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
        <PickerList
          mode="single"
          search
          searchPlaceholder={placeholder}
          // The engine already ranked and capped the rows (EXP-892), so cmdk
          // renders them verbatim.
          query={query}
          onQueryChange={setQuery}
          shouldFilter={false}
          // Nothing is ever pre-picked here — the picker LINKS an issue, it
          // does not edit a value — so `value` stays null and no row is marked.
          value={null}
          onChange={(issueId) => {
            const issue = resultsById.get(issueId)
            if (issue) handlePick(issue)
          }}
          items={issuePickerItems(
            results.map((issue) => {
              // The glyph is the issue's own status row, resolved against the
              // team's synced statuses exactly as every list row resolves it.
              const status = toStatusPickerStatus(resolve(issue))
              return {
                id: issue.id,
                identifier: issue.identifier,
                title: issue.title,
                icon: status.icon,
                color: status.colorHex ?? undefined,
              }
            })
          )}
          // EXP-962: the in-list empty line (the same one `ListEmpty` draws
          // elsewhere) — no icon column, so one list's empty is not louder
          // than every other list's.
          emptyText={
            query.trim()
              ? `No issues match "${query}"`
              : `No issues to pick from`
          }
          className="min-h-0 bg-transparent **:data-[slot=command-input-wrapper]:border-border/50"
          listClassName="max-h-none flex-1 overflow-y-auto"
        />
      </DialogContent>
    </Dialog>
  )
}

import { Search } from "lucide-react"
import type { Issue } from "@/db/schema"
import { Checkbox } from "@/components/ui/checkbox"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { StatusIcon } from "@/components/issue-properties/status-dropdown"

// The Issues tab of the unified launch dialog (EXP-257) — a presentational
// extraction of the Start-coding dialog's issue picker column. All state
// (search, selection, the derived row list and its guards) stays in the
// dialog shell; this only renders it.

// Hard cap per run — parity with the server zod cap (issueIds max 30) and the
// desktop launcher's MAX_ISSUES_PER_RUN. Beyond it the server would reject with
// a zod BAD_REQUEST whose `[`-prefixed message is discarded into a misleading
// "could not be delivered" toast, so block the submit in the shell instead.
export const MAX_ISSUES_PER_RUN = 30
// Above this, batches get a soft token-cost note (matches the native sheets).
export const BATCH_COST_HINT_THRESHOLD = 6

export function IssuesPane({
  search,
  onSearchChange,
  rows,
  selected,
  onToggle,
  count,
  overCap,
  spansRepos,
  blocked,
}: {
  search: string
  onSearchChange: (value: string) => void
  /** Checked issues pinned first, search matches after (shell-derived). */
  rows: Issue[]
  selected: Set<string>
  onToggle: (issueId: string) => void
  count: number
  overCap: boolean
  spansRepos: boolean
  blocked: boolean
}) {
  return (
    <div className="flex min-h-0 flex-col gap-2">
      <Label>Issues</Label>
      <div className="relative">
        <Search className="absolute left-2.5 top-1/2 size-4 -translate-y-1/2 text-muted-foreground" />
        <Input
          value={search}
          onChange={(e) => onSearchChange(e.target.value)}
          placeholder="Search issues…"
          className="h-9 pl-8"
        />
      </div>
      <div className="max-h-44 overflow-y-auto rounded-md border border-border sm:max-h-none sm:min-h-32 sm:flex-1">
        {rows.length === 0 ? (
          <div className="px-3 py-6 text-center text-xs text-muted-foreground">
            {search.trim()
              ? `No issues match "${search}"`
              : `No codeable issues in repo-backed boards.`}
          </div>
        ) : (
          rows.map((issue) => {
            const checked = selected.has(issue.id)
            return (
              <div
                key={issue.id}
                role="button"
                tabIndex={0}
                onClick={() => onToggle(issue.id)}
                onKeyDown={(e) => {
                  if (e.key === `Enter` || e.key === ` `) {
                    e.preventDefault()
                    onToggle(issue.id)
                  }
                }}
                className="flex cursor-pointer items-center gap-2 border-b border-border/30 px-3 py-2 last:border-b-0 hover:bg-muted/50"
              >
                <Checkbox
                  checked={checked}
                  tabIndex={-1}
                  className="pointer-events-none"
                />
                <StatusIcon status={issue.status} className="size-4 shrink-0" />
                <span className="shrink-0 font-mono text-xs text-muted-foreground">
                  {issue.identifier}
                </span>
                <span className="flex-1 truncate text-sm">{issue.title}</span>
              </div>
            )
          })
        )}
      </div>
      {count > 0 && (
        <p className="text-xs text-muted-foreground">
          {count} issue{count === 1 ? `` : `s`} selected
        </p>
      )}
      {overCap && (
        <p className="text-xs text-destructive">
          At most {MAX_ISSUES_PER_RUN} issues per run — split the batch.
        </p>
      )}
      {spansRepos && (
        <p className="text-xs text-destructive">
          Pick issues from a single repository per run.
        </p>
      )}
      {!blocked && count > BATCH_COST_HINT_THRESHOLD && (
        <p className="text-xs text-muted-foreground">
          Large batches are token-expensive.
        </p>
      )}
    </div>
  )
}

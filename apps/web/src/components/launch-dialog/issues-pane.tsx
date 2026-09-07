import type { Issue } from "@/db/schema"
import { GlassGroup, GlassSearchRow } from "@/components/ui/glass-rows"
import { IssueStatusIcon } from "@/components/issue-properties/status-dropdown"
import { PriorityIcon } from "@/components/issue-properties/priority-dropdown"
import { conceptIcon } from "@/lib/icons.generated"
import { cn } from "@/lib/utils"

// The Issues tab of the unified launch dialog (EXP-257) — a presentational
// extraction of the Start-coding dialog's issue picker column. All state
// (search, selection, the derived row list and its guards) stays in the
// dialog shell; this only renders it.
//
// EXP-768: ONE glass group on every client — the search field is the group's
// first row and the hairline-divided issue rows follow, no caption above the
// card. The row anatomy is the mobile one: selection glyph, priority,
// identifier, status, title.

// Hard cap per run — parity with the server zod cap (issueIds max 30) and the
// desktop launcher's MAX_ISSUES_PER_RUN. Beyond it the server would reject with
// a zod BAD_REQUEST whose `[`-prefixed message is discarded into a misleading
// "could not be delivered" toast, so block the submit in the shell instead.
export const MAX_ISSUES_PER_RUN = 30
// Above this, batches get a soft token-cost note (matches the native sheets).
export const BATCH_COST_HINT_THRESHOLD = 6

// The natives' circle / circle-check selection glyph (EXP-721 row idiom).
const SelectedIcon = conceptIcon(`ui-selected`)
const UnselectedIcon = conceptIcon(`ui-unselected`)

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
    // Shrink only under `sm:` — see the actions pane's note (EXP-313).
    <div className="flex shrink-0 flex-col gap-2 sm:min-h-0 sm:shrink">
      <GlassGroup className="sm:min-h-0 sm:flex-1">
        <GlassSearchRow
          value={search}
          onChange={onSearchChange}
          placeholder="Search issues"
        />
        {/* Only the rows scroll — the search row stays put (the mobile
            sheets' bounded list). The list carries its own hairlines since
            the group's `divide-y` only reaches its direct children. */}
        <div className="max-h-44 divide-y divide-glass-stroke overflow-y-auto sm:max-h-none sm:min-h-32 sm:flex-1">
          {rows.length === 0 ? (
            <div className="px-4 py-3 text-sm text-foreground/70">
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
                  aria-pressed={checked}
                  onClick={() => onToggle(issue.id)}
                  onKeyDown={(e) => {
                    if (e.key === `Enter` || e.key === ` `) {
                      e.preventDefault()
                      onToggle(issue.id)
                    }
                  }}
                  className={cn(
                    `flex cursor-pointer items-center gap-2.5 px-4 py-2`,
                    checked ? `bg-glass-active` : `hover:bg-glass-active/50`
                  )}
                >
                  {checked ? (
                    <SelectedIcon className="size-5 shrink-0 text-foreground" />
                  ) : (
                    <UnselectedIcon className="size-5 shrink-0 text-muted-foreground" />
                  )}
                  <PriorityIcon
                    priority={issue.priority}
                    className="size-4 shrink-0"
                  />
                  <span className="min-w-[3.75rem] shrink-0 font-mono text-xs text-muted-foreground">
                    {issue.identifier}
                  </span>
                  <IssueStatusIcon issue={issue} className="size-4 shrink-0" />
                  <span className="flex-1 truncate text-sm">{issue.title}</span>
                </div>
              )
            })
          )}
        </div>
      </GlassGroup>
      {count > 0 && (
        <p className="text-xs text-muted-foreground">
          {count} issue{count === 1 ? `` : `s`} selected
        </p>
      )}
      {overCap && (
        <p className="text-xs text-destructive">
          At most {MAX_ISSUES_PER_RUN} issues per run. Split the batch.
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

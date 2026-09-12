import { Link } from "@tanstack/react-router"
import type { IssueGroup } from "@/lib/board-view"
import { IssueStatusIcon } from "@/components/issue-properties/status-dropdown"
import { GlassSectionHeader, ListRow } from "@/components/ui/glass-rows"

// EXP-818 (the navigation rule, web): an issue opened with no list context of
// its own brings its BOARD along — so the issue page is a master-detail on md+,
// the board's list on the left exactly like the Agent page's sessions list and
// the inbox's stream. Compact by design: status glyph, identifier, title, one
// section per status ROW (EXP-314), the open issue highlighted. The full
// `IssueList` (bulk select, context menus, row actions) stays the board page's.
//
// Below md the issue is its own screen, as before — this pane never renders
// there, so a phone keeps the whole width for the issue.

export function BoardIssueListPane({
  groups,
  teamSlug,
  boardSlug,
  boardSlugById,
  activeIssueId,
  from,
}: {
  groups: IssueGroup[]
  teamSlug: string
  /** The board every row belongs to — the single-board case. */
  boardSlug: string
  /** EXP-851: a CROSS-BOARD list (the sidebar's My issues nav) resolves each
   * row's board here; `boardSlug` is the fallback. */
  boardSlugById?: Map<string, string>
  activeIssueId: string
  /** EXP-851: the `?from=` token every row hands the issue it opens, so the
   * detail keeps THIS list beside it (`lib/detail-origin.ts`). */
  from?: string
}) {
  const visible = groups.filter((group) => group.issues.length > 0)
  return (
    <div className="flex-1 overflow-y-auto p-2" data-testid="board-issue-pane">
      {visible.length === 0 && (
        <div className="px-3 py-2 text-xs text-muted-foreground">
          No issues in this board.
        </div>
      )}
      {visible.map((group) => (
        <div key={group.status.id} className="mb-2">
          <GlassSectionHeader
            label={group.status.name}
            trailing={
              <span className="text-xs text-foreground/50 tabular-nums">
                {group.issues.length}
              </span>
            }
          />
          <div className="flex flex-col">
            {group.issues.map((issue) => (
              <ListRow
                key={issue.id}
                asChild
                interactive
                active={issue.id === activeIssueId}
                className="gap-2"
              >
                <Link
                  to="/t/$teamSlug/boards/$boardSlug/issues/$issueIdentifier"
                  params={{
                    teamSlug,
                    boardSlug: boardSlugById?.get(issue.boardId) ?? boardSlug,
                    issueIdentifier: issue.identifier,
                  }}
                  search={from ? { from } : {}}
                >
                  <IssueStatusIcon issue={issue} className="!h-3.5 !w-3.5" />
                  <span className="shrink-0 font-mono text-xs text-muted-foreground">
                    {issue.identifier}
                  </span>
                  <span className="min-w-0 flex-1 truncate text-sm">
                    {issue.title}
                  </span>
                </Link>
              </ListRow>
            ))}
          </div>
        </div>
      ))}
    </div>
  )
}

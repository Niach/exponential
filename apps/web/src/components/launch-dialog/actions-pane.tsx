import type {
  ActionRepoOption,
  TeamAction,
} from "@/components/action-editor-dialog"
import { ActionInputFields } from "@/components/launch-dialog/action-input-fields"
import { GlassGroup, GlassSearchRow } from "@/components/ui/glass-rows"
import { conceptIcon } from "@/lib/icons.generated"
import { getActionIcon } from "@/lib/board-icons"
import { cn } from "@/lib/utils"

// EXP-721: the mobile row idiom — a LEADING selection glyph (the natives'
// circle / circle-check), then the action's own icon, then name over an
// optional one-line description. The trailing check is gone: selection reads
// from the leading glyph plus the row tint.
const SelectedIcon = conceptIcon(`ui-selected`)
const UnselectedIcon = conceptIcon(`ui-unselected`)

// The Actions tab of the unified launch dialog (EXP-257): search + a
// single-select list (the builtin "Fix merge conflicts" pinned first by its
// `builtin` flag; "Create action" moved to its own dedicated dialog in
// EXP-431), followed by the selected action's typed input fields. Selection
// state and the fetched lists live in the shell.
//
// EXP-768: ONE glass group like the Issues tab — the search row heads the
// group, the hairline-divided action rows follow, no caption above the card.

export function ActionsPane({
  actions,
  search,
  onSearchChange,
  selectedActionId,
  onSelect,
  inputValues,
  onInputChange,
  repos,
  teamId,
  seedPrIssueId,
}: {
  /** Builtin-first sorted list; null while the fetch is in flight. */
  actions: TeamAction[] | null
  search: string
  onSearchChange: (value: string) => void
  selectedActionId: string | null
  onSelect: (actionId: string) => void
  inputValues: Record<string, string>
  onInputChange: (key: string, value: string) => void
  repos: ActionRepoOption[]
  teamId: string
  /** Any issue id linked to the PR a `pr` input should open pre-picked. */
  seedPrIssueId?: string
}) {
  const query = search.trim().toLowerCase()
  const rows = (actions ?? []).filter(
    (action) =>
      !query ||
      action.name.toLowerCase().includes(query) ||
      (action.description ?? ``).toLowerCase().includes(query)
  )
  const selectedAction =
    (actions ?? []).find((action) => action.id === selectedActionId) ?? null

  return (
    // `min-h-0` only under `sm:` — in the mobile scroll column it let the
    // flex layout squash the pane and paint its content over the next
    // section (EXP-313); mobile lays out at natural height and the shell
    // scrolls.
    <div className="flex shrink-0 flex-col gap-2 sm:min-h-0 sm:shrink">
      <GlassGroup className="sm:min-h-0 sm:flex-1">
        <GlassSearchRow
          value={search}
          onChange={onSearchChange}
          placeholder="Search actions"
        />
        {/* Only the rows scroll — the search row stays put; the list draws
            its own hairlines (the group's `divide-y` stops at its children). */}
        <div className="max-h-44 divide-y divide-glass-stroke overflow-y-auto sm:max-h-none sm:min-h-32 sm:flex-1">
          {actions === null ? (
            <div className="px-4 py-3 text-sm text-foreground/70">Loading…</div>
          ) : rows.length === 0 ? (
            <div className="px-4 py-3 text-sm text-foreground/70">
              {query ? `No actions match "${search}"` : `No actions yet.`}
            </div>
          ) : (
            rows.map((action) => {
              const selected = action.id === selectedActionId
              const RowIcon = getActionIcon(action)
              return (
                <div
                  key={action.id}
                  role="button"
                  tabIndex={0}
                  aria-pressed={selected}
                  onClick={() => onSelect(action.id)}
                  onKeyDown={(e) => {
                    if (e.key === `Enter` || e.key === ` `) {
                      e.preventDefault()
                      onSelect(action.id)
                    }
                  }}
                  className={cn(
                    `flex cursor-pointer items-center gap-2.5 px-4 py-2`,
                    selected ? `bg-glass-active` : `hover:bg-glass-active/50`
                  )}
                >
                  {selected ? (
                    <SelectedIcon className="size-5 shrink-0 text-foreground" />
                  ) : (
                    <UnselectedIcon className="size-5 shrink-0 text-muted-foreground" />
                  )}
                  <RowIcon className="size-4 shrink-0 text-muted-foreground" />
                  <span className="flex min-w-0 flex-1 flex-col">
                    <span className="truncate text-sm">{action.name}</span>
                    {action.description && (
                      <span className="truncate text-xs text-muted-foreground">
                        {action.description}
                      </span>
                    )}
                  </span>
                </div>
              )
            })
          )}
        </div>
      </GlassGroup>
      {selectedAction && selectedAction.inputs.length > 0 && (
        <ActionInputFields
          defs={selectedAction.inputs}
          values={inputValues}
          onChange={onInputChange}
          repos={repos}
          teamId={teamId}
          seedPrIssueId={seedPrIssueId}
        />
      )}
    </div>
  )
}

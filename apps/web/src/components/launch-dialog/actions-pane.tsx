import { Check, Search } from "lucide-react"
import type {
  ActionRepoOption,
  TeamAction,
} from "@/components/action-editor-dialog"
import { ActionInputFields } from "@/components/launch-dialog/action-input-fields"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { getActionIcon } from "@/lib/board-icons"
import { cn } from "@/lib/utils"

// The Actions tab of the unified launch dialog (EXP-257): search + a
// single-select list (the builtin "Fix merge conflicts" pinned first by its
// `builtin` flag; "Create action" moved to its own dedicated dialog in
// EXP-431), followed by the selected action's typed input fields. Selection
// state and the fetched lists live in the shell.

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
      <Label>Actions</Label>
      <div className="relative">
        <Search className="absolute left-2.5 top-1/2 size-4 -translate-y-1/2 text-muted-foreground" />
        <Input
          value={search}
          onChange={(e) => onSearchChange(e.target.value)}
          placeholder="Search actions…"
          className="h-9 pl-8"
        />
      </div>
      <div className="max-h-44 overflow-y-auto rounded-md border border-border sm:max-h-none sm:min-h-32 sm:flex-1">
        {actions === null ? (
          <div className="px-3 py-6 text-center text-xs text-muted-foreground">
            Loading…
          </div>
        ) : rows.length === 0 ? (
          <div className="px-3 py-6 text-center text-xs text-muted-foreground">
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
                onClick={() => onSelect(action.id)}
                onKeyDown={(e) => {
                  if (e.key === `Enter` || e.key === ` `) {
                    e.preventDefault()
                    onSelect(action.id)
                  }
                }}
                className={cn(
                  `flex cursor-pointer items-center gap-2 border-b border-border/30 px-3 py-2 last:border-b-0`,
                  selected ? `bg-muted` : `hover:bg-muted/50`
                )}
              >
                <RowIcon className="size-4 shrink-0 text-muted-foreground" />
                <span className="min-w-0 flex-1 truncate text-sm">
                  {action.name}
                </span>
                {selected && <Check className="size-4 shrink-0" />}
              </div>
            )
          })
        )}
      </div>
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

import { Checkbox } from "./checkbox"
import { cn } from "./cn"
import { Label } from "./label"
import { Switch } from "./switch"

// FEED-76: ONE team/board scope picker. The MCP consent screen (EXP-792) and
// the Create-API-key dialog show the same control: an "Everything" switch
// over the member's teams, each a whole-team checkbox with its boards
// indented beneath (a board under a ticked team is covered by it and reads
// disabled-checked). The raw selection lives in the caller; what is SENT is
// `effectiveScopeSelection` — boards a selected team already covers drop out,
// "Everything" clears both lists — applied at submit so unticking a team
// restores the boards the user had ticked under it.

export interface ScopePickerBoard {
  id: string
  name: string
  prefix: string
}

export interface ScopePickerTeam {
  id: string
  name: string
  boards: ScopePickerBoard[]
}

export interface ScopeSelection {
  allTeams: boolean
  teamIds: string[]
  boardIds: string[]
}

/** The picker's starting value: everything. */
export const EMPTY_SCOPE_SELECTION: ScopeSelection = {
  allTeams: true,
  teamIds: [],
  boardIds: [],
}

/** Something is selected (everything, or at least one team/board). */
export function hasScopeSelection(value: ScopeSelection): boolean {
  return value.allTeams || value.teamIds.length > 0 || value.boardIds.length > 0
}

/** What to send: boards covered by a selected whole team are dropped; an
 *  all-teams pick carries no ids at all. */
export function effectiveScopeSelection(
  tree: readonly ScopePickerTeam[],
  value: ScopeSelection
): ScopeSelection {
  if (value.allTeams) return { allTeams: true, teamIds: [], boardIds: [] }
  const selectedTeams = new Set(value.teamIds)
  const covered = new Set(
    tree
      .filter((team) => selectedTeams.has(team.id))
      .flatMap((team) => team.boards.map((board) => board.id))
  )
  return {
    allTeams: false,
    teamIds: [...value.teamIds],
    boardIds: value.boardIds.filter((id) => !covered.has(id)),
  }
}

export interface ScopeCaptionInput {
  teams: ReadonlyArray<{ name: string }>
  boards: ReadonlyArray<{ name: string; prefix: string }>
}

/** The one-line caption a key/grant row wears: "All teams" or
 *  "Scoped to Acme, Web (WEB)". Mirrored by the IDE (users.rs scope_caption). */
export function scopeCaption(scope: ScopeCaptionInput | null | undefined): string {
  if (!scope) return `All teams`
  const parts = [
    ...scope.teams.map((team) => team.name),
    ...scope.boards.map((board) => `${board.name} (${board.prefix})`),
  ]
  return parts.length === 0 ? `Scoped to nothing reachable` : `Scoped to ${parts.join(`, `)}`
}

export interface ScopePickerProps {
  tree: readonly ScopePickerTeam[]
  value: ScopeSelection
  onChange: (next: ScopeSelection) => void
  /** Prefix for the control ids when two pickers share a page. */
  idPrefix?: string
  className?: string
}

const toggle = (list: string[], id: string, on: boolean) =>
  on ? (list.includes(id) ? list : [...list, id]) : list.filter((item) => item !== id)

export function ScopePicker({
  tree,
  value,
  onChange,
  idPrefix = `scope`,
  className,
}: ScopePickerProps) {
  const selectedTeams = new Set(value.teamIds)
  const selectedBoards = new Set(value.boardIds)
  return (
    <div className={cn(`space-y-4`, className)} data-slot="scope-picker">
      <div className="flex items-start justify-between gap-3 rounded-md border p-3">
        <div className="space-y-0.5">
          <Label htmlFor={`${idPrefix}-all-teams`}>Everything</Label>
          <p className="text-xs text-muted-foreground">
            All teams and boards, including ones created later.
          </p>
        </div>
        <Switch
          id={`${idPrefix}-all-teams`}
          checked={value.allTeams}
          onCheckedChange={(checked) =>
            onChange({ ...value, allTeams: checked === true })
          }
        />
      </div>

      {!value.allTeams && (
        <div className="max-h-72 space-y-3 overflow-y-auto rounded-md border p-3">
          {tree.length === 0 && (
            <p className="text-sm text-muted-foreground">
              You aren&apos;t a member of any team yet.
            </p>
          )}
          {tree.map((team) => {
            const wholeTeam = selectedTeams.has(team.id)
            return (
              <div key={team.id} className="space-y-1.5" data-team={team.id}>
                <div className="flex items-center gap-2">
                  <Checkbox
                    id={`${idPrefix}-team-${team.id}`}
                    checked={wholeTeam}
                    onCheckedChange={(checked) =>
                      onChange({
                        ...value,
                        teamIds: toggle(value.teamIds, team.id, checked === true),
                      })
                    }
                  />
                  <Label htmlFor={`${idPrefix}-team-${team.id}`} className="font-medium">
                    {team.name}
                  </Label>
                  <span className="text-xs text-muted-foreground">whole team</span>
                </div>
                <div className="ml-6 space-y-1">
                  {team.boards.map((board) => (
                    <div key={board.id} className="flex items-center gap-2">
                      <Checkbox
                        id={`${idPrefix}-board-${board.id}`}
                        disabled={wholeTeam}
                        checked={wholeTeam || selectedBoards.has(board.id)}
                        onCheckedChange={(checked) =>
                          onChange({
                            ...value,
                            boardIds: toggle(value.boardIds, board.id, checked === true),
                          })
                        }
                      />
                      <Label
                        htmlFor={`${idPrefix}-board-${board.id}`}
                        className="font-normal"
                      >
                        {board.name}
                        <span className="ml-1.5 text-xs text-muted-foreground">
                          {board.prefix}
                        </span>
                      </Label>
                    </div>
                  ))}
                  {team.boards.length === 0 && (
                    <p className="text-xs text-muted-foreground">No boards yet.</p>
                  )}
                </div>
              </div>
            )
          })}
        </div>
      )}
    </div>
  )
}

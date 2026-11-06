// #EXP-1715 (composer-menu.json `teamsPick`): the Assistant's Teams pick,
// which teams a turn may touch. The "+" › Teams submenu = THE search list of
// multi picker rows (the team's own mark + name, the switcher's order);
// nothing picked = every team (null). While the pick is a subset, the picked
// teams ride as the composer's removable pill chips.
import { ChipRemoveButton, Pill, SearchList, TeamAvatar, type PickerItem } from "@exp/ui"
import { COMPOSER_MENU_TEST_IDS } from "@/components/launch-dialog/composer-menu"

export interface PickTeam {
  id: string
  name: string
  iconUrl: string | null
}

/** The teams a pick holds (null = every team), in the teams' order. */
export function pickedTeamIds(picked: readonly string[] | null, teams: readonly PickTeam[]): string[] {
  return teams.filter((team) => picked === null || picked.includes(team.id)).map((team) => team.id)
}

/** A changed set back to the stored pick: every team = null; never none. */
export function nextTeamsPick(next: readonly string[], teams: readonly PickTeam[]): string[] | null {
  const kept = teams.filter((team) => next.includes(team.id)).map((team) => team.id)
  if (kept.length === 0 || kept.length === teams.length) return null
  return kept
}

export function TeamPickSearchList({
  teams,
  picked,
  onChange,
}: {
  teams: readonly PickTeam[]
  picked: readonly string[] | null
  onChange: (next: string[] | null) => void
}) {
  const value = pickedTeamIds(picked, teams)
  const items: PickerItem[] = teams.map((team) => ({ value: team.id, label: team.name, keywords: [team.name] }))
  return (
    <div className="flex max-h-[min(24rem,var(--radix-dropdown-menu-content-available-height,24rem))] min-h-0 flex-col">
      <SearchList
        mode="multi"
        items={items}
        value={value}
        // The last checked team stays: a turn always touches one.
        onChange={(next) => next.length > 0 && onChange(nextTeamsPick(next, teams))}
        inputTestId={COMPOSER_MENU_TEST_IDS.submenuSearch}
        renderItem={(item) => {
          const team = teams.find((row) => row.id === item.value)
          return (
            <>
              <TeamAvatar iconUrl={team?.iconUrl} size={16} />
              <span className="min-w-0 flex-1 truncate text-sm">{item.label}</span>
            </>
          )
        }}
      />
    </div>
  )
}

/** The picked teams as the composer's chips (a subset only). */
export function TeamPickChips({
  teams,
  picked,
  onChange,
  disabled,
}: {
  teams: readonly PickTeam[]
  picked: readonly string[] | null
  onChange: (next: string[] | null) => void
  disabled?: boolean
}) {
  if (picked === null) return null
  const chosen = teams.filter((team) => picked.includes(team.id))
  if (chosen.length === 0 || chosen.length === teams.length) return null
  return (
    <>
      {chosen.map((team) => (
        <Pill
          key={team.id}
          size="sm"
          mode="readonly"
          className="max-w-[16rem] pr-1"
          data-testid={`agent-composer-chip-team-${team.id}`}
          title={team.name}
        >
          <TeamAvatar iconUrl={team.iconUrl} size={12} />
          <span className="min-w-0 truncate">{team.name}</span>
          <ChipRemoveButton
            label={`Remove ${team.name}`}
            testId={`agent-composer-chip-team-${team.id}-remove`}
            disabled={disabled}
            onRemove={() =>
              onChange(
                chosen.length === 1 ? null : nextTeamsPick(chosen.filter((row) => row.id !== team.id).map((row) => row.id), teams)
              )
            }
          />
        </Pill>
      ))}
    </>
  )
}

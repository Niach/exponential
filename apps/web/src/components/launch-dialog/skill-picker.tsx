// #EXP-1659: the team's skills as multi picker rows (skills.json `menuRow`):
// the composer's "+" › Skills (inside the menu, the MCP servers' twin) and
// the agent page's Skills row. A row = the skill's name, its description as
// the caption, the skill glyph; name order (the caller passes `useSkills`).
import { SearchList, conceptIcon, type PickerItem } from "@exp/ui"
import { COMPOSER_MENU_TEST_IDS } from "@/components/launch-dialog/composer-menu"

const SkillIcon = conceptIcon(`ui-docs`)

export interface SkillPickerRow {
  id: string
  name: string
  description: string
}

export function skillPickerItems(skills: readonly SkillPickerRow[]): PickerItem[] {
  return skills.map((skill) => ({
    value: skill.id,
    label: skill.name,
    icon: SkillIcon,
    description: skill.description,
    keywords: [skill.name, skill.description],
  }))
}

/** The "+" menu's Skills submenu (composer-menu.json + skills.json `menuRow`):
 *  THE search list, rows of one height: the name over a one-line description. */
export function SkillSearchList({
  skills,
  selectedIds,
  onToggle,
}: {
  skills: readonly SkillPickerRow[]
  selectedIds: readonly string[]
  onToggle: (id: string) => void
}) {
  return (
    <div className="flex max-h-[min(24rem,var(--radix-dropdown-menu-content-available-height,24rem))] min-h-0 flex-col">
      <SearchList
        mode="multi"
        items={skillPickerItems(skills)}
        value={selectedIds}
        onChange={(next) => {
          const added = next.find((id) => !selectedIds.includes(id))
          const removed = selectedIds.find((id) => !next.includes(id))
          const changed = added ?? removed
          if (changed) onToggle(changed)
        }}
        inputTestId={COMPOSER_MENU_TEST_IDS.submenuSearch}
        rowClassName="h-11"
        renderItem={(item) => (
          <>
            <SkillIcon className="size-4 shrink-0 text-muted-foreground" />
            <span className="flex min-w-0 flex-1 flex-col">
              <span className="truncate text-sm">{item.label}</span>
              <span className="truncate text-xs text-muted-foreground">{item.description}</span>
            </span>
          </>
        )}
      />
    </div>
  )
}

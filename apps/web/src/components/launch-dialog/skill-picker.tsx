// #EXP-1659: the team's skills as multi picker rows (skills.json `menuRow`):
// the composer's "+" › Skills (inside the menu, the MCP servers' twin) and
// the agent page's Skills row. A row = the skill's name, its description as
// the caption, the skill glyph; name order (the caller passes `useSkills`).
import { PickerMenuRows, conceptIcon, type PickerItem } from "@exp/ui"

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

/** The rows INSIDE a menu: the menu stays open across toggles. */
export function SkillMenuRows({
  skills,
  selectedIds,
  onToggle,
}: {
  skills: readonly SkillPickerRow[]
  selectedIds: readonly string[]
  onToggle: (id: string) => void
}) {
  return (
    <PickerMenuRows
      mode="multi"
      items={skillPickerItems(skills)}
      value={selectedIds}
      onChange={(next) => {
        const added = next.find((id) => !selectedIds.includes(id))
        const removed = selectedIds.find((id) => !next.includes(id))
        const changed = added ?? removed
        if (changed) onToggle(changed)
      }}
    />
  )
}

// #EXP-1656: team skills, the pure reference every client mirrors
// (fixtures/skills.json is the spec; the server's limits live in
// packages/db-schema domain.ts SKILL_*).

import fixture from "../fixtures/skills.json" with { type: "json" }

export const SKILLS = fixture
export const SKILL_LIMITS = fixture.limits

export type SkillField = `name` | `description` | `body`

export interface SkillForm {
  name: string
  description: string
  body: string
}

export interface SkillFormError {
  field: SkillField
  message: string
}

const namePattern = new RegExp(fixture.limits.namePattern)
const utf8Bytes = (value: string) => new TextEncoder().encode(value).length

/** The skill page's errors, keyed by field in field order ([] = valid). */
export function validateSkillForm(form: SkillForm): SkillFormError[] {
  const { errors } = fixture.page
  const out: SkillFormError[] = []
  const name = form.name.trim()
  if (!name) out.push({ field: `name`, message: errors.nameRequired })
  else if (name.length > fixture.limits.nameMax) out.push({ field: `name`, message: errors.nameTooLong })
  else if (!namePattern.test(name)) out.push({ field: `name`, message: errors.nameInvalid })
  const description = form.description.trim()
  if (!description) out.push({ field: `description`, message: errors.descriptionRequired })
  else if (description.length > fixture.limits.descriptionMax)
    out.push({ field: `description`, message: errors.descriptionTooLong })
  if (!form.body.trim()) out.push({ field: `body`, message: errors.bodyRequired })
  else if (utf8Bytes(form.body) > fixture.limits.bodyMaxBytes) out.push({ field: `body`, message: errors.bodyTooLong })
  return out
}

/** Settings › Skills and the + submenu order: by name, code-point order. */
export function sortSkills<T extends { name: string }>(skills: readonly T[]): T[] {
  return [...skills].sort((a, b) => (a.name < b.name ? -1 : a.name > b.name ? 1 : 0))
}

/** The launch pick's seed (the MCP servers' rule): the saved pick clamped
 *  to skills that still exist, else the enabled-by-default ones; list order. */
export function preselectSkillIds(
  skills: readonly { id: string; name: string; enabledByDefault: boolean }[],
  saved: readonly string[] | null
): string[] {
  const sorted = sortSkills(skills)
  if (saved !== null) {
    const picked = new Set(saved)
    return sorted.filter((skill) => picked.has(skill.id)).map((skill) => skill.id)
  }
  return sorted.filter((skill) => skill.enabledByDefault).map((skill) => skill.id)
}

/** The + menu row's value: the number picked, none at zero. */
export function skillsMenuValue(count: number): string | null {
  if (count <= 0) return null
  return (count === 1 ? fixture.menuValue.one : fixture.menuValue.other).replace(`{n}`, String(count))
}

/** The agent page row's value: `N skills`, `None` at zero. */
export function agentSkillsValue(count: number): string {
  if (count <= 0) return fixture.agentRow.none
  return (count === 1 ? fixture.agentValue.one : fixture.agentValue.other).replace(`{n}`, String(count))
}

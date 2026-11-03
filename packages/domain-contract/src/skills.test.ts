import { describe, expect, it } from "vitest"
import fixture from "../fixtures/skills.json" with { type: "json" }
import {
  agentSkillsValue,
  preselectSkillIds,
  skillsMenuValue,
  sortSkills,
  validateSkillForm,
  type SkillForm,
} from "./skills"

type Spelled = string | { repeat: string; times: number }
const spell = (value: Spelled) => (typeof value === `string` ? value : value.repeat.repeat(value.times))

describe(`skills.json (#EXP-1656)`, () => {
  it.each(fixture.validationCases)(`validate: $name`, ({ form, expected }) => {
    const spelled = Object.fromEntries(
      Object.entries(form as Record<string, Spelled>).map(([key, value]) => [key, spell(value)])
    ) as unknown as SkillForm
    expect(validateSkillForm(spelled)).toEqual(expected)
  })
  it.each(fixture.preselectCases)(`preselect: $name`, ({ skills, saved, expected }) => {
    expect(preselectSkillIds(skills, saved)).toEqual(expected)
  })
  it.each(fixture.sortCases)(`sort: $name`, ({ names, expected }) => {
    expect(sortSkills(names.map((name) => ({ name }))).map((s) => s.name)).toEqual(expected)
  })
  it.each(fixture.menuValueCases)(`menu value: $name`, ({ count, expected }) => {
    expect(skillsMenuValue(count)).toBe(expected)
  })
  it.each(fixture.agentValueCases)(`agent value: $name`, ({ count, expected }) => {
    expect(agentSkillsValue(count)).toBe(expected)
  })
  it(`the + row sits after MCP servers on launch, temporary only`, () => {
    expect(fixture.menuRow).toMatchObject({ id: `skills`, surfaces: [`launch`], after: `mcp-servers` })
    expect(fixture.menuRow.when).toEqual([`temporary`, `skills`])
  })
  it(`phones and members never write`, () => {
    expect(fixture.permissions.owner.write).toBe(true)
    expect(fixture.permissions.member.write).toBe(false)
    expect(fixture.permissions.phone.write).toBe(false)
    expect(fixture.permissions.phone.pick).toBe(true)
  })
})

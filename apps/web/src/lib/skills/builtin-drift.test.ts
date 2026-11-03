import { readFileSync } from "node:fs"
import { resolve } from "node:path"
import { describe, expect, it } from "vitest"
import { BUILTIN_SKILLS, isSkillFilePath, SKILL_LIMITS } from "@exp/domain-contract/skills"
import { buildSkills, MOTION_GALLERY, readSkillsDir } from "../../../../../packages/exponential-ui/scripts/generate-skills"

// #VAPP-213: the built-in skills are GENERATED from the docs
// (packages/exponential-ui/scripts/generate-skills.ts) and committed under
// packages/exponential-ui/skills. A doc edit without a regen fails here:
// run `bun packages/exponential-ui/scripts/generate-skills.ts`.

const built = buildSkills()

describe(`built-in skills (#VAPP-213)`, () => {
  it(`the committed skills match the docs (regenerate on drift)`, () => {
    const disk = readSkillsDir()
    expect([...disk.keys()].sort()).toEqual([...built.keys()].sort())
    for (const [path, content] of built) expect(disk.get(path), path).toBe(content)
  })

  it(`one skill per fixture name, each inside the skills limits`, () => {
    const manifest = JSON.parse(built.get(`index.json`)!) as {
      skills: { name: string; description: string; bytes: number; files: string[] }[]
    }
    expect(manifest.skills.map((s) => s.name)).toEqual([...BUILTIN_SKILLS.names].sort())
    for (const skill of manifest.skills) {
      expect(skill.name).toMatch(new RegExp(SKILL_LIMITS.namePattern))
      expect(skill.description.length).toBeLessThanOrEqual(SKILL_LIMITS.descriptionMax)
      expect(skill.bytes).toBeLessThanOrEqual(SKILL_LIMITS.bodyMaxBytes)
      for (const file of skill.files) expect(isSkillFilePath(file), file).toBe(true)
    }
    expect(BUILTIN_SKILLS.editorDelivery.every((name) => BUILTIN_SKILLS.names.includes(name))).toBe(true)
  })

  it(`the Motion examples are the site's gallery`, () => {
    const page = readFileSync(resolve(import.meta.dirname, `../../../../site/src/components/eui/trees.ts`), `utf8`)
    const gallery = [...page.matchAll(/eval\/motion\/fixtures\/dialect\/([a-z0-9-]+)\.json/g)].map((m) => m[1])
    expect(gallery).toEqual([...MOTION_GALLERY])
  })
})

// The fixture is the contract; every mirror replays it.
import { describe, expect, test } from "bun:test"
import { readFileSync } from "node:fs"
import { join } from "node:path"

import fixture from "../fixtures/skills.json" with { type: "json" }
import shipped from "../../exponential-ui/skills/index.json" with { type: "json" }
import {
  MCP_BUILTIN_NAMES,
  parseSkillUri,
  servedSkills,
  skillDigest,
  skillDirectoryRead,
  skillResourceRead,
  skillsGet,
  skillsList,
  teamSkillMd,
  byteSize,
  type BuiltinSource,
  type ReachableTeam,
} from "./skills"

const given = fixture.given as unknown as { teams: ReachableTeam[]; builtins: BuiltinSource[] }
const served = servedSkills(given.teams, given.builtins)
const DIGEST = /^sha256:[0-9a-f]{64}$/

describe(`digests`, () => {
  for (const entry of fixture.digestCases) {
    test(entry.name, () => {
      expect(skillDigest(entry.content)).toBe(entry.digest)
      expect(byteSize(entry.content)).toBe(entry.size)
    })
  }

  test(`the overview's code-review listing, byte for byte`, () => {
    const { skillMd, checklist, listing } = fixture.specExamples.codeReview
    expect(listing.resources.map((r) => [r.digest, r.size])).toEqual([
      [skillDigest(skillMd), byteSize(skillMd)],
      [skillDigest(checklist), byteSize(checklist)],
    ])
  })

  test(`the spec's git-workflow entry has the listing shape`, () => {
    const g = fixture.specExamples.gitWorkflow
    expect(g.resources[0]!.uri).toBe(g.uri)
    expect(g.resources[0]!.digest).toMatch(DIGEST)
    expect(g.uri.split(`/`).at(-2)).toBe(g.frontmatter.name)
  })
})

describe(`teamSkillMd`, () => {
  for (const entry of fixture.teamSkillMdCases) {
    test(entry.name, () => expect(teamSkillMd(entry.skill.name, entry.skill.description, entry.skill.body)).toBe(entry.expected))
  }
})

describe(`parseSkillUri`, () => {
  for (const entry of fixture.uriCases) test(String(entry.uri), () => expect(parseSkillUri(entry.uri)).toEqual(entry.expected as never))
})

describe(`skills/list`, () => {
  for (const entry of fixture.listCases) {
    test(entry.name, () => {
      const teams = entry.teams === `given` ? given.teams : (entry.teams as ReachableTeam[])
      expect(skillsList(servedSkills(teams, given.builtins), (entry as { params?: object }).params)).toEqual(entry.expected as never)
    })
  }

  test(`frontmatter carries name and description only`, () => {
    const list = skillsList(served) as { skills: { frontmatter: object }[] }
    for (const skill of list.skills) expect(Object.keys(skill.frontmatter).sort()).toEqual([`description`, `name`])
  })
})

describe(`skills/get`, () => {
  for (const entry of fixture.getCases) test(entry.name, () => expect(skillsGet(served, entry.uri)).toEqual(entry.expected as never))
})

describe(`resources/read`, () => {
  for (const entry of fixture.readCases) test(entry.name, () => expect(skillResourceRead(served, entry.uri)).toEqual(entry.expected as never))

  test(`every manifest entry's digest is over the bytes read`, () => {
    const list = skillsList(served) as { skills: { resources: { uri: string; digest: string; size: number }[] }[] }
    for (const skill of list.skills) {
      expect(skill.resources[0]!.uri.endsWith(`/SKILL.md`)).toBe(true)
      for (const r of skill.resources) {
        const read = skillResourceRead(served, r.uri) as { contents: { text: string }[] }
        expect(skillDigest(read.contents[0]!.text)).toBe(r.digest)
        expect(byteSize(read.contents[0]!.text)).toBe(r.size)
      }
    }
  })
})

describe(`resources/directory/read`, () => {
  for (const entry of fixture.directoryCases) test(entry.name, () => expect(skillDirectoryRead(served, entry.uri)).toEqual(entry.expected as never))
})

describe(`the shipped built-ins`, () => {
  const dir = join(import.meta.dirname, `..`, `..`, `exponential-ui`, `skills`)
  const read = (path: string) => readFileSync(join(dir, path), `utf8`)
  const sources: BuiltinSource[] = shipped.skills.map((s) => ({
    name: s.name,
    description: s.description,
    skillMd: read(`${s.name}/SKILL.md`),
    files: s.files.map((path) => ({ path, content: read(`${s.name}/${path}`) })),
  }))

  test(`every MCP built-in ships`, () => {
    for (const name of MCP_BUILTIN_NAMES) expect(shipped.skills.map((s) => s.name)).toContain(name)
  })

  test(`complete manifests inside the spec's limits, named like their path`, () => {
    const builtins = servedSkills([], sources)
    expect(builtins.map((s) => s.name)).toEqual([...MCP_BUILTIN_NAMES])
    const list = skillsList(builtins) as { cacheScope: string; skills: { uri: string; frontmatter: { name: string }; resources: { uri: string; digest: string; size: number }[] }[] }
    expect(list.cacheScope).toBe(fixture.cacheScope.builtin)
    for (const skill of list.skills) {
      const source = sources.find((s) => s.name === skill.frontmatter.name)!
      expect(skill.uri).toBe(`skill://${fixture.builtinRoot}/${source.name}/SKILL.md`)
      expect(skill.resources.map((r) => r.uri)).toEqual([`SKILL.md`, ...[...source.files.map((f) => f.path)].sort()].map((p) => `skill://${fixture.builtinRoot}/${source.name}/${p}`))
      expect(skill.resources.length).toBeLessThanOrEqual(fixture.limits.maxFiles)
      expect(skill.resources.reduce((n, r) => n + r.size, 0)).toBeLessThanOrEqual(fixture.limits.maxBytes)
      expect(source.skillMd).toStartWith(`---\nname: ${source.name}\n`)
      expect(source.skillMd).not.toMatch(/^allowed-tools:/m)
    }
  })
})

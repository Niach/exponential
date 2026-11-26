// The fixture is the contract; every mirror replays it.
import { describe, expect, test } from "bun:test"

import fixture from "../fixtures/hub-client.json" with { type: "json" }
import {
  clampPollMs,
  digestSet,
  elicitResult,
  hubSkillMd,
  hubToolNames,
  skillDirName,
  taskTrace,
  verifySkillFile,
  type ElicitAction,
  type HubTaskStatus,
} from "./hub-client"

describe(`tool names`, () => {
  for (const entry of fixture.toolNameCases) {
    test(`${entry.server}/${entry.tool}`, () => {
      expect(hubToolNames(entry.server, entry.tool)).toEqual({ claude: entry.claude.after, codex: entry.codex.after })
    })
  }
})

describe(`elicit results`, () => {
  for (const entry of fixture.elicitResultCases) {
    test(entry.name, () => expect(elicitResult(entry.action as ElicitAction)).toEqual(entry.expected as never))
  }
  test(`no live session = the copy's answer`, () => expect(elicitResult(null).action).toBe(fixture.copy.noSession as `cancel`))
})

describe(`task polling`, () => {
  for (const entry of fixture.pollCases) test(String(entry.pollIntervalMs), () => expect(clampPollMs(entry.pollIntervalMs)).toBe(entry.expected))
})

describe(`task traces`, () => {
  for (const entry of fixture.traceCases) {
    test(`${entry.status} ${JSON.stringify(entry.statusMessage)}`, () => {
      expect(taskTrace({ ...entry, status: entry.status as HubTaskStatus })).toEqual(entry.expected)
    })
  }
})

describe(`skill files`, () => {
  for (const entry of fixture.verifyCases) {
    test(entry.name, () => {
      expect(verifySkillFile(entry.entry, entry.bytes)).toBe(entry.expected as never)
      expect(verifySkillFile(entry.entry, new TextEncoder().encode(entry.bytes))).toBe(entry.expected as never)
    })
  }
})

describe(`digest sets`, () => {
  for (const entry of fixture.digestSetCases) test(entry.name, () => expect(digestSet(entry.resources)).toBe(entry.expected))
})

describe(`dir names`, () => {
  for (const entry of fixture.dirNameCases) {
    test(entry.expected, () => {
      const name = skillDirName(entry.server, entry.name)
      expect(name).toBe(entry.expected)
      expect(name.length).toBeLessThanOrEqual(fixture.skills.nameMax)
      expect(name).toMatch(new RegExp(fixture.skills.namePattern))
    })
  }
})

describe(`SKILL.md rewrite`, () => {
  for (const entry of fixture.skillMdCases) test(entry.name, () => expect(hubSkillMd(entry.dirName, entry.bytes)).toBe(entry.expected))
})

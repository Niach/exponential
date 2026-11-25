import { describe, expect, it } from "bun:test"
import { MEMBER_SPACE, mayOptIn, maySpaceRun, pinCovers, spaceEnds, spaceSections, spaceTeams } from "./member-space"

describe(`member-space.json`, () => {
  for (const c of MEMBER_SPACE.optInCases) it(`opt-in: ${c.name}`, () => expect(mayOptIn(c)).toBe(c.expected))
  for (const c of MEMBER_SPACE.teamCases)
    it(`teams: ${c.name}`, () => expect(spaceTeams(c.assistantTeamIds, c.hostOwnerTeamIds, c.memberTeamIds)).toEqual(c.expected))
  for (const c of MEMBER_SPACE.startCases) it(`start: ${c.name}`, () => expect(maySpaceRun(c.tree, c.rootDeviceIsSpace)).toBe(c.expected))
  for (const c of MEMBER_SPACE.pinCases) it(`pin: ${c.name}`, () => expect(pinCovers(c.pin, c.rootIsOwnAssistant)).toBe(c.expected))
  for (const c of MEMBER_SPACE.sectionCases)
    it(`sections: ${c.name}`, () => expect(spaceSections(c.devices, c.teams, c.boundId, c.me)).toEqual(c.expected))
  for (const c of MEMBER_SPACE.endCases) it(`end: ${c.name}`, () => expect(spaceEnds(c.spaceTeamId, c.spaceTeams)).toBe(c.ends))
})

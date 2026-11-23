import { describe, expect, it } from "vitest"
import composerMenu from "@exp/domain-contract/fixtures/composer-menu.json"
import deviceDoctor from "@exp/domain-contract/fixtures/device-doctor.json"
import {
  agentStartVerdict,
  assertComputerUsePick,
  COMPUTER_USE_REFUSALS,
  COMPUTER_USE_SWITCH_CAP,
  computerUseAtStart,
  computerUseStartLog,
  raisesComputerUse,
} from "@/lib/sessions/computer-use"

// #EXP-1899 (composer-menu.json `computerUse`): what `sessions.computer_use`
// stores. true = the session follows the device's switch, false = opted out.
describe(`computer use: what a new session stores`, () => {
  const person = { viaMcp: false, caller: null, from: null }
  const agent = { viaMcp: true, from: null }

  it(`a person's start without the key follows the switch`, () => {
    expect(computerUseAtStart({ ...person, pick: undefined })).toBe(true)
    expect(computerUseAtStart({ ...person, pick: true })).toBe(true)
  })

  it(`an explicit false always opts out`, () => {
    expect(computerUseAtStart({ ...person, pick: false })).toBe(false)
    expect(computerUseAtStart({ ...agent, caller: true, pick: false })).toBe(false)
    expect(computerUseAtStart({ viaMcp: false, caller: null, from: true, pick: false })).toBe(false)
  })

  it(`a person's child or continuation inherits unless it picks`, () => {
    expect(computerUseAtStart({ viaMcp: false, caller: null, from: false, pick: undefined })).toBe(false)
    expect(computerUseAtStart({ viaMcp: false, caller: null, from: false, pick: true })).toBe(true)
  })

  it(`a root a session starts over MCP takes the starting session's value`, () => {
    expect(computerUseAtStart({ ...agent, caller: true, pick: undefined })).toBe(true)
    expect(computerUseAtStart({ ...agent, caller: false, pick: undefined })).toBe(false)
  })

  it(`a child started over MCP takes its parent's value`, () => {
    expect(computerUseAtStart({ viaMcp: true, caller: true, from: true, pick: undefined })).toBe(true)
    expect(computerUseAtStart({ viaMcp: true, caller: true, from: false, pick: undefined })).toBe(false)
  })

  it(`an MCP start with nothing to inherit from is off`, () => {
    expect(computerUseAtStart({ ...agent, caller: null, pick: undefined })).toBe(false)
  })

  it(`an explicit true over MCP where caller and parent follow changes nothing`, () => {
    expect(raisesComputerUse({ ...agent, caller: true, pick: true })).toBe(false)
    expect(computerUseAtStart({ ...agent, caller: true, pick: true })).toBe(true)
    expect(computerUseAtStart({ viaMcp: true, caller: true, from: true, pick: true })).toBe(true)
  })

  // #FEED-302: an explicit true that RAISES it is the owner rule's to decide.
  it(`an explicit true over MCP from an opted-out caller or parent raises it: on with the owner rule, refused without`, () => {
    const raises = [
      { ...agent, caller: false, pick: true },
      { ...agent, caller: null, pick: true },
      { viaMcp: true, caller: true, from: false, pick: true },
      { viaMcp: true, caller: false, from: true, pick: true },
    ]
    for (const start of raises) {
      expect(raisesComputerUse(start)).toBe(true)
      expect(computerUseAtStart({ ...start, ownerStart: true })).toBe(true)
      expect(() => computerUseAtStart(start)).toThrow(
        start.caller === null ? COMPUTER_USE_REFUSALS.caller : COMPUTER_USE_REFUSALS.owner
      )
    }
  })

  it(`nothing but an explicit true over MCP raises it`, () => {
    expect(raisesComputerUse({ ...agent, caller: false, pick: undefined })).toBe(false)
    expect(raisesComputerUse({ ...agent, caller: false, pick: false })).toBe(false)
    expect(raisesComputerUse({ ...person, pick: true })).toBe(false)
    // The owner rule never turns an omitted key or an opt-out on.
    expect(computerUseAtStart({ ...agent, caller: false, pick: undefined, ownerStart: true })).toBe(false)
    expect(computerUseAtStart({ ...agent, caller: true, pick: false, ownerStart: true })).toBe(false)
  })
})

// #FEED-302 (composer-menu.json `computerUse.agentStart`).
describe(`computer use: a session starts another with it over MCP`, () => {
  const spec = composerMenu.computerUse.agentStart

  for (const c of spec.cases) {
    it(c.name, () => {
      expect(agentStartVerdict(c)).toBe(c.expected)
    })
  }

  it(`every refusal the cases name has its sentence, and only the device's switch allows`, () => {
    for (const c of spec.cases) {
      if (c.expected !== `allowed`) expect(COMPUTER_USE_REFUSALS[c.expected as keyof typeof COMPUTER_USE_REFUSALS]).toBeTruthy()
      if (c.expected === `allowed`) expect(c.switchOn && c.caller && c.ownTree && c.ownDevice).toBe(true)
    }
    for (const caller of [true, false]) {
      for (const ownTree of [true, false]) {
        for (const ownDevice of [true, false]) {
          expect(agentStartVerdict({ caller, ownTree, ownDevice, switchOn: false })).not.toBe(`allowed`)
        }
      }
    }
  })

  // #EXP-2006: the sentences name controls that exist: the start key and the
  // device's switch by its label, never a session menu.
  it(`the refusals point at the start key and the device's switch`, () => {
    expect(COMPUTER_USE_REFUSALS.running).toContain(`${composerMenu.computerUse.payloadKey}: true`)
    expect(COMPUTER_USE_REFUSALS.switch).toContain(`${deviceDoctor.labels.computer_use} switch`)
    for (const sentence of Object.values(COMPUTER_USE_REFUSALS)) expect(sentence).not.toMatch(/menu/i)
  })

  it(`the log line names the calling session`, () => {
    expect(computerUseStartLog(`abc`)).toBe(spec.log.replace(`{session}`, `abc`))
    expect(computerUseStartLog(`abc`)).toContain(`abc`)
  })
})

describe(`computer use: a pick for an existing session`, () => {
  it(`a person sets both ways`, () => {
    expect(() => assertComputerUsePick({ pick: true, viaMcp: false, caller: null, target: false })).not.toThrow()
    expect(() => assertComputerUsePick({ pick: false, viaMcp: false, caller: null, target: true })).not.toThrow()
  })

  it(`over MCP false always works and true never raises`, () => {
    expect(() => assertComputerUsePick({ pick: false, viaMcp: true, caller: false, target: true })).not.toThrow()
    expect(() => assertComputerUsePick({ pick: undefined, viaMcp: true, caller: false, target: false })).not.toThrow()
    // On itself, on an opted-out session, from an opted-out caller.
    expect(() => assertComputerUsePick({ pick: true, viaMcp: true, caller: false, target: false })).toThrow(
      COMPUTER_USE_REFUSALS.running
    )
    expect(() => assertComputerUsePick({ pick: true, viaMcp: true, caller: true, target: false })).toThrow(
      COMPUTER_USE_REFUSALS.running
    )
    expect(() => assertComputerUsePick({ pick: true, viaMcp: true, caller: false, target: true })).toThrow(
      COMPUTER_USE_REFUSALS.running
    )
    expect(() => assertComputerUsePick({ pick: true, viaMcp: true, caller: true, target: true })).not.toThrow()
  })
})

describe(`computer use: the fixture names the gate's cap`, () => {
  it(`switchCap sits beside cap`, () => {
    expect(composerMenu.computerUse.switchCap).toBe(`computer-use-switch`)
    expect(COMPUTER_USE_SWITCH_CAP).toBe(composerMenu.computerUse.switchCap)
    expect(composerMenu.computerUse.cap).toBe(`computer-use-run`)
  })
})

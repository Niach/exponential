import { describe, expect, it } from "vitest"
import composerMenu from "@exp/domain-contract/fixtures/composer-menu.json"
import {
  assertComputerUsePick,
  COMPUTER_USE_MCP_REFUSAL,
  COMPUTER_USE_SWITCH_CAP,
  computerUseAtStart,
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

  it(`an explicit true over MCP is refused when the caller or the parent is opted out, else changes nothing`, () => {
    expect(() => computerUseAtStart({ ...agent, caller: false, pick: true })).toThrow(COMPUTER_USE_MCP_REFUSAL)
    expect(() => computerUseAtStart({ ...agent, caller: null, pick: true })).toThrow(COMPUTER_USE_MCP_REFUSAL)
    expect(() => computerUseAtStart({ viaMcp: true, caller: true, from: false, pick: true })).toThrow(
      COMPUTER_USE_MCP_REFUSAL
    )
    expect(computerUseAtStart({ ...agent, caller: true, pick: true })).toBe(true)
    expect(computerUseAtStart({ viaMcp: true, caller: true, from: true, pick: true })).toBe(true)
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
      COMPUTER_USE_MCP_REFUSAL
    )
    expect(() => assertComputerUsePick({ pick: true, viaMcp: true, caller: true, target: false })).toThrow(
      COMPUTER_USE_MCP_REFUSAL
    )
    expect(() => assertComputerUsePick({ pick: true, viaMcp: true, caller: false, target: true })).toThrow(
      COMPUTER_USE_MCP_REFUSAL
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

import { describe, expect, it } from "vitest"
import {
  deriveEntryStates,
  type GettingStartedSignals,
} from "./getting-started-model"

const NONE: GettingStartedSignals = {
  hasDesktopDevice: false,
  hasServerDevice: false,
  githubInstalled: false,
  hasInvitedTeam: false,
  hasBoard: false,
  hasRepoBoard: false,
  hasCodingSession: false,
  helpdeskEnabled: false,
  hasWidget: false,
  mcpConnected: false,
}

const OWNER = { canManageWidgets: true, isOwner: true, canManageMembers: true }
const MEMBER = {
  canManageWidgets: false,
  isOwner: false,
  canManageMembers: false,
}

function stateOf(
  signals: GettingStartedSignals,
  key: string,
  options = OWNER
) {
  const { entries } = deriveEntryStates(signals, options)
  return entries.find((entry) => entry.key === key)
}

describe(`deriveEntryStates`, () => {
  it(`emits the single static order desktop → github → invite → board → coding → server → widget → helpdesk → mcp`, () => {
    const { entries } = deriveEntryStates(NONE, OWNER)
    expect(entries.map((entry) => entry.key)).toEqual([
      `desktop`,
      `github`,
      `invite`,
      `board`,
      `coding`,
      `server`,
      `widget`,
      `helpdesk`,
      `mcp`,
    ])
  })

  it(`starts with everything undone: coding locked on desktop, widget locked on board`, () => {
    const { done, total } = deriveEntryStates(NONE, OWNER)
    expect(done).toBe(0)
    expect(total).toBe(9)
    expect(stateOf(NONE, `coding`)).toEqual({
      key: `coding`,
      state: `locked`,
      lockedBy: `desktop`,
    })
    expect(stateOf(NONE, `widget`)).toEqual({
      key: `widget`,
      state: `locked`,
      lockedBy: `board`,
    })
    // Helpdesk has no prereq — available from the start.
    expect(stateOf(NONE, `helpdesk`)).toEqual({
      key: `helpdesk`,
      state: `available`,
    })
  })

  it(`coding points at github once any machine is registered`, () => {
    const signals = { ...NONE, hasDesktopDevice: true }
    expect(stateOf(signals, `coding`)).toEqual({
      key: `coding`,
      state: `locked`,
      lockedBy: `github`,
    })
  })

  it(`a server-kind machine satisfies the coding device feeder too`, () => {
    const signals = { ...NONE, hasServerDevice: true }
    expect(stateOf(signals, `coding`)).toEqual({
      key: `coding`,
      state: `locked`,
      lockedBy: `github`,
    })
  })

  it(`coding stays locked on the board step once github is connected but no board has a repo`, () => {
    const signals = {
      ...NONE,
      hasDesktopDevice: true,
      githubInstalled: true,
      hasBoard: true,
    }
    expect(stateOf(signals, `coding`)).toEqual({
      key: `coding`,
      state: `locked`,
      lockedBy: `board`,
    })
  })

  it(`coding unlocks with a repo-backed board and a machine`, () => {
    const signals = {
      ...NONE,
      hasDesktopDevice: true,
      githubInstalled: true,
      hasBoard: true,
      hasRepoBoard: true,
    }
    expect(stateOf(signals, `coding`)).toEqual({
      key: `coding`,
      state: `available`,
    })
  })

  it(`coding stays locked on desktop with a repo-backed board but no machine`, () => {
    const signals = {
      ...NONE,
      githubInstalled: true,
      hasBoard: true,
      hasRepoBoard: true,
    }
    expect(stateOf(signals, `coding`)).toEqual({
      key: `coding`,
      state: `locked`,
      lockedBy: `desktop`,
    })
  })

  it(`widget unlocks once any board exists`, () => {
    const signals = { ...NONE, hasBoard: true }
    expect(stateOf(signals, `widget`)).toEqual({
      key: `widget`,
      state: `available`,
    })
  })

  it(`done propagates over locks — an existing signal beats a missing prereq`, () => {
    // A coding session synced from before (e.g. the repo board was trashed,
    // or the device was removed) must still render the green check, never a
    // lock.
    const signals = { ...NONE, hasCodingSession: true, hasWidget: true }
    expect(stateOf(signals, `coding`)).toEqual({ key: `coding`, state: `done` })
    expect(stateOf(signals, `widget`)).toEqual({ key: `widget`, state: `done` })
  })

  it(`simple entries complete from their signals`, () => {
    const signals = {
      ...NONE,
      hasDesktopDevice: true,
      hasServerDevice: true,
      githubInstalled: true,
      hasInvitedTeam: true,
      hasBoard: true,
      helpdeskEnabled: true,
      mcpConnected: true,
    }
    expect(stateOf(signals, `desktop`)?.state).toBe(`done`)
    expect(stateOf(signals, `github`)?.state).toBe(`done`)
    expect(stateOf(signals, `invite`)?.state).toBe(`done`)
    expect(stateOf(signals, `board`)?.state).toBe(`done`)
    expect(stateOf(signals, `server`)?.state).toBe(`done`)
    expect(stateOf(signals, `helpdesk`)?.state).toBe(`done`)
    expect(stateOf(signals, `mcp`)?.state).toBe(`done`)
  })

  it(`a server device completes the server entry but not the desktop one`, () => {
    const signals = { ...NONE, hasServerDevice: true }
    expect(stateOf(signals, `server`)?.state).toBe(`done`)
    expect(stateOf(signals, `desktop`)?.state).toBe(`available`)
  })

  it(`members get 6 entries — invite, widget and helpdesk are hidden`, () => {
    const { entries, total } = deriveEntryStates(NONE, MEMBER)
    expect(total).toBe(6)
    expect(entries.map((entry) => entry.key)).toEqual([
      `desktop`,
      `github`,
      `board`,
      `coding`,
      `server`,
      `mcp`,
    ])
  })

  it(`counts done against the viewer's own total`, () => {
    const signals = {
      ...NONE,
      hasDesktopDevice: true,
      githubInstalled: true,
      hasInvitedTeam: true,
      hasBoard: true,
      hasRepoBoard: true,
      hasCodingSession: true,
      helpdeskEnabled: true,
      mcpConnected: true,
    }
    // Owner: server + widget still open → 7/9. Member: invite, widget and
    // helpdesk hidden, server open → 5/6.
    expect(deriveEntryStates(signals, OWNER)).toMatchObject({
      done: 7,
      total: 9,
    })
    expect(deriveEntryStates(signals, MEMBER)).toMatchObject({
      done: 5,
      total: 6,
    })
  })
})

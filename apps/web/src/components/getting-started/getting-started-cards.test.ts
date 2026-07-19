import { describe, expect, it } from "vitest"
import {
  deriveEntryStates,
  type GettingStartedSignals,
} from "./getting-started-model"

const NONE: GettingStartedSignals = {
  githubInstalled: false,
  hasBoard: false,
  hasRepoBoard: false,
  hasCodingSession: false,
  helpdeskEnabled: false,
  hasWidget: false,
  mcpConnected: false,
}

const OWNER = { canManageWidgets: true, isOwner: true }
const MEMBER = { canManageWidgets: false, isOwner: false }

function stateOf(
  signals: GettingStartedSignals,
  key: string,
  options = OWNER
) {
  const { entries } = deriveEntryStates(signals, options)
  return entries.find((entry) => entry.key === key)
}

describe(`deriveEntryStates`, () => {
  it(`emits the single static order github → board → coding → widget → helpdesk → mcp`, () => {
    const { entries } = deriveEntryStates(NONE, OWNER)
    expect(entries.map((entry) => entry.key)).toEqual([
      `github`,
      `board`,
      `coding`,
      `widget`,
      `helpdesk`,
      `mcp`,
    ])
  })

  it(`starts with everything undone: coding locked on github, widget locked on board`, () => {
    const { done, total } = deriveEntryStates(NONE, OWNER)
    expect(done).toBe(0)
    expect(total).toBe(6)
    expect(stateOf(NONE, `coding`)).toEqual({
      key: `coding`,
      state: `locked`,
      lockedBy: `github`,
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

  it(`coding stays locked on the board step once github is connected but no board has a repo`, () => {
    const signals = { ...NONE, githubInstalled: true, hasBoard: true }
    expect(stateOf(signals, `coding`)).toEqual({
      key: `coding`,
      state: `locked`,
      lockedBy: `board`,
    })
  })

  it(`coding unlocks with a repo-backed board`, () => {
    const signals = {
      ...NONE,
      githubInstalled: true,
      hasBoard: true,
      hasRepoBoard: true,
    }
    expect(stateOf(signals, `coding`)).toEqual({
      key: `coding`,
      state: `available`,
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
    // A coding session synced from before (e.g. the repo board was trashed)
    // must still render the green check, never a lock.
    const signals = { ...NONE, hasCodingSession: true, hasWidget: true }
    expect(stateOf(signals, `coding`)).toEqual({ key: `coding`, state: `done` })
    expect(stateOf(signals, `widget`)).toEqual({ key: `widget`, state: `done` })
  })

  it(`simple entries complete from their signals`, () => {
    const signals = {
      ...NONE,
      githubInstalled: true,
      hasBoard: true,
      helpdeskEnabled: true,
      mcpConnected: true,
    }
    expect(stateOf(signals, `github`)?.state).toBe(`done`)
    expect(stateOf(signals, `board`)?.state).toBe(`done`)
    expect(stateOf(signals, `helpdesk`)?.state).toBe(`done`)
    expect(stateOf(signals, `mcp`)?.state).toBe(`done`)
  })

  it(`members get 4 entries — the owner-only widget and helpdesk ones are hidden`, () => {
    const { entries, total } = deriveEntryStates(NONE, MEMBER)
    expect(total).toBe(4)
    expect(entries.map((entry) => entry.key)).toEqual([
      `github`,
      `board`,
      `coding`,
      `mcp`,
    ])
  })

  it(`counts done against the viewer's own total`, () => {
    const signals = {
      ...NONE,
      githubInstalled: true,
      hasBoard: true,
      hasRepoBoard: true,
      hasCodingSession: true,
      helpdeskEnabled: true,
      mcpConnected: true,
    }
    // Owner: widget still open → 5/6. Member: widget+helpdesk hidden → 4/4.
    expect(deriveEntryStates(signals, OWNER)).toMatchObject({
      done: 5,
      total: 6,
    })
    expect(deriveEntryStates(signals, MEMBER)).toMatchObject({
      done: 4,
      total: 4,
    })
  })
})

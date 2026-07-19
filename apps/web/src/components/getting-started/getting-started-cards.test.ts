import { describe, expect, it } from "vitest"
import {
  deriveEntryStates,
  type GettingStartedSignals,
} from "./getting-started-model"

const NONE: GettingStartedSignals = {
  githubInstalled: false,
  hasProject: false,
  hasRepoProject: false,
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
  it(`emits the single static order github → project → coding → widget → helpdesk → mcp`, () => {
    const { entries } = deriveEntryStates(NONE, OWNER)
    expect(entries.map((entry) => entry.key)).toEqual([
      `github`,
      `project`,
      `coding`,
      `widget`,
      `helpdesk`,
      `mcp`,
    ])
  })

  it(`starts with everything undone: coding locked on github, widget locked on project`, () => {
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
      lockedBy: `project`,
    })
    // Helpdesk has no prereq — available from the start.
    expect(stateOf(NONE, `helpdesk`)).toEqual({
      key: `helpdesk`,
      state: `available`,
    })
  })

  it(`coding stays locked on the project step once github is connected but no project has a repo`, () => {
    const signals = { ...NONE, githubInstalled: true, hasProject: true }
    expect(stateOf(signals, `coding`)).toEqual({
      key: `coding`,
      state: `locked`,
      lockedBy: `project`,
    })
  })

  it(`coding unlocks with a repo-backed project`, () => {
    const signals = {
      ...NONE,
      githubInstalled: true,
      hasProject: true,
      hasRepoProject: true,
    }
    expect(stateOf(signals, `coding`)).toEqual({
      key: `coding`,
      state: `available`,
    })
  })

  it(`widget unlocks once any project exists`, () => {
    const signals = { ...NONE, hasProject: true }
    expect(stateOf(signals, `widget`)).toEqual({
      key: `widget`,
      state: `available`,
    })
  })

  it(`done propagates over locks — an existing signal beats a missing prereq`, () => {
    // A coding session synced from before (e.g. the repo project was trashed)
    // must still render the green check, never a lock.
    const signals = { ...NONE, hasCodingSession: true, hasWidget: true }
    expect(stateOf(signals, `coding`)).toEqual({ key: `coding`, state: `done` })
    expect(stateOf(signals, `widget`)).toEqual({ key: `widget`, state: `done` })
  })

  it(`simple entries complete from their signals`, () => {
    const signals = {
      ...NONE,
      githubInstalled: true,
      hasProject: true,
      helpdeskEnabled: true,
      mcpConnected: true,
    }
    expect(stateOf(signals, `github`)?.state).toBe(`done`)
    expect(stateOf(signals, `project`)?.state).toBe(`done`)
    expect(stateOf(signals, `helpdesk`)?.state).toBe(`done`)
    expect(stateOf(signals, `mcp`)?.state).toBe(`done`)
  })

  it(`members get 4 entries — the owner-only widget and helpdesk ones are hidden`, () => {
    const { entries, total } = deriveEntryStates(NONE, MEMBER)
    expect(total).toBe(4)
    expect(entries.map((entry) => entry.key)).toEqual([
      `github`,
      `project`,
      `coding`,
      `mcp`,
    ])
  })

  it(`counts done against the viewer's own total`, () => {
    const signals = {
      ...NONE,
      githubInstalled: true,
      hasProject: true,
      hasRepoProject: true,
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

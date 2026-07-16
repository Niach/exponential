import { describe, expect, it } from "vitest"
import {
  deriveEntryStates,
  gettingStartedEntryOrder,
  type GettingStartedSignals,
} from "./getting-started-model"

const NONE: GettingStartedSignals = {
  githubInstalled: false,
  hasProject: false,
  hasRepoProject: false,
  hasCodingSession: false,
  hasPublicProject: false,
  hasHelpdeskProject: false,
  hasWidget: false,
  mcpConnected: false,
}

function stateOf(
  signals: GettingStartedSignals,
  key: string,
  canManageWidgets = true
) {
  const { entries } = deriveEntryStates(signals, { canManageWidgets })
  return entries.find((entry) => entry.key === key)
}

describe(`gettingStartedEntryOrder`, () => {
  it(`leads with the feedback track on public boards`, () => {
    expect(gettingStartedEntryOrder(true)).toEqual([
      `feedback-board`,
      `widget`,
      `mcp`,
      `github`,
      `project`,
      `coding`,
    ])
  })

  it(`leads with the coding track everywhere else`, () => {
    const expected = [
      `github`,
      `project`,
      `coding`,
      `feedback-board`,
      `widget`,
      `mcp`,
    ]
    expect(gettingStartedEntryOrder(false)).toEqual(expected)
    expect(gettingStartedEntryOrder(undefined)).toEqual(expected)
  })
})

describe(`deriveEntryStates`, () => {
  it(`starts with everything undone: coding locked on github, widget locked on project`, () => {
    const { entries, done, total } = deriveEntryStates(NONE, {
      canManageWidgets: true,
    })
    expect(done).toBe(0)
    expect(total).toBe(6)
    expect(entries.map((entry) => entry.key)).toEqual([
      `github`,
      `project`,
      `coding`,
      `feedback-board`,
      `widget`,
      `mcp`,
    ])
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
      hasPublicProject: true,
      mcpConnected: true,
    }
    expect(stateOf(signals, `github`)?.state).toBe(`done`)
    expect(stateOf(signals, `project`)?.state).toBe(`done`)
    expect(stateOf(signals, `feedback-board`)?.state).toBe(`done`)
    expect(stateOf(signals, `mcp`)?.state).toBe(`done`)
  })

  it(`members (no widget management) get 5 entries — the widget one is hidden`, () => {
    const { entries, total } = deriveEntryStates(NONE, {
      canManageWidgets: false,
    })
    expect(total).toBe(5)
    expect(entries.some((entry) => entry.key === `widget`)).toBe(false)
  })

  it(`counts done against the viewer's own total`, () => {
    const signals = {
      ...NONE,
      githubInstalled: true,
      hasProject: true,
      hasRepoProject: true,
      hasCodingSession: true,
      hasPublicProject: true,
      mcpConnected: true,
    }
    // Owner: widget still open → 5/6. Member: widget hidden → 5/5.
    expect(deriveEntryStates(signals, { canManageWidgets: true })).toMatchObject(
      { done: 5, total: 6 }
    )
    expect(
      deriveEntryStates(signals, { canManageWidgets: false })
    ).toMatchObject({ done: 5, total: 5 })
  })
})

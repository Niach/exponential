import { describe, expect, it } from "vitest"
import {
  isChatSession,
  sessionIdentity,
  tabPhaseLabel,
} from "@/lib/session-identity"

// EXP-740: the pure half of "what is this run" — the dock strip, the session
// route and the chat page all read it, so it is pinned here rather than
// re-derived per surface.

function session(over: {
  issueId?: string | null
  actionId?: string | null
  actionName?: string | null
  status?: string
  needsInput?: boolean
}) {
  return {
    issueId: over.issueId ?? null,
    actionId: over.actionId ?? null,
    actionName: over.actionName ?? null,
    status: over.status ?? `running`,
    needsInput: over.needsInput ?? false,
  } as never
}

describe(`isChatSession`, () => {
  it(`is the reserved name on an issue-less, action-less row`, () => {
    expect(isChatSession(session({ actionName: `Chat` }))).toBe(true)
  })

  it(`is not a batch run`, () => {
    expect(isChatSession(session({}))).toBe(false)
  })

  it(`is not an action run that happens to be NAMED Chat`, () => {
    // A team action called "Chat" carries a real `action_id`; only the hidden
    // builtin runs with none.
    expect(
      isChatSession(session({ actionId: `a1`, actionName: `Chat` }))
    ).toBe(false)
  })

  it(`is not an issue run`, () => {
    expect(
      isChatSession(session({ issueId: `i1`, actionName: `Chat` }))
    ).toBe(false)
  })
})

describe(`sessionIdentity`, () => {
  const issue = { identifier: `EXP-1`, title: `  Ship it  ` }

  it(`leads with the issue`, () => {
    expect(sessionIdentity({ session: session({ issueId: `i1` }), issue })).toEqual(
      { identifier: `EXP-1`, subject: `Ship it` }
    )
  })

  it(`names an untitled issue`, () => {
    expect(
      sessionIdentity({
        session: session({ issueId: `i1` }),
        issue: { identifier: `EXP-2`, title: `   ` },
      })
    ).toEqual({ identifier: `EXP-2`, subject: `Untitled issue` })
  })

  it(`names a chat run Chat`, () => {
    expect(
      sessionIdentity({ session: session({ actionName: `Chat` }), issue: undefined })
    ).toEqual({ identifier: null, subject: `Chat` })
  })

  it(`keeps an action's name snapshot`, () => {
    expect(
      sessionIdentity({
        session: session({ actionId: `a1`, actionName: `Nightly triage` }),
        issue: undefined,
      })
    ).toEqual({ identifier: null, subject: `Nightly triage` })
  })

  it(`falls back to Batch run`, () => {
    expect(sessionIdentity({ session: session({}), issue: undefined })).toEqual({
      identifier: null,
      subject: `Batch run`,
    })
  })

  it(`says so while the issue is still syncing`, () => {
    expect(
      sessionIdentity({ session: session({ issueId: `i1` }), issue: undefined })
    ).toEqual({ identifier: null, subject: `Issue syncing…` })
  })
})

describe(`tabPhaseLabel`, () => {
  const device = { label: `macbook` }

  it(`names the offline machine first`, () => {
    expect(
      tabPhaseLabel({ session: session({}), device, paused: true })
    ).toBe(`Paused · macbook is offline`)
  })

  it(`falls back when the run never named a device`, () => {
    expect(
      tabPhaseLabel({ session: session({}), device: { label: null }, paused: true })
    ).toBe(`Paused · the device is offline`)
  })

  it(`reports an ended run`, () => {
    expect(
      tabPhaseLabel({
        session: session({ status: `ended` }),
        device,
        paused: false,
      })
    ).toBe(`Session ended`)
  })

  it(`beats live with needs-input`, () => {
    expect(
      tabPhaseLabel({
        session: session({ needsInput: true }),
        device,
        paused: false,
      })
    ).toBe(`Needs your input · macbook`)
  })

  it(`is live otherwise`, () => {
    expect(tabPhaseLabel({ session: session({}), device, paused: false })).toBe(
      `Live · macbook`
    )
    expect(
      tabPhaseLabel({ session: session({}), device: { label: null }, paused: false })
    ).toBe(`Live`)
  })
})

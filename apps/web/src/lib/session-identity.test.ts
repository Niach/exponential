import { describe, expect, it } from "vitest"
import { sessionIdentity, tabPhaseLabel } from "@/lib/session-identity"

const base = { title: null, actionName: null, name: null }

describe(`sessionIdentity`, () => {
  it(`prefers the title, then the action, the agent name, the first issue`, () => {
    const issue = { identifier: `EXP-1`, title: `Login` }
    expect(sessionIdentity({ session: { ...base, title: `Mine` }, issue })).toEqual({
      identifier: `EXP-1`,
      subject: `Mine`,
    })
    expect(sessionIdentity({ session: { ...base, actionName: `Tidy` }, issue }).subject).toBe(
      `Tidy`
    )
    expect(sessionIdentity({ session: { ...base, name: `Ada` }, issue: undefined })).toEqual({
      identifier: null,
      subject: `Ada`,
    })
    expect(sessionIdentity({ session: base, issue }).subject).toBe(`Login`)
    expect(sessionIdentity({ session: base, issue: undefined }).subject).toBe(`Untitled`)
  })
})

describe(`tabPhaseLabel`, () => {
  const device = { label: `mac` }
  it(`names the phase`, () => {
    const s = (status: string, needsInput = false) =>
      ({ status, needsInput }) as never
    expect(tabPhaseLabel({ session: s(`running`), device, paused: true })).toBe(
      `Paused · mac is offline`
    )
    expect(tabPhaseLabel({ session: s(`ended`), device, paused: false })).toBe(`Session ended`)
    expect(tabPhaseLabel({ session: s(`idle`, true), device, paused: false })).toBe(
      `Needs your input · mac`
    )
    expect(tabPhaseLabel({ session: s(`queued`), device, paused: false })).toBe(`Queued · mac`)
    expect(tabPhaseLabel({ session: s(`idle`), device, paused: false })).toBe(`Live · mac`)
  })
})

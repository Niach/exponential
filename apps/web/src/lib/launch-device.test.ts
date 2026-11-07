import { describe, expect, it, vi } from "vitest"
import {
  defaultCandidateId,
  loadEditorDevice,
  resolveLaunchDeviceId,
  saveEditorDevice,
} from "@/lib/launch-device"

// EXP-836: the launch surfaces' device precedence, as a pure rule.
describe(`resolveLaunchDeviceId`, () => {
  const mac = { deviceId: `mac`, isDefault: true }
  const mint = { deviceId: `mint` }
  const pi = { deviceId: `pi` }

  it(`lets an explicit request win over the default machine and a pick`, () => {
    expect(
      resolveLaunchDeviceId([mac, mint], { requested: `mint` })
    ).toBe(`mint`)
    expect(
      resolveLaunchDeviceId([mac, mint], { requested: `mint`, picked: `mac` })
    ).toBe(`mint`)
  })

  it(`falls back while the requested machine is not a candidate`, () => {
    // The devices shape has not hydrated it yet (or it went offline): the
    // fallback renders, and the caller keeps the request so the machine wins
    // the moment it appears.
    expect(resolveLaunchDeviceId([mac, pi], { requested: `mint` })).toBe(`mac`)
    expect(resolveLaunchDeviceId([], { requested: `mint` })).toBeNull()
  })

  it(`prefers a pick over the default, and the default over the first row`, () => {
    expect(resolveLaunchDeviceId([pi, mac], { picked: `pi` })).toBe(`pi`)
    expect(resolveLaunchDeviceId([pi, mac])).toBe(`mac`)
    expect(resolveLaunchDeviceId([pi, mint])).toBe(`pi`)
  })

  it(`drops a stale pick instead of selecting nothing`, () => {
    expect(resolveLaunchDeviceId([pi, mac], { picked: `gone` })).toBe(`mac`)
  })

  it(`reports the flagged default, or null`, () => {
    expect(defaultCandidateId([pi, mac])).toBe(`mac`)
    expect(defaultCandidateId([pi, mint])).toBeNull()
    expect(defaultCandidateId([])).toBeNull()
  })
})

// #FEED-204: the app editor's pick, per app on this browser.
describe(`the app editor's remembered device`, () => {
  it(`keeps one pick per app`, () => {
    // The runner's jsdom does not always ship a working localStorage.
    const store = new Map<string, string>()
    vi.stubGlobal(`localStorage`, {
      getItem: (key: string) => store.get(key) ?? null,
      setItem: (key: string, value: string) => void store.set(key, value),
    })
    expect(loadEditorDevice(`a1`)).toBeNull()
    saveEditorDevice(`a1`, `mint`)
    saveEditorDevice(`a2`, `mac`)
    expect(loadEditorDevice(`a1`)).toBe(`mint`)
    expect(loadEditorDevice(`a2`)).toBe(`mac`)
    vi.unstubAllGlobals()
  })
})

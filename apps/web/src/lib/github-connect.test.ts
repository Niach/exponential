import { describe, expect, it, vi } from "vitest"

vi.mock(`@/lib/auth/client`, () => ({ authClient: { linkSocial: vi.fn() } }))

import {
  githubConnectPagePath,
  isSameOriginPath,
  isSignedOutConnectStart,
} from "@/lib/github-connect"

describe(`isSameOriginPath`, () => {
  it(`accepts a path on this origin only`, () => {
    expect(isSameOriginPath(`/t/acme/settings/repositories`)).toBe(true)
    expect(isSameOriginPath(`/`)).toBe(true)
    expect(isSameOriginPath(`//evil.example/x`)).toBe(false)
    expect(isSameOriginPath(`https://evil.example/x`)).toBe(false)
    expect(isSameOriginPath(``)).toBe(false)
    expect(isSameOriginPath(null)).toBe(false)
  })

  it(`refuses the paths a browser resolves to another host`, () => {
    const origin = `https://app.example`
    for (const value of [
      `/\\evil.example`,
      `/\\/evil.example`,
      `/\t/evil.example`,
      `/\n/evil.example`,
      `/\r/evil.example`,
      `/\u0000/evil.example`,
    ]) {
      expect([value, isSameOriginPath(value, origin)]).toEqual([value, false])
    }
    expect(isSameOriginPath(`/t/acme?from=/x`, origin)).toBe(true)
    // What window.location.assign would do with the old pass-through.
    expect(new URL(`/\\evil.example`, origin).origin).toBe(`https://evil.example`)
  })
})

describe(`githubConnectPagePath`, () => {
  it(`carries the opener's path as from, same origin only`, () => {
    expect(
      githubConnectPagePath({
        teamId: `t1`,
        returnTo: `popup`,
        from: `/t/acme/settings/repositories?x=1`,
      })
    ).toBe(
      `/integrations/github?team=t1&return=popup&from=%2Ft%2Facme%2Fsettings%2Frepositories%3Fx%3D1`
    )
    expect(githubConnectPagePath({ from: `https://evil.example/` })).toBe(
      `/integrations/github`
    )
  })
})

describe(`isSignedOutConnectStart (compat shim)`, () => {
  it(`is a native start only before GitHub's install redirect`, () => {
    expect(isSignedOutConnectStart(`app`, `?return=app&team=t1`)).toBe(true)
    expect(
      isSignedOutConnectStart(`app`, `?return=app&installation_id=1&setup_action=install`)
    ).toBe(false)
    expect(isSignedOutConnectStart(undefined, `?installation_id=1`)).toBe(false)
    expect(isSignedOutConnectStart(`popup`, `?return=popup`)).toBe(false)
  })
})

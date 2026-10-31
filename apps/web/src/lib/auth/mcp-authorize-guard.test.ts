// @vitest-environment node
import { describe, expect, it, vi } from "vitest"

vi.mock(`@/db/connection`, () => ({ db: {} }))
const { isAppsResource, tagAppsConsent } = await import(`./mcp-authorize-guard`)

// #VAPP-182: an authorize request naming the apps resource lands on the
// consent page with `resource=apps`; every other answer passes untouched.
const authorize = (resource?: string) =>
  new Request(`https://exp.test/api/auth/mcp/authorize?client_id=c&prompt=consent${resource ? `&resource=${encodeURIComponent(resource)}` : ``}`)
const consentRedirect = () =>
  new Response(null, { status: 302, headers: { location: `/auth/consent?consent_code=k&client_id=c&scope=openid`, "set-cookie": `a=b` } })

describe(`mcp authorize: the apps resource (VAPP-182)`, () => {
  it(`recognizes the apps endpoint only`, () => {
    expect(isAppsResource(`https://exp.test/api/mcp/apps`)).toBe(true)
    expect(isAppsResource(`https://exp.test/api/mcp/apps/`)).toBe(true)
    expect(isAppsResource(`https://exp.test/api/mcp`)).toBe(false)
    expect(isAppsResource(`not a url`)).toBe(false)
    expect(isAppsResource(null)).toBe(false)
  })

  it(`tags the consent redirect of an apps request, keeping its cookie`, () => {
    const tagged = tagAppsConsent(authorize(`https://exp.test/api/mcp/apps`), consentRedirect())
    expect(tagged.status).toBe(302)
    const location = new URL(tagged.headers.get(`location`)!, `https://exp.test`)
    expect(location.pathname).toBe(`/auth/consent`)
    expect(location.searchParams.get(`resource`)).toBe(`apps`)
    expect(location.searchParams.get(`consent_code`)).toBe(`k`)
    expect(tagged.headers.get(`set-cookie`)).toBe(`a=b`)
  })

  it(`leaves the builtin resource and non-consent redirects alone`, () => {
    const builtin = consentRedirect()
    expect(tagAppsConsent(authorize(`https://exp.test/api/mcp`), builtin)).toBe(builtin)
    const callback = new Response(null, { status: 302, headers: { location: `http://localhost/cb?code=x` } })
    expect(tagAppsConsent(authorize(`https://exp.test/api/mcp/apps`), callback)).toBe(callback)
  })
})

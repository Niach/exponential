import { describe, expect, it } from "vitest"
import { currentAuthPath, withCurrentAuthPath } from "./legacy-auth-paths"

describe(`1.6 auth path aliases`, () => {
  it(`maps the MCP OAuth endpoints onto oauth2`, () => {
    expect(currentAuthPath(`/api/auth/mcp/authorize`)).toBe(`/api/auth/oauth2/authorize`)
    expect(currentAuthPath(`/api/auth/mcp/token`)).toBe(`/api/auth/oauth2/token`)
    expect(currentAuthPath(`/api/auth/mcp/register`)).toBe(`/api/auth/oauth2/register`)
    expect(currentAuthPath(`/api/auth/mcp/userinfo`)).toBe(`/api/auth/oauth2/userinfo`)
  })

  it(`maps the generic OIDC callback onto the shared one`, () => {
    expect(currentAuthPath(`/api/auth/oauth2/callback/authentik`)).toBe(`/api/auth/callback/authentik`)
  })

  it(`leaves every other path alone`, () => {
    expect(currentAuthPath(`/api/auth/oauth2/token`)).toBeNull()
    expect(currentAuthPath(`/api/auth/callback/github`)).toBeNull()
    expect(currentAuthPath(`/api/auth/mcp/get-session`)).toBeNull()
  })

  it(`re-addresses a POST with its body and query`, async () => {
    const original = new Request(`https://exp.test/api/auth/mcp/token?x=1`, {
      method: `POST`,
      headers: { "content-type": `application/x-www-form-urlencoded` },
      body: `grant_type=refresh_token&refresh_token=r`,
    })
    const moved = withCurrentAuthPath(original)
    expect(moved.url).toBe(`https://exp.test/api/auth/oauth2/token?x=1`)
    expect(moved.method).toBe(`POST`)
    expect(moved.headers.get(`content-type`)).toBe(`application/x-www-form-urlencoded`)
    expect(await moved.text()).toBe(`grant_type=refresh_token&refresh_token=r`)
  })
})

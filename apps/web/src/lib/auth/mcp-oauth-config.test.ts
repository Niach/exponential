import { describe, expect, it } from "vitest"
import { LEGACY_PROTECTED_RESOURCE_PATH, legacyProtectedResourceResponse } from "./mcp-oauth-config"

const env = { BETTER_AUTH_URL: `https://exp.test` } as NodeJS.ProcessEnv

describe(`the 1.6 protected-resource metadata URL`, () => {
  it(`answers what /api/mcp's 401 names, with the builtin resource`, async () => {
    const res = legacyProtectedResourceResponse(LEGACY_PROTECTED_RESOURCE_PATH, env)!
    expect(res.status).toBe(200)
    expect(res.headers.get(`access-control-allow-origin`)).toBe(`*`)
    expect(await res.json()).toEqual({
      resource: `https://exp.test/api/mcp`,
      authorization_servers: [`https://exp.test/api/auth`],
      scopes_supported: [`openid`, `profile`, `email`, `offline_access`],
      bearer_methods_supported: [`header`],
    })
  })

  it(`is that one path only`, () => {
    expect(legacyProtectedResourceResponse(`${LEGACY_PROTECTED_RESOURCE_PATH}/`, env)).not.toBeNull()
    expect(legacyProtectedResourceResponse(`/api/auth/get-session`, env)).toBeNull()
    expect(legacyProtectedResourceResponse(`/.well-known/oauth-protected-resource`, env)).toBeNull()
  })
})

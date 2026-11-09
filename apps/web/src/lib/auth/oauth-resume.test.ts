import { describe, expect, it } from "vitest"
import { oauthResumeUrlFor } from "./oauth-resume"

describe(`oauthResumeUrlFor`, () => {
  it(`resumes the authorize request without the provider's signing params`, () => {
    const url = oauthResumeUrlFor(
      `?response_type=code&client_id=c&redirect_uri=http%3A%2F%2Flocalhost%3A1%2Fcb&state=s&code_challenge=x&code_challenge_method=S256&resource=https%3A%2F%2Fexp.test%2Fapi%2Fmcp&exp=1&ba_iat=2&ba_param=client_id&ba_param=sig&sig=zz&redirect=%2F`
    )!
    expect(url.startsWith(`/api/auth/oauth2/authorize?`)).toBe(true)
    const params = new URL(url, `https://exp.test`).searchParams
    expect(Object.fromEntries(params)).toEqual({
      response_type: `code`,
      client_id: `c`,
      redirect_uri: `http://localhost:1/cb`,
      state: `s`,
      code_challenge: `x`,
      code_challenge_method: `S256`,
      resource: `https://exp.test/api/mcp`,
    })
  })

  it(`is not an OAuth resume without client_id and response_type`, () => {
    expect(oauthResumeUrlFor(`?redirect=%2Ft%2Facme`)).toBeNull()
    expect(oauthResumeUrlFor(`?client_id=c`)).toBeNull()
  })
})

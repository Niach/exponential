// @vitest-environment node
import { beforeEach, describe, expect, it, vi } from "vitest"

const h = vi.hoisted(() => ({
  getAccessToken: vi.fn(),
  accountRows: [] as Array<{ accountId: string; createdAt: Date }>,
}))

vi.mock(`@/lib/auth`, () => ({
  auth: { api: { getAccessToken: h.getAccessToken } },
}))

vi.mock(`@/db/connection`, () => ({
  db: {
    select: () => ({
      from: () => ({ where: () => ({ limit: async () => h.accountRows }) }),
    }),
  },
}))

import {
  githubTokensFromRefreshResponse,
  githubUserInfo,
  githubUserToken,
  refreshGithubAccessToken,
} from "./github-user"

describe(`githubTokensFromRefreshResponse`, () => {
  const now = new Date(`2026-10-04T00:00:00Z`)

  it(`maps GitHub's answer to Better Auth's token shape`, () => {
    expect(
      githubTokensFromRefreshResponse(
        {
          access_token: `ghu_new`,
          expires_in: 28800,
          refresh_token: `ghr_new`,
          refresh_token_expires_in: 15811200,
          scope: ``,
          token_type: `bearer`,
        },
        now
      )
    ).toEqual({
      tokenType: `bearer`,
      accessToken: `ghu_new`,
      refreshToken: `ghr_new`,
      accessTokenExpiresAt: new Date(now.getTime() + 28800 * 1000),
      refreshTokenExpiresAt: new Date(now.getTime() + 15811200 * 1000),
      scopes: undefined,
    })
  })

  it(`throws on GitHub's 200 + error answer (a used refresh token)`, () => {
    expect(() =>
      githubTokensFromRefreshResponse({
        error: `bad_refresh_token`,
        error_description: `The refresh token passed is incorrect or expired.`,
      })
    ).toThrow(/bad_refresh_token/)
  })

  it(`throws when no access token came back`, () => {
    expect(() => githubTokensFromRefreshResponse({})).toThrow()
    expect(() => githubTokensFromRefreshResponse(null)).toThrow()
  })
})

describe(`refreshGithubAccessToken`, () => {
  it(`posts the refresh grant and throws on a 200 error body`, async () => {
    const fetchMock = vi.fn(async (..._args: unknown[]) =>
      Response.json({ error: `bad_refresh_token` })
    )
    vi.stubGlobal(`fetch`, fetchMock)
    try {
      await expect(
        refreshGithubAccessToken(`ghr_old`, { clientId: `cid`, clientSecret: `sec` })
      ).rejects.toThrow(/bad_refresh_token/)
      const [url, init] = fetchMock.mock.calls[0] as [string, RequestInit]
      expect(url).toBe(`https://github.com/login/oauth/access_token`)
      const body = new URLSearchParams(String(init.body))
      expect(Object.fromEntries(body)).toEqual({
        grant_type: `refresh_token`,
        client_id: `cid`,
        client_secret: `sec`,
        refresh_token: `ghr_old`,
      })
      expect((init.headers as Record<string, string>).accept).toBe(`application/json`)
    } finally {
      vi.unstubAllGlobals()
    }
  })
})

describe(`githubUserInfo`, () => {
  const profile = { id: 7, login: `octo`, name: null, email: `public@example.com` }

  it(`takes the primary VERIFIED address`, () => {
    expect(
      githubUserInfo(profile, [
        { email: `other@example.com`, primary: false, verified: true },
        { email: `main@example.com`, primary: true, verified: true },
      ])
    ).toMatchObject({ id: `7`, name: `octo`, email: `main@example.com`, emailVerified: true })
  })

  it(`drops an unverified primary: no email, so no implicit linking`, () => {
    expect(
      githubUserInfo(profile, [
        { email: `victim@example.com`, primary: true, verified: false },
        { email: `other@example.com`, primary: false, verified: true },
      ])
    ).toMatchObject({ email: null, emailVerified: false })
  })

  it(`falls back to the public (always verified) email when the list is unreadable`, () => {
    expect(githubUserInfo(profile, null)).toMatchObject({
      email: `public@example.com`,
      emailVerified: true,
    })
    expect(githubUserInfo({ ...profile, email: null }, null)).toMatchObject({
      email: null,
      emailVerified: false,
    })
  })
})

describe(`githubUserToken single-flight`, () => {
  beforeEach(() => {
    h.getAccessToken.mockReset()
    h.accountRows = [{ accountId: `7`, createdAt: new Date() }]
  })

  it(`concurrent callers for one user share ONE refresh`, async () => {
    let release!: (value: { accessToken: string }) => void
    h.getAccessToken.mockImplementation(
      () => new Promise((resolve) => (release = resolve))
    )
    const first = githubUserToken(`u1`)
    const second = githubUserToken(`u1`)
    expect(second).toBe(first)
    await vi.waitFor(() => expect(h.getAccessToken).toHaveBeenCalledTimes(1))
    release({ accessToken: `ghu_1` })
    expect(await first).toEqual({ state: `ok`, token: `ghu_1` })
    expect(await second).toEqual({ state: `ok`, token: `ghu_1` })

    // Settled: the next call looks up again.
    h.getAccessToken.mockResolvedValue({ accessToken: `ghu_2` })
    expect(await githubUserToken(`u1`)).toEqual({ state: `ok`, token: `ghu_2` })
    expect(h.getAccessToken).toHaveBeenCalledTimes(2)
  })

  it(`different users do not share, and a failure clears the entry`, async () => {
    h.getAccessToken.mockRejectedValueOnce(new Error(`bad_refresh_token`))
    expect(await githubUserToken(`u2`)).toEqual({ state: `dead` })
    h.getAccessToken.mockResolvedValue({ accessToken: `ghu_3` })
    const [a, b] = await Promise.all([githubUserToken(`u2`), githubUserToken(`u3`)])
    expect(a).toEqual({ state: `ok`, token: `ghu_3` })
    expect(b).toEqual({ state: `ok`, token: `ghu_3` })
    expect(h.getAccessToken).toHaveBeenCalledTimes(3)
  })
})

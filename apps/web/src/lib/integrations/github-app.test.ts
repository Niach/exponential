import { describe, expect, it, vi } from "vitest"

import { resolveInstallationTokenWith } from "@/lib/integrations/github-app"

// resolveInstallationTokenWith is the pure resolution policy behind
// resolveRepoInstallationToken — GitHub's two round-trips (per-repo installation
// lookup + token mint) are injected, so it can be exercised without a real App
// JWT. These cases pin the fallback behavior that fixes EXP-16's spurious 412.
describe(`resolveInstallationTokenWith`, () => {
  const repo = `Niach/exponential`

  it(`mints against the live installation id and ignores the fallback on a hit`, async () => {
    const resolveId = vi.fn(async () => 42)
    const mintToken = vi.fn(async () => `tok-live`)

    const token = await resolveInstallationTokenWith(repo, 7, {
      resolveId,
      mintToken,
    })

    expect(token).toBe(`tok-live`)
    expect(mintToken).toHaveBeenCalledTimes(1)
    expect(mintToken).toHaveBeenCalledWith(42)
  })

  it(`falls back to the stored installation id when the live lookup 404s`, async () => {
    // GitHub's per-repo endpoint reports "not installed" (null) even though the
    // App still covers the repo through the installation persisted at connect
    // time — mint against that instead of 412ing.
    const resolveId = vi.fn(async () => null)
    const mintToken = vi.fn(async () => `tok-fallback`)

    const token = await resolveInstallationTokenWith(repo, 7, {
      resolveId,
      mintToken,
    })

    expect(token).toBe(`tok-fallback`)
    expect(mintToken).toHaveBeenCalledTimes(1)
    expect(mintToken).toHaveBeenCalledWith(7)
  })

  it(`returns null when the live lookup misses and there is no fallback`, async () => {
    const resolveId = vi.fn(async () => null)
    const mintToken = vi.fn(async () => `never`)

    const token = await resolveInstallationTokenWith(repo, null, {
      resolveId,
      mintToken,
    })

    expect(token).toBeNull()
    expect(mintToken).not.toHaveBeenCalled()
  })

  it(`returns null (not a throw) when the fallback mint fails — App genuinely lost access`, async () => {
    const resolveId = vi.fn(async () => null)
    const mintToken = vi.fn(async () => {
      throw new Error(`GitHub installation token failed (404)`)
    })

    const token = await resolveInstallationTokenWith(repo, 7, {
      resolveId,
      mintToken,
    })

    expect(token).toBeNull()
  })

  it(`propagates a throw on the LIVE mint (a transient error must not read as "not installed")`, async () => {
    const resolveId = vi.fn(async () => 42)
    const mintToken = vi.fn(async () => {
      throw new Error(`GitHub installation token failed (500)`)
    })

    await expect(
      resolveInstallationTokenWith(repo, 7, { resolveId, mintToken })
    ).rejects.toThrow(/500/)
  })
})

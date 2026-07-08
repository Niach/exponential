import { describe, expect, it, vi } from "vitest"

import { resolveInstallationTokenWith } from "@/lib/integrations/github-app"

// resolveInstallationTokenWith is the pure resolution policy behind
// resolveRepoInstallationToken — GitHub's round-trips (per-repo installation
// lookup + token mint + repo-access probe) are injected, so it can be exercised
// without a real App JWT. These cases pin the fallback behavior that fixes the
// spurious 412 AND the verification that stops the fallback from minting
// tokens that can't reach a repo removed from the installation's selection.
describe(`resolveInstallationTokenWith`, () => {
  const repo = `Niach/exponential`

  it(`mints against the live installation id and skips the verify probe on a hit`, async () => {
    const resolveId = vi.fn(async () => 42)
    const mintToken = vi.fn(async () => `tok-live`)
    const verifyRepo = vi.fn(async () => true)

    const resolved = await resolveInstallationTokenWith(repo, 7, {
      resolveId,
      mintToken,
      verifyRepo,
    })

    expect(resolved).toEqual({ token: `tok-live`, installationId: 42 })
    expect(mintToken).toHaveBeenCalledTimes(1)
    expect(mintToken).toHaveBeenCalledWith(42)
    expect(verifyRepo).not.toHaveBeenCalled()
  })

  it(`falls back to the stored installation id when the live lookup 404s and the probe passes`, async () => {
    // GitHub's per-repo endpoint reports "not installed" (null) even though the
    // App still covers the repo through the installation persisted at connect
    // time — mint against that instead of 412ing, verified by the repo probe.
    const resolveId = vi.fn(async () => null)
    const mintToken = vi.fn(async () => `tok-fallback`)
    const verifyRepo = vi.fn(async () => true)

    const resolved = await resolveInstallationTokenWith(repo, 7, {
      resolveId,
      mintToken,
      verifyRepo,
    })

    expect(resolved).toEqual({ token: `tok-fallback`, installationId: 7 })
    expect(mintToken).toHaveBeenCalledTimes(1)
    expect(mintToken).toHaveBeenCalledWith(7)
    expect(verifyRepo).toHaveBeenCalledWith(repo, `tok-fallback`)
  })

  it(`returns null when the fallback token can't reach the repo (really removed)`, async () => {
    // The repo was dropped from the installation's "Only select repositories"
    // set: the mint succeeds (the installation exists) but the token can't
    // access the repo — the old blind fallback handed that token out anyway.
    const resolveId = vi.fn(async () => null)
    const mintToken = vi.fn(async () => `tok-dead`)
    const verifyRepo = vi.fn(async () => false)

    const resolved = await resolveInstallationTokenWith(repo, 7, {
      resolveId,
      mintToken,
      verifyRepo,
    })

    expect(resolved).toBeNull()
  })

  it(`returns the fallback token when the verify probe THROWS (transient error ≠ no access)`, async () => {
    const resolveId = vi.fn(async () => null)
    const mintToken = vi.fn(async () => `tok-fallback`)
    const verifyRepo = vi.fn(async () => {
      throw new Error(`GitHub repo probe failed (500)`)
    })

    const resolved = await resolveInstallationTokenWith(repo, 7, {
      resolveId,
      mintToken,
      verifyRepo,
    })

    expect(resolved).toEqual({ token: `tok-fallback`, installationId: 7 })
  })

  it(`returns null when the live lookup misses and there is no fallback`, async () => {
    const resolveId = vi.fn(async () => null)
    const mintToken = vi.fn(async () => `never`)
    const verifyRepo = vi.fn(async () => true)

    const resolved = await resolveInstallationTokenWith(repo, null, {
      resolveId,
      mintToken,
      verifyRepo,
    })

    expect(resolved).toBeNull()
    expect(mintToken).not.toHaveBeenCalled()
  })

  it(`returns null (not a throw) when the fallback mint fails — App genuinely lost access`, async () => {
    const resolveId = vi.fn(async () => null)
    const mintToken = vi.fn(async () => {
      throw new Error(`GitHub installation token failed (404)`)
    })
    const verifyRepo = vi.fn(async () => true)

    const resolved = await resolveInstallationTokenWith(repo, 7, {
      resolveId,
      mintToken,
      verifyRepo,
    })

    expect(resolved).toBeNull()
    expect(verifyRepo).not.toHaveBeenCalled()
  })

  it(`propagates a throw on the LIVE mint (a transient error must not read as "not installed")`, async () => {
    const resolveId = vi.fn(async () => 42)
    const mintToken = vi.fn(async () => {
      throw new Error(`GitHub installation token failed (500)`)
    })
    const verifyRepo = vi.fn(async () => true)

    await expect(
      resolveInstallationTokenWith(repo, 7, { resolveId, mintToken, verifyRepo })
    ).rejects.toThrow(/500/)
  })
})

import { beforeEach, describe, expect, it, vi } from "vitest"

// One installation resolves for every user; the repos query then dedupes across
// installations. Keeping db + install resolution stable lets the tests isolate
// the per-user repo cache behavior (fresh serve, refresh bypass, invalidation).
const INSTALL_ROWS = [{ installationId: 1, accountLogin: `acme` }]

vi.mock(`@/db/connection`, () => ({
  db: {
    select: () => ({
      from: () => ({
        where: () => Promise.resolve(INSTALL_ROWS),
        limit: () => Promise.resolve(INSTALL_ROWS),
      }),
    }),
  },
}))

const listInstallationRepos = vi.fn(async () => ({
  repos: [
    {
      fullName: `acme/repo`,
      private: false,
      defaultBranch: `main`,
      installationId: 1,
    },
  ],
  hasMore: false,
}))

// resolveInstallations now admin-gates unattributed rows; treat the test
// caller as a regular user with attributed rows.
vi.mock(`@/lib/admin`, () => ({
  isUserAdmin: vi.fn(async () => false),
}))

// Captures the signed setup-state token passed into each minted install URL
// so the platform tests can decode its markers.
const installUrlStates: (string | undefined)[] = []

vi.mock(`@/lib/integrations/github-app`, () => ({
  githubAppConfigured: () => true,
  githubAppInstallUrl: (state?: string) => {
    installUrlStates.push(state)
    return `https://install.example`
  },
  listAppInstallations: vi.fn(async () => []),
  listInstallationRepos: () => listInstallationRepos(),
}))

import {
  integrationsRouter,
  invalidateRepoCache,
} from "@/lib/trpc/integrations"
import {
  githubSetupStateWantsDialog,
  githubSetupStateWantsMobile,
} from "@/lib/integrations/github-setup-state"

 
function callerFor(userId: string) {
  return integrationsRouter.createCaller({
    session: { user: { id: userId } },
     
  } as any)
}

describe(`integrations.github.repos cache`, () => {
  beforeEach(() => {
    listInstallationRepos.mockClear()
  })

  it(`serves a fresh cache entry without re-hitting GitHub`, async () => {
    const caller = callerFor(`user-fresh`)
    await caller.github.repos()
    expect(listInstallationRepos).toHaveBeenCalledTimes(1)

    // Second call within the TTL is served from cache — GitHub not re-hit.
    await caller.github.repos()
    expect(listInstallationRepos).toHaveBeenCalledTimes(1)
  })

  it(`bypasses the cache when refresh is set`, async () => {
    const caller = callerFor(`user-refresh`)
    await caller.github.repos()
    expect(listInstallationRepos).toHaveBeenCalledTimes(1)

    // refresh: true drops the cached entry before fetching → re-hits GitHub.
    await caller.github.repos({ refresh: true })
    expect(listInstallationRepos).toHaveBeenCalledTimes(2)
  })

  it(`re-hits GitHub after invalidateRepoCache (the setup-redirect path)`, async () => {
    const userId = `user-setup`
    const caller = callerFor(userId)
    await caller.github.repos()
    expect(listInstallationRepos).toHaveBeenCalledTimes(1)

    // The setup route invalidates the user's entry when an install lands.
    invalidateRepoCache(userId)

    await caller.github.repos()
    expect(listInstallationRepos).toHaveBeenCalledTimes(2)
  })
})

describe(`integrations.github.repos install URL platform marker`, () => {
  beforeEach(() => {
    process.env.BETTER_AUTH_SECRET = `test-secret-test-secret-test-secret!`
    installUrlStates.length = 0
  })

  it(`marks the minted state mobile only when platform is "mobile"`, async () => {
    const caller = callerFor(`user-platform`)

    await caller.github.repos({ platform: `mobile` })
    const mobileState = installUrlStates.at(-1) ?? null
    expect(githubSetupStateWantsMobile(mobileState)).toBe(true)
    // The mobile marker rides alongside the dialog flag, never instead of it.
    expect(githubSetupStateWantsDialog(mobileState)).toBe(true)

    // Cached serve (same user, within TTL) still mints per-request, so a web
    // call right after a mobile one must NOT inherit the mobile marker.
    await caller.github.repos()
    const webState = installUrlStates.at(-1) ?? null
    expect(githubSetupStateWantsMobile(webState)).toBe(false)
    expect(githubSetupStateWantsDialog(webState)).toBe(true)

    await caller.github.repos({ platform: `web` })
    expect(githubSetupStateWantsMobile(installUrlStates.at(-1) ?? null)).toBe(
      false
    )
  })
})

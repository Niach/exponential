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

vi.mock(`@/lib/integrations/github-app`, () => ({
  githubAppConfigured: () => true,
  githubAppInstallUrl: () => `https://install.example`,
  listAppInstallations: vi.fn(async () => []),
  listInstallationRepos: () => listInstallationRepos(),
}))

import {
  integrationsRouter,
  invalidateRepoCache,
} from "@/lib/trpc/integrations"

 
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

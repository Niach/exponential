import { beforeEach, describe, expect, it, vi } from "vitest"
import { mintGithubSetupState } from "@/lib/integrations/github-setup-state"

// --- Mocked module boundaries ------------------------------------------------
// Records which tables the route inserts into so a test can assert that the
// forged-installation path never creates a team↔installation LINK.
const insertedTables: string[] = []

vi.mock(`@/db/connection`, () => {
  const chain = () => {
    const c: Record<string, unknown> = {}
    const passthrough = () => c
    c.values = passthrough
    c.onConflictDoUpdate = passthrough
    c.onConflictDoNothing = passthrough
    c.set = passthrough
    c.where = passthrough
    // The installations upsert ends in `.returning(...)`; hand back a row id.
    c.returning = async () => [{ id: `inst-row-1` }]
    // A bare awaited insert (the links path) resolves to nothing.
    c.then = (resolve: (v: unknown) => unknown) => resolve(undefined)
    return c
  }
  return {
    db: {
      insert: (table: { _?: { name?: string } }) => {
        // drizzle table objects don't expose a stable name here, so tag via the
        // schema mock below instead.
        insertedTables.push((table as { __name?: string }).__name ?? `unknown`)
        return chain()
      },
    },
  }
})

vi.mock(`@/db/schema`, () => ({
  githubInstallations: { __name: `github_installations` },
  githubInstallationLinks: { __name: `github_installation_links` },
}))

const githubOAuthConfigured = vi.fn(() => true)
vi.mock(`@/lib/integrations/github-app`, () => ({
  getInstallation: async () => ({ account: `Victim`, accountType: `User` }),
  githubOAuthConfigured: () => githubOAuthConfigured(),
  githubOAuthAuthorizeUrl: (state?: string) =>
    state ? `https://github.com/login/oauth/authorize?state=${state}` : null,
}))

const resolveSessionUserId = vi.fn(async () => `attacker` as string | null)
vi.mock(`@/lib/auth/resolve-bearer`, () => ({
  resolveSessionUserId: () => resolveSessionUserId(),
}))

const assertCanManageRepos = vi.fn(async () => {})
vi.mock(`@/lib/trpc/integrations`, () => ({
  assertCanManageRepos: () => assertCanManageRepos(),
  invalidateRepoCache: () => {},
  invalidateRepoCacheForInstallation: async () => {},
}))

// Imported AFTER the mocks are registered.
const { handleSetup } = await import("./setup")

function setupRequest(state: string, installationId: number): Request {
  const url = new URL(`https://app.example/api/integrations/github/setup`)
  url.searchParams.set(`installation_id`, String(installationId))
  url.searchParams.set(`setup_action`, `install`)
  url.searchParams.set(`state`, state)
  return new Request(url.toString())
}

describe(`github setup route — forged installation_id`, () => {
  beforeEach(() => {
    process.env.BETTER_AUTH_SECRET = `test-secret-test-secret-test-secret!`
    insertedTables.length = 0
    githubOAuthConfigured.mockReturnValue(true)
    resolveSessionUserId.mockResolvedValue(`attacker`)
  })

  it(`does NOT link an unverified installation id when OAuth is configured — it bounces to the OAuth claim`, async () => {
    // The attacker mints a perfectly valid state for THEIR OWN team and
    // pairs it with a stranger's guessable installation id.
    const state = mintGithubSetupState(`attacker`, {
      teamId: `attacker-team`,
    })!
    const res = await handleSetup(setupRequest(state, 144861041))

    // The installation row may be recorded, but NO link is ever created.
    expect(insertedTables).not.toContain(`github_installation_links`)
    // The caller is handed into the proof-of-control OAuth flow instead.
    expect(res.status).toBe(302)
    expect(res.headers.get(`location`)).toContain(
      `github.com/login/oauth/authorize`
    )
  })

  it(`retains the direct link only as the self-hosted fallback (no OAuth secret)`, async () => {
    githubOAuthConfigured.mockReturnValue(false)
    const state = mintGithubSetupState(`owner`, {
      teamId: `own-team`,
    })!
    resolveSessionUserId.mockResolvedValue(`owner`)
    await handleSetup(setupRequest(state, 144861041))
    expect(insertedTables).toContain(`github_installation_links`)
  })
})

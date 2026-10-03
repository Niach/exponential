import { and, eq } from "drizzle-orm"
import { z } from "zod"
import { TRPCError } from "@trpc/server"
import { router, authedProcedure, type Context } from "@/lib/trpc"
import { accounts } from "@/db/auth-schema"
import { assertTeamMember } from "@/lib/team-membership"
import { REPO_FULL_NAME_RE } from "@/lib/repo-full-name"
import {
  fetchUserRepoAccess,
  githubAppConfigured,
  githubAppInstallUrl,
  githubConnectConfigured,
  GithubUserApiError,
  installationIdForRepo,
  installationManageUrl,
  listUserInstallationRepos,
  listUserInstallations,
  type InstallationRepo,
  type UserInstallation,
} from "@/lib/integrations/github-app"
import {
  GITHUB_PROVIDER_ID,
  githubLoginForToken,
  githubUserToken,
} from "@/lib/integrations/github-user"
import { appBaseUrl } from "@/lib/notification-email-policy"
import { assertNotApiKeySession } from "@/lib/auth/api-key-kind"
import { assertNotLastWayIn } from "@/lib/auth/sign-in-methods"
import {
  captureOAuthTokens,
  revokeOAuthTokensBestEffort,
} from "@/lib/auth/oauth-revocation"

// SLOP-7: ONE GitHub flow. A member's GitHub connection is their Better Auth
// `github` account (lib/integrations/github-user.ts); installations and
// repositories are listed LIVE off GitHub with that token — nothing is
// claimed per team, nothing is snapshotted. Connecting a repository (the
// `repositories.add` path, `resolveRepoForConnect` below) proves PUSH access
// with the caller's own token and resolves the App installation that mints
// the repo-scoped tokens every later clone/PR uses. The team's `repositories`
// row is the only thing persisted, and it is the whole authorization for
// token mints: a row exists because a member with push access put it there.
//
// Every output keeps the field names older native builds decode
// (`installed`, `installUrl`, `connectUrl`, `installations[].needsReauth/
// stale/suspended/manageUrl`, `repos[]`); the new fields (`linked`, `login`,
// `needsReconnect`) ride alongside.

// --- The guided page -------------------------------------------------------
// `/integrations/github` is the ONE web step every surface opens: Connect
// GitHub → Install the app → Pick a repository → done. Web surfaces open it
// as a popup over the page they are on (the opener re-probes on focus); a
// native client opens it in the system browser with `return=app`, and its
// done state hands back through `exponential://github-connected`.
export function githubConnectPageUrl(opts: {
  teamId?: string
  boardId?: string
  returnTo?: `app` | `popup`
}): string {
  const params = new URLSearchParams()
  if (opts.teamId) params.set(`team`, opts.teamId)
  if (opts.boardId) params.set(`board`, opts.boardId)
  if (opts.returnTo) params.set(`return`, opts.returnTo)
  const query = params.toString()
  return `${appBaseUrl()}/integrations/github${query ? `?${query}` : ``}`
}

// --- Live discovery, cached per user ---------------------------------------
// GitHub's secondary rate limits bite on fan-out (every picker open, every
// focus re-probe), so each user's installations + repos are held ~60s.
// `refresh` (the picker's Refresh, the page's return from GitHub) bypasses.
const DISCOVERY_TTL_MS = 60_000

interface Discovery {
  login: string | null
  installations: UserInstallation[]
  /** Per installation: the push-able repos and whether the page cap cut
   * the listing. Filled lazily (status asks for installations only). */
  repos: Map<number, { repos: InstallationRepo[]; hasMore: boolean }>
  expiresAt: number
}

const discoveryCache = new Map<string, Discovery>()

export function invalidateGithubDiscovery(userId: string): void {
  discoveryCache.delete(userId)
}

/** What the user's token can tell us. `token` = `none` (not linked) or
 * `dead` (linked, token refused/unrefreshable → reconnect) short-circuit. */
type DiscoveryState =
  | { token: `none` | `dead` }
  | { token: `ok`; value: string; discovery: Discovery }

async function discover(
  userId: string,
  opts: { refresh?: boolean }
): Promise<DiscoveryState> {
  const tokenState = await githubUserToken(userId)
  if (tokenState.state !== `ok`) return { token: tokenState.state }
  const token = tokenState.token
  const cached = discoveryCache.get(userId)
  if (!opts.refresh && cached && cached.expiresAt > Date.now()) {
    return { token: `ok`, value: token, discovery: cached }
  }
  try {
    const [installations, login] = await Promise.all([
      listUserInstallations(token),
      githubLoginForToken(token),
    ])
    const discovery: Discovery = {
      login,
      installations,
      repos: new Map(),
      expiresAt: Date.now() + DISCOVERY_TTL_MS,
    }
    discoveryCache.set(userId, discovery)
    return { token: `ok`, value: token, discovery }
  } catch (err) {
    // A refused token (401) is a dead connection the UI must say so about;
    // anything else is GitHub hiccuping, which the caller surfaces as-is.
    if (err instanceof GithubUserApiError && err.status === 401) {
      discoveryCache.delete(userId)
      return { token: `dead` }
    }
    throw err
  }
}

async function reposOf(
  token: string,
  discovery: Discovery,
  installation: UserInstallation
): Promise<{ repos: InstallationRepo[]; hasMore: boolean }> {
  const cached = discovery.repos.get(installation.id)
  if (cached) return cached
  // A suspended installation lists nothing and would only 403.
  const listed = installation.suspended
    ? { repos: [] as InstallationRepo[], hasMore: false }
    : await listUserInstallationRepos(token, installation.id)
  discovery.repos.set(installation.id, listed)
  return listed
}

function installationSummary(inst: UserInstallation, hasMore = false) {
  return {
    installationId: inst.id,
    accountLogin: inst.account || null,
    accountType: inst.accountType || null,
    manageUrl: installationManageUrl({
      installationId: inst.id,
      accountLogin: inst.account || null,
      accountType: inst.accountType || null,
    }),
    suspended: inst.suspended,
    // Pre-SLOP-7 fields older clients still read: nothing is ever stale or
    // in need of a re-auth now — a dead token is reported on the whole
    // connection (`needsReconnect`), never per installation.
    needsReauth: false,
    stale: false,
    hasMore,
  }
}

type InstallationSummary = ReturnType<typeof installationSummary>

/** The connection half every surface renders, shared by `status` and
 * `repos`. */
function connectionFields(
  input: { teamId: string; mobile: boolean },
  state: DiscoveryState
) {
  const connectUrl = githubConnectPageUrl({
    teamId: input.teamId,
    returnTo: input.mobile ? `app` : undefined,
  })
  return {
    configured: true as const,
    connectConfigured: githubConnectConfigured(),
    linked: state.token !== `none`,
    needsReconnect: state.token === `dead`,
    login: state.token === `ok` ? state.discovery.login : null,
    installed:
      state.token === `ok` && state.discovery.installations.length > 0,
    installUrl: githubAppInstallUrl(),
    connectUrl,
  }
}

function notConfigured() {
  return {
    configured: false as const,
    connectConfigured: false,
    linked: false,
    needsReconnect: false,
    login: null as string | null,
    installed: false,
    installUrl: null as string | null,
    connectUrl: null as string | null,
    installations: [] as InstallationSummary[],
  }
}

// --- The connect gate --------------------------------------------------------
// The ONE check behind every "put this repo into the team" write
// (`repositories.add`, `boards.create{fullName}`, MCP `repositories_add`, the
// picker's by-name lookup): the CALLER's own GitHub token must see the repo
// with push access, and the App must be installed on it (the installation
// is what mints the repo-scoped tokens later). The resolved installation is
// what the row persists — never a client-supplied id.
export interface ResolvedRepo {
  installationId: number
  private: boolean
  defaultBranch: string
}

export async function resolveRepoForConnect(
  userId: string,
  fullName: string
): Promise<ResolvedRepo> {
  if (!githubAppConfigured()) {
    throw new TRPCError({
      code: `PRECONDITION_FAILED`,
      message: `GitHub isn't configured on this server.`,
    })
  }
  const tokenState = await githubUserToken(userId)
  if (tokenState.state === `none`) {
    throw new TRPCError({
      code: `PRECONDITION_FAILED`,
      message: `Connect GitHub first (team settings → Repositories), then try again.`,
    })
  }
  if (tokenState.state === `dead`) {
    throw new TRPCError({
      code: `PRECONDITION_FAILED`,
      message: `Your GitHub connection expired. Reconnect GitHub in team settings → Repositories, then try again.`,
    })
  }
  let access: Awaited<ReturnType<typeof fetchUserRepoAccess>>
  try {
    access = await fetchUserRepoAccess(tokenState.token, fullName)
  } catch (err) {
    if (err instanceof GithubUserApiError && err.status === 401) {
      throw new TRPCError({
        code: `PRECONDITION_FAILED`,
        message: `Your GitHub connection expired. Reconnect GitHub in team settings → Repositories, then try again.`,
      })
    }
    throw err
  }
  if (!access) {
    throw new TRPCError({
      code: `NOT_FOUND`,
      message: `GitHub can't find ${fullName} for your account. Check the name, or ask for access on GitHub.`,
    })
  }
  if (!access.push) {
    throw new TRPCError({
      code: `FORBIDDEN`,
      message: `You need push access to ${fullName} on GitHub to connect it.`,
    })
  }
  const installationId = await installationIdForRepo(fullName)
  if (installationId == null) {
    const owner = fullName.split(`/`)[0]
    throw new TRPCError({
      code: `PRECONDITION_FAILED`,
      message: `The Exponential GitHub App isn't installed on ${fullName}. Install it for ${owner} on GitHub (and grant the repository), then try again.`,
    })
  }
  return {
    installationId,
    private: access.private,
    defaultBranch: access.defaultBranch,
  }
}

// --- Disconnect --------------------------------------------------------------
// Unlinks the caller's GitHub account (the Better Auth row), revoking the
// grant best-effort. Repositories already in the team keep working: their
// tokens mint off the App installation, not off this user token. Honours
// the sign-in-methods rule (EXP-1126): with GitHub login on, the row may be
// the account's only way in.
async function unlinkGithubAccount(ctx: {
  db: Context[`db`]
  session: NonNullable<Context[`session`]>
}): Promise<void> {
  await assertNotApiKeySession(ctx.db, ctx.session)
  const userId = ctx.session.user.id
  const [row] = await ctx.db
    .select({ id: accounts.id })
    .from(accounts)
    .where(
      and(eq(accounts.userId, userId), eq(accounts.providerId, GITHUB_PROVIDER_ID))
    )
    .limit(1)
  if (!row) return
  await assertNotLastWayIn(ctx.db, userId, { providerId: GITHUB_PROVIDER_ID })
  const tokens = (await captureOAuthTokens(ctx.db, userId)).filter(
    (t) => t.providerId === GITHUB_PROVIDER_ID
  )
  await ctx.db.delete(accounts).where(eq(accounts.id, row.id))
  await revokeOAuthTokensBestEffort(tokens)
  invalidateGithubDiscovery(userId)
}

const platformInput = z.enum([`web`, `mobile`]).optional()

export const integrationsRouter = router({
  github: router({
    // The viewer's GitHub connection, for a team context: linked or not, the
    // login, the installations their token can see, and the two hops (the
    // guided page, GitHub's install page). Member-gated. `platform: "mobile"`
    // marks the page URL to hand back through the deep link.
    status: authedProcedure
      .input(z.object({ teamId: z.string().uuid(), platform: platformInput }))
      .query(async ({ ctx, input }) => {
        const userId = ctx.session.user.id
        const mobile = input.platform === `mobile`
        await assertTeamMember(userId, input.teamId)
        if (!githubAppConfigured()) return notConfigured()
        const state = await discover(userId, {})
        return {
          ...connectionFields({ teamId: input.teamId, mobile }, state),
          installations:
            state.token === `ok`
              ? state.discovery.installations.map((inst) =>
                  installationSummary(inst)
                )
              : ([] as InstallationSummary[]),
        }
      }),

    // The repositories the viewer may connect: every push-able repo of every
    // installation their token sees, deduped and sorted. Backs the pickers.
    // `refresh` bypasses the cache (back from a GitHub hop).
    repos: authedProcedure
      .input(
        z.object({
          teamId: z.string().uuid(),
          refresh: z.boolean().optional(),
          platform: platformInput,
        })
      )
      .query(async ({ ctx, input }) => {
        const userId = ctx.session.user.id
        const mobile = input.platform === `mobile`
        await assertTeamMember(userId, input.teamId)
        if (!githubAppConfigured()) {
          return {
            ...notConfigured(),
            repos: [] as InstallationRepo[],
            hasMore: false,
          }
        }
        const state = await discover(userId, { refresh: input.refresh })
        const connection = connectionFields({ teamId: input.teamId, mobile }, state)
        if (state.token !== `ok`) {
          return {
            ...connection,
            repos: [] as InstallationRepo[],
            hasMore: false,
            installations: [] as InstallationSummary[],
          }
        }
        const seen = new Set<string>()
        const merged: InstallationRepo[] = []
        let hasMore = false
        const installations: InstallationSummary[] = []
        for (const inst of state.discovery.installations) {
          let listed: { repos: InstallationRepo[]; hasMore: boolean }
          try {
            listed = await reposOf(state.value, state.discovery, inst)
          } catch (err) {
            if (err instanceof GithubUserApiError && err.status === 401) {
              invalidateGithubDiscovery(userId)
              return {
                ...connection,
                linked: true,
                needsReconnect: true,
                installed: false,
                repos: [] as InstallationRepo[],
                hasMore: false,
                installations: [] as InstallationSummary[],
              }
            }
            // One account GitHub cannot list right now must not blank the
            // rest of the picker.
            listed = { repos: [], hasMore: false }
          }
          if (listed.hasMore) hasMore = true
          for (const repo of listed.repos) {
            if (seen.has(repo.fullName)) continue
            seen.add(repo.fullName)
            merged.push(repo)
          }
          installations.push(installationSummary(inst, listed.hasMore))
        }
        merged.sort((a, b) => a.fullName.localeCompare(b.fullName))
        return { ...connection, repos: merged, hasMore, installations }
      }),

    // The picker's "Add by name" escape hatch: a repo the listing's page cap
    // hides (or one the user knows by name) resolves through EXACTLY the
    // connect gate, so a refusal names the real reason. Read-only.
    lookupRepo: authedProcedure
      .input(
        z.object({
          teamId: z.string().uuid(),
          fullName: z
            .string()
            .min(1)
            .max(255)
            .regex(REPO_FULL_NAME_RE, `Expected "owner/name"`),
        })
      )
      .query(async ({ ctx, input }) => {
        const userId = ctx.session.user.id
        await assertTeamMember(userId, input.teamId)
        const resolved = await resolveRepoForConnect(userId, input.fullName)
        return {
          fullName: input.fullName,
          private: resolved.private,
          defaultBranch: resolved.defaultBranch,
          installationId: resolved.installationId,
        }
      }),

    // Disconnect the caller's GitHub account. `installationId` is accepted
    // and ignored (the pre-SLOP-7 clients send one): there is no per-team
    // link to sever anymore, only the person's own connection.
    unlink: authedProcedure
      .input(
        z.object({
          teamId: z.string().uuid(),
          installationId: z.number().int().positive().optional(),
        })
      )
      .mutation(async ({ ctx, input }) => {
        await assertTeamMember(ctx.session.user.id, input.teamId)
        await unlinkGithubAccount(ctx)
        return { ok: true as const }
      }),

    disconnect: authedProcedure.mutation(async ({ ctx }) => {
      await unlinkGithubAccount(ctx)
      return { ok: true as const }
    }),
  }),
})

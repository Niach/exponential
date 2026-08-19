import { and, asc, eq, inArray, isNull, sql } from "drizzle-orm"
import { z } from "zod"
import { router, authedProcedure } from "@/lib/trpc"
// Procedures in this router read/write through `ctx.db` like every other
// router; the module-level `db` is reserved for the exported helpers that
// non-tRPC callers (webhooks, the setup/callback routes, MCP) reach for and for
// the deliberately out-of-band suspension heal.
import { db } from "@/db/connection"
import {
  githubInstallationLinks,
  githubInstallationRepoGrants,
  githubInstallations,
  repositories,
} from "@/db/schema"
import { assertTeamMember } from "@/lib/team-membership"
import { TRPCError } from "@trpc/server"
import {
  githubAppConfigured,
  githubAppInstallUrl,
  githubOAuthAuthorizeUrl,
  githubOAuthConfigured,
  installationIdForRepo,
  installationManageUrl,
  listAllInstallationRepos,
  type InstallationRepo,
} from "@/lib/integrations/github-app"
import {
  mintGithubSetupState,
  readGithubClaimTicket,
} from "@/lib/integrations/github-setup-state"

type Db = typeof db
type Tx = Parameters<Parameters<Db[`transaction`]>[0]>[0]
// Every helper below only READS, so both the pooled connection and an open
// transaction satisfy it. Passing the caller's executor (never the module-level
// `db`) is what lets the connect path run its checks inside — and under the row
// locks of — the transaction that writes the repository row (EXP-371).
type Executor = Pick<Db, `select`>

// EXP-557 per-user sharing: connecting a repo is something any member does
// with THEIR OWN GitHub connection (grant rows scoped to the actor), and a
// connected repo is managed by its sharer or a team owner. The old
// owner-or-instance-admin `assertCanManageRepos` gate is gone — instance
// admins differ from normal users only by admin-console access.

// --- The link row lock (EXP-371) --------------------------------------------
// Unlinking guards on "no connected repo uses this installation"
// (assertInstallationNotInUse) while connecting guards on "this installation is
// linked to the team" (assertRepoInstallationAccess) — a TOCTOU pair. A
// transaction alone does NOT close it: under READ COMMITTED the unlinking
// transaction simply never sees a repository row a concurrent connect commits
// after its guard ran, so the delete lands anyway and leaves a connected repo
// whose token path is gone.
//
// Both sides therefore serialize on the SAME github_installation_links row:
// the connect path locks it inside the transaction that inserts the repository
// row, and every deleter locks it before running its in-use guard. Whoever
// takes the lock first wins — the loser either sees the freshly connected
// repository (unlink → CONFLICT) or finds its link already gone (connect →
// CONFLICT). Rows are locked in id order so two multi-row lockers can't
// deadlock, and callers delete only what they locked.
export async function lockInstallationLinks(
  tx: Tx,
  linkIds: string[]
): Promise<string[]> {
  if (linkIds.length === 0) return []
  const rows = await tx
    .select({ id: githubInstallationLinks.id })
    .from(githubInstallationLinks)
    .where(inArray(githubInstallationLinks.id, linkIds))
    .orderBy(asc(githubInstallationLinks.id))
    .for(`update`)
  return rows.map((row) => row.id)
}

// The team's link rows for a set of GitHub installation ids — the lookup the
// unlink paths need before they can lock (the link table keys on the
// installation's uuid PK, callers speak GitHub's numeric id).
async function teamLinkRows(
  exec: Executor,
  teamId: string,
  installationIds: number[]
): Promise<
  Array<{
    linkId: string
    installationId: number
    createdByUserId: string | null
  }>
> {
  if (installationIds.length === 0) return []
  return exec
    .select({
      linkId: githubInstallationLinks.id,
      installationId: githubInstallations.installationId,
      createdByUserId: githubInstallationLinks.createdByUserId,
    })
    .from(githubInstallationLinks)
    .innerJoin(
      githubInstallations,
      eq(githubInstallations.id, githubInstallationLinks.githubInstallationId)
    )
    .where(
      and(
        eq(githubInstallationLinks.teamId, teamId),
        inArray(githubInstallations.installationId, installationIds)
      )
    )
}

// CONFLICT while the team still has connected (non-archived) repos under the
// installation — mirroring repositories.remove's boards-restrict — so no repo
// row silently loses its token path. Shared by `unlink` and the claim page's
// unlink path. MUST run after lockInstallationLinks on the same executor: only
// then is a concurrent connect either already visible here or still blocked.
async function assertInstallationNotInUse(
  exec: Executor,
  teamId: string,
  installationId: number
) {
  const inUse = await exec
    .select({ id: repositories.id })
    .from(repositories)
    .where(
      and(
        eq(repositories.teamId, teamId),
        eq(repositories.installationId, installationId),
        isNull(repositories.archivedAt)
      )
    )
  if (inUse.length > 0) {
    throw new TRPCError({
      code: `CONFLICT`,
      message: `${inUse.length} connected ${
        inUse.length === 1 ? `repository uses` : `repositories use`
      } this GitHub account. Remove them before disconnecting.`,
    })
  }
}

// The GitHub App INSTALL page URL (new install / grant more repos). Also the
// claim fallback when the App has no OAuth client secret configured: the
// signed state carries the target team, and the setup redirect links the
// installation to it after the round-trip. `dialog: true` lands the redirect
// on the self-closing /integrations/github/installed page; `mobile: true`
// serves the exponential://github-connected deep-link page instead.
function installUrlFor(
  userId: string,
  teamId: string,
  opts?: { mobile?: boolean }
): string | null {
  return githubAppInstallUrl(
    mintGithubSetupState(userId, {
      dialog: true,
      mobile: opts?.mobile,
      teamId,
    })
  )
}

// The OAuth claim URL — the mobile-friendly primary connect path: a single
// authorize screen (instant auto-redirect on re-auth), then the callback
// enumerates the user's installations and links them to the team without
// ever visiting GitHub's configure page. Null when the App's OAuth client
// secret isn't configured (self-hosted fallback = installUrl).
function connectUrlFor(
  userId: string,
  teamId: string,
  opts?: { mobile?: boolean }
): string | null {
  return githubOAuthAuthorizeUrl(
    mintGithubSetupState(userId, {
      dialog: true,
      mobile: opts?.mobile,
      teamId,
      oauth: true,
    })
  )
}

interface ResolvedInstallation {
  // The github_installation_links row this resolution came through — the
  // handle the connect path locks before writing (EXP-371).
  linkId: string
  installationId: number
  accountLogin: string | null
  accountType: string | null
  // Non-null while GitHub has the installation SUSPENDED (REV2-29). The link
  // survives a suspension — only `installation.deleted` drops it — so this is
  // the health signal every surface reads, never the claim itself.
  suspendedAt: Date | null
  // Who linked this installation to the team (EXP-557 viewer scoping: a link
  // you created counts as YOUR installation even before any grant lands).
  // NULL on legacy rows.
  createdByUserId: string | null
}

// The installations a team may browse/connect: exactly its claimed links.
// No admin bypass, no unattributed fallback — an unlinked installation is
// invisible to every picker (the old "admins see all ownerless installs" rule
// leaked one account's repos into unrelated contexts).
async function resolveTeamInstallations(
  exec: Executor,
  teamId: string
): Promise<ResolvedInstallation[]> {
  return exec
    .select({
      linkId: githubInstallationLinks.id,
      installationId: githubInstallations.installationId,
      accountLogin: githubInstallations.accountLogin,
      accountType: githubInstallations.accountType,
      suspendedAt: githubInstallations.suspendedAt,
      createdByUserId: githubInstallationLinks.createdByUserId,
    })
    .from(githubInstallationLinks)
    .innerJoin(
      githubInstallations,
      eq(githubInstallations.id, githubInstallationLinks.githubInstallationId)
    )
    .where(eq(githubInstallationLinks.teamId, teamId))
}

// --- Repo grants (the user-scoped access boundary) --------------------------
// A team ↔ installation LINK is installation-granular, but GitHub
// attributes an installation to a user who can access even ONE of its repos.
// github_installation_repo_grants (captured at OAuth-callback time from
// `GET /user/installations/{id}/repositories`) records what the connecting
// user could actually access; when the App's OAuth secret is configured, repo
// DISCOVERY (the `repos` query) and CONNECT (assertRepoInstallationAccess)
// are confined to the ACTING USER's granted repos (EXP-557: you browse and
// connect only what your own OAuth proved — connecting then SHARES the repo
// with the team). Without the OAuth secret there is no user-scoped capture
// path at all (single-tenant/trusted self-host), so the installation-wide
// behavior stays — exactly mirroring setup.ts's githubOAuthConfigured()
// split. Token minting is deliberately NOT grant-gated (already-connected
// repos keep working; the gate is for discovery/connect).

// Every granted (installationId, fullName, …, grantedByUserId) for a team,
// restricted to the given linked installation ids. Callers slice per viewer
// (discovery, needsReauth) or across everyone (the owner's stale-link
// detection) from the same rows.
async function teamGrantRows(
  exec: Executor,
  teamId: string,
  installationIds: number[]
): Promise<
  Array<{
    installationId: number
    fullName: string
    private: boolean
    defaultBranch: string | null
    grantedByUserId: string | null
  }>
> {
  if (installationIds.length === 0) return []
  return exec
    .select({
      installationId: githubInstallationRepoGrants.installationId,
      fullName: githubInstallationRepoGrants.fullName,
      private: githubInstallationRepoGrants.private,
      defaultBranch: githubInstallationRepoGrants.defaultBranch,
      grantedByUserId: githubInstallationRepoGrants.grantedByUserId,
    })
    .from(githubInstallationRepoGrants)
    .where(
      and(
        eq(githubInstallationRepoGrants.teamId, teamId),
        inArray(githubInstallationRepoGrants.installationId, installationIds)
      )
    )
}

// The connect-time grant gate, ACTING-USER-scoped (EXP-557): only a grant the
// actor's own OAuth captured entitles them to connect the repo — a teammate's
// grant doesn't. No-op when the OAuth secret isn't configured (no capture
// path exists — trusted single-tenant fallback).
async function assertRepoGrant(
  exec: Executor,
  teamId: string,
  userId: string,
  installationId: number,
  fullName: string
): Promise<void> {
  if (!githubOAuthConfigured()) return
  const [row] = await exec
    .select({ id: githubInstallationRepoGrants.id })
    .from(githubInstallationRepoGrants)
    .where(
      and(
        eq(githubInstallationRepoGrants.teamId, teamId),
        eq(githubInstallationRepoGrants.installationId, installationId),
        eq(githubInstallationRepoGrants.fullName, fullName),
        eq(githubInstallationRepoGrants.grantedByUserId, userId)
      )
    )
    .limit(1)
  if (row) return
  throw new TRPCError({
    code: `FORBIDDEN`,
    message: `You don't have access to ${fullName} on GitHub, or your connection is stale. Reconnect GitHub in team settings → Repositories to refresh which repositories you can access.`,
  })
}

// Short-lived in-process cache of the installable repos so re-opening the
// board dialog doesn't hammer GitHub (and its secondary rate limits). Keyed
// per (team, user) since EXP-557 — discovery is viewer-scoped.
const REPOS_TTL_MS = 60_000
interface CachedRepos {
  repos: InstallationRepo[]
  hasMore: boolean
  installations: Array<
    ResolvedInstallation & { hasMore: boolean; needsReauth: boolean }
  >
  expiresAt: number
}
const repoCache = new Map<string, CachedRepos>()

// `\0` can't appear in a uuid or a Better Auth user id, so the compound key
// never collides across teams.
function repoCacheKey(teamId: string, userId: string): string {
  return `${teamId}\0${userId}`
}

// Drop a team's cached repo lists (every member's entry) so the next `repos`
// query re-hits the DB/GitHub. Called after a claim/link lands or when the UI
// asks for a forced refresh, both of which mean the installable set changed.
export function invalidateRepoCache(teamId: string): void {
  for (const key of repoCache.keys()) {
    if (key.startsWith(`${teamId}\0`)) repoCache.delete(key)
  }
}

// Installation-wide invalidation (webhooks: repos granted/removed, install
// suspended): drop every linked team's entry.
export async function invalidateRepoCacheForInstallation(
  installationId: number
): Promise<void> {
  const linked = await db
    .select({ teamId: githubInstallationLinks.teamId })
    .from(githubInstallationLinks)
    .innerJoin(
      githubInstallations,
      eq(githubInstallations.id, githubInstallationLinks.githubInstallationId)
    )
    .where(eq(githubInstallations.installationId, installationId))
  for (const row of linked) invalidateRepoCache(row.teamId)
}

// --- Suspension (REV2-29) ---------------------------------------------------
// GitHub suspension is reversible and the claim links now survive it, so a
// suspended installation is INERT (no discovery, no connect) but recoverable:
// the `unsuspend` webhook clears `suspended_at` and everything resumes. The one
// way that could get stuck is a missed/failed `unsuspend` delivery — which
// would leave a perfectly healthy installation inert forever — so every read of
// a suspended installation probes GitHub at most once a minute and clears the
// mark itself when the probe succeeds. Minting an installation token IS the
// definitive test: GitHub 403s a suspended installation ("This installation has
// been suspended"), and `listAllInstallationRepos` mints one. Bounded by
// construction: healthy installations never probe.
const SUSPEND_PROBE_TTL_MS = 60_000
const suspendProbedAt = new Map<number, number>()

async function healSuspendedInstallations(
  installs: ResolvedInstallation[]
): Promise<ResolvedInstallation[]> {
  const suspended = installs.filter((i) => i.suspendedAt != null)
  if (suspended.length === 0) return installs
  const now = Date.now()
  const cleared = new Set<number>()
  await Promise.all(
    suspended.map(async (inst) => {
      const lastProbe = suspendProbedAt.get(inst.installationId) ?? 0
      if (now - lastProbe < SUSPEND_PROBE_TTL_MS) return
      suspendProbedAt.set(inst.installationId, now)
      try {
        await listAllInstallationRepos(inst.installationId, { maxPages: 1 })
        // Deliberately the module-level `db`, never the caller's transaction:
        // clearing the mark is best-effort bookkeeping about GitHub's state, so
        // it must survive a caller whose transaction later rolls back — and a
        // long connect transaction must not hold a row lock on
        // github_installations while it talks to GitHub.
        await db
          .update(githubInstallations)
          .set({ suspendedAt: null })
          .where(eq(githubInstallations.installationId, inst.installationId))
        cleared.add(inst.installationId)
      } catch {
        // Still suspended (or GitHub/the heal write hiccuped) — keep the mark.
      }
    })
  )
  if (cleared.size === 0) return installs
  return installs.map((inst) =>
    cleared.has(inst.installationId) ? { ...inst, suspendedAt: null } : inst
  )
}

// EXP-557 viewer scoping: the installations that count as YOURS — links you
// created ∪ installations where your own OAuth captured at least one grant.
// Callers only reach for this when OAuth is configured (grants exist); the
// trusted non-OAuth self-host keeps the whole team-linked set for everyone.
function viewerInstallations(
  installs: ResolvedInstallation[],
  userId: string,
  grants: Array<{ installationId: number; grantedByUserId: string | null }>
): ResolvedInstallation[] {
  const mine = new Set(
    grants
      .filter((g) => g.grantedByUserId === userId)
      .map((g) => g.installationId)
  )
  return installs.filter(
    (inst) =>
      inst.createdByUserId === userId || mine.has(inst.installationId)
  )
}

// The suspended installations' account labels, for actionable error copy.
function suspendedLabel(installs: ResolvedInstallation[]): string {
  const logins = installs
    .filter((i) => i.suspendedAt != null)
    .map((i) => i.accountLogin ?? `installation ${i.installationId}`)
  return logins.join(`, `)
}

// The connect side of the link lock (EXP-371). Re-reads the resolved link row
// under FOR UPDATE inside the caller's transaction — the same transaction that
// goes on to insert/un-archive the repository row — so an unlink can neither
// slip its in-use guard past that insert nor delete the link behind it. A
// vanished row means an unlink committed first: fail the connect closed.
async function lockResolvedLink(
  tx: Tx,
  inst: ResolvedInstallation,
  fullName: string
): Promise<number> {
  const locked = await lockInstallationLinks(tx, [inst.linkId])
  if (locked.length === 0) {
    throw new TRPCError({
      code: `CONFLICT`,
      message: `The GitHub account serving ${fullName} was disconnected from this team while connecting. Reconnect it in team settings → Repositories, then try again.`,
    })
  }
  return inst.installationId
}

// Connect-path authorization: connecting a repo (repositories.add /
// boards.create inline) must resolve to an installation LINKED to the target
// team — the App JWT itself can reach every installation of the App, so
// without this check any owner who knows a repo's full name could bind an
// unrelated account's private repo to their team. Returns the
// authoritative installation id so callers persist that instead of trusting
// the client-supplied one. When GitHub's per-repo lookup 404s (it's flaky when
// the App spans several accounts), fall back to scanning the team's
// linked installations' repo lists — bounded, connect-time only.
// On OAuth-configured instances the resolved installation must ALSO carry the
// ACTING USER's grant for this exact repo (assertRepoGrant, EXP-557) — the
// link alone is installation-granular, and neither a single-repo collaborator
// nor a teammate riding someone else's grant may connect through it.
// A SUSPENDED installation is refused with its own actionable message
// (REV2-29): it can't mint a token, so connecting through it would register a
// repo row that fails at the first clone with a misleading "reconnect" error.
// Runs inside the CONNECT TRANSACTION (connectRepositoryInTx's `tx`) — not a
// convenience: the returned installation is only meaningful while this
// transaction holds the link row's lock, which is what stops a concurrent
// unlink stranding the repository row this transaction writes.
export async function assertRepoInstallationAccess(
  tx: Tx,
  teamId: string,
  userId: string,
  fullName: string
): Promise<number> {
  const linked = await resolveTeamInstallations(tx, teamId)
  if (linked.length === 0) {
    throw new TRPCError({
      code: `PRECONDITION_FAILED`,
      message: `No GitHub account is connected to this team. Connect one in team settings → Repositories, then try again.`,
    })
  }
  const healed = await healSuspendedInstallations(linked)
  const installs = healed.filter((i) => i.suspendedAt == null)
  if (installs.length === 0) {
    throw new TRPCError({
      code: `PRECONDITION_FAILED`,
      message: `GitHub suspended the Exponential app for ${suspendedLabel(healed)}. Unsuspend it on GitHub (team settings → Repositories → Manage), then try again.`,
    })
  }
  const repoInstallationId = await installationIdForRepo(fullName)
  if (repoInstallationId != null) {
    const matched = installs.find(
      (i) => i.installationId === repoInstallationId
    )
    if (!matched) {
      const suspendedMatch = healed.find(
        (i) => i.installationId === repoInstallationId && i.suspendedAt != null
      )
      throw new TRPCError({
        code: suspendedMatch ? `PRECONDITION_FAILED` : `FORBIDDEN`,
        message: suspendedMatch
          ? `GitHub suspended the Exponential app for ${suspendedMatch.accountLogin ?? `installation ${suspendedMatch.installationId}`}, which owns ${fullName}. Unsuspend it on GitHub (team settings → Repositories → Manage), then try again.`
          : `${fullName} belongs to a GitHub App installation that isn't connected to this team. Connect that GitHub account in team settings → Repositories first.`,
      })
    }
    // The link alone is installation-granular; the ACTOR's grant (captured
    // user-scoped at OAuth time) proves they can actually access THIS repo.
    await assertRepoGrant(tx, teamId, userId, repoInstallationId, fullName)
    return lockResolvedLink(tx, matched, fullName)
  }
  for (const inst of installs) {
    // On GitHub a full_name maps to exactly one repo (and so one installation
    // of this App) — a scan hit is authoritative; gate it and stop. The grant
    // check runs OUTSIDE the try so its FORBIDDEN is never swallowed.
    let found = false
    try {
      const { repos } = await listAllInstallationRepos(inst.installationId)
      found = repos.some((r) => r.fullName === fullName)
    } catch {
      // A revoked/suspended installation must not fail the whole scan.
    }
    if (found) {
      await assertRepoGrant(tx, teamId, userId, inst.installationId, fullName)
      return lockResolvedLink(tx, inst, fullName)
    }
  }
  throw new TRPCError({
    code: `PRECONDITION_FAILED`,
    message: `The Exponential GitHub App has no access to ${fullName}. Grant it on GitHub (team settings → Repositories → Manage), then try again.`,
  })
}

// Token-mint gate: is this installation claimed by the repo's team?
// (repositories.installationToken re-checks the link at mint time so a repo
// row can't keep minting through an installation the team disconnected.)
// Deliberately a PURE claim check — `suspended_at` is NOT consulted (REV2-29).
// Suspension is a health state, not a revoked claim: GitHub itself refuses to
// mint for a suspended installation, so folding it in here would only swap that
// honest failure for this gate's "reconnect the account" copy and send owners
// through a connect flow that cannot fix a suspension.
export async function isInstallationLinkedToTeam(
  teamId: string,
  installationId: number
): Promise<boolean> {
  const [row] = await db
    .select({ id: githubInstallationLinks.id })
    .from(githubInstallationLinks)
    .innerJoin(
      githubInstallations,
      eq(githubInstallations.id, githubInstallationLinks.githubInstallationId)
    )
    .where(
      and(
        eq(githubInstallationLinks.teamId, teamId),
        eq(githubInstallations.installationId, installationId)
      )
    )
    .limit(1)
  return Boolean(row)
}

function installationSummary(inst: ResolvedInstallation) {
  return {
    installationId: inst.installationId,
    accountLogin: inst.accountLogin,
    accountType: inst.accountType,
    manageUrl: installationManageUrl(inst),
    // REV2-29: GitHub has this installation suspended — the claim link is
    // intact (it survives suspension) but nothing can mint a token through it.
    // Every client surfaces this instead of rendering the account/repos as
    // healthy while the launcher's token mint fails.
    suspended: inst.suspendedAt != null,
  }
}

export const integrationsRouter = router({
  github: router({
    // GitHub connection state for a team (drives the settings section and
    // the pickers' empty state). Member-gated. Token resolution is
    // storage-free (the App JWT looks up a repo's installation on demand);
    // this only reflects what's linked. `platform: "mobile"` marks the minted
    // URLs' state like `repos` does (EXP-368: the desktop IDE sends it too).
    status: authedProcedure
      .input(
        z.object({
          teamId: z.string().uuid(),
          platform: z.enum([`web`, `mobile`]).optional(),
        })
      )
      .query(async ({ ctx, input }) => {
        const userId = ctx.session.user.id
        const { teamId } = input
        const mobile = input.platform === `mobile`
        const member = await assertTeamMember(userId, teamId)
        if (!githubAppConfigured()) {
          return {
            configured: false as const,
            installed: false,
            installUrl: null as string | null,
            connectUrl: null as string | null,
            accounts: [] as string[],
            installations: [] as Array<
              ReturnType<typeof installationSummary> & {
                needsReauth: boolean
                stale: boolean
              }
            >,
          }
        }
        // Suspended links are probed (and self-healed) here too — this is the
        // settings section's only data source, so a stale mark would strand the
        // whole GitHub surface behind a suspension banner (REV2-29).
        const linked = await healSuspendedInstallations(
          await resolveTeamInstallations(ctx.db, teamId)
        )
        const oauth = githubOAuthConfigured()
        const grants = oauth
          ? await teamGrantRows(
              ctx.db,
              teamId,
              linked.map((i) => i.installationId)
            )
          : []
        // EXP-557 viewer scoping: you see YOUR GitHub connections (links you
        // created ∪ installations your grants attribute to you). Owners
        // additionally see every STALE link — linked but with zero grants
        // from ANYONE (a legacy/orphaned claim that can only warn forever) —
        // so they can disconnect it. Non-OAuth instances keep the whole
        // team-linked set (no grants exist to scope by).
        const anyGrantIds = new Set(grants.map((g) => g.installationId))
        const myGrantIds = new Set(
          grants
            .filter((g) => g.grantedByUserId === userId)
            .map((g) => g.installationId)
        )
        const mine = oauth ? viewerInstallations(linked, userId, grants) : linked
        const mineIds = new Set(mine.map((i) => i.installationId))
        const staleExtra =
          oauth && member?.role === `owner`
            ? linked.filter(
                (inst) =>
                  !mineIds.has(inst.installationId) &&
                  !anyGrantIds.has(inst.installationId)
              )
            : []
        const visible = [...mine, ...staleExtra]
        return {
          configured: true as const,
          installed: visible.length > 0,
          installUrl: installUrlFor(userId, teamId, { mobile }),
          connectUrl: connectUrlFor(userId, teamId, { mobile }),
          // Login-only convenience mirror of `installations` — kept ONLY for
          // shipped native builds that decode it non-optionally (the iOS build
          // in App Review). Cleanup: EXP-558.
          accounts: visible
            .map((r) => r.accountLogin)
            .filter((a): a is string => Boolean(a)),
          installations: visible.map((inst) => ({
            ...installationSummary(inst),
            // Per-viewer: YOUR installation with zero grants FROM YOU needs a
            // reconnect (suspended installs need an unsuspend instead —
            // never nudge for the wrong fix, parity with `repos`).
            needsReauth:
              oauth &&
              inst.suspendedAt == null &&
              mineIds.has(inst.installationId) &&
              !myGrantIds.has(inst.installationId),
            // Zero grants from ANY member AND not the viewer's own link:
            // reconnecting can never heal this one — the UI renders a
            // Disconnect affordance instead (server-enforced
            // link-creator-or-owner on `unlink`). A zero-grant link the VIEWER
            // created IS healed by a plain reconnect (the next OAuth writes
            // their grants), so it keeps the `needsReauth` nudge above —
            // covers the EXP-365 empty-capture race and legacy pre-0012 links.
            stale:
              oauth &&
              inst.suspendedAt == null &&
              !anyGrantIds.has(inst.installationId) &&
              inst.createdByUserId !== userId,
          })),
        }
      }),

    // Repos the team can connect, aggregated across its linked
    // installations and deduped. Backs the repo pickers. `hasMore` signals the
    // per-installation page cap truncated the set so the UI can point at
    // "manage repos on GitHub". `refresh` bypasses the cache so returning from
    // a GitHub hop (new repos granted) reflects immediately. `platform:
    // "mobile"` (any native deep-link-capable client — iOS/Android/desktop
    // IDE) marks the minted URLs' state so the callbacks deep-link
    // `exponential://github-connected` back into the app; web callers omit it.
    repos: authedProcedure
      .input(
        z.object({
          teamId: z.string().uuid(),
          refresh: z.boolean().optional(),
          platform: z.enum([`web`, `mobile`]).optional(),
        })
      )
      .query(async ({ ctx, input }) => {
        const userId = ctx.session.user.id
        const { teamId } = input
        const mobile = input.platform === `mobile`
        await assertTeamMember(userId, teamId)
        if (input.refresh) invalidateRepoCache(teamId)
        if (!githubAppConfigured()) {
          return {
            configured: false as const,
            installed: false,
            installUrl: null as string | null,
            connectUrl: null as string | null,
            repos: [] as InstallationRepo[],
            hasMore: false,
            installations: [] as Array<
              ReturnType<typeof installationSummary> & {
                hasMore: boolean
                needsReauth: boolean
              }
            >,
          }
        }

        // Probe-heal first (REV2-29) so a stale suspension mark can't hide a
        // healthy account's repos: `installed` still counts the LINK, but a
        // still-suspended installation contributes no connectable repos —
        // connecting through it would register a repo row that can't clone.
        const linked = await healSuspendedInstallations(
          await resolveTeamInstallations(ctx.db, teamId)
        )
        // EXP-557 viewer scoping (OAuth instances): the pickers list YOUR
        // installations and YOUR granted repos only — connecting shares the
        // repo with the team, but discovery never unions across members.
        // Non-OAuth self-hosts have no grants to scope by and keep the
        // team-linked set. Grants are read for ALL linked installations —
        // they also ATTRIBUTE a suspended installation to the viewer (so its
        // banner still renders); the repo merge below skips inactive ids.
        const oauthScoped = githubOAuthConfigured()
        const grants = oauthScoped
          ? await teamGrantRows(
              ctx.db,
              teamId,
              linked.map((i) => i.installationId)
            )
          : []
        const installs = oauthScoped
          ? viewerInstallations(linked, userId, grants)
          : linked
        const urls = {
          installUrl: installUrlFor(userId, teamId, { mobile }),
          connectUrl: connectUrlFor(userId, teamId, { mobile }),
        }

        if (installs.length === 0) {
          return {
            configured: true as const,
            installed: false,
            ...urls,
            repos: [] as InstallationRepo[],
            hasMore: false,
            installations: [] as Array<
              ReturnType<typeof installationSummary> & {
                hasMore: boolean
                needsReauth: boolean
              }
            >,
          }
        }

        const cacheKey = repoCacheKey(teamId, userId)
        const cached = repoCache.get(cacheKey)
        if (cached && cached.expiresAt > Date.now()) {
          return {
            configured: true as const,
            installed: true,
            ...urls,
            repos: cached.repos,
            hasMore: cached.hasMore,
            installations: cached.installations.map((inst) => ({
              ...installationSummary(inst),
              hasMore: inst.hasMore,
              needsReauth: inst.needsReauth,
            })),
          }
        }

        const seen = new Set<string>()
        const merged: InstallationRepo[] = []
        let hasMore = false
        const withMeta: Array<
          ResolvedInstallation & { hasMore: boolean; needsReauth: boolean }
        > = []
        // Suspended installations stay in `installations` (the UI needs to
        // explain the suspension) but contribute zero repos on either path.
        const activeIds = new Set(
          installs
            .filter((i) => i.suspendedAt == null)
            .map((i) => i.installationId)
        )
        if (oauthScoped) {
          // Grant path (OAuth configured): the pickers list exactly the repos
          // THE VIEWER proved user-scoped access to at OAuth time (EXP-557) —
          // never the installation-wide selection (which leaks every repo of
          // an account to a single-repo collaborator) and never a teammate's
          // grants, with zero GitHub round-trips. The grant snapshot is
          // bounded (capture pages are capped), so hasMore is always false
          // here; re-running the connect flow is the refresh. Your linked
          // installation with no grants from you needs exactly that —
          // surfaced as `needsReauth`.
          const myGrants = grants.filter((g) => g.grantedByUserId === userId)
          const myGrantIds = new Set(myGrants.map((g) => g.installationId))
          for (const grant of myGrants) {
            if (!activeIds.has(grant.installationId)) continue
            if (seen.has(grant.fullName)) continue
            seen.add(grant.fullName)
            merged.push({
              fullName: grant.fullName,
              private: grant.private,
              defaultBranch: grant.defaultBranch ?? `main`,
              installationId: grant.installationId,
            })
          }
          for (const inst of installs) {
            withMeta.push({
              ...inst,
              hasMore: false,
              // A suspended installation needs an UNSUSPEND on GitHub, not a
              // re-auth — never nudge for the wrong fix.
              needsReauth:
                inst.suspendedAt == null &&
                !myGrantIds.has(inst.installationId),
            })
          }
        } else {
          // No OAuth secret ⇒ no user-scoped capture path exists (trusted
          // single-tenant self-host) — keep the installation-wide listing.
          for (const inst of installs) {
            let instHasMore = false
            // A suspended installation can't mint the token this listing needs
            // — skip the guaranteed-failing round-trip.
            if (activeIds.has(inst.installationId)) {
              try {
                const { repos, hasMore: more } = await listAllInstallationRepos(
                  inst.installationId
                )
                instHasMore = more
                if (more) hasMore = true
                for (const repo of repos) {
                  if (seen.has(repo.fullName)) continue
                  seen.add(repo.fullName)
                  merged.push(repo)
                }
              } catch {
                // A single revoked/404 installation must not fail the whole
                // list.
              }
            }
            withMeta.push({ ...inst, hasMore: instHasMore, needsReauth: false })
          }
        }
        merged.sort((a, b) => a.fullName.localeCompare(b.fullName))
        repoCache.set(cacheKey, {
          repos: merged,
          hasMore,
          installations: withMeta,
          expiresAt: Date.now() + REPOS_TTL_MS,
        })

        return {
          configured: true as const,
          installed: true,
          ...urls,
          repos: merged,
          hasMore,
          installations: withMeta.map((inst) => ({
            ...installationSummary(inst),
            hasMore: inst.hasMore,
            needsReauth: inst.needsReauth,
          })),
        }
      }),

    // The claim page's data: which GitHub accounts the OAuth callback proved
    // control of, and which are already linked to the target team.
    claimPreview: authedProcedure
      .input(z.object({ ticket: z.string().min(1) }))
      .query(async ({ ctx, input }) => {
        const claim = readGithubClaimTicket(input.ticket, ctx.session.user.id)
        if (!claim) {
          throw new TRPCError({
            code: `PRECONDITION_FAILED`,
            message: `This claim link expired or belongs to another session. Restart the connect flow from team settings.`,
          })
        }
        // Any member may claim the installations their own OAuth proved
        // control of (EXP-557) — connecting your account is how you share
        // your repos with the team.
        await assertTeamMember(ctx.session.user.id, claim.w)
        const rows = await ctx.db
          .select({
            id: githubInstallations.id,
            installationId: githubInstallations.installationId,
            accountLogin: githubInstallations.accountLogin,
            accountType: githubInstallations.accountType,
          })
          .from(githubInstallations)
          .where(inArray(githubInstallations.installationId, claim.ids))
        const linked = await ctx.db
          .select({
            githubInstallationId: githubInstallationLinks.githubInstallationId,
          })
          .from(githubInstallationLinks)
          .where(eq(githubInstallationLinks.teamId, claim.w))
        const linkedIds = new Set(linked.map((l) => l.githubInstallationId))
        // Active repo counts drive the picker's unlink affordance: an account
        // whose repos are still connected can't be unchecked (mirrors the
        // `unlink` CONFLICT guard), so the page disables the row with a note.
        const repoCounts = await ctx.db
          .select({
            installationId: repositories.installationId,
            count: sql<number>`count(*)::int`,
          })
          .from(repositories)
          .where(
            and(
              eq(repositories.teamId, claim.w),
              inArray(repositories.installationId, claim.ids),
              isNull(repositories.archivedAt)
            )
          )
          .groupBy(repositories.installationId)
        const countByInstallation = new Map(
          repoCounts.map((r) => [r.installationId, r.count])
        )
        return {
          teamId: claim.w,
          mobile: claim.m === true,
          dialog: claim.d === true,
          installations: rows.map((row) => ({
            installationId: row.installationId,
            accountLogin: row.accountLogin,
            accountType: row.accountType,
            alreadyLinked: linkedIds.has(row.id),
            activeRepoCount:
              countByInstallation.get(row.installationId) ?? 0,
          })),
        }
      }),

    // Apply the claim page's link/unlink selection. The ticket bounds the
    // changeable set to exactly what the OAuth enumeration proved control
    // of; unlinking is refused while the account still has connected repos
    // (same guard as `unlink`). All guards run before any write, so a
    // CONFLICT leaves the links untouched — and the ticket is deliberately
    // not single-use, so the user can fix the selection and save again.
    claimLinks: authedProcedure
      .input(
        z
          .object({
            ticket: z.string().min(1),
            linkIds: z.array(z.number().int().positive()).default([]),
            unlinkIds: z.array(z.number().int().positive()).default([]),
          })
          .refine((v) => v.linkIds.length > 0 || v.unlinkIds.length > 0, {
            message: `Nothing to change.`,
          })
      )
      .mutation(async ({ ctx, input }) => {
        const userId = ctx.session.user.id
        const claim = readGithubClaimTicket(input.ticket, userId)
        if (!claim) {
          throw new TRPCError({
            code: `PRECONDITION_FAILED`,
            message: `This claim link expired or belongs to another session. Restart the connect flow from team settings.`,
          })
        }
        const allowed = new Set(claim.ids)
        if (
          [...input.linkIds, ...input.unlinkIds].some(
            (id) => !allowed.has(id)
          )
        ) {
          throw new TRPCError({
            code: `FORBIDDEN`,
            message: `Selection includes an installation this claim didn't verify.`,
          })
        }
        // Link and unlink are applied in a fixed order, so an id in BOTH lists
        // would resolve to "unlinked" by accident of ordering rather than by
        // what the caller asked for — refuse the ambiguous save outright.
        const unlinkSet = new Set(input.unlinkIds)
        if (input.linkIds.some((id) => unlinkSet.has(id))) {
          throw new TRPCError({
            code: `BAD_REQUEST`,
            message: `Selection links and unlinks the same installation.`,
          })
        }
        // Linking is member-level (EXP-557: you claim your own controlled
        // installations); the unlink side below additionally requires the
        // actor to be each link's creator or a team owner.
        const member = await assertTeamMember(userId, claim.w)
        const isOwner = member?.role === `owner`
        // One transaction for the whole save, opened by the LOCK on every link
        // this save would delete (EXP-371) — the in-use guards below are only
        // race-free while that lock is held, and a CONFLICT now rolls the
        // link inserts back instead of relying on statement ordering.
        const result = await ctx.db.transaction(async (tx) => {
          const unlinkRows = await teamLinkRows(tx, claim.w, input.unlinkIds)
          const foreign = unlinkRows.filter(
            (row) => !isOwner && row.createdByUserId !== userId
          )
          if (foreign.length > 0) {
            throw new TRPCError({
              code: `FORBIDDEN`,
              message: `Only the member who connected a GitHub account or a team owner can disconnect it.`,
            })
          }
          const lockedLinkIds = await lockInstallationLinks(
            tx,
            unlinkRows.map((row) => row.linkId)
          )
          for (const installationId of input.unlinkIds) {
            await assertInstallationNotInUse(tx, claim.w, installationId)
          }
          let linked = 0
          if (input.linkIds.length > 0) {
            const rows = await tx
              .select({ id: githubInstallations.id })
              .from(githubInstallations)
              .where(
                inArray(githubInstallations.installationId, input.linkIds)
              )
            if (rows.length > 0) {
              await tx
                .insert(githubInstallationLinks)
                .values(
                  rows.map((row) => ({
                    teamId: claim.w,
                    githubInstallationId: row.id,
                    createdByUserId: userId,
                  }))
                )
                .onConflictDoNothing()
            }
            linked = rows.length
          }
          // Delete exactly the locked rows: a link (re)created concurrently was
          // never locked, so it is not this save's to remove.
          if (lockedLinkIds.length > 0) {
            await tx
              .delete(githubInstallationLinks)
              .where(inArray(githubInstallationLinks.id, lockedLinkIds))
          }
          return { linked, unlinked: lockedLinkIds.length, teamId: claim.w }
        })
        invalidateRepoCache(claim.w)
        return result
      }),

    // Remove a team ↔ installation link. Allowed for the link's creator or a
    // team owner (EXP-557). Blocked (CONFLICT) while the team still has
    // connected repos under that installation — mirroring
    // repositories.remove's boards-restrict — so no repo row silently loses
    // its token path.
    unlink: authedProcedure
      .input(
        z.object({
          teamId: z.string().uuid(),
          installationId: z.number().int().positive(),
        })
      )
      .mutation(async ({ ctx, input }) => {
        const userId = ctx.session.user.id
        const member = await assertTeamMember(userId, input.teamId)
        const isOwner = member?.role === `owner`
        await ctx.db.transaction(async (tx) => {
          // Lock BEFORE the guard (EXP-371): a repositories.add racing this
          // unlink is now either already committed — so the guard below sees
          // its repo row and CONFLICTs — or blocked on this lock until the
          // link is gone, which fails its own connect closed.
          const linkRows = await teamLinkRows(tx, input.teamId, [
            input.installationId,
          ])
          // Creator-or-owner: a member may only sever links they created
          // themselves (NULL-creator legacy links are owner-only).
          if (
            linkRows.some(
              (row) => !isOwner && row.createdByUserId !== userId
            )
          ) {
            throw new TRPCError({
              code: `FORBIDDEN`,
              message: `Only the member who connected this GitHub account or a team owner can disconnect it.`,
            })
          }
          const lockedLinkIds = await lockInstallationLinks(
            tx,
            linkRows.map((row) => row.linkId)
          )
          await assertInstallationNotInUse(
            tx,
            input.teamId,
            input.installationId
          )
          if (lockedLinkIds.length > 0) {
            await tx
              .delete(githubInstallationLinks)
              .where(inArray(githubInstallationLinks.id, lockedLinkIds))
          }
        })
        invalidateRepoCache(input.teamId)
        return { ok: true as const }
      }),
  }),
})

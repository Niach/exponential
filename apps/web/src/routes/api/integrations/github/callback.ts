import { createFileRoute } from "@tanstack/react-router"
import { TRPCError } from "@trpc/server"
import { and, eq, inArray, isNull } from "drizzle-orm"
import { db } from "@/db/connection"
import {
  githubInstallationLinks,
  githubInstallationRepoGrants,
  githubInstallations,
  repositories,
} from "@/db/schema"
import { resolveSessionUserId } from "@/lib/auth/resolve-bearer"
import {
  exchangeGithubOAuthCode,
  getAuthenticatedGithubLogin,
  getUserOrgMembershipState,
  githubAppInstallUrl,
  listUserInstallationRepos,
  listUserInstallations,
  partitionControlledInstallations,
} from "@/lib/integrations/github-app"
import { mobileConnectedResponse } from "@/lib/integrations/github-return-page"
import {
  consumeGithubSetupState,
  githubSetupStateWantsDialog,
  githubSetupStateWantsMobile,
  mintGithubClaimTicket,
  mintGithubSetupState,
} from "@/lib/integrations/github-setup-state"
import {
  assertCanManageRepos,
  invalidateRepoCache,
} from "@/lib/trpc/integrations"

// GitHub App OAuth callback — the PRIMARY team claim path. The user
// arrives from a single lightweight authorize screen (no configure page, no
// forced settings change — the thing that made the install round-trip so
// tedious, especially on mobile). We exchange the code for a TRANSIENT
// user-to-server token, ask GitHub which installations that user can access
// (`GET /user/installations`), and — since EXP-363 — keep only the ones the
// user CONTROLS (owns the User-type installation, or is an active member of
// the Organization-type one): GitHub's enumeration includes any installation
// the user can reach ONE repo of, so a collaborator on a stranger's repo
// would otherwise claim the stranger's whole account as the team's GitHub
// connection. Controlled installations are mirrored into github_installations
// and linked to the state's target team: directly when there's exactly one,
// via the /integrations/github/claim picker page when there are several. The
// token is used for this one round of lookups and discarded — never
// persisted, so expiry/refresh never exist.
// One short retry when a CONTROLLED installation lists zero repos: GitHub's
// `GET /user/installations/{id}/repositories` runs empty for a few seconds
// right after an installation is created (eventual consistency), and a
// zero-grant capture strands the connect until a manual reconnect (EXP-365).
const GRANT_CAPTURE_RETRY_MS = 2000

const sleep = (ms: number) =>
  new Promise<void>((resolve) => setTimeout(resolve, ms))

export async function handleCallback(request: Request): Promise<Response> {
  const url = new URL(request.url)
  const code = url.searchParams.get(`code`)
  const state = url.searchParams.get(`state`)
  const fromMobile = githubSetupStateWantsMobile(state)
  const fromDialog = githubSetupStateWantsDialog(state)

  // `login`/`install` feed the claim page's no-linkable-installation error
  // cards: which GitHub account was actually authorized, and a signed install
  // link so the dead end always offers the way out (GitHub's authorize screen
  // itself has no install step — the exact trap of EXP-363).
  // Mobile flows never see the web claim page (it sits behind the login wall
  // a bearer-only native user can't pass) — every error hands back to the app
  // via the deep-link page instead (EXP-365).
  const errorRedirect = (
    error: string,
    extra?: { login?: string; install?: string }
  ) => {
    if (fromMobile) return mobileConnectedResponse(error)
    let location = `/integrations/github/claim?error=${error}`
    if (extra?.login) location += `&login=${encodeURIComponent(extra.login)}`
    if (extra?.install)
      location += `&install=${encodeURIComponent(extra.install)}`
    return new Response(null, {
      status: 302,
      headers: { location },
    })
  }

  try {
    // Validate the state BEFORE spending the one-shot authorization code: a
    // logged-out landing must burn neither the nonce nor the code, so a
    // signed-in retry can still succeed. The acting user is the claim's
    // embedded user: identical to the session user when a session exists, and
    // the state-proven initiator for cookie-less mobile flows (native clients
    // are bearer-only — see consumeGithubSetupState's mobile exception).
    const sessionUserId = await resolveSessionUserId(request)
    const claim = consumeGithubSetupState(state, sessionUserId, {
      expectOauth: true,
    })
    if (!claim?.teamId) {
      return errorRedirect(`session`)
    }
    const actingUserId = claim.userId
    const teamId = claim.teamId

    if (!code) return errorRedirect(`exchange`)
    const userToken = await exchangeGithubOAuthCode(code)
    if (!userToken) return errorRedirect(`exchange`)

    const installations = await listUserInstallations(userToken)

    // EXP-363: keep only installations the OAuth user controls. A null login
    // means /user itself failed — nothing is verifiable, so fail closed as a
    // transient exchange error (retry-able), not as "not the owner".
    const viewerLogin = await getAuthenticatedGithubLogin(userToken)
    if (!viewerLogin) return errorRedirect(`exchange`)
    const { controlled, orgPermissionBlocked, undeterminedIds } =
      await partitionControlledInstallations(installations, {
        viewerLogin,
        orgMembership: (org) => getUserOrgMembershipState(userToken, org),
      })
    const controlledIds = new Set(controlled.map((inst) => inst.id))

    // Mirror every CONTROLLED installation (account fields only). The rows
    // must exist before the claim page can render account names, and the
    // upsert also heals stale logins after renames. Uncontrolled enumerations
    // are never mirrored from here — the installation webhook is their
    // authoritative writer.
    const rowIds = new Map<number, string>()
    for (const inst of controlled) {
      const [row] = await db
        .insert(githubInstallations)
        .values({
          installationId: inst.id,
          accountLogin: inst.account || null,
          accountType: inst.accountType || null,
        })
        .onConflictDoUpdate({
          target: githubInstallations.installationId,
          set: {
            accountLogin: inst.account || null,
            accountType: inst.accountType || null,
            updatedAt: new Date(),
          },
        })
        .returning({ id: githubInstallations.id })
      if (row) rowIds.set(inst.id, row.id)
    }

    // Capture this user's USER-SCOPED repo entitlement per installation while
    // the transient token still exists — `GET /user/installations` attributes
    // an installation to anyone who can access even ONE of its repos, so the
    // link alone must never open the whole installation. These grant rows are
    // what integrations.repos (discovery) and assertRepoInstallationAccess
    // (connect) gate on. REPLACE semantics per (team, installation, user)
    // so a re-auth cleanly refreshes this user's set. Best-effort per
    // installation: one failed listing must not abort the linking below (that
    // installation just contributes no grants until the next re-auth).
    // The loop walks ALL enumerated installations, but only CONTROLLED ones
    // capture repos — an uncontrolled enumeration runs the DELETE alone, so a
    // re-auth actively scrubs any pre-EXP-363 collaborator grants that would
    // otherwise resurrect if the real owner later linked the installation to
    // a team both users share. UNDETERMINED installations (org membership
    // unverifiable because the App's members-read permission isn't approved
    // yet) are skipped entirely: ambiguity blocks linking above, but must not
    // destroy a possibly-legitimate member's existing grants — they'd only
    // come back via a post-approval re-auth.
    for (const inst of installations) {
      if (undeterminedIds.has(inst.id)) continue
      try {
        let repos: Awaited<
          ReturnType<typeof listUserInstallationRepos>
        >[`repos`] = []
        if (controlledIds.has(inst.id)) {
          repos = (await listUserInstallationRepos(userToken, inst.id)).repos
          if (repos.length === 0) {
            // A CONTROLLED installation listing zero repos is usually the
            // fresh-install propagation race — GitHub's user-installation repo
            // endpoint runs empty for a few seconds right after the install is
            // created (EXP-365: this stranded a brand-new connect with zero
            // grants until a manual reconnect). One short retry rides it out;
            // an account that is genuinely empty stays honestly empty.
            await sleep(GRANT_CAPTURE_RETRY_MS)
            repos = (await listUserInstallationRepos(userToken, inst.id)).repos
            if (repos.length === 0) {
              console.warn(
                `[github-callback] controlled installation ${inst.id} listed zero repos after retry — capturing an empty grant set`
              )
            }
          }
        }
        await db.transaction(async (tx) => {
          await tx
            .delete(githubInstallationRepoGrants)
            .where(
              and(
                eq(githubInstallationRepoGrants.teamId, teamId),
                eq(githubInstallationRepoGrants.installationId, inst.id),
                eq(githubInstallationRepoGrants.grantedByUserId, actingUserId)
              )
            )
          if (repos.length > 0) {
            await tx.insert(githubInstallationRepoGrants).values(
              repos.map((repo) => ({
                teamId,
                installationId: inst.id,
                fullName: repo.fullName,
                private: repo.private,
                defaultBranch: repo.defaultBranch ?? null,
                grantedByUserId: actingUserId,
              }))
            )
          }
        })
      } catch (err) {
        console.warn(
          `[github-callback] repo-grant capture failed for installation ${inst.id}:`,
          err
        )
      }
    }

    // Self-heal pre-EXP-363 residue (EXP-365): the broken pre-fix claim flow
    // could link a stranger's installation to a team, and such a link can
    // never leave on its own — the linking user doesn't control it, so a
    // re-auth scrubs-only and `needsReauth` warns forever. This re-auth just
    // proved which installations the user controls, so reap the team's links
    // that are ALL of: created by this same user (their own residue, never a
    // teammate's connection), not controlled and not undetermined by this
    // enumeration, not suspended (REV2-29: links survive suspension, and a
    // suspended installation may be missing from the enumeration without
    // proving lost control), and load-bearing for nothing — zero team grants
    // from ANY member and zero active repository rows (mirrors `unlink`'s
    // guard: deleting a link kills token minting for its connected repos).
    try {
      await assertCanManageRepos(actingUserId, teamId)
      const links = await db
        .select({
          linkId: githubInstallationLinks.id,
          createdByUserId: githubInstallationLinks.createdByUserId,
          installationId: githubInstallations.installationId,
          suspendedAt: githubInstallations.suspendedAt,
        })
        .from(githubInstallationLinks)
        .innerJoin(
          githubInstallations,
          eq(githubInstallations.id, githubInstallationLinks.githubInstallationId)
        )
        .where(eq(githubInstallationLinks.teamId, teamId))
      const candidates = links.filter(
        (link) =>
          link.createdByUserId === actingUserId &&
          !controlledIds.has(link.installationId) &&
          !undeterminedIds.has(link.installationId) &&
          link.suspendedAt == null
      )
      if (candidates.length > 0) {
        const candidateIds = candidates.map((link) => link.installationId)
        const granted = await db
          .select({
            installationId: githubInstallationRepoGrants.installationId,
          })
          .from(githubInstallationRepoGrants)
          .where(
            and(
              eq(githubInstallationRepoGrants.teamId, teamId),
              inArray(githubInstallationRepoGrants.installationId, candidateIds)
            )
          )
        const connected = await db
          .select({ installationId: repositories.installationId })
          .from(repositories)
          .where(
            and(
              eq(repositories.teamId, teamId),
              inArray(repositories.installationId, candidateIds),
              isNull(repositories.archivedAt)
            )
          )
        const loadBearing = new Set([
          ...granted.map((row) => row.installationId),
          ...connected.map((row) => row.installationId),
        ])
        const staleLinkIds = candidates
          .filter((link) => !loadBearing.has(link.installationId))
          .map((link) => link.linkId)
        if (staleLinkIds.length > 0) {
          await db
            .delete(githubInstallationLinks)
            .where(inArray(githubInstallationLinks.id, staleLinkIds))
          console.warn(
            `[github-callback] self-healed ${staleLinkIds.length} stale installation link(s) for team ${teamId}`
          )
        }
      }
    } catch (err) {
      // Not a repo manager (demoted since linking) — leave the links alone.
      // Anything else (a DB failure mid-heal) is unexpected; log it while
      // staying fail-safe toward leaving links untouched.
      if (!(err instanceof TRPCError)) {
        console.warn(`[github-callback] self-heal aborted unexpectedly:`, err)
      }
    }

    // Token's job is done — it never leaves this scope. The grant set just
    // changed, so any cached (grant-derived) repo list for this team is
    // stale even when no new link lands below.
    invalidateRepoCache(teamId)

    // Every no-linkable-installation dead end carries a signed install link:
    // the connect flow is OAuth-FIRST and GitHub's authorize screen has no
    // install step, so without this a user who never installed the App (or
    // only has collaborator access to someone else's) had no path to GitHub's
    // account/repo selection page at all.
    if (installations.length === 0 || controlled.length === 0) {
      const installState = mintGithubSetupState(actingUserId, {
        dialog: fromDialog,
        mobile: fromMobile,
        teamId,
      })
      const installUrl = githubAppInstallUrl(installState) ?? undefined
      if (installations.length === 0) {
        return errorRedirect(`none`, { install: installUrl })
      }
      // Enumerated but nothing controlled: a collaborator-only authorize
      // (notowner), or org installations stuck behind an unapproved
      // members-read permission update (orgperm).
      return errorRedirect(orgPermissionBlocked ? `orgperm` : `notowner`, {
        login: viewerLogin,
        install: installUrl,
      })
    }

    if (controlled.length === 1) {
      const rowId = rowIds.get(controlled[0].id)
      try {
        await assertCanManageRepos(actingUserId, teamId)
      } catch {
        return errorRedirect(`forbidden`)
      }
      if (rowId) {
        await db
          .insert(githubInstallationLinks)
          .values({
            teamId,
            githubInstallationId: rowId,
            createdByUserId: actingUserId,
          })
          .onConflictDoNothing()
        invalidateRepoCache(teamId)
      }
      if (fromMobile) return mobileConnectedResponse()
      return new Response(null, {
        status: 302,
        headers: {
          location: fromDialog ? `/integrations/github/installed` : `/`,
        },
      })
    }

    // Several controlled installations — hand off to the in-app account
    // picker. The ticket binds user + team + the exact verified id set;
    // linking is idempotent, so it needs no nonce. Known limitation: a
    // cookie-less mobile flow lands on the web claim page's login wall here —
    // acceptable for now, multi-installation accounts are the rare case and
    // the page shows the mobile return card once signed in.
    const ticket = mintGithubClaimTicket({
      u: actingUserId,
      w: teamId,
      ids: controlled.map((i) => i.id),
      ...(fromMobile ? { m: true } : {}),
      ...(fromDialog ? { d: true } : {}),
    })
    if (!ticket) return errorRedirect(`session`)
    return new Response(null, {
      status: 302,
      headers: {
        location: `/integrations/github/claim?ticket=${encodeURIComponent(ticket)}`,
      },
    })
  } catch (err) {
    console.error(`[github-callback] failed:`, err)
    return errorRedirect(`exchange`)
  }
}

export const Route = createFileRoute(`/api/integrations/github/callback`)({
  server: {
    handlers: {
      GET: ({ request }) => handleCallback(request),
    },
  },
})

import { createFileRoute } from "@tanstack/react-router"
import { db } from "@/db/connection"
import { githubInstallationLinks, githubInstallations } from "@/db/schema"
import { resolveSessionUserId } from "@/lib/auth/resolve-bearer"
import {
  getInstallation,
  githubOAuthAuthorizeUrl,
  githubOAuthConfigured,
} from "@/lib/integrations/github-app"
import { mobileConnectedResponse } from "@/lib/integrations/github-return-page"
import {
  consumeGithubSetupState,
  githubSetupStateWantsDialog,
  githubSetupStateWantsMobile,
  mintGithubSetupState,
} from "@/lib/integrations/github-setup-state"
import {
  assertCanManageRepos,
  invalidateRepoCache,
  invalidateRepoCacheForInstallation,
} from "@/lib/trpc/integrations"

// GitHub App install "Setup URL": GitHub redirects the user's browser here after
// they install (or, with "Redirect on update" enabled, reconfigure) the App,
// with `?installation_id=…&setup_action=…`. We mirror the installation row and
// — when the signed state carries a target team — create the team ↔
// installation link (the claim). This round-trip is the claim FALLBACK for
// instances without the App's OAuth client secret; the primary claim path is
// the OAuth callback (./callback.ts). Token minting itself doesn't need any of
// this — the App JWT finds a repo's installation on demand.
export async function handleSetup(request: Request): Promise<Response> {
  const url = new URL(request.url)
  const installationId = Number(url.searchParams.get(`installation_id`))
  const state = url.searchParams.get(`state`)
  // When the install was launched from an in-app dialog (the dialog flag rides
  // inside the signed state payload), land on a small client page that lets
  // the original tab re-detect the connection; otherwise land on the app
  // root — GitHub App management lives in team settings → Repositories,
  // which self-detects on focus. A mobile-marked state (native client launched
  // the install in an external browser) instead gets a 200 page that fires the
  // exponential://github-connected deep link to hand the user back to the app.
  const fromMobile = githubSetupStateWantsMobile(state)
  const fromDialog = githubSetupStateWantsDialog(state)
  const okLocation = fromDialog ? `/integrations/github/installed` : `/`
  const ok = fromMobile
    ? mobileConnectedResponse()
    : new Response(null, {
        status: 302,
        headers: { location: okLocation },
      })

  try {
    // `installation_id` is a guessable integer and this redirect is reachable
    // by anyone, so creating a team link demands proof that the CURRENT
    // session user launched this very flow from inside the app: the signed
    // single-use state token minted with the install link (HMAC over user id +
    // team id + nonce + expiry, see github-setup-state.ts) must verify
    // AND match the session. Record the installation regardless — "installed"
    // must not depend on who happened to be signed in — just unlinked
    // (claimable later via the OAuth flow or another round-trip).
    const sessionUserId = await resolveSessionUserId(request)
    const claim = consumeGithubSetupState(state, sessionUserId)
    if (Number.isFinite(installationId) && installationId > 0) {
      const info = await getInstallation(installationId)
      const [row] = await db
        .insert(githubInstallations)
        .values({
          installationId,
          accountLogin: info?.account ?? null,
          accountType: info?.accountType ?? null,
        })
        .onConflictDoUpdate({
          target: githubInstallations.installationId,
          set: {
            accountLogin: info?.account ?? null,
            accountType: info?.accountType ?? null,
            updatedAt: new Date(),
          },
        })
        .returning({ id: githubInstallations.id })

      // SECURITY (cross-account repo leak): the URL `installation_id` is
      // attacker-controlled and UNVERIFIED. A valid signed `state` proves only
      // that the caller launched a connect flow for THEIR OWN team — it is
      // minted before any installation exists, so it binds user + nonce +
      // team and NEVER binds an installation id. The old code here trusted
      // the URL id anyway, so any signed-in user could hand-craft
      // /api/integrations/github/setup?installation_id=<guessable>&state=<their
      // own> and link a stranger's installation to their team, then read
      // its private repos through the installation token. (GitHub does NOT sign
      // this redirect, so "it came from GitHub's install page" cannot be
      // assumed.)
      //
      // The only proof of GitHub *account* control is the OAuth
      // `/user/installations` enumeration (the ./callback route), which links
      // exactly the installations GitHub says the user can access. So whenever
      // OAuth is configured — all of cloud, and any multi-tenant instance — we
      // NEVER link from this redirect; we hand the caller into that OAuth claim
      // to prove control. The direct link survives ONLY as the self-hosted
      // fallback for instances with no OAuth client secret (single-tenant,
      // trusted), which have no other link path.
      if (claim?.teamId && sessionUserId && row) {
        if (githubOAuthConfigured()) {
          // Refresh caches for any team already legitimately linked to
          // this installation (setup_action=update grants/revokes repos), then
          // bounce into the proof-of-control OAuth flow instead of linking.
          await invalidateRepoCacheForInstallation(installationId)
          const oauthUrl = githubOAuthAuthorizeUrl(
            mintGithubSetupState(sessionUserId, {
              dialog: fromDialog,
              mobile: fromMobile,
              teamId: claim.teamId,
              oauth: true,
            })
          )
          if (oauthUrl) {
            return new Response(null, {
              status: 302,
              headers: { location: oauthUrl },
            })
          }
          // Falls through to `ok` if the URL couldn't be minted — installation
          // stays recorded but unlinked (claimable later via the OAuth flow).
        } else {
          // Self-hosted fallback (no OAuth client secret ⇒ no proof-of-control
          // path exists). Single-tenant/trusted only.
          try {
            await assertCanManageRepos(sessionUserId, claim.teamId)
            await db
              .insert(githubInstallationLinks)
              .values({
                teamId: claim.teamId,
                githubInstallationId: row.id,
                createdByUserId: sessionUserId,
              })
              .onConflictDoNothing()
            invalidateRepoCache(claim.teamId)
          } catch (err) {
            console.warn(
              `[github-setup] link skipped (cannot manage team ${claim.teamId}):`,
              err
            )
          }
        }
      }

      // The installation's repo selection may have just changed (this redirect
      // also fires on setup_action=update when "Redirect on update" is on) —
      // drop every linked team's cached repo list.
      await invalidateRepoCacheForInstallation(installationId)
    }
    return ok
  } catch (err) {
    console.error(`[github-setup] failed:`, err)
    // The install itself succeeded on GitHub's side — only our recording/
    // linking failed (webhooks + the OAuth claim cover that) — so a mobile
    // flow must still hand the user back to the app.
    if (fromMobile) return mobileConnectedResponse()
    return new Response(null, {
      status: 302,
      headers: { location: `/` },
    })
  }
}

export const Route = createFileRoute(`/api/integrations/github/setup`)({
  server: {
    handlers: {
      GET: ({ request }) => handleSetup(request),
    },
  },
})

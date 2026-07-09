import { createFileRoute } from "@tanstack/react-router"
import { and, eq } from "drizzle-orm"
import { db } from "@/db/connection"
import {
  githubInstallationLinks,
  githubInstallationRepoGrants,
  githubInstallations,
} from "@/db/schema"
import { resolveSessionUserId } from "@/lib/auth/resolve-bearer"
import {
  exchangeGithubOAuthCode,
  listUserInstallationRepos,
  listUserInstallations,
} from "@/lib/integrations/github-app"
import { mobileConnectedResponse } from "@/lib/integrations/github-return-page"
import {
  consumeGithubSetupState,
  githubSetupStateWantsDialog,
  githubSetupStateWantsMobile,
  mintGithubClaimTicket,
} from "@/lib/integrations/github-setup-state"
import {
  assertCanManageRepos,
  invalidateRepoCache,
} from "@/lib/trpc/integrations"

// GitHub App OAuth callback — the PRIMARY workspace claim path. The user
// arrives from a single lightweight authorize screen (no configure page, no
// forced settings change — the thing that made the install round-trip so
// tedious, especially on mobile). We exchange the code for a TRANSIENT
// user-to-server token, ask GitHub which installations that user can access
// (`GET /user/installations` — GitHub's own proof of account control), mirror
// them into github_installations, and link them to the state's target
// workspace: directly when there's exactly one, via the /integrations/github/
// claim picker page when there are several. The token is used for that single
// enumeration and discarded — never persisted, so expiry/refresh never exist.
async function handleCallback(request: Request): Promise<Response> {
  const url = new URL(request.url)
  const code = url.searchParams.get(`code`)
  const state = url.searchParams.get(`state`)
  const fromMobile = githubSetupStateWantsMobile(state)
  const fromDialog = githubSetupStateWantsDialog(state)

  const errorRedirect = (error: string) =>
    new Response(null, {
      status: 302,
      headers: { location: `/integrations/github/claim?error=${error}` },
    })

  try {
    // Validate the state BEFORE spending the one-shot authorization code: a
    // logged-out landing must burn neither the nonce nor the code, so a
    // signed-in retry can still succeed.
    const sessionUserId = await resolveSessionUserId(request)
    const claim = consumeGithubSetupState(state, sessionUserId, {
      expectOauth: true,
    })
    if (!claim?.workspaceId || !sessionUserId) {
      return errorRedirect(`session`)
    }
    const workspaceId = claim.workspaceId

    if (!code) return errorRedirect(`exchange`)
    const userToken = await exchangeGithubOAuthCode(code)
    if (!userToken) return errorRedirect(`exchange`)

    const installations = await listUserInstallations(userToken)

    // Mirror every enumerated installation (account fields only). The rows
    // must exist before the claim page can render account names, and the
    // upsert also heals stale logins after renames.
    const rowIds = new Map<number, string>()
    for (const inst of installations) {
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
    // (connect) gate on. REPLACE semantics per (workspace, installation, user)
    // so a re-auth cleanly refreshes this user's set. Best-effort per
    // installation: one failed listing must not abort the linking below (that
    // installation just contributes no grants until the next re-auth).
    // Captured for ALL enumerated installations regardless of which the user
    // ultimately links — the pickers only ever read grants for LINKED
    // installations, so grants for un-linked ones are inert.
    for (const inst of installations) {
      try {
        const { repos } = await listUserInstallationRepos(userToken, inst.id)
        await db.transaction(async (tx) => {
          await tx
            .delete(githubInstallationRepoGrants)
            .where(
              and(
                eq(githubInstallationRepoGrants.workspaceId, workspaceId),
                eq(githubInstallationRepoGrants.installationId, inst.id),
                eq(githubInstallationRepoGrants.grantedByUserId, sessionUserId)
              )
            )
          if (repos.length > 0) {
            await tx.insert(githubInstallationRepoGrants).values(
              repos.map((repo) => ({
                workspaceId,
                installationId: inst.id,
                fullName: repo.fullName,
                private: repo.private,
                defaultBranch: repo.defaultBranch ?? null,
                grantedByUserId: sessionUserId,
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
    // Token's job is done — it never leaves this scope. The grant set just
    // changed, so any cached (grant-derived) repo list for this workspace is
    // stale even when no new link lands below.
    invalidateRepoCache(workspaceId)

    if (installations.length === 0) {
      return errorRedirect(`none`)
    }

    if (installations.length === 1) {
      const rowId = rowIds.get(installations[0].id)
      try {
        await assertCanManageRepos(sessionUserId, workspaceId)
      } catch {
        return errorRedirect(`forbidden`)
      }
      if (rowId) {
        await db
          .insert(githubInstallationLinks)
          .values({
            workspaceId,
            githubInstallationId: rowId,
            createdByUserId: sessionUserId,
          })
          .onConflictDoNothing()
        invalidateRepoCache(workspaceId)
      }
      if (fromMobile) return mobileConnectedResponse()
      return new Response(null, {
        status: 302,
        headers: {
          location: fromDialog ? `/integrations/github/installed` : `/`,
        },
      })
    }

    // Several installations — hand off to the in-app account picker. The
    // ticket binds user + workspace + the exact verified id set; linking is
    // idempotent, so it needs no nonce.
    const ticket = mintGithubClaimTicket({
      u: sessionUserId,
      w: workspaceId,
      ids: installations.map((i) => i.id),
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
    if (fromMobile) return mobileConnectedResponse()
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

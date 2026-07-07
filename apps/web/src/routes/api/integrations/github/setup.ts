import { createFileRoute } from "@tanstack/react-router"
import { sql } from "drizzle-orm"
import { db } from "@/db/connection"
import { githubInstallations } from "@/db/schema"
import { resolveSessionUserId } from "@/lib/auth/resolve-bearer"
import { getInstallation } from "@/lib/integrations/github-app"
import {
  consumeGithubSetupState,
  githubSetupStateWantsDialog,
} from "@/lib/integrations/github-setup-state"
import { invalidateRepoCache } from "@/lib/trpc/integrations"

// GitHub App install "Setup URL": GitHub redirects the user's browser here after
// they install (or update) the App, with `?installation_id=…&setup_action=…`.
// We capture the installation (for the UI "installed" state) plus the
// installation → user mapping, then bounce back into the app. Token minting
// itself doesn't need this — the App JWT finds a repo's installation on demand.
async function handleSetup(request: Request): Promise<Response> {
  const url = new URL(request.url)
  const installationId = Number(url.searchParams.get(`installation_id`))
  const state = url.searchParams.get(`state`)
  // When the install was launched from an in-app dialog (the dialog flag rides
  // inside the signed state payload), land on a small client page that lets
  // the original tab re-detect the connection; otherwise land on the app
  // root — the standalone /account/integrations page is gone (L25: GitHub App
  // management lives in workspace settings → Repositories, which self-detects
  // on focus).
  const fromDialog = githubSetupStateWantsDialog(state)
  const okLocation = fromDialog ? `/integrations/github/installed` : `/`
  const ok = new Response(null, {
    status: 302,
    headers: { location: okLocation },
  })

  try {
    // `installation_id` is a guessable integer and this redirect is reachable
    // by anyone, so user attribution demands proof that the CURRENT session
    // user launched this very install from inside the app: the signed
    // single-use state token minted with the install link (HMAC over user id +
    // nonce + expiry, see github-setup-state.ts) must verify AND match the
    // session. Record the installation regardless — "installed" must not
    // depend on who happened to be signed in — just unattributed (visible to
    // instance admins only, like the webhook path).
    const sessionUserId = await resolveSessionUserId(request)
    const userId = consumeGithubSetupState(state, sessionUserId)?.userId ?? null
    if (Number.isFinite(installationId) && installationId > 0) {
      const info = await getInstallation(installationId)
      await db
        .insert(githubInstallations)
        .values({
          installationId,
          accountLogin: info?.account ?? null,
          accountType: info?.accountType ?? null,
          userId,
        })
        .onConflictDoUpdate({
          target: githubInstallations.installationId,
          set: {
            accountLogin: info?.account ?? null,
            accountType: info?.accountType ?? null,
            // Attribution is first-writer-wins: only fill a NULL user_id,
            // never clobber an existing owner — an overwrite would both steal
            // the connect-path authority over this installation and flip the
            // real owner's UI to "not installed".
            ...(userId
              ? {
                  userId: sql`coalesce(${githubInstallations.userId}, ${userId})`,
                }
              : {}),
            updatedAt: new Date(),
          },
        })
      // The user's installable-repo set just changed; drop their cached list so
      // the next `repos` query (the re-detect fired when they return) re-hits
      // GitHub instead of serving a stale pre-install snapshot.
      if (userId) invalidateRepoCache(userId)
    }
    return ok
  } catch (err) {
    console.error(`[github-setup] failed:`, err)
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

import { createFileRoute } from "@tanstack/react-router"
import { db } from "@/db/connection"
import { githubInstallations } from "@/db/schema"
import { resolveSessionUserId } from "@/lib/auth/resolve-bearer"
import { getInstallation } from "@/lib/integrations/github-app"

// GitHub App install "Setup URL": GitHub redirects the user's browser here after
// they install (or update) the App, with `?installation_id=…&setup_action=…`.
// We capture the installation (for the UI "installed" state) plus the
// installation → user mapping when the browser carries a session, then bounce
// back to Integrations. Token minting itself doesn't need this — the App JWT
// finds a repo's installation on demand.
async function handleSetup(request: Request): Promise<Response> {
  const url = new URL(request.url)
  const installationId = Number(url.searchParams.get(`installation_id`))
  // When the install was launched from the in-app project/agent dialog
  // (state=dialog), land on a small client page that lets the original tab
  // re-detect the connection; otherwise keep the standalone Integrations flow.
  const fromDialog = url.searchParams.get(`state`) === `dialog`
  const okLocation = fromDialog
    ? `/integrations/github/installed`
    : `/account/integrations?github=installed`
  const ok = new Response(null, {
    status: 302,
    headers: { location: okLocation },
  })

  try {
    // User attribution is best-effort: the redirect can land without a session
    // (fresh browser, expired cookie). Record the installation regardless —
    // "installed" must not depend on who happened to be signed in — and only
    // stamp user_id when we actually know the user (never clobber an existing
    // owner with null).
    const userId = await resolveSessionUserId(request)
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
            ...(userId ? { userId } : {}),
            updatedAt: new Date(),
          },
        })
    }
    return ok
  } catch (err) {
    console.error(`[github-setup] failed:`, err)
    return new Response(null, {
      status: 302,
      headers: { location: `/account/integrations?github=error` },
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

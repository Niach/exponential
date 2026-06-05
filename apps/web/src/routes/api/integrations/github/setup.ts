import { createFileRoute } from "@tanstack/react-router"
import { db } from "@/db/connection"
import { githubInstallations } from "@/db/schema"
import { resolveSessionUserId } from "@/lib/auth/resolve-bearer"
import { getInstallation } from "@/lib/integrations/github-app"

// GitHub App install "Setup URL": GitHub redirects the user's browser here after
// they install (or update) the App, with `?installation_id=…&setup_action=…`.
// We're in the user's authenticated session, so we capture the installation →
// user mapping (for the UI "installed" state) and bounce back to Integrations.
// Token minting itself doesn't need this — the App JWT finds a repo's
// installation on demand.
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
    const userId = await resolveSessionUserId(request)
    if (userId && Number.isFinite(installationId) && installationId > 0) {
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
            userId,
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

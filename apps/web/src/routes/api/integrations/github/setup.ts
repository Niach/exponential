import { createFileRoute } from "@tanstack/react-router"
import { sql } from "drizzle-orm"
import { db } from "@/db/connection"
import { githubInstallations } from "@/db/schema"
import { resolveSessionUserId } from "@/lib/auth/resolve-bearer"
import { getInstallation } from "@/lib/integrations/github-app"
import {
  consumeGithubSetupState,
  githubSetupStateWantsDialog,
  githubSetupStateWantsMobile,
} from "@/lib/integrations/github-setup-state"
import { invalidateRepoCache } from "@/lib/trpc/integrations"

// Deep link a native client listens for after launching the install flow in
// an external browser/Custom Tab. Fired from a 200 HTML page (not a 302 to
// exp://) for the same reason as /api/mobile-oauth-return: a bare redirect to
// a custom scheme leaves the browser tab spinning on an uncompletable
// navigation, while a rendered page both fires the handoff from JS and shows
// a confirmation the browser can display.
const MOBILE_DEEP_LINK = `exp://github-connected`

function renderGithubConnectedPage(): string {
  return `<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8" />
<meta name="viewport" content="width=device-width, initial-scale=1" />
<title>GitHub connected — Exponential</title>
<style>
  :root { color-scheme: dark; }
  html, body { height: 100%; margin: 0; }
  body {
    display: grid; place-items: center;
    font-family: ui-sans-serif, system-ui, -apple-system, "Segoe UI", Roboto, sans-serif;
    background: #09090b; color: #fafafa;
    -webkit-font-smoothing: antialiased;
  }
  .card {
    text-align: center; padding: 2.5rem 2rem; max-width: 24rem;
    border: 1px solid #27272a; border-radius: 16px; background: #18181b;
  }
  .check {
    width: 48px; height: 48px; margin: 0 auto 1.25rem;
    border-radius: 999px; background: #22c55e1a;
    display: grid; place-items: center; color: #22c55e;
  }
  h1 { font-size: 1.25rem; font-weight: 600; margin: 0 0 0.5rem; }
  p { font-size: 0.9rem; line-height: 1.5; color: #a1a1aa; margin: 0 0 1.5rem; }
  a.btn {
    display: inline-block; text-decoration: none; font-size: 0.875rem; font-weight: 500;
    padding: 0.5rem 1rem; border-radius: 8px; background: #fafafa; color: #09090b;
  }
</style>
</head>
<body>
  <main class="card">
    <div class="check">
      <svg width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round"><path d="M20 6 9 17l-5-5"/></svg>
    </div>
    <h1>GitHub connected</h1>
    <p>Exponential is opening. You can close this tab and return to the app.</p>
    <a class="btn" href="${MOBILE_DEEP_LINK}">Return to the app</a>
  </main>
  <script>
    // Hand off to the native app immediately; the confirmation card stays put.
    window.location.href = ${JSON.stringify(MOBILE_DEEP_LINK)};
  </script>
</body>
</html>
`
}

function mobileConnectedResponse(): Response {
  return new Response(renderGithubConnectedPage(), {
    status: 200,
    headers: {
      "content-type": `text/html; charset=utf-8`,
      "cache-control": `no-store`,
    },
  })
}

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
  // on focus). A mobile-marked state (native client launched the install in an
  // external browser) instead gets a 200 page that fires the
  // exp://github-connected deep link to hand the user back to the app.
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
    // The install itself succeeded on GitHub's side — only our recording/
    // attribution failed (webhooks + admin self-heal cover that) — so a
    // mobile flow must still hand the user back to the app.
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

import { githubConnectedDeepLink } from "@/lib/deep-link"

// Deep link a native client listens for after launching the GitHub install or
// OAuth-claim flow in an external browser/Custom Tab. Fired from a 200 HTML
// page (not a 302 to exponential://) for the same reason as
// /api/mobile-oauth-return: a bare redirect to a custom scheme leaves the
// browser tab spinning on an uncompletable navigation, while a rendered page
// both fires the handoff from JS and shows a confirmation the browser can
// display. Shared by the install setup route and the OAuth claim callback.
//
// EXP-365: error outcomes ALSO hand back to the app (with `?error=<code>` on
// the deep link) instead of redirecting to web routes — the callback's web
// error pages live behind the login wall, which a bearer-only native user
// can't pass inside the in-app browser tab.

// Human copy per callback error code; the claim page (claim.tsx) holds the
// web-side equivalents. Unknown codes fall back to the generic line.
const ERROR_COPY: Record<string, string> = {
  session: `The connect link expired or was already used. Start the connect flow again from the app.`,
  exchange: `GitHub sign-in didn't complete. Return to the app and try again.`,
  none: `The Exponential GitHub App isn't installed for any account you can access yet. Return to the app and use Connect GitHub to install it.`,
  notowner: `The authorized GitHub account only has collaborator access to existing installations. Install the App on your own account or organization.`,
  orgperm: `Your organization hasn't approved the App's members-read permission yet. An org admin must accept it on GitHub, then reconnect.`,
  forbidden: `Only team owners can connect GitHub accounts to a team.`,
}

export function renderGithubConnectedPage(error?: string): string {
  const deepLink = githubConnectedDeepLink(error)
  const failed = Boolean(error)
  const title = failed ? `Couldn't connect GitHub` : `GitHub connected`
  const body = failed
    ? (ERROR_COPY[error!] ??
      `Something went wrong while connecting GitHub. Return to the app and try again.`)
    : `Exponential is opening. You can close this tab and return to the app.`
  const icon = failed
    ? `<svg width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round"><path d="M18 6 6 18"/><path d="m6 6 12 12"/></svg>`
    : `<svg width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round"><path d="M20 6 9 17l-5-5"/></svg>`
  return `<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8" />
<meta name="viewport" content="width=device-width, initial-scale=1" />
<title>${title} — Exponential</title>
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
    border-radius: 999px; background: ${failed ? `#ef44441a` : `#22c55e1a`};
    display: grid; place-items: center; color: ${failed ? `#ef4444` : `#22c55e`};
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
      ${icon}
    </div>
    <h1>${title}</h1>
    <p>${body}</p>
    <a class="btn" href="${deepLink}">Return to the app</a>
  </main>
  <script>
    // Hand off to the native app immediately; the card stays put.
    window.location.href = ${JSON.stringify(deepLink)};
  </script>
</body>
</html>
`
}

export function mobileConnectedResponse(error?: string): Response {
  return new Response(renderGithubConnectedPage(error), {
    status: 200,
    headers: {
      "content-type": `text/html; charset=utf-8`,
      "cache-control": `no-store`,
    },
  })
}

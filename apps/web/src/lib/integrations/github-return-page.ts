import { githubConnectedDeepLink } from "@/lib/deep-link"

// Deep link a native client listens for after launching the GitHub install or
// OAuth-claim flow in an external browser/Custom Tab. Fired from a 200 HTML
// page (not a 302 to exponential://) for the same reason as
// /api/mobile-oauth-return: a bare redirect to a custom scheme leaves the
// browser tab spinning on an uncompletable navigation, while a rendered page
// both fires the handoff from JS and shows a confirmation the browser can
// display. Shared by the install setup route and the OAuth claim callback.
const MOBILE_DEEP_LINK = githubConnectedDeepLink()

export function renderGithubConnectedPage(): string {
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

export function mobileConnectedResponse(): Response {
  return new Response(renderGithubConnectedPage(), {
    status: 200,
    headers: {
      "content-type": `text/html; charset=utf-8`,
      "cache-control": `no-store`,
    },
  })
}

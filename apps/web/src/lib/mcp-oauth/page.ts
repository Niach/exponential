// EXP-792: the tiny HTML page the anonymous OAuth callback answers with. A
// provider redirects the user's browser here after consent; there is
// nothing to hand off (the device finishes the exchange on its own), so the
// page only confirms and tells the user to close the tab. Same look as the
// GitHub return page (lib/integrations/github-return-page.ts) minus the
// deep link.

export function escapeHtml(value: string): string {
  return value
    .replace(/&/g, `&amp;`)
    .replace(/</g, `&lt;`)
    .replace(/>/g, `&gt;`)
    .replace(/"/g, `&quot;`)
    .replace(/'/g, `&#39;`)
}

export function renderMcpOauthPage(args: {
  ok: boolean
  title: string
  body: string
}): string {
  const icon = args.ok
    ? `<svg width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round"><path d="M20 6 9 17l-5-5"/></svg>`
    : `<svg width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round"><path d="M18 6 6 18"/><path d="m6 6 12 12"/></svg>`
  return `<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8" />
<meta name="viewport" content="width=device-width, initial-scale=1" />
<meta name="robots" content="noindex" />
<title>${escapeHtml(args.title)} · Exponential</title>
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
    border-radius: 999px; background: ${args.ok ? `#22c55e1a` : `#ef44441a`};
    display: grid; place-items: center; color: ${args.ok ? `#22c55e` : `#ef4444`};
  }
  h1 { font-size: 1.25rem; font-weight: 600; margin: 0 0 0.5rem; }
  p { font-size: 0.9rem; line-height: 1.5; color: #a1a1aa; margin: 0; }
</style>
</head>
<body>
  <main class="card">
    <div class="check">
      ${icon}
    </div>
    <h1>${escapeHtml(args.title)}</h1>
    <p>${escapeHtml(args.body)}</p>
  </main>
</body>
</html>
`
}

export function mcpOauthPageResponse(args: {
  ok: boolean
  title: string
  body: string
}): Response {
  return new Response(renderMcpOauthPage(args), {
    status: 200,
    headers: {
      "content-type": `text/html; charset=utf-8`,
      "cache-control": `no-store`,
      "x-robots-tag": `noindex`,
    },
  })
}

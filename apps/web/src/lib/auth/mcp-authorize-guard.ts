import { eq } from "drizzle-orm"
import { db } from "@/db/connection"
import { oauthApplications } from "@/db/schema"

// Pre-flight for GET /api/auth/mcp/authorize, run before better-auth sees the
// request. Two jobs:
//
// 1. Fail LOUDLY on stale client registrations. MCP clients cache their
//    dynamic client registration forever; if the server has lost the row
//    (fresh database, reset instance) better-auth redirects the browser to
//    its /error page, which bounces to the app root — the user just sees the
//    webapp open and the flow silently dies. Render an actionable error page
//    instead. (Per RFC 6749 §4.1.2.1 an unvalidated redirect_uri must never
//    receive the error, so a page for the human is the correct channel.)
//
// 2. Force prompt=consent so EVERY authorization passes through the
//    /auth/consent scope-selection screen — mcp_grants rows are only written
//    there, and a token without a grant has no access.
export async function guardMcpAuthorize(
  request: Request
): Promise<{ request: Request } | { response: Response }> {
  const url = new URL(request.url)
  const clientId = url.searchParams.get(`client_id`)
  const redirectUri = url.searchParams.get(`redirect_uri`)

  if (clientId) {
    const [client] = await db
      .select({
        redirectUrls: oauthApplications.redirectUrls,
        disabled: oauthApplications.disabled,
      })
      .from(oauthApplications)
      .where(eq(oauthApplications.clientId, clientId))
      .limit(1)

    if (!client || client.disabled) {
      return {
        response: errorPage(
          `This MCP client isn't registered here`,
          `Your MCP client presented a saved registration this server doesn't know — usually because the client cached credentials from before the server's database changed.`,
          `To fix it, clear this server's authentication in your MCP client and authenticate again. In Claude Code: run /mcp, select this server, choose "Clear authentication", then authenticate — the client will re-register automatically.`
        ),
      }
    }

    if (
      redirectUri &&
      !client.redirectUrls.split(`,`).includes(redirectUri)
    ) {
      return {
        response: errorPage(
          `Sign-in link out of date`,
          `The callback address your MCP client is using doesn't match the one it registered.`,
          `Clear this server's authentication in your MCP client and authenticate again to refresh the registration.`
        ),
      }
    }
  }

  if (url.searchParams.get(`prompt`) !== `consent`) {
    url.searchParams.set(`prompt`, `consent`)
    return { request: new Request(url, request) }
  }
  return { request }
}

function errorPage(title: string, detail: string, remedy: string) {
  const esc = (s: string) =>
    s.replace(/&/g, `&amp;`).replace(/</g, `&lt;`).replace(/>/g, `&gt;`)
  const html = `<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<meta name="robots" content="noindex">
<title>${esc(title)} — Exponential</title>
<style>
  body { margin: 0; min-height: 100vh; display: flex; align-items: center; justify-content: center;
         background: #09090b; color: #fafafa; font: 15px/1.6 ui-sans-serif, system-ui, sans-serif; }
  main { max-width: 26rem; padding: 2rem; }
  h1 { font-size: 1.15rem; margin: 0 0 .75rem; }
  p { margin: 0 0 .75rem; color: #a1a1aa; }
  p.remedy { color: #fafafa; }
  code { background: #18181b; border: 1px solid #27272a; border-radius: 4px; padding: 0 .3em; }
</style>
</head>
<body>
<main>
  <h1>${esc(title)}</h1>
  <p>${esc(detail)}</p>
  <p class="remedy">${esc(remedy)}</p>
</main>
</body>
</html>`
  return new Response(html, {
    status: 400,
    headers: { "content-type": `text/html; charset=utf-8` },
  })
}

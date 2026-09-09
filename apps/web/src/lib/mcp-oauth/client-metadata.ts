// EXP-792: the OAuth Client ID Metadata Document (CIMD, the MCP
// authorization spec's client-registration shortcut). A provider that
// advertises `client_id_metadata_document_supported` accepts this URL AS the
// client id and fetches it to learn the redirect URIs, so the device needs
// no dynamic registration. Static, anonymous and cacheable by construction;
// only meaningful on a public https instance (the provider must be able to
// fetch it, and a plain-http client id is refused by every spec-following
// AS), so anything else 404s and the device falls back to DCR.
import { appBaseUrl } from "@/lib/notification-email-policy"

export function mcpOauthClientMetadata(base = appBaseUrl()) {
  const root = base.replace(/\/$/, ``)
  return {
    client_id: `${root}/api/mcp-oauth/client.json`,
    client_name: `Exponential`,
    client_uri: root,
    redirect_uris: [
      `${root}/api/mcp-oauth/callback`,
      // The device's loopback listener picks its port at bind time; RFC 8252
      // lets an AS match loopback redirects on host alone.
      `http://127.0.0.1/callback`,
      `http://localhost/callback`,
    ],
    grant_types: [`authorization_code`, `refresh_token`],
    response_types: [`code`],
    token_endpoint_auth_method: `none`,
  }
}

export function handleMcpOauthClientMetadata(): Response {
  const base = appBaseUrl()
  if (!base.startsWith(`https://`)) {
    return new Response(`Not found`, {
      status: 404,
      headers: { "cache-control": `no-store` },
    })
  }
  return new Response(JSON.stringify(mcpOauthClientMetadata(base)), {
    status: 200,
    headers: {
      "content-type": `application/json`,
      "cache-control": `public, max-age=3600`,
    },
  })
}

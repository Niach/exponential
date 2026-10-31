// Test helper: a fake MCP server + OAuth authorization server behind one
// `fetch` stand-in. The MCP endpoint is https://mcp.example.com/mcp; its PRM
// answers ONLY at the path-suffixed well-known (the Sentry shape), naming
// https://auth.example.com as the AS. The token endpoint mints `at-<n>` /
// `rt-<n>` pairs and records every form it received; the MCP endpoint wants
// `Authorization: Bearer <a live access token>`, answers initialize as JSON
// with an `mcp-session-id` and tools/list as SSE.
export const MCP_URL = `https://mcp.example.com/mcp`
export const AS_ISSUER = `https://auth.example.com`

export interface FakeProviderOptions {
  cimd?: boolean
  registration?: boolean
  /** The refresh grant answers 400 invalid_grant. */
  refreshFails?: boolean
  /** The refresh answer carries no new refresh token. */
  refreshRotates?: boolean
  expiresIn?: number
  tools?: number
  /** Fields merged over the AS metadata document (hostile-provider cases). */
  metadata?: Record<string, unknown>
}

export interface FakeProvider {
  fetch: (input: RequestInfo | URL, init?: RequestInit) => Promise<Response>
  calls: Array<{ method: string; url: string }>
  tokenForms: URLSearchParams[]
  registrations: unknown[]
  mcpRequests: Array<{ headers: Headers; body: Record<string, unknown> }>
  liveTokens: Set<string>
}

const json = (body: unknown, status = 200, headers: Record<string, string> = {}) =>
  new Response(JSON.stringify(body), {
    status,
    headers: { "content-type": `application/json`, ...headers },
  })

export function fakeProvider(options: FakeProviderOptions = {}): FakeProvider {
  let minted = 0
  const provider: FakeProvider = {
    calls: [],
    tokenForms: [],
    registrations: [],
    mcpRequests: [],
    liveTokens: new Set(),
    fetch: async (input, init = {}) => {
      const url = typeof input === `string` ? input : input.toString()
      const method = (init.method ?? `GET`).toUpperCase()
      provider.calls.push({ method, url })
      if (url === `https://mcp.example.com/.well-known/oauth-protected-resource/mcp`) {
        return json({
          resource: MCP_URL,
          authorization_servers: [AS_ISSUER],
          scopes_supported: [`read`, `write`],
        })
      }
      if (url === `${AS_ISSUER}/.well-known/oauth-authorization-server`) {
        return json({
          issuer: AS_ISSUER,
          authorization_endpoint: `${AS_ISSUER}/authorize`,
          token_endpoint: `${AS_ISSUER}/token`,
          ...(options.registration === false
            ? {}
            : { registration_endpoint: `${AS_ISSUER}/register` }),
          client_id_metadata_document_supported: options.cimd === true,
          ...options.metadata,
        })
      }
      if (url.startsWith(`${AS_ISSUER}/register`) && method === `POST`) {
        provider.registrations.push(JSON.parse(String(init.body)))
        return json({ client_id: `dcr-client-${provider.registrations.length}` }, 201)
      }
      if (url === `${AS_ISSUER}/token` && method === `POST`) {
        const form = new URLSearchParams(String(init.body))
        provider.tokenForms.push(form)
        if (form.get(`grant_type`) === `refresh_token` && options.refreshFails) {
          return json({ error: `invalid_grant`, error_description: `refresh token revoked` }, 400)
        }
        minted += 1
        const access = `at-${minted}`
        provider.liveTokens.add(access)
        const rotate =
          form.get(`grant_type`) === `authorization_code` || options.refreshRotates !== false
        return json({
          access_token: access,
          token_type: `bearer`,
          expires_in: options.expiresIn ?? 3600,
          ...(rotate ? { refresh_token: `rt-${minted}` } : {}),
        })
      }
      if (url === MCP_URL) {
        const headers = new Headers(init.headers)
        if (method === `DELETE`) return new Response(null, { status: 204 })
        const body = JSON.parse(String(init.body)) as Record<string, unknown>
        provider.mcpRequests.push({ headers, body })
        const auth = headers.get(`authorization`) ?? ``
        if (!provider.liveTokens.has(auth.replace(/^Bearer /, ``))) {
          return json({ error: `unauthorized` }, 401, {
            "www-authenticate": `Bearer resource_metadata="https://mcp.example.com/.well-known/oauth-protected-resource/mcp"`,
          })
        }
        if (body.method === `initialize`) {
          return json(
            { jsonrpc: `2.0`, id: body.id, result: { protocolVersion: `2025-06-18`, capabilities: {} } },
            200,
            { "mcp-session-id": `sess-1` }
          )
        }
        if (body.method === `notifications/initialized`) {
          return new Response(null, { status: 202 })
        }
        if (body.method === `tools/list`) {
          const tools = Array.from({ length: options.tools ?? 3 }, (_, i) => ({ name: `t${i}` }))
          const sse =
            `: keep-alive\n\n` +
            `event: message\ndata: ${JSON.stringify({ jsonrpc: `2.0`, method: `notifications/progress` })}\n\n` +
            `event: message\ndata: ${JSON.stringify({ jsonrpc: `2.0`, id: body.id, result: { tools } })}\n\n`
          return new Response(sse, { status: 200, headers: { "content-type": `text/event-stream` } })
        }
      }
      return new Response(`not found`, { status: 404 })
    },
  }
  return provider
}

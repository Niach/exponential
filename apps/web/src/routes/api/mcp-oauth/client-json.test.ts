import { afterEach, describe, expect, it } from "vitest"

// EXP-792: the OAuth Client ID Metadata Document. Static, cacheable JSON on
// a public https base; a plain-http base (dev, LAN self-host) 404s so the
// device falls back to dynamic registration.

import {
  handleMcpOauthClientMetadata,
  mcpOauthClientMetadata,
} from "@/lib/mcp-oauth/client-metadata"

const ORIGINAL = process.env.BETTER_AUTH_URL

afterEach(() => {
  if (ORIGINAL === undefined) delete process.env.BETTER_AUTH_URL
  else process.env.BETTER_AUTH_URL = ORIGINAL
})

describe(`GET /api/mcp-oauth/client.json`, () => {
  it(`serves the CIMD document on an https base`, async () => {
    process.env.BETTER_AUTH_URL = `https://app.exponential.dev/`
    const res = handleMcpOauthClientMetadata()
    expect(res.status).toBe(200)
    expect(res.headers.get(`content-type`)).toBe(`application/json`)
    expect(res.headers.get(`cache-control`)).toBe(`public, max-age=3600`)
    expect(await res.json()).toEqual({
      client_id: `https://app.exponential.dev/api/mcp-oauth/client.json`,
      client_name: `Exponential`,
      client_uri: `https://app.exponential.dev`,
      redirect_uris: [
        `https://app.exponential.dev/api/mcp-oauth/callback`,
        `http://127.0.0.1/callback`,
        `http://localhost/callback`,
      ],
      grant_types: [`authorization_code`, `refresh_token`],
      response_types: [`code`],
      token_endpoint_auth_method: `none`,
    })
  })

  it(`404s when the app base is not https`, () => {
    process.env.BETTER_AUTH_URL = `http://localhost:5173`
    const res = handleMcpOauthClientMetadata()
    expect(res.status).toBe(404)
    expect(res.headers.get(`cache-control`)).toBe(`no-store`)
  })

  it(`the document's client_id is its own URL`, () => {
    const doc = mcpOauthClientMetadata(`https://exp.example.com`)
    expect(doc.client_id).toBe(`https://exp.example.com/api/mcp-oauth/client.json`)
    expect(doc.redirect_uris[0]).toBe(`https://exp.example.com/api/mcp-oauth/callback`)
  })
})

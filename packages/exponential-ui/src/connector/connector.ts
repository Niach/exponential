// VAPP-91: the Exponential connector. A third-party host runs a declarative
// vapp against an Exponential instance: OAuth to Exponential (the MCP OAuth
// grant, EXP-792 — discovery, dynamic client registration, PKCE S256, the
// person's consent picks the teams/boards) and `exp:` binding resolvers over
// that instance's MCP tools (an OAuth token is confined to /api/mcp and to
// its grant). Plain fetch + WebCrypto; it knows the instance's PUBLIC HTTP
// API only, nothing of the app's code.

import type { HostFunction } from "../host/runtime"
import type { ParsedSource, SourceEmit, SourceResolvers } from "../host/sources"

export interface AuthServerMetadata {
  issuer: string
  authorization_endpoint: string
  token_endpoint: string
  registration_endpoint?: string
}

export interface ConnectorTokens {
  accessToken: string
  refreshToken?: string
  /** Epoch ms. */
  expiresAt?: number
}

export interface ConnectorOptions {
  /** The Exponential instance, e.g. `https://app.exponential.at`. */
  baseUrl: string
  /** Where the instance sends the person back with `?code=&state=`. */
  redirectUri: string
  clientName?: string
  /** A client id registered earlier (skips dynamic registration). */
  clientId?: string
  tokens?: ConnectorTokens
  /** Called whenever tokens change (persist them). */
  onTokens?: (tokens: ConnectorTokens) => void
  fetch?: typeof fetch
  /** Seconds between re-reads of a bound source (MCP has no live feed). */
  pollSeconds?: number
}

const b64url = (bytes: Uint8Array) => btoa(String.fromCharCode(...bytes)).replace(/\+/g, `-`).replace(/\//g, `_`).replace(/=+$/, ``)

/** A PKCE verifier and its S256 challenge. */
export async function pkcePair(): Promise<{ verifier: string; challenge: string }> {
  const verifier = b64url(crypto.getRandomValues(new Uint8Array(32)))
  const digest = new Uint8Array(await crypto.subtle.digest(`SHA-256`, new TextEncoder().encode(verifier)))
  return { verifier, challenge: b64url(digest) }
}

export class ExponentialConnector {
  private metadata?: AuthServerMetadata
  private clientId?: string
  tokens?: ConnectorTokens
  private rpcId = 0

  constructor(private options: ConnectorOptions) {
    this.clientId = options.clientId
    this.tokens = options.tokens
  }

  private get fetch(): typeof fetch {
    return this.options.fetch ?? fetch
  }

  private url(path: string): string {
    return new URL(path, this.options.baseUrl).href
  }

  async discover(): Promise<AuthServerMetadata> {
    if (this.metadata) return this.metadata
    const res = await this.fetch(this.url(`/.well-known/oauth-authorization-server`))
    if (!res.ok) throw new Error(`discovery failed: HTTP ${res.status}`)
    this.metadata = (await res.json()) as AuthServerMetadata
    return this.metadata
  }

  /** Dynamic client registration (a public client: PKCE, no secret). */
  async register(): Promise<string> {
    if (this.clientId) return this.clientId
    const meta = await this.discover()
    if (!meta.registration_endpoint) throw new Error(`the instance offers no dynamic client registration; pass clientId`)
    const res = await this.fetch(meta.registration_endpoint, {
      method: `POST`,
      headers: { "content-type": `application/json` },
      body: JSON.stringify({ client_name: this.options.clientName ?? `Exponential UI host`, redirect_uris: [this.options.redirectUri], grant_types: [`authorization_code`, `refresh_token`], response_types: [`code`], token_endpoint_auth_method: `none` }),
    })
    if (!res.ok) throw new Error(`registration failed: HTTP ${res.status}`)
    this.clientId = ((await res.json()) as { client_id: string }).client_id
    return this.clientId
  }

  /** Where to send the person. Keep `verifier` + `state` for the callback. */
  async authorizeUrl(): Promise<{ url: string; verifier: string; state: string }> {
    const meta = await this.discover()
    const clientId = await this.register()
    const { verifier, challenge } = await pkcePair()
    const state = b64url(crypto.getRandomValues(new Uint8Array(16)))
    const url = new URL(meta.authorization_endpoint)
    url.search = new URLSearchParams({ response_type: `code`, client_id: clientId, redirect_uri: this.options.redirectUri, code_challenge: challenge, code_challenge_method: `S256`, state, scope: `openid profile email offline_access` }).toString()
    return { url: url.href, verifier, state }
  }

  private async token(body: Record<string, string>): Promise<ConnectorTokens> {
    const meta = await this.discover()
    const res = await this.fetch(meta.token_endpoint, { method: `POST`, headers: { "content-type": `application/x-www-form-urlencoded` }, body: new URLSearchParams({ client_id: this.clientId ?? ``, ...body }).toString() })
    if (!res.ok) throw new Error(`token request failed: HTTP ${res.status}`)
    const json = (await res.json()) as { access_token: string; refresh_token?: string; expires_in?: number }
    this.tokens = { accessToken: json.access_token, refreshToken: json.refresh_token ?? this.tokens?.refreshToken, expiresAt: json.expires_in ? Date.now() + json.expires_in * 1000 : undefined }
    this.options.onTokens?.(this.tokens)
    return this.tokens
  }

  /** The callback: trade the code for tokens. */
  async exchange(code: string, verifier: string): Promise<ConnectorTokens> {
    await this.register()
    return this.token({ grant_type: `authorization_code`, code, code_verifier: verifier, redirect_uri: this.options.redirectUri })
  }

  async refresh(): Promise<ConnectorTokens> {
    if (!this.tokens?.refreshToken) throw new Error(`no refresh token; authorize again`)
    return this.token({ grant_type: `refresh_token`, refresh_token: this.tokens.refreshToken })
  }

  /** One MCP tool call on the instance, as the person who consented.
   *  Returns the tool's JSON payload (its text content parsed). */
  async callTool(name: string, args: Record<string, unknown> = {}): Promise<unknown> {
    if (!this.tokens) throw new Error(`not connected; authorize first`)
    if (this.tokens.expiresAt && this.tokens.expiresAt < Date.now() + 30_000 && this.tokens.refreshToken) await this.refresh()
    const res = await this.fetch(this.url(`/api/mcp`), {
      method: `POST`,
      headers: { "content-type": `application/json`, accept: `application/json, text/event-stream`, authorization: `Bearer ${this.tokens.accessToken}` },
      body: JSON.stringify({ jsonrpc: `2.0`, id: ++this.rpcId, method: `tools/call`, params: { name, arguments: args } }),
    })
    if (!res.ok) throw new Error(`MCP HTTP ${res.status}`)
    const text = await res.text()
    const line = text.trimStart().startsWith(`{`) ? text : (text.split(`\n`).find((l) => l.startsWith(`data:`))?.slice(5) ?? `{}`)
    const reply = JSON.parse(line) as { result?: { content?: { type: string; text?: string }[]; structuredContent?: unknown; isError?: boolean }; error?: { message?: string } }
    if (reply.error) throw new Error(reply.error.message ?? `MCP error`)
    const result = reply.result ?? {}
    const body = result.structuredContent ?? (result.content ?? []).filter((c) => c.type === `text`).map((c) => c.text ?? ``).join(``)
    if (result.isError) throw new Error(typeof body === `string` ? body : `tool error`)
    if (typeof body !== `string`) return body
    try {
      return JSON.parse(body)
    } catch {
      return body
    }
  }

  /** The `exp:` scheme over MCP: `exp:issues?board=<id>&team=<id>&limit=<n>&search=<q>`,
   *  `exp:boards?team=<id>`, `exp:teams`, `exp:members?team=<id>`. Each
   *  emits `{rows, count}` and re-reads every `pollSeconds` (default 30, the
   *  `poll` param overrides; 0 = once). Unknown names emit `null`. */
  sources(): SourceResolvers {
    return { exp: (source, emit) => this.follow(source, emit) }
  }

  private follow(source: ParsedSource, emit: SourceEmit): () => void {
    const p = source.params
    const read = async (): Promise<unknown> => {
      switch (source.name) {
        case `issues`: {
          const args: Record<string, unknown> = { limit: Number(p.limit ?? 50) }
          if (p.board) args.boardId = p.board
          if (p.team) args.teamId = p.team
          if (p.search) args.search = p.search
          return this.callTool(`exponential_issues_list`, args)
        }
        case `boards`:
          return this.callTool(`exponential_boards_list`, p.team ? { teamId: p.team } : {})
        case `teams`:
          return this.callTool(`exponential_teams_list`, {})
        case `members`:
          return this.callTool(`exponential_members_list`, p.team ? { teamId: p.team } : {})
        default:
          return null
      }
    }
    let live = true
    let timer: ReturnType<typeof setTimeout> | undefined
    const seconds = Number(p.poll ?? this.options.pollSeconds ?? 30)
    const tick = async () => {
      try {
        const value = await read()
        if (!live) return
        const rows = Array.isArray(value) ? value : (value as { rows?: unknown[] } | null)?.rows
        emit(rows ? { rows, count: rows.length } : value)
      } catch (e) {
        if (live) emit({ rows: [], count: 0, error: e instanceof Error ? e.message : String(e) })
      }
      if (live && seconds > 0) timer = setTimeout(tick, seconds * 1000)
    }
    void tick()
    return () => {
      live = false
      if (timer) clearTimeout(timer)
    }
  }

  /** `exponential.mcp({tool, arguments})` for an ExponentialHost (gate it
   *  with an `ask` policy: it acts as the person). */
  functions(): Record<string, HostFunction> {
    return { "exponential.mcp": (args) => this.callTool(String(args.tool ?? ``), (args.arguments ?? {}) as Record<string, unknown>) }
  }
}

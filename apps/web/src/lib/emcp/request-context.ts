// #NEW-144: what every tool handler of `/api/mcp` gets beside the SDK's own
// context: `ctx.client` (the caller's per-request capabilities, parsed ONCE
// by the gate, packages/emcp/fixtures/protocol.json `clientCases`),
// `ctx.principal` (the member and the credential an MRTR requestState is
// sealed to) and `ctx.call` (the wire tool and its raw arguments, what the
// state is bound to).
import { clientContextOf, type ClientContext } from "@exp/emcp/protocol"

export type { ClientContext } from "@exp/emcp/protocol"

/** A legacy client that declared nothing (tests, internal callers). */
export const NO_CLIENT: ClientContext = clientContextOf(null)

export interface Principal {
  userId: string
  /** The credential: `oauth:<clientId>`, `key:<keyId>` or `member`. */
  grant: string
}

export interface EmcpContext {
  client: ClientContext
  principal: Principal
  /** The wire call this request serves (`method` absent = `tools/call`):
   *  the tool (prompt, resource uri) and its raw arguments. */
  call?: { method?: string; tool: string; args: Record<string, unknown> }
}

/** The SDK's handler context (`mcpReq`: method, inputResponses,
 *  requestState…) with Exponential's. */
export type EmcpToolContext = EmcpContext & {
  mcpReq?: {
    method?: string
    inputResponses?: Record<string, unknown>
    requestState?: <T = unknown>() => T | undefined
  }
}

/** The credential name of a caller. */
export function grantOf(credential: { clientId: string | null; keyId?: string | null }): string {
  if (credential.clientId) return `oauth:${credential.clientId}`
  if (credential.keyId) return `key:${credential.keyId}`
  return `member`
}

/** `extra` (the SDK's context, or whatever a test passed) with ours. */
export function withEmcpContext(extra: unknown, ctx: EmcpContext): EmcpToolContext {
  const base = typeof extra === `object` && extra !== null ? extra : {}
  return { ...base, ...ctx }
}

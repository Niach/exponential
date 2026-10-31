// The one outbound HTTP door for server-side MCP work (discovery, client
// registration, token exchange/refresh, probe, test). 20s budget per
// request. On the cloud (CLOUD_INSTANCE=true) a member-typed URL must not
// reach the instance's own network: the host has to resolve to public
// addresses only (lib/import/public-address.ts), redirects are refused so a
// public URL cannot bounce to a private one, and the residual DNS-rebind
// window is the same accepted one as the import path. A self-host may point
// at its LAN (a local MCP server is a legitimate target there) and its GET
// discovery may follow redirects; a POST never does, anywhere: a 307/308
// from a token or registration endpoint would re-post the refresh token or
// client secret to wherever it pointed.
import {
  isPublicAddress,
  resolvesToPublicAddresses,
} from "@/lib/import/public-address"

export const MCP_HTTP_TIMEOUT_MS = 20_000

export class McpHttpError extends Error {}

function cloud(): boolean {
  return (process.env.CLOUD_INSTANCE ?? ``).toLowerCase() === `true`
}

/** Refuse a URL the server may not fetch. Throws McpHttpError with a
 * sentence safe to show the member. */
export async function assertFetchableUrl(raw: string): Promise<URL> {
  let url: URL
  try {
    url = new URL(raw)
  } catch {
    throw new McpHttpError(`not an absolute URL`)
  }
  if (url.protocol !== `https:` && url.protocol !== `http:`) {
    throw new McpHttpError(`unsupported URL scheme`)
  }
  if (!cloud()) return url
  if (url.protocol !== `https:`) {
    throw new McpHttpError(`only https:// URLs are reachable from this instance`)
  }
  const host = url.hostname.replace(/^\[|\]$/g, ``)
  const literal = /^[\d.]+$/.test(host) || host.includes(`:`)
  const ok = literal
    ? isPublicAddress(host)
    : await resolvesToPublicAddresses(host).catch(() => false)
  if (!ok) {
    throw new McpHttpError(`${url.hostname} is not a public address`)
  }
  return url
}

/** fetch() behind the guard and the timeout. Transport failures become a
 * McpHttpError naming only the kind of failure (never the URL's query,
 * never a token). */
export async function mcpFetch(url: string, init: RequestInit = {}): Promise<Response> {
  await assertFetchableUrl(url)
  const method = (init.method ?? `GET`).toUpperCase()
  try {
    return await fetch(url, {
      ...init,
      redirect: method === `GET` && !cloud() ? `follow` : `error`,
      signal: AbortSignal.timeout(MCP_HTTP_TIMEOUT_MS),
    })
  } catch (e) {
    const name = (e as { name?: string })?.name
    // A refused redirect: undici wraps it as TypeError("fetch failed") with
    // an "unexpected redirect" cause, Bun throws "Unexpected redirect".
    const text = `${(e as { message?: string })?.message ?? ``} ${
      (e as { cause?: { message?: string } })?.cause?.message ?? ``
    }`
    throw new McpHttpError(
      name === `TimeoutError` || name === `AbortError`
        ? `timed out`
        : /redirect/i.test(text)
          ? `answered with a redirect`
          : `could not connect`
    )
  }
}

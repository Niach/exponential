import "@dotenvx/dotenvx/config"
import { ELECTRIC_PROTOCOL_QUERY_PARAMS } from "@electric-sql/client"

/**
 * Gets the Electric SQL endpoint URL based on environment configuration.
 *
 * If running in production, or `USE_ELECTRIC_URL` is set to `true`, the `ELECTRIC_URL` env var is used,
 * if available, otherwise the default cloud endpoint is used.
 * Otherwise, the local docker endpoint is used, assuming default port 30000.
 */
function getElectricUrl(): string {
  return process.env.ELECTRIC_URL || `http://localhost:30000`
}

/**
 * Prepares the Electric SQL proxy URL from a request URL
 * Copies over Electric-specific query params and adds auth if configured
 * @param requestUrl - The incoming request URL
 * @returns The prepared Electric SQL origin URL
 */
export function prepareElectricUrl(requestUrl: string): URL {
  const url = new URL(requestUrl)
  const electricUrl = getElectricUrl()
  const originUrl = new URL(`${electricUrl}/v1/shape`)

  // Copy Electric-specific query params
  url.searchParams.forEach((value, key) => {
    if (ELECTRIC_PROTOCOL_QUERY_PARAMS.includes(key)) {
      originUrl.searchParams.set(key, value)
    }
  })

  // Add Electric Cloud authentication if configured
  if (process.env.ELECTRIC_SOURCE_ID && process.env.ELECTRIC_SECRET) {
    originUrl.searchParams.set(`source_id`, process.env.ELECTRIC_SOURCE_ID)
    originUrl.searchParams.set(`secret`, process.env.ELECTRIC_SECRET)
  }

  return originUrl
}

/**
 * Proxies a request to Electric SQL and returns the response.
 *
 * Buffers the upstream body fully before responding so the Bun server can
 * send a properly-framed HTTP/1.1 response with a known content-length —
 * streaming `response.body` directly produced chunked-encoding tails that
 * Traefik logged as `EOF` → 502. Forwarding the inbound AbortSignal cancels
 * the upstream when the browser hangs up (very common: the Electric client
 * cancels long-polls every time a shape handle is invalidated).
 */
export async function proxyElectricRequest(
  originUrl: URL,
  signal?: AbortSignal
): Promise<Response> {
  let response: Response
  try {
    response = await fetch(originUrl, { signal })
  } catch (error) {
    if (signal?.aborted || (error instanceof Error && error.name === `AbortError`)) {
      // Client hung up before upstream responded — nothing to send back.
      return new Response(null, { status: 499, statusText: `Client Closed Request` })
    }
    return new Response(`Upstream fetch failed`, { status: 502 })
  }

  let body: ArrayBuffer
  try {
    body = await response.arrayBuffer()
  } catch (error) {
    if (signal?.aborted || (error instanceof Error && error.name === `AbortError`)) {
      return new Response(null, { status: 499, statusText: `Client Closed Request` })
    }
    return new Response(`Upstream body read failed`, { status: 502 })
  }

  const headers = new Headers(response.headers)
  headers.delete(`content-encoding`)
  headers.delete(`content-length`)
  headers.delete(`transfer-encoding`)
  headers.set(`vary`, `cookie`)

  return new Response(body, {
    status: response.status,
    statusText: response.statusText,
    headers,
  })
}

/**
 * Which backend a capture run talks to, read off the environment. The
 * preflight probes THAT stack's services rather than the repo's default
 * compose project, so an isolated stack (its own compose project or relay
 * on other ports, VAPP-170) passes without a stand-in.
 */

/** The steer relay the stub dials and the clients steer through. */
export function relayUrl(env: Record<string, string | undefined>): string {
  return env.SHOTS_RELAY_URL?.trim() || `ws://localhost:4002`
}

/** The relay's public `/healthz` (HTTP on the WebSocket's authority). */
export function relayHealthUrl(ws: string): string {
  const url = new URL(ws)
  url.protocol = url.protocol === `wss:` ? `https:` : url.protocol === `ws:` ? `http:` : url.protocol
  url.pathname = `/healthz`
  url.search = ``
  url.hash = ``
  return url.toString()
}

/**
 * An isolated stack names its own proxy (`SHOTS_PROXY_URL`): the repo's
 * default compose services are not what it runs on, so they are not checked.
 */
export function isolatedStack(env: Record<string, string | undefined>): boolean {
  const proxy = env.SHOTS_PROXY_URL?.trim().replace(/\/+$/, ``)
  return Boolean(proxy) && proxy !== `https://localhost:3000`
}

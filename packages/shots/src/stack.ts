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

/**
 * What a `--skip-seed` run makes of this checkout's stub record
 * (apps/web/scripts/lib/stub-record.ts): `none` = start a stub, `starting` =
 * wait for that one to come online (#EXP-1886: starting a second stub then
 * broke every later lane), `online` = reuse it. A record for another relay or
 * database, or of a dead process, is `none`.
 */
export function stubState(
  record: { pid: number; relayUrl: string; databaseUrl: string; online: boolean } | undefined,
  here: { relayUrl: string; databaseUrl?: string; alive: (pid: number) => boolean }
): `none` | `starting` | `online` {
  if (!record || record.relayUrl !== here.relayUrl) return `none`
  if (here.databaseUrl && record.databaseUrl && record.databaseUrl !== here.databaseUrl) return `none`
  if (!here.alive(record.pid)) return `none`
  return record.online ? `online` : `starting`
}

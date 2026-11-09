// #LOGS-2: diagnostics (Sentry SDKs) → our self-hosted GlitchTip, ALWAYS
// through this backend. Every client (web, desktop, cli, eDE, iOS, Android)
// asks `GET /api/monitoring/config?platform=<p>` for its DSN and sends its
// envelopes to the tunnel `POST /api/monitoring`, which forwards them to
// GLITCHTIP_INTERNAL_URL; only the web server's own SDK talks to the internal
// URL directly. Dependency-free: the route files and the server SDK init
// import it.
//
// OFF unless configured: no DSN lives in the repo, so self-hosters and dev
// servers answer `{ enabled: false }` until GLITCHTIP_INTERNAL_URL and the
// platform's SENTRY_DSN_<PLATFORM> are set.

export const MONITORING_PLATFORMS = [
  `web`,
  `desktop`,
  `cli`,
  `ede`,
  `ios`,
  `android`,
] as const
export type MonitoringPlatform = (typeof MONITORING_PLATFORMS)[number]

/** The DSN env var per platform (cli shares desktop); `server` = the web server's own SDK. */
const DSN_ENV: Record<MonitoringPlatform | `server`, string> = {
  web: `SENTRY_DSN_WEB`,
  desktop: `SENTRY_DSN_DESKTOP`,
  cli: `SENTRY_DSN_DESKTOP`,
  ede: `SENTRY_DSN_EDE`,
  ios: `SENTRY_DSN_IOS`,
  android: `SENTRY_DSN_ANDROID`,
  server: `SENTRY_DSN_SERVER`,
}

export type MonitoringConfig =
  | {
      enabled: true
      dsn: string
      environment: string
      tracesSampleRate: number
    }
  | { enabled: false }

type Env = Record<string, string | undefined>

export function isMonitoringPlatform(value: unknown): value is MonitoringPlatform {
  return (MONITORING_PLATFORMS as readonly unknown[]).includes(value)
}

/** A DSN's parts, or null when it is not one (`https://<key>@<host>/<projectId>`). */
export type ParsedDsn = { publicKey: string; host: string; projectId: string }

export function parseDsn(dsn: string | undefined): ParsedDsn | null {
  if (!dsn) return null
  try {
    const url = new URL(dsn.trim())
    const projectId = url.pathname.split(`/`).filter(Boolean).pop() ?? ``
    if (!url.username || !/^\d+$/.test(projectId)) return null
    return { publicKey: url.username, host: url.host, projectId }
  } catch {
    return null
  }
}

/** The internal base URL (no trailing slash), or null when unset/invalid. */
export function glitchtipInternalUrl(env: Env): string | null {
  const raw = env.GLITCHTIP_INTERNAL_URL?.trim()
  if (!raw) return null
  try {
    const url = new URL(raw)
    if (url.protocol !== `http:` && url.protocol !== `https:`) return null
    return url.toString().replace(/\/+$/, ``)
  } catch {
    return null
  }
}

/** The DSN configured for one side, or null = off (no internal URL, no or a bad DSN). */
export function dsnFor(env: Env, side: MonitoringPlatform | `server`): string | null {
  if (!glitchtipInternalUrl(env)) return null
  const dsn = env[DSN_ENV[side]]?.trim()
  return dsn && parseDsn(dsn) ? dsn : null
}

/**
 * `SENTRY_ENVIRONMENT` when set, else derived from the instance: a dev server
 * is `dev`, the staging cloud (next.exponential.at) `staging`, every other
 * built instance `prod`.
 */
export function deriveEnvironment(env: Env, isProductionBuild: boolean): string {
  const explicit = env.SENTRY_ENVIRONMENT?.trim()
  if (explicit) return explicit
  if (!isProductionBuild) return `dev`
  try {
    const host = new URL(env.BETTER_AUTH_URL ?? ``).hostname
    if (host.startsWith(`next.`) || host.startsWith(`staging.`)) return `staging`
  } catch {
    /* no or bad base URL: a built instance is prod */
  }
  return `prod`
}

/** `SENTRY_TRACES_SAMPLE_RATE` in [0, 1], default 0.1. */
export function tracesSampleRate(env: Env): number {
  const raw = env.SENTRY_TRACES_SAMPLE_RATE
  if (raw === undefined || raw.trim() === ``) return 0.1
  const n = Number(raw)
  return Number.isFinite(n) && n >= 0 && n <= 1 ? n : 0.1
}

/** THE payload of `GET /api/monitoring/config` (the contract every client codes against). */
export function monitoringConfig(
  env: Env,
  platform: MonitoringPlatform | `server`,
  isProductionBuild: boolean
): MonitoringConfig {
  const dsn = dsnFor(env, platform)
  if (!dsn) return { enabled: false }
  return {
    enabled: true,
    dsn,
    environment: deriveEnvironment(env, isProductionBuild),
    tracesSampleRate: tracesSampleRate(env),
  }
}

/**
 * The GlitchTip projects the tunnel forwards to, keyed by project id: those
 * of the configured per-platform DSNs and nothing else. Host and scheme of an
 * envelope's DSN are ignored (the public host is not what we forward to),
 * and the tunnel forwards with the configured DSN's own key.
 */
export function allowedDsns(env: Env): Map<string, ParsedDsn> {
  const allowed = new Map<string, ParsedDsn>()
  for (const side of Object.keys(DSN_ENV) as (keyof typeof DSN_ENV)[]) {
    const parsed = parseDsn(dsnFor(env, side) ?? undefined)
    if (parsed) allowed.set(parsed.projectId, parsed)
  }
  return allowed
}

/** The internal GlitchTip envelope endpoint for one project (auth by `sentry_key`). */
export function internalEnvelopeUrl(internalBase: string, dsn: ParsedDsn): string {
  return (
    `${internalBase}/api/${dsn.projectId}/envelope/` +
    `?sentry_key=${encodeURIComponent(dsn.publicKey)}&sentry_version=7`
  )
}

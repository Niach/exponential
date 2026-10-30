/**
 * Which database the capture helpers (`seed:screenshots`, `screenshots:ids`,
 * `screenshots:prune-devices`) write to.
 *
 * They run as `bun run …` in `apps/web`, and bun loads `apps/web/.env` there:
 * the SHARED dev database. A throwaway capture stack (its own server on
 * `SHOTS_DEV_URL` / `CAPTURE_BASE_URL`, its own DB) that forgot to export its
 * `DATABASE_URL` therefore seeded and pruned the shared dev DB while it
 * photographed its own (twice on 2026-10-28: device rows deleted). So:
 *
 *   - the capture DB is `SHOTS_DATABASE_URL`, else an EXPORTED `DATABASE_URL`
 *     (never the .env file), and it is handed to every helper explicitly
 *     (an exported variable wins over bun's .env);
 *   - with a throwaway stack configured, a missing capture DB, or one that is
 *     the shared dev DB from `.env`, is REFUSED before anything is seeded,
 *     looked up or pruned.
 *
 * VAPP-170: a worktree whose `.env` files are the isolated stack's OWN (a
 * private compose project on other ports) says so with
 * `SHOTS_PRIVATE_DOTENV=1`: its `.env` DB is then the capture DB, not the
 * shared one, and only the compose default (`localhost:54321/exponential`,
 * the F27 shared dev DB) stays refused.
 */
import { existsSync, readFileSync } from "node:fs"
import { join } from "node:path"

import { repoRoot } from "./paths.ts"

/** The compose default every worktree shares (F27, `local-db-guard.ts`). */
const COMPOSE_SHARED_DB_URL = `postgres://localhost:54321/exponential`

/** The stack every dev runs (`bun dev` / Caddy): not a throwaway one. */
const DEFAULT_STACK_URLS = new Set([`http://localhost:5173`, `http://127.0.0.1:5173`, `http://localhost:3000`, `http://127.0.0.1:3000`])

export interface CaptureDbInput {
  env: Record<string, string | undefined>
  /** `DATABASE_URL` of `apps/web/.env` (else the root `.env`): the shared dev DB. */
  sharedUrl: string | undefined
}

export type CaptureDb = { env: Record<string, string> } | { refused: string }

/** The `DATABASE_URL` a `.env` file sets, if any. */
export function databaseUrlFromDotenv(text: string): string | undefined {
  for (const line of text.split(`\n`)) {
    const match = /^\s*(?:export\s+)?DATABASE_URL\s*=\s*(.*)$/.exec(line)
    if (!match) continue
    const value = match[1]!.trim().replace(/^(['"])(.*)\1$/, `$2`)
    if (value) return value
  }
  return undefined
}

/** Host, port and database name: two spellings of one database compare equal. */
function dbKey(url: string): string | undefined {
  try {
    const parsed = new URL(url)
    const host = parsed.hostname === `127.0.0.1` || parsed.hostname === `::1` || parsed.hostname === `[::1]` ? `localhost` : parsed.hostname
    return `${host}:${parsed.port || `5432`}/${parsed.pathname.replace(/^\//, ``)}`
  } catch {
    return undefined
  }
}

export function sameDatabase(a: string, b: string): boolean {
  const left = dbKey(a)
  return left !== undefined && left === dbKey(b)
}

/** A capture stack other than the default dev server. */
export function throwawayStackUrl(env: Record<string, string | undefined>): string | undefined {
  for (const name of [`SHOTS_DEV_URL`, `CAPTURE_BASE_URL`] as const) {
    const url = env[name]?.trim().replace(/\/+$/, ``)
    if (url && !DEFAULT_STACK_URLS.has(url)) return url
  }
  return undefined
}

/** The env the capture helpers run with, or why they must not run. */
export function resolveCaptureDb({ env, sharedUrl }: CaptureDbInput): CaptureDb {
  const url = env.SHOTS_DATABASE_URL?.trim() || env.DATABASE_URL?.trim()
  const stack = throwawayStackUrl(env)
  const shared = env.SHOTS_PRIVATE_DOTENV?.trim() === `1` ? COMPOSE_SHARED_DB_URL : sharedUrl
  if (stack) {
    if (!url) {
      return {
        refused: `a throwaway capture stack is configured (${stack}) but no SHOTS_DATABASE_URL / DATABASE_URL is exported: the helpers would fall back to apps/web/.env, the SHARED dev DB. Export the stack's DATABASE_URL.`,
      }
    }
    if (shared && sameDatabase(url, shared)) {
      return {
        refused: `a throwaway capture stack is configured (${stack}) but the capture DATABASE_URL is the shared dev DB from .env: refusing to seed, look up or prune it. Export the stack's own DATABASE_URL (or SHOTS_PRIVATE_DOTENV=1 when this worktree's .env is the isolated stack's own).`,
      }
    }
  }
  return { env: url ? { DATABASE_URL: url } : {} }
}

/** The shared dev DB's URL, read from the `.env` files bun would load. */
export function sharedDevDatabaseUrl(): string | undefined {
  for (const path of [join(repoRoot(), `apps/web/.env`), join(repoRoot(), `.env`)]) {
    if (!existsSync(path)) continue
    const url = databaseUrlFromDotenv(readFileSync(path, `utf8`))
    if (url) return url
  }
  return undefined
}

/** `resolveCaptureDb` on this process; throws on a refusal. */
export function captureDbEnv(): Record<string, string> {
  const resolved = resolveCaptureDb({ env: process.env, sharedUrl: sharedDevDatabaseUrl() })
  if (`refused` in resolved) throw new Error(resolved.refused)
  return resolved.env
}

/**
 * The override the helpers need to touch the shared dev DB (F27: they refuse
 * `localhost:54321/exponential` without `--shared-dev-db`). The default stack
 * (no `SHOTS_DEV_URL` / `CAPTURE_BASE_URL` beyond :5173 / :3000) seeds the
 * shared dev DB on purpose, so it passes the flag; a throwaway stack never
 * does, so the helper refuses it as a second line behind `resolveCaptureDb`.
 */
export function sharedDevDbArgs(env: Record<string, string | undefined>): string[] {
  return throwawayStackUrl(env) ? [] : [`--shared-dev-db`]
}

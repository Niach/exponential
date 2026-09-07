// Per-user client-platform ledger (EXP-759). Every authenticated API request
// tells us which client made it — natives send `x-client-version:
// <platform>/<version>` (the 426 gate's header, lib/client-version.ts), and a
// cookie-only request is the web app — so resolveSession touches
// `user_client_platforms` on the way through. The admin console reads it to
// answer "who uses which clients" and "which client did a user start on".
//
// Type-only db import (same rule as lib/conversion/events.ts): this module is
// pulled in by the auth chokepoint, and a runtime @/db/connection import would
// force DATABASE_URL onto every unit test touching it. Callers pass the handle.
import type { db } from "@/db/connection"
import { sql } from "drizzle-orm"
import { userClientPlatforms } from "@/db/schema"
import {
  CLIENT_VERSION_HEADER,
  parseClientVersionHeader,
  type ClientPlatform,
} from "@/lib/client-version"

// `web` is deliberately NOT part of client-version.ts's PLATFORMS: a
// `web/<v>` header must never pass the native 426 gate as a native.
export type UserClientPlatform = ClientPlatform | `web`

export const USER_CLIENT_PLATFORMS: UserClientPlatform[] = [
  `web`,
  `ios`,
  `android`,
  `desktop`,
  `cli`,
]

// Header present → its platform. No header AND no token credential → the
// cookie session, i.e. the web app. A token WITHOUT the header (`expu_` api
// keys driving MCP agents, pre-header native builds) is not a client platform
// we can name — skip rather than guess.
export function deriveClientPlatform(
  request: Request
): { platform: UserClientPlatform; version: string | null } | null {
  const parsed = parseClientVersionHeader(
    request.headers.get(CLIENT_VERSION_HEADER)
  )
  if (parsed) return parsed
  if (
    request.headers.get(`authorization`) ||
    request.headers.get(`x-api-key`)
  ) {
    return null
  }
  return { platform: `web`, version: null }
}

// In-process throttle: at most one write per user+platform per interval per
// process. The (user_id, platform) primary key is the real guarantee (the
// captureReturnVisit precedent) — this map only saves round-trips on the
// natives' ~14 shape long-polls per minute. Bounded by a hard clear.
const TOUCH_INTERVAL_MS = 15 * 60_000
const FAILED_TOUCH_COOLDOWN_MS = 60_000
// `last_version` is varchar(32); the header parser does not bound it.
const MAX_VERSION_LENGTH = 32
const MAX_TRACKED = 20_000
const lastTouched = new Map<string, number>()

export function resetClientPlatformThrottle(): void {
  lastTouched.clear()
}

/** Fire-and-forget upsert; returns whether a write was issued. NEVER throws
 * and is never awaited by the caller — analytics must not be able to break
 * session resolution. A failed write shortens the throttle slot to
 * FAILED_TOUCH_COOLDOWN_MS instead of clearing it, so a persistently failing
 * client (an over-long version string, a deleted user with a still-cached
 * session, a DB outage) costs one round-trip per cooldown, not per request. */
export function touchUserClientPlatform(
  dbx: typeof db,
  args: {
    userId: string
    platform: UserClientPlatform
    version: string | null
    now?: number
  }
): boolean {
  const now = args.now ?? Date.now()
  const key = `${args.userId}\0${args.platform}`
  const last = lastTouched.get(key)
  if (last !== undefined && now - last < TOUCH_INTERVAL_MS) return false
  if (lastTouched.size >= MAX_TRACKED) lastTouched.clear()
  lastTouched.set(key, now)
  const onFailure = (err: unknown) => {
    lastTouched.set(key, now - TOUCH_INTERVAL_MS + FAILED_TOUCH_COOLDOWN_MS)
    console.error(`[client-platforms] touch failed:`, err)
  }
  try {
    void dbx
      .insert(userClientPlatforms)
      .values({
        userId: args.userId,
        platform: args.platform,
        lastVersion: args.version?.slice(0, MAX_VERSION_LENGTH) ?? null,
      })
      .onConflictDoUpdate({
        target: [userClientPlatforms.userId, userClientPlatforms.platform],
        set: {
          lastSeenAt: sql`now()`,
          lastVersion: sql`coalesce(excluded.last_version, ${userClientPlatforms.lastVersion})`,
        },
      })
      .catch(onFailure)
  } catch (err) {
    onFailure(err)
  }
  return true
}

// The ledger folded to one row per user, ordered by first use (so
// `platforms[0]` is the client the user started on). A grouped 1:0..1
// subquery so callers can LEFT JOIN it without widening their own fan-out;
// shared by admin.listUsers and the conversions signup journey.
export function platformsByUserSubquery(dbx: typeof db) {
  return dbx
    .select({
      userId: userClientPlatforms.userId,
      platforms: sql<
        string[]
      >`array_agg(${userClientPlatforms.platform} order by ${userClientPlatforms.firstSeenAt})`.as(
        `platforms`
      ),
      lastSeenAt: sql<Date>`max(${userClientPlatforms.lastSeenAt})`.as(
        `platform_last_seen_at`
      ),
    })
    .from(userClientPlatforms)
    .groupBy(userClientPlatforms.userId)
    .as(`ucp`)
}

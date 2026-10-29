import { Database } from "bun:sqlite"
import { createHash } from "node:crypto"
import { mkdirSync } from "node:fs"
import { dirname } from "node:path"

// The accountless mode's state (EXP-1428): installs and the capabilities they
// minted, in one SQLite file next to the relay. No accounts, so nothing here
// is tied to a person; an install is a device token plus the hash of the
// secret that manages it.
//
// Only HASHES of the install secret and of each capability are stored: a
// leaked database file must not let anyone push to (or take over) an install.
// The device token itself has to stay readable — the relay hands it to APNs
// or FCM on every push — and is useless without our APNs key / Firebase
// credential.

export type Platform = `apns` | `fcm`
export type ApnsEnvironment = `production` | `sandbox`

export interface InstallRow {
  id: string
  secretHash: string
  platform: Platform
  token: string
  topic: string | null
  environment: ApnsEnvironment | null
  createdAt: number
  updatedAt: number
}

export interface CapabilityRow {
  id: string
  installId: string
  tokenHash: string
  label: string | null
  expiresAt: number | null
  maxPerHour: number
  windowStart: number
  windowCount: number
  lastUsedAt: number | null
  createdAt: number
}

/** sha256 hex: what the store keeps instead of a secret or capability. */
export function hashSecret(value: string): string {
  return createHash(`sha256`).update(value).digest(`hex`)
}

// Migrations are inline and idempotent: the relay is one process with one
// file, so CREATE … IF NOT EXISTS at open is the whole migration story. A
// future column goes in as an ALTER guarded by a PRAGMA table_info check.
const SCHEMA = [
  `CREATE TABLE IF NOT EXISTS installs (
    id TEXT PRIMARY KEY,
    secret_hash TEXT NOT NULL,
    platform TEXT NOT NULL,
    token TEXT NOT NULL,
    topic TEXT,
    environment TEXT,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
  )`,
  `CREATE TABLE IF NOT EXISTS capabilities (
    id TEXT PRIMARY KEY,
    install_id TEXT NOT NULL,
    token_hash TEXT NOT NULL UNIQUE,
    label TEXT,
    expires_at INTEGER,
    max_per_hour INTEGER NOT NULL,
    window_start INTEGER NOT NULL,
    window_count INTEGER NOT NULL,
    last_used_at INTEGER,
    created_at INTEGER NOT NULL
  )`,
  `CREATE INDEX IF NOT EXISTS capabilities_install_id ON capabilities (install_id)`,
]

interface InstallDbRow {
  id: string
  secret_hash: string
  platform: Platform
  token: string
  topic: string | null
  environment: ApnsEnvironment | null
  created_at: number
  updated_at: number
}

interface CapabilityDbRow {
  id: string
  install_id: string
  token_hash: string
  label: string | null
  expires_at: number | null
  max_per_hour: number
  window_start: number
  window_count: number
  last_used_at: number | null
  created_at: number
}

function toInstall(row: InstallDbRow): InstallRow {
  return {
    id: row.id,
    secretHash: row.secret_hash,
    platform: row.platform,
    token: row.token,
    topic: row.topic,
    environment: row.environment,
    createdAt: row.created_at,
    updatedAt: row.updated_at,
  }
}

function toCapability(row: CapabilityDbRow): CapabilityRow {
  return {
    id: row.id,
    installId: row.install_id,
    tokenHash: row.token_hash,
    label: row.label,
    expiresAt: row.expires_at,
    maxPerHour: row.max_per_hour,
    windowStart: row.window_start,
    windowCount: row.window_count,
    lastUsedAt: row.last_used_at,
    createdAt: row.created_at,
  }
}

const HOUR_MS = 60 * 60 * 1000

export class RelayStore {
  constructor(readonly db: Database) {
    // WAL keeps a push (read + counter write) from blocking behind a
    // registration; harmless on ":memory:".
    db.exec(`PRAGMA journal_mode = WAL`)
    for (const statement of SCHEMA) db.exec(statement)
  }

  /** Opens (creating the directory if needed) the store at `path`; ":memory:" for tests. */
  static open(path: string): RelayStore {
    if (path !== `:memory:`) mkdirSync(dirname(path), { recursive: true })
    return new RelayStore(new Database(path, { create: true }))
  }

  insertInstall(row: InstallRow): void {
    this.db
      .query(
        `INSERT INTO installs (id, secret_hash, platform, token, topic, environment, created_at, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?)`
      )
      .run(row.id, row.secretHash, row.platform, row.token, row.topic, row.environment, row.createdAt, row.updatedAt)
  }

  getInstall(id: string): InstallRow | null {
    const row = this.db.query<InstallDbRow, [string]>(`SELECT * FROM installs WHERE id = ?`).get(id)
    return row ? toInstall(row) : null
  }

  updateInstallToken(id: string, token: string, environment: ApnsEnvironment | null, now: number): void {
    this.db
      .query(`UPDATE installs SET token = ?, environment = ?, updated_at = ? WHERE id = ?`)
      .run(token, environment, now, id)
  }

  /** Deletes the install and every capability it minted; false when it was already gone. */
  deleteInstall(id: string): boolean {
    return this.db.transaction(() => {
      this.db.query(`DELETE FROM capabilities WHERE install_id = ?`).run(id)
      return this.db.query(`DELETE FROM installs WHERE id = ?`).run(id).changes > 0
    })()
  }

  insertCapability(row: CapabilityRow): void {
    this.db
      .query(
        `INSERT INTO capabilities (id, install_id, token_hash, label, expires_at, max_per_hour, window_start, window_count, last_used_at, created_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)`
      )
      .run(
        row.id,
        row.installId,
        row.tokenHash,
        row.label,
        row.expiresAt,
        row.maxPerHour,
        row.windowStart,
        row.windowCount,
        row.lastUsedAt,
        row.createdAt
      )
  }

  countCapabilities(installId: string): number {
    const row = this.db
      .query<{ n: number }, [string]>(`SELECT COUNT(*) AS n FROM capabilities WHERE install_id = ?`)
      .get(installId)
    return row?.n ?? 0
  }

  listCapabilities(installId: string): CapabilityRow[] {
    return this.db
      .query<CapabilityDbRow, [string]>(`SELECT * FROM capabilities WHERE install_id = ? ORDER BY created_at, id`)
      .all(installId)
      .map(toCapability)
  }

  deleteCapability(installId: string, id: string): boolean {
    return this.db.query(`DELETE FROM capabilities WHERE id = ? AND install_id = ?`).run(id, installId).changes > 0
  }

  findCapability(tokenHash: string): CapabilityRow | null {
    const row = this.db
      .query<CapabilityDbRow, [string]>(`SELECT * FROM capabilities WHERE token_hash = ?`)
      .get(tokenHash)
    return row ? toCapability(row) : null
  }

  /**
   * Spends one push from the capability's hourly budget (a fixed one-hour
   * window starting at its first push) and stamps last_used_at. False when
   * the budget is spent. bun:sqlite is synchronous and the relay is one
   * process, so the read-then-write cannot interleave with another push.
   */
  consumeCapability(capability: CapabilityRow, now: number): boolean {
    const fresh = now - capability.windowStart >= HOUR_MS
    const windowStart = fresh ? now : capability.windowStart
    const windowCount = fresh ? 0 : capability.windowCount
    if (windowCount >= capability.maxPerHour) return false
    this.db
      .query(`UPDATE capabilities SET window_start = ?, window_count = ?, last_used_at = ? WHERE id = ?`)
      .run(windowStart, windowCount + 1, now, capability.id)
    return true
  }

  /** Expired capabilities are dead weight; the relay sweeps them now and then. */
  deleteExpiredCapabilities(now: number): number {
    return this.db
      .query(`DELETE FROM capabilities WHERE expires_at IS NOT NULL AND expires_at <= ?`)
      .run(now).changes
  }
}

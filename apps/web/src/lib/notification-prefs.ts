// user_notification_prefs access (SERVER-ONLY table — tRPC + the email digest
// sweep read it; it is never an Electric shape). A missing row means all
// defaults (email on, every type on, daily digest); rows are minted lazily
// with a random unsubscribeToken on first read/write/send.

import { randomUUID } from "node:crypto"
import { and, eq, inArray } from "drizzle-orm"
import { db } from "@/db/connection"
import { userNotificationPrefs } from "@/db/schema"
import type { NotificationType } from "@/lib/domain"
import type {
  DigestCadence,
  EmailPrefsLike,
  TypePrefsLike,
} from "@/lib/notification-email-policy"

export interface EmailPrefs extends EmailPrefsLike {
  userId: string
  unsubscribeToken: string
  // EXP-801: may OTHER members' agents message this user over MCP?
  allowAgentMessages: boolean
}

// Mint prefs rows for users that don't have one yet (lazy default + token).
// Safe under races: ON CONFLICT DO NOTHING on the pk.
async function ensurePrefsRows(userIds: string[]): Promise<void> {
  if (userIds.length === 0) return
  await db
    .insert(userNotificationPrefs)
    .values(
      userIds.map((userId) => ({ userId, unsubscribeToken: randomUUID() }))
    )
    .onConflictDoNothing({ target: userNotificationPrefs.userId })
}

export async function getOrCreateEmailPrefs(userId: string): Promise<EmailPrefs> {
  await ensurePrefsRows([userId])
  const [row] = await db
    .select()
    .from(userNotificationPrefs)
    .where(eq(userNotificationPrefs.userId, userId))
    .limit(1)
  return {
    userId: row.userId,
    emailEnabled: row.emailEnabled,
    typePrefs: row.typePrefs,
    digest: row.digest,
    digestHour: row.digestHour,
    unsubscribeToken: row.unsubscribeToken,
    allowAgentMessages: row.allowAgentMessages,
  }
}

export async function updateEmailPrefs(
  userId: string,
  patch: {
    emailEnabled?: boolean
    typePrefs?: Partial<Record<NotificationType, boolean>>
    digest?: DigestCadence
    digestHour?: number
    allowAgentMessages?: boolean
  }
): Promise<EmailPrefs> {
  await ensurePrefsRows([userId])
  if (Object.keys(patch).length > 0) {
    await db
      .update(userNotificationPrefs)
      .set(patch)
      .where(eq(userNotificationPrefs.userId, userId))
  }
  return await getOrCreateEmailPrefs(userId)
}

// One-click unsubscribe: the token IS the auth. Returns true when a row
// matched (idempotent — an already-unsubscribed token still returns true).
export async function unsubscribeByToken(token: string): Promise<boolean> {
  if (!token) return false
  const updated = await db
    .update(userNotificationPrefs)
    .set({ emailEnabled: false })
    .where(eq(userNotificationPrefs.unsubscribeToken, token))
    .returning({ userId: userNotificationPrefs.userId })
  return updated.length > 0
}

// Prefs for a set of users keyed by userId — the digest sweep's prefs source.
// Missing rows are minted first so every outgoing digest email has an
// unsubscribe token.
export async function getEmailPrefsMap(
  userIds: string[]
): Promise<Map<string, EmailPrefs>> {
  if (userIds.length === 0) return new Map()
  await ensurePrefsRows(userIds)

  const rows = await db
    .select()
    .from(userNotificationPrefs)
    .where(inArray(userNotificationPrefs.userId, userIds))

  return new Map(
    rows.map((row) => [
      row.userId,
      {
        userId: row.userId,
        emailEnabled: row.emailEnabled,
        typePrefs: row.typePrefs,
        digest: row.digest,
        digestHour: row.digestHour,
        unsubscribeToken: row.unsubscribeToken,
        allowAgentMessages: row.allowAgentMessages,
      },
    ])
  )
}

// Per-type prefs only, keyed by userId — the push fan-out's gate (EXP-369).
// READ-ONLY on purpose: getEmailPrefsMap MINTS missing rows (plus an
// unsubscribe token) and must never run on every notification fan-out. A user
// with no row is simply absent from the map, which reads as all-defaults.
export async function getTypePrefsMap(
  userIds: string[]
): Promise<Map<string, TypePrefsLike>> {
  if (userIds.length === 0) return new Map()
  const rows = await db
    .select({
      userId: userNotificationPrefs.userId,
      typePrefs: userNotificationPrefs.typePrefs,
    })
    .from(userNotificationPrefs)
    .where(inArray(userNotificationPrefs.userId, userIds))
  return new Map(rows.map((row) => [row.userId, row.typePrefs]))
}

// EXP-801: the recipients among `userIds` who turned OFF messages from
// teammates' agents. READ-ONLY like getTypePrefsMap (no row minting on a
// send path); a user with no row has the default (allowed) and is absent.
export async function getAgentMessageBlocklist(
  userIds: string[]
): Promise<Set<string>> {
  if (userIds.length === 0) return new Set()
  const rows = await db
    .select({ userId: userNotificationPrefs.userId })
    .from(userNotificationPrefs)
    .where(
      and(
        inArray(userNotificationPrefs.userId, userIds),
        eq(userNotificationPrefs.allowAgentMessages, false)
      )
    )
  return new Set(rows.map((row) => row.userId))
}

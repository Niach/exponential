// user_notification_prefs access (SERVER-ONLY table — tRPC + the email digest
// sweep read it; it is never an Electric shape). A missing row means all
// defaults (email on, every type on, hourly digest); rows are minted lazily
// with a random unsubscribeToken on first read/write/send.

import { randomUUID } from "node:crypto"
import { eq, inArray } from "drizzle-orm"
import { db } from "@/db/connection"
import { userNotificationPrefs } from "@/db/schema"
import type { NotificationType } from "@/lib/domain"
import type {
  DigestCadence,
  EmailPrefsLike,
} from "@/lib/notification-email-policy"

export interface EmailPrefs extends EmailPrefsLike {
  userId: string
  unsubscribeToken: string
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
    unsubscribeToken: row.unsubscribeToken,
  }
}

export async function updateEmailPrefs(
  userId: string,
  patch: {
    emailEnabled?: boolean
    typePrefs?: Partial<Record<NotificationType, boolean>>
    digest?: DigestCadence
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
        unsubscribeToken: row.unsubscribeToken,
      },
    ])
  )
}

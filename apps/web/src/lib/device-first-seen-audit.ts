import { sql } from "drizzle-orm"
import type { db as Db } from "@/db/connection"

// FEED-165: who registered a machine id FIRST (`device_first_seen`, the
// FEED-159 pin-owner signal), recomputed from the EARLIEST EVIDENCE instead
// of 0206's "pin holder, else eldest live row" (which locked in a squat that
// held the pin, or one whose row turned eldest after the owner's Remove +
// relaunch). Migration 0209 runs the same rule as SQL (keep the two in step:
// the real-db suite runs 0209 and expects this audit to agree with it).
//
// Candidates per id = the accounts with a `devices` row, a `device_removals`
// row (a Remove hard-deletes the row; the removal keeps the account a
// candidate, its sessions keep the evidence), the pin, or the current
// first-seen row. Teammates who only STARTED sessions on a shared box are
// not candidates. A candidate's evidence = its first `sessions` row on the
// id (`user_id` = the starter), its eldest `devices` row, the pin's
// `created_at`, the current `first_seen_at`; the earliest wins, ties
// sessions > devices > pin > first-seen, then the lower user id. 0209 then
// skips three kinds of row this audit does NOT (so it lists them as
// mismatches on purpose, for a person): a first-seen account holding the
// pin, one with a removal for the id, and a row 0206 did not write (an
// admin `pinTo`, a register after it).

export const EVIDENCE_SOURCES = [`sessions`, `devices`, `pin`, `first_seen`] as const
export type EvidenceSource = (typeof EVIDENCE_SOURCES)[number]

/** One `WITH` body naming `evidence` (every candidate's every source) and
 *  `earliest` (the winner per id); 0209 inlines the same text. */
export const EARLIEST_EVIDENCE_CTE = `
  candidates AS (
    SELECT device_id, user_id FROM devices
    UNION SELECT device_id, user_id FROM device_removals
    UNION SELECT device_id, user_id FROM device_keys
    UNION SELECT device_id, user_id FROM device_first_seen
  ),
  evidence AS (
    SELECT s.device_id, s.user_id, 'sessions' AS source, 1 AS rank, min(s.created_at) AS at
      FROM sessions s
      JOIN candidates c ON c.device_id = s.device_id AND c.user_id = s.user_id
      GROUP BY s.device_id, s.user_id
    UNION ALL
    SELECT device_id, user_id, 'devices', 2, min(created_at) FROM devices GROUP BY device_id, user_id
    UNION ALL
    SELECT device_id, user_id, 'pin', 3, created_at FROM device_keys
    UNION ALL
    SELECT device_id, user_id, 'first_seen', 4, first_seen_at FROM device_first_seen
  ),
  earliest AS (
    SELECT DISTINCT ON (device_id) device_id, user_id, source, at
      FROM evidence
      ORDER BY device_id, at, rank, user_id
  )`

export type FirstSeenFinding = {
  deviceId: string
  /** `mismatch` = the first-seen account is not the earliest-evidence one
   *  (or there is no first-seen row); `pin` = they agree, but another
   *  account holds the pin (the first-seen account's next proven register
   *  takes it over and withdraws that account's shares). */
  kind: `mismatch` | `pin`
  firstSeen: { userId: string; email: string | null; at: string } | null
  earliest: { userId: string; email: string | null; source: EvidenceSource; at: string; hasRow: boolean }
  pin: { userId: string; email: string | null; at: string } | null
  /** Every candidate's evidence, earliest first. */
  evidence: { userId: string; source: EvidenceSource; at: string }[]
}

/** Read-only: the ids 0209's rule disagrees with, and the ids whose pin
 *  another account than the first-seen one holds. */
export async function deviceFirstSeenFindings(database: Pick<typeof Db, `execute`>): Promise<FirstSeenFinding[]> {
  const result = await database.execute(
    sql.raw(`
    WITH ${EARLIEST_EVIDENCE_CTE}
    SELECT e.device_id AS "deviceId",
      CASE WHEN fs.user_id IS DISTINCT FROM e.user_id THEN 'mismatch' ELSE 'pin' END AS kind,
      CASE WHEN fs.device_id IS NULL THEN NULL ELSE json_build_object(
        'userId', fs.user_id, 'email', fsu.email, 'at', fs.first_seen_at) END AS "firstSeen",
      json_build_object('userId', e.user_id, 'email', eu.email, 'source', e.source, 'at', e.at,
        'hasRow', EXISTS (SELECT 1 FROM devices d WHERE d.device_id = e.device_id AND d.user_id = e.user_id)) AS earliest,
      CASE WHEN k.device_id IS NULL THEN NULL ELSE json_build_object(
        'userId', k.user_id, 'email', ku.email, 'at', k.created_at) END AS pin,
      (SELECT json_agg(json_build_object('userId', v.user_id, 'source', v.source, 'at', v.at)
         ORDER BY v.at, v.rank, v.user_id)
         FROM evidence v WHERE v.device_id = e.device_id) AS evidence
    FROM earliest e
    LEFT JOIN users eu ON eu.id = e.user_id
    LEFT JOIN device_first_seen fs ON fs.device_id = e.device_id
    LEFT JOIN users fsu ON fsu.id = fs.user_id
    LEFT JOIN device_keys k ON k.device_id = e.device_id
    LEFT JOIN users ku ON ku.id = k.user_id
    WHERE fs.user_id IS DISTINCT FROM e.user_id
       OR (k.device_id IS NOT NULL AND k.user_id <> e.user_id)
    ORDER BY e.device_id
  `)
  )
  return result.rows as unknown as FirstSeenFinding[]
}

/** The admin call that settles a finding: pin it to the earliest-evidence
 *  account (`pinTo` needs that account's `devices` row). */
export function settleCall(finding: FirstSeenFinding): string {
  return `admin.clearDeviceKey(${JSON.stringify({ deviceId: finding.deviceId, pinTo: finding.earliest.userId })})`
}

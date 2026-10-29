-- FEED-165: recompute every `device_first_seen` row from the EARLIEST
-- EVIDENCE. 0206 took the pin holder first, else the eldest live `devices`
-- row, which locked in a pre-deploy squat whose row turned eldest after the
-- owner's Remove + relaunch (a squat that already held the pin stays: the
-- first guard below; the audit lists it for `pinTo`).
-- Same rule as `EARLIEST_EVIDENCE_CTE` (apps/web/src/lib/device-first-seen-audit.ts,
-- the audit `bun run audit:device-first-seen`); keep the two in step.
--
-- Candidates per id: accounts with a `devices` row, a `device_removals` row
-- (Remove hard-deletes the row; the removal keeps the account a candidate),
-- the pin, or the current first-seen row. Teammates who only started
-- sessions on a shared box are not candidates. Evidence: the candidate's
-- first `sessions` row on the id (`user_id` = the starter), its eldest
-- `devices` row, the pin's `created_at`, the current `first_seen_at`.
-- Earliest wins; ties sessions > devices > pin > first-seen, then user id.
-- Ids without any evidence keep their row.
--
-- Pin-owner rule (`pinDeviceKeyInTx`): the first-seen account's proven
-- register takes a pin another account holds and withdraws its shares, so
-- 0213 never moves first-seen away from a pin holder (below).
-- An admin settles a disputed id with `admin.clearDeviceKey({pinTo})`.
--
-- Three guards (the audit applies none, so it lists these ids as
-- mismatches on purpose, for a person to review and settle with `pinTo`):
-- * first-seen never moves AWAY from the current pin holder: a migration
--   never hands a live pin to another account (its next register would
--   take it and withdraw the holder's shares). The data cannot tell a
--   pre-deploy squat's pin from an owner's after a Remove + relaunch (whose
--   original row is gone), so the worst case is 0206's status quo, flagged.
-- * first-seen never moves AWAY from an account with a `device_removals` row
--   for the id: Remove hard-deleted its original row, so a squat's older row
--   would otherwise read as the earliest evidence. (A relaunch deletes the
--   removal, so a Remove + relaunch leaves no trace to guard on.)
-- * only rows 0206's backfill wrote are recomputed: `pinTo` (on master since
--   #1301) and registers after the deploy write `now()`. A 0206 row carries
--   its holder's pin `created_at` or one of its holder's `devices`
--   `created_at`, or (its source row since gone) a time before 0206 was
--   generated (journal `when` 1793190174891), which no later write can have.
WITH
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
  )
INSERT INTO "device_first_seen" ("device_id", "user_id", "first_seen_at")
SELECT device_id, user_id, at FROM earliest
ON CONFLICT ("device_id") DO UPDATE
  SET "user_id" = EXCLUDED."user_id", "first_seen_at" = EXCLUDED."first_seen_at"
  WHERE ("device_first_seen"."user_id" IS DISTINCT FROM EXCLUDED."user_id"
     OR "device_first_seen"."first_seen_at" IS DISTINCT FROM EXCLUDED."first_seen_at")
    AND ("device_first_seen"."user_id" = EXCLUDED."user_id" OR NOT EXISTS (
      SELECT 1 FROM device_keys k
        WHERE k.device_id = "device_first_seen"."device_id" AND k.user_id = "device_first_seen"."user_id"))
    AND ("device_first_seen"."user_id" = EXCLUDED."user_id" OR NOT EXISTS (
      SELECT 1 FROM device_removals r
        WHERE r.device_id = "device_first_seen"."device_id" AND r.user_id = "device_first_seen"."user_id"))
    AND ("device_first_seen"."first_seen_at" < to_timestamp(1793190174.891)
      OR EXISTS (SELECT 1 FROM device_keys k
        WHERE k.device_id = "device_first_seen"."device_id" AND k.user_id = "device_first_seen"."user_id"
          AND k.created_at = "device_first_seen"."first_seen_at")
      OR EXISTS (SELECT 1 FROM devices d
        WHERE d.device_id = "device_first_seen"."device_id" AND d.user_id = "device_first_seen"."user_id"
          AND d.created_at = "device_first_seen"."first_seen_at"));

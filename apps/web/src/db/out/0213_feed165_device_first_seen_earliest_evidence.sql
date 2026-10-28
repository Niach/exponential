-- FEED-165: recompute every `device_first_seen` row from the EARLIEST
-- EVIDENCE. 0206 took the pin holder first, else the eldest live `devices`
-- row, which locked in a pre-deploy squat: a squat that already held the
-- pin, or one whose row turned eldest after the owner's Remove + relaunch.
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
-- Pin-owner rule (`pinDeviceKeyInTx`): where the new first-seen account
-- is not the current pin holder, the pin STAYS until the first-seen
-- account's next proven register, which takes it over and withdraws the
-- holder's shares (unless the holder proves the same key: the same box
-- under a second login). The holder's own registers can no longer re-pin.
-- An admin settles a disputed id with `admin.clearDeviceKey({pinTo})`.
-- Runs once, in the release that ships 0206, so no admin `pinTo` exists yet
-- for it to override.
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
  WHERE "device_first_seen"."user_id" IS DISTINCT FROM EXCLUDED."user_id"
     OR "device_first_seen"."first_seen_at" IS DISTINCT FROM EXCLUDED."first_seen_at";

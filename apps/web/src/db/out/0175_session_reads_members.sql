-- EXP-1323 follow-up: 0174 seeded `session_reads` for each session's STARTER
-- only, so after deploy every SHARED thread with an agent message renders
-- bold for every OTHER member. Seed every current member of a shared
-- session's team as having read it "as of now" (the same `read_at = now()`
-- mark 0174 wrote; `unreadAt` compares the newest agent message against it).
-- Idempotent: the (user_id, session_id) unique index absorbs reruns and the
-- rows 0174 already wrote.
INSERT INTO "session_reads" ("user_id", "session_id", "read_at")
SELECT tm."user_id", s."id", now()
FROM "sessions" s
JOIN "team_members" tm ON tm."team_id" = s."team_id"
WHERE s."shared"
ON CONFLICT DO NOTHING;

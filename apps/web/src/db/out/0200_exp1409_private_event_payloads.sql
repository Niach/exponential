-- #EXP-1409: `session.ended` events written before this change carried every ended session's `title` (often the prompt) and `branch`, a PRIVATE one's included, to the whole team (the `events` shape) and into triggered agents' conversations (the firing quotes the payload). First the firings a conversation's readers may not see (`askHidden`: the session private, or gone, and the conversation shared or another member's) are rewritten to quote the ids-only payload, exactly as `TRIGGER_COPY.event` prints it; then every such event row keeps `sessionId` + `rootId` only.
UPDATE "messages" m SET
  "body" = 'Event `' || e."name" || '` (from ' || CASE e."emitted_by" WHEN 'session' THEN 'a session' WHEN 'user' THEN 'a person' ELSE 'the server' END || '):'
    || E'\n\n```json\n{\n  "rootId": ' || coalesce(e."payload"->'rootId', 'null'::jsonb)::text
    || E',\n  "sessionId": ' || coalesce(e."payload"->'sessionId', 'null'::jsonb)::text || E'\n}\n```',
  "updated_at" = now()
FROM "sessions" l, "events" e LEFT JOIN "sessions" o ON o."id"::text = e."payload"->>'sessionId'
WHERE l."id" = m."session_id" AND e."id"::text = m."card"->>'eventId'
  AND m."card"->>'event' = 'trigger_fired' AND e."name" = 'session.ended'
  AND (o."id" IS NULL OR (NOT o."shared" AND (l."shared" OR l."user_id" <> o."user_id")))
  AND (m."body" LIKE E'%\n  "title": %' OR m."body" LIKE E'%\n  "branch": %');--> statement-breakpoint
UPDATE "events" e SET
  "payload" = jsonb_build_object('sessionId', e."payload"->'sessionId', 'rootId', e."payload"->'rootId')
WHERE e."name" = 'session.ended' AND e."emitted_by" = 'server'
  AND (e."payload"->'title' IS NOT NULL OR e."payload"->'branch' IS NOT NULL)
  AND NOT EXISTS (SELECT 1 FROM "sessions" s WHERE s."id"::text = e."payload"->>'sessionId' AND s."shared");

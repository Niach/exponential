-- #EXP-1399 review: one `ask` mirror of a card per conversation. Drop any duplicate (keep the oldest), close the mirrors whose origin session already ended or is gone, then pin it with a unique index.
DELETE FROM "messages" m
USING "messages" k
WHERE m."card"->>'kind' = 'ask'
  AND k."card"->>'kind' = 'ask'
  AND k."session_id" = m."session_id"
  AND k."card"->>'messageId' = m."card"->>'messageId'
  AND (k."created_at", k."id") < (m."created_at", m."id");--> statement-breakpoint
UPDATE "messages" m SET
  "answered_at" = now(),
  "updated_at" = now()
WHERE m."answered_at" IS NULL
  AND m."card"->>'kind' = 'ask'
  AND NOT EXISTS (
    SELECT 1 FROM "sessions" s
    WHERE s."id"::text = m."card"->>'sessionId' AND s."status" <> 'ended'
  );--> statement-breakpoint
CREATE UNIQUE INDEX "uq_messages_ask_mirror" ON "messages" USING btree ("session_id",(card->>'messageId')) WHERE card->>'kind' = 'ask';

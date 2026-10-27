-- #EXP-1399 review: ask mirrors written before this change carry the origin card's text in plaintext (its synced body, MCP `sessions get`). Every client draws a mirror from its ORIGIN card, so EVERY ask mirror's body becomes the neutral `askHiddenText` (the private lead + the session id only).
UPDATE "messages" m SET
  "body" = (CASE
      WHEN (SELECT o."card"->>'kind' FROM "messages" o WHERE o."id"::text = m."card"->>'messageId') = 'plan'
        OR m."body" LIKE 'Proposes a plan:%'
      THEN 'Proposes a plan in a private chat.'
      ELSE 'Asks a question in a private chat.'
    END)
    || E'\n' || 'Read it with exponential_sessions({op: "get", id: "' || (m."card"->>'sessionId')
    || '"}) and answer with exponential_sessions_send({sessionId: "' || (m."card"->>'sessionId')
    || '", answer: {…}}) if you may see it; otherwise end your turn, a person answers in the card.',
  "updated_at" = now()
WHERE m."card"->>'kind' = 'ask'
  AND m."body" NOT LIKE 'Asks a question in a private chat.%'
  AND m."body" NOT LIKE 'Proposes a plan in a private chat.%';--> statement-breakpoint
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